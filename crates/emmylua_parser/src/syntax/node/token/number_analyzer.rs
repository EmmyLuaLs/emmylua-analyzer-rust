use std::fmt::Display;

use crate::{
    LuaSyntaxToken,
    parser_error::{LuaParseError, LuaParseErrorKind},
};

pub fn float_token_value(token: &LuaSyntaxToken) -> Result<f64, LuaParseError> {
    let text = token.text();
    let hex = text.starts_with("0x") || text.starts_with("0X");

    // This section handles the parsing of hexadecimal floating-point numbers.
    // Hexadecimal floating-point literals are of the form 0x1.8p3, where:
    // - "0x1.8" is the significand (integer and fractional parts in hexadecimal)
    // - "p3" is the exponent (in decimal, base 2 exponent)
    let value = if hex {
        let hex_float_text = &text[2..];
        let exponent_position = hex_float_text
            .find('p')
            .or_else(|| hex_float_text.find('P'));
        let (float_part, exponent_part) = if let Some(pos) = exponent_position {
            (&hex_float_text[..pos], &hex_float_text[(pos + 1)..])
        } else {
            (hex_float_text, "")
        };

        let (integer_part, fraction_value) = if let Some(dot_pos) = float_part.find('.') {
            let (int_part, frac_part) = float_part.split_at(dot_pos);
            let int_value = if !int_part.is_empty() {
                i64::from_str_radix(int_part, 16).unwrap_or(0)
            } else {
                0
            };
            let frac_part = &frac_part[1..];
            let frac_value = if !frac_part.is_empty() {
                let frac_part_value = i64::from_str_radix(frac_part, 16).unwrap_or(0);
                frac_part_value as f64 * 16f64.powi(-(frac_part.len() as i32))
            } else {
                0.0
            };
            (int_value, frac_value)
        } else {
            (i64::from_str_radix(float_part, 16).unwrap_or(0), 0.0)
        };

        let mut value = integer_part as f64 + fraction_value;
        if !exponent_part.is_empty()
            && let Ok(exp) = exponent_part.parse::<i32>()
        {
            value *= 2f64.powi(exp);
        }
        value
    } else {
        let (float_part, exponent_part) =
            if let Some(pos) = text.find('e').or_else(|| text.find('E')) {
                (&text[..pos], &text[(pos + 1)..])
            } else {
                (text, "")
            };

        let mut value = float_part.parse::<f64>().map_err(|e| {
            LuaParseError::new(
                LuaParseErrorKind::SyntaxError,
                &t!(
                    "The float literal '%{text}' is invalid, %{err}",
                    text = text,
                    err = e
                ),
                token.text_range(),
            )
        })?;

        if !exponent_part.is_empty()
            && let Ok(exp) = exponent_part.parse::<i32>()
        {
            value *= 10f64.powi(exp);
        }
        value
    };

    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntegerRepr {
    Normal,
    Hex,
    Bin,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberResult {
    Int(i64),
    Uint(u64),
    Float(f64),
}

impl NumberResult {
    pub fn is_unsigned(&self) -> bool {
        matches!(self, NumberResult::Uint(_))
    }

    pub fn is_signed(&self) -> bool {
        matches!(self, NumberResult::Int(_))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, NumberResult::Float(_))
    }

    pub fn as_integer(&self) -> Option<i64> {
        match self {
            NumberResult::Int(v) => Some(*v),
            _ => None,
        }
    }
}

impl Display for NumberResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NumberResult::Int(v) => write!(f, "{}", v),
            NumberResult::Uint(v) => write!(f, "{}", v),
            NumberResult::Float(v) => write!(f, "{}", v),
        }
    }
}

