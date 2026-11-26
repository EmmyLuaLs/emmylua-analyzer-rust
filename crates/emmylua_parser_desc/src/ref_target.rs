use emmylua_parser::{Reader, SourceRange};
use rowan::{TextRange, TextSize};
use std::str::FromStr;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LuaDescRefPathItem {
    Name(String),
    Number(i64),
    Type(String),
}

/// A Rust-like intra-doc link label, e.g. ``[`Widget`]``.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct IntraDocLink<'a> {
    /// Symbol path without the surrounding backticks.
    pub target: &'a str,
    /// Byte offset of `target` inside the original label.
    pub target_start: usize,
}

/// Parses a Rust-like intra-doc link label.
///
/// Only the code-span form is supported: the label must be a single inline code
/// span (``[`Widget`]``, not `[Widget]`) whose content is a Lua symbol path.
/// This keeps ordinary Markdown shortcut/reference links untouched.
pub fn parse_intra_doc_link(label: &str) -> Option<IntraDocLink<'_>> {
    let target = label.strip_prefix('`')?.strip_suffix('`')?;
    if target.is_empty() || target.contains('`') || !is_doc_path(target) {
        return None;
    }

    Some(IntraDocLink {
        target,
        target_start: 1,
    })
}

fn is_doc_path(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphanumeric() || first == '_') {
        return false;
    }

    text.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        && !text.contains("..")
}

impl LuaDescRefPathItem {
    pub fn is_name(&self) -> bool {
        matches!(self, LuaDescRefPathItem::Name(_))
    }

    pub fn get_name(&self) -> Option<&str> {
        match self {
            LuaDescRefPathItem::Name(name) => Some(name),
            _ => None,
        }
    }
}

pub fn parse_ref_target(
    text: &str,
    range: TextRange,
    cursor_offset: TextSize,
) -> Option<Vec<(LuaDescRefPathItem, TextRange)>> {
    let cursor_offset: usize = cursor_offset.into();

    let mut reader = Reader::new_with_range(&text[range], range.into());
    let mut result = Vec::new();

    while !reader.is_eof() {
        match reader.current_char() {
            '[' if reader.prev_char() == '.' => {
                reader.bump();
                reader.reset_buff();
                match eat_type(&mut reader) {
                    Ok(Type::String) => {
                        let name = reader.current_text();
                        result.push((
                            LuaDescRefPathItem::Name(name[1..name.len() - 1].to_string()),
                            SourceRange::new(
                                reader.current_range().start_offset + 1,
                                reader.current_range().length - 2,
                            )
                            .into(),
                        ));
                    }
                    Ok(Type::Number) => {
                        if let Ok(num) = i64::from_str(reader.current_text()) {
                            result.push((
                                LuaDescRefPathItem::Number(num),
                                reader.current_range().into(),
                            ));
                        } else {
                            result.push((
                                LuaDescRefPathItem::Name(reader.current_text().to_string()),
                                reader.current_range().into(),
                            ));
                        }
                    }
                    Ok(Type::Complex) => {
                        result.push((
                            LuaDescRefPathItem::Type(reader.current_text().to_string()),
                            reader.current_range().into(),
                        ));
                    }
                    Err(()) => return None,
                }

                if reader.current_range().end_offset() >= cursor_offset {
                    reader.reset_buff();
                    break;
                }

                reader.bump();
                reader.bump();
                reader.reset_buff();
            }
            '.' => {
                result.push((
                    LuaDescRefPathItem::Name(reader.current_text().to_string()),
                    reader.current_range().into(),
                ));

                if reader.current_range().end_offset() >= cursor_offset {
                    reader.reset_buff();
                    break;
                }

                reader.bump();
                reader.reset_buff();
            }
            '-' | '_' | 'a'..='z' | 'A'..='Z' | '0'..='9' => reader.bump(),
            _ => {
                if reader.current_range().end_offset() < cursor_offset {
                    // Illegal character before cursor, bail.
                    return None;
                } else {
                    // Illegal character after cursor, ignore.
                    break;
                }
            }
        }
    }

    if !reader.current_range().is_empty() {
        result.push((
            LuaDescRefPathItem::Name(reader.current_text().to_string()),
            reader.current_range().into(),
        ));
    }

    Some(result)
}

enum Type {
    String,
    Number,
    Complex,
}

fn eat_type(reader: &mut Reader) -> Result<Type, ()> {
    if matches!(reader.current_char(), '"' | '\'') {
        if !eat_string(reader) {
            return Err(());
        }

        if reader.current_char() == ']' {
            return if matches!(reader.next_char(), '.' | '\0') {
                Ok(Type::String)
            } else {
                Err(())
            };
        }
    }

    let mut depth = 1;
    while !reader.is_eof() {
        match reader.current_char() {
            ']' | ')' | '>' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return if matches!(reader.next_char(), '.' | '\0') {
                        if reader
                            .current_text()
                            .chars()
                            .skip_while(|c| *c == '-')
                            .all(|c| c.is_ascii_digit())
                        {
                            Ok(Type::Number)
                        } else {
                            Ok(Type::Complex)
                        }
                    } else {
                        Err(())
                    };
                }
                reader.bump()
            }
            '[' | '(' | '<' | '{' => {
                depth += 1;
                reader.bump()
            }
            '"' | '\'' => {
                if !eat_string(reader) {
                    return Err(());
                }
            }
            _ => reader.bump(),
        }
    }

    Err(())
}

