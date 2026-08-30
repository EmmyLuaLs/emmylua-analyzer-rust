use emmylua_code_analysis::DocumentView;
use emmylua_parser::{LuaSyntaxNode, LuaSyntaxToken, LuaTokenKind};
use lsp_types::{Color, ColorInformation};
use rowan::{TextRange, TextSize};

pub fn build_colors(root: LuaSyntaxNode, document: &DocumentView) -> Vec<ColorInformation> {
    let mut result = vec![];
    let string_tokens = root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
        .filter(|it| {
            it.kind() == LuaTokenKind::TkString.into()
                || it.kind() == LuaTokenKind::TkLongString.into()
        });

    for token in string_tokens {
        try_build_color_information(token, document, &mut result);
    }

    result
}

fn try_build_color_information(
    token: LuaSyntaxToken,
    document: &DocumentView,
    result: &mut Vec<ColorInformation>,
) -> Option<()> {
    let text = token.text();
    let bytes = text.as_bytes();
    let len = bytes.len();

    let mut i = 0;
    while i + 6 <= len {
        if bytes[i].is_ascii_hexdigit() {
            let is_start_boundary = if i == 0 {
                true
            } else {
                !bytes[i - 1].is_ascii_alphanumeric()
            };
            if !is_start_boundary {
                i += 1;
                continue;
            }

            let mut j = i + 1;
            while j < len && bytes[j].is_ascii_hexdigit() {
                j += 1;
            }

            if j - i == 6 || j - i == 8 {
                let color_text = &text[i..j];
                if let Some(color) = parse_hex_color(color_text) {
                    let source_text_range = token.text_range();
                    let start = if bytes[i - 1] == b'#' { i - 1 } else { i };
                    let text_range = TextRange::new(
                        source_text_range.start() + TextSize::new(start as u32),
                        source_text_range.start() + TextSize::new(j as u32),
                    );
                    let lsp_range = document.to_lsp_range(text_range)?;

                    result.push(ColorInformation {
                        range: lsp_range,
                        color,
                    });
                }
            }

            i = j;
        } else {
            i += 1;
        }
    }

    Some(())
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    match hex.len() {
        6 => {
            // RGB format
            let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
            Some(Color {
                red: r,
                green: g,
                blue: b,
                alpha: 1.0,
            })
        }
        8 => {
            // RGBA format
            let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0;
            Some(Color {
                red: r,
                green: g,
                blue: b,
                alpha: a,
            })
        }
        _ => None, // Unsupported length
    }
}

pub fn convert_color_to_hex(color: Color, len: usize) -> String {
    let r = (color.red * 255.0).round() as u8;
    let g = (color.green * 255.0).round() as u8;
    let b = (color.blue * 255.0).round() as u8;
    match len {
        6 => format!("{:02X}{:02X}{:02X}", r, g, b),
        7 => format!("#{:02X}{:02X}{:02X}", r, g, b),
        8 => {
            let a = (color.alpha * 255.0).round() as u8;
            format!("{:02X}{:02X}{:02X}{:02X}", r, g, b, a)
        }
        9 => {
            let a = (color.alpha * 255.0).round() as u8;
            format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a)
        }
        _ => "".to_string(),
    }
}