pub fn int_token_value(token: &LuaSyntaxToken) -> Result<NumberResult, LuaParseError> {
    let text = token.text();
    let repr = if text.starts_with("0x") || text.starts_with("0X") {
        IntegerRepr::Hex
    } else if text.starts_with("0b") || text.starts_with("0B") {
        IntegerRepr::Bin
    } else {
        IntegerRepr::Normal
    };

    // 检查是否有无符号后缀并去除后缀
    let mut is_luajit_unsigned = false;
    let mut suffix_count = 0;
    for c in text.chars().rev() {
        if c == 'u' || c == 'U' {
            is_luajit_unsigned = true;
            suffix_count += 1;
        } else if c == 'l' || c == 'L' {
            suffix_count += 1;
        } else {
            break;
        }
    }

    let text = &text[..text.len() - suffix_count];

    // 首先尝试解析为有符号整数
    let signed_value = match repr {
        IntegerRepr::Hex => {
            let text = &text[2..];
            i64::from_str_radix(text, 16)
        }
        IntegerRepr::Bin => {
            let text = &text[2..];
            i64::from_str_radix(text, 2)
        }
        IntegerRepr::Normal => text.parse::<i64>(),
    };

    match signed_value {
        Ok(value) => Ok(NumberResult::Int(value)),
        Err(e) => {
            // 按照Lua的行为：如果整数溢出，尝试解析为浮点数
            if matches!(
                *e.kind(),
                std::num::IntErrorKind::NegOverflow | std::num::IntErrorKind::PosOverflow
            ) {
                // 如果是luajit无符号整数，尝试解析为u64
                if is_luajit_unsigned {
                    let unsigned_value = match repr {
                        IntegerRepr::Hex => {
                            let text = &text[2..];
                            u64::from_str_radix(text, 16)
                        }
                        IntegerRepr::Bin => {
                            let text = &text[2..];
                            u64::from_str_radix(text, 2)
                        }
                        IntegerRepr::Normal => text.parse::<u64>(),
                    };

                    if let Ok(value) = unsigned_value {
                        return Ok(NumberResult::Uint(value));
                    }
                } else {
                    // Lua 5.4行为：对于十六进制/二进制整数溢出，解析为u64然后reinterpret为i64
                    // 例如：0xFFFFFFFFFFFFFFFF = -1
                    if matches!(repr, IntegerRepr::Hex | IntegerRepr::Bin) {
                        let unsigned_value = match repr {
                            IntegerRepr::Hex => {
                                let text = &text[2..];
                                u64::from_str_radix(text, 16)
                            }
                            IntegerRepr::Bin => {
                                let text = &text[2..];
                                u64::from_str_radix(text, 2)
                            }
                            _ => unreachable!(),
                        };

                        if let Ok(value) = unsigned_value {
                            // Reinterpret u64 as i64 (补码转换)
                            return Ok(NumberResult::Int(value as i64));
                        } else {
                            // 超过64位，转换为浮点数
                            // 例如：0x13121110090807060504030201
                            let hex_str = match repr {
                                IntegerRepr::Hex => &text[2..],
                                IntegerRepr::Bin => &text[2..],
                                _ => unreachable!(),
                            };

                            // 手动将十六进制转为浮点数
                            let base = if matches!(repr, IntegerRepr::Hex) {
                                16.0
                            } else {
                                2.0
                            };
                            let mut result = 0.0f64;
                            for c in hex_str.chars() {
                                if let Some(digit) = c.to_digit(base as u32) {
                                    result = result * base + (digit as f64);
                                }
                            }
                            return Ok(NumberResult::Float(result));
                        }
                    } else if let Ok(f) = text.parse::<f64>() {
                        // 十进制整数溢出，解析为浮点数
                        return Ok(NumberResult::Float(f));
                    }
                }

                Err(LuaParseError::new(
                    LuaParseErrorKind::SyntaxError,
                    &t!("malformed number"),
                    token.text_range(),
                ))
            } else {
                Err(LuaParseError::new(
                    LuaParseErrorKind::SyntaxError,
                    &t!(
                        "Failed to parse integer literal %{text}: %{err}",
                        text = token.text(),
                        err = e
                    ),
                    token.text_range(),
                ))
            }
        }
    }
}