#[must_use]
fn eat_string(reader: &mut Reader) -> bool {
    let end_ch = reader.current_char();
    reader.bump();
    while !reader.is_eof() {
        match reader.current_char() {
            '\\' => {
                reader.bump();
                reader.bump();
            }
            ch if ch == end_ch => {
                reader.bump();
                return true;
            }
            _ => {
                reader.bump();
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[gtest]
    fn test_parse_ref_target_simple() {
        let res = parse_ref_target("a.b.c.d", TextRange::up_to(7.into()), 7.into());
        expect_eq!(
            res,
            Some(vec![
                (
                    LuaDescRefPathItem::Name("a".to_string()),
                    TextRange::at(0.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Name("b".to_string()),
                    TextRange::at(2.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Name("c".to_string()),
                    TextRange::at(4.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Name("d".to_string()),
                    TextRange::at(6.into(), 1.into())
                ),
            ])
        )
    }

    #[gtest]
    fn test_parse_ref_target_simple_partial() {
        let res = parse_ref_target("a.abc.d", TextRange::up_to(7.into()), 2.into());
        expect_eq!(
            res,
            Some(vec![
                (
                    LuaDescRefPathItem::Name("a".to_string()),
                    TextRange::at(0.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Name("abc".to_string()),
                    TextRange::at(2.into(), 3.into())
                ),
            ])
        );

        let res = parse_ref_target("a.abc.d", TextRange::up_to(7.into()), 3.into());
        expect_eq!(
            res,
            Some(vec![
                (
                    LuaDescRefPathItem::Name("a".to_string()),
                    TextRange::at(0.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Name("abc".to_string()),
                    TextRange::at(2.into(), 3.into())
                ),
            ])
        );

        let res = parse_ref_target("a.abc.d", TextRange::up_to(7.into()), 5.into());
        expect_eq!(
            res,
            Some(vec![
                (
                    LuaDescRefPathItem::Name("a".to_string()),
                    TextRange::at(0.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Name("abc".to_string()),
                    TextRange::at(2.into(), 3.into())
                ),
            ])
        );
    }

    #[gtest]
    fn test_parse_ref_target_type() {
        let res = parse_ref_target("a.b.[c.d].e", TextRange::up_to(11.into()), 11.into());
        expect_eq!(
            res,
            Some(vec![
                (
                    LuaDescRefPathItem::Name("a".to_string()),
                    TextRange::at(0.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Name("b".to_string()),
                    TextRange::at(2.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Type("c.d".to_string()),
                    TextRange::at(5.into(), 3.into())
                ),
                (
                    LuaDescRefPathItem::Name("e".to_string()),
                    TextRange::at(10.into(), 1.into())
                ),
            ])
        )
    }

    #[gtest]
    fn test_parse_ref_target_type_at_end() {
        let res = parse_ref_target("a.b.[c.d]", TextRange::up_to(9.into()), 9.into());
        expect_eq!(
            res,
            Some(vec![
                (
                    LuaDescRefPathItem::Name("a".to_string()),
                    TextRange::at(0.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Name("b".to_string()),
                    TextRange::at(2.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Type("c.d".to_string()),
                    TextRange::at(5.into(), 3.into())
                ),
            ])
        )
    }

    #[gtest]
    fn test_parse_ref_target_type_braces_strings() {
        let res = parse_ref_target(
            "a.b.[fun(x: table<int, string>): { n: int, lit: \"}]\" }]",
            TextRange::up_to(55.into()),
            55.into(),
        );
        expect_eq!(
            res,
            Some(vec![
                (
                    LuaDescRefPathItem::Name("a".to_string()),
                    TextRange::at(0.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Name("b".to_string()),
                    TextRange::at(2.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Type(
                        "fun(x: table<int, string>): { n: int, lit: \"}]\" }".to_string()
                    ),
                    TextRange::at(5.into(), 49.into())
                ),
            ])
        )
    }

    #[gtest]
    fn test_parse_ref_target_type_string_literal() {
        let res = parse_ref_target("a.b.['c']", TextRange::up_to(9.into()), 9.into());
        expect_eq!(
            res,
            Some(vec![
                (
                    LuaDescRefPathItem::Name("a".to_string()),
                    TextRange::at(0.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Name("b".to_string()),
                    TextRange::at(2.into(), 1.into())
                ),
                (
                    LuaDescRefPathItem::Name("c".to_string()),
                    TextRange::at(6.into(), 1.into())
                ),
            ])
        )
    }
}

#[cfg(test)]
mod intra_doc_link_tests {
    use super::parse_intra_doc_link;

    #[test]
    fn accepts_code_span_paths() {
        for (label, target) in [
            ("`Widget`", "Widget"),
            ("`Widget.name`", "Widget.name"),
            ("`ns.Widget.name`", "ns.Widget.name"),
            ("`Widget.`", "Widget."),
            ("`_private.value`", "_private.value"),
        ] {
            let link = parse_intra_doc_link(label)
                .unwrap_or_else(|| panic!("expected {label} to be a link"));
            assert_eq!(link.target, target);
            assert_eq!(link.target_start, 1);
        }
    }

    #[test]
    fn rejects_non_link_forms() {
        for label in [
            "Widget",
            "[`Widget`]",
            "`Widget.name()`",
            "`Widget name`",
            "``",
            "`Widget..name`",
            "`Widget-name`",
            "`Widget/name`",
            "`Widget` extra",
            "`Widget`.`name`",
            "`.Widget`",
        ] {
            assert!(
                parse_intra_doc_link(label).is_none(),
                "expected {label} not to be a link"
            );
        }
    }
}
