use emmylua_parser::{
    float_token_value, int_token_value, LuaAstNode, LuaClosureExpr, LuaLiteralExpr, LuaSyntaxKind,
    LuaSyntaxToken, LuaTokenKind,
};

use crate::{DiagnosticCode, LuaSignatureId, SemanticModel};

use super::DiagnosticContext;

pub const CODES: &[DiagnosticCode] = &[DiagnosticCode::SyntaxError, DiagnosticCode::LuaSyntaxError];

pub fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) -> Option<()> {
    if let Some(parse_errors) = semantic_model.get_file_parse_error() {
        for parse_error in parse_errors {
            context.add_diagnostic(
                DiagnosticCode::SyntaxError,
                parse_error.1,
                parse_error.0,
                None,
            );
        }
    }

    let root = semantic_model.get_root();
    for node_or_token in root.syntax().descendants_with_tokens() {
        if let Some(token) = node_or_token.into_token() {
            match token.kind().into() {
                LuaTokenKind::TkInt => {
                    if let Err(err) = int_token_value(&token) {
                        context.add_diagnostic(
                            DiagnosticCode::SyntaxError,
                            err.range,
                            err.message,
                            None,
                        );
                    }
                }
                LuaTokenKind::TkFloat => {
                    if let Err(err) = float_token_value(&token) {
                        context.add_diagnostic(
                            DiagnosticCode::SyntaxError,
                            err.range,
                            err.message,
                            None,
                        );
                    }
                }
                LuaTokenKind::TkString => {
                    if let Err(err) = check_normal_string_error(&token) {
                        context.add_diagnostic(
                            DiagnosticCode::SyntaxError,
                            token.text_range(),
                            err,
                            None,
                        );
                    }
                }
                LuaTokenKind::TkDots => {
                    check_dots_literal_error(context, semantic_model, &token);
                }
                _ => {}
            }
        }
    }

    Some(())
}

// this function is like string_token_value, but optimize for performance
fn check_normal_string_error(string_token: &LuaSyntaxToken) -> Result<(), String> {
    let text = string_token.text();
    if text.len() < 2 {
        return Ok(());
    }

    let mut chars = text.chars().peekable();
    let delimiter = chars.next().unwrap();

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next_char) = chars.next() {
                    match next_char {
                        'a' | 'b' | 'f' | 'n' | 'r' | 't' | 'v' | '\\' | '\'' | '\"' | '\r'
                        | '\n' => {}
                        'x' => {
                            // Hexadecimal escape sequence
                            let hex = chars.by_ref().take(2).collect::<String>();
                            if hex.len() == 2 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                                if let Err(_) = u8::from_str_radix(&hex, 16) {
                                    return Err(t!(
                                        "Invalid hex escape sequence '\\x%{hex}'",
                                        hex = hex
                                    )
                                    .to_string());
                                }
                            } else {
                                return Err(t!(
                                    "Invalid hex escape sequence '\\x%{hex}'",
                                    hex = hex
                                )
                                .to_string());
                            }
                        }
                        'u' => {
                            // Unicode escape sequence
                            if let Some('{') = chars.next() {
                                let unicode_hex =
                                    chars.by_ref().take_while(|c| *c != '}').collect::<String>();
                                if let Ok(code_point) = u32::from_str_radix(&unicode_hex, 16) {
                                    if std::char::from_u32(code_point).is_none() {
                                        return Err(t!(
                                                "Invalid unicode escape sequence '\\u{{%{unicode_hex}}}'",
                                                unicode_hex = unicode_hex
                                            ).to_string(),
                                        );
                                    }
                                }
                            }
                        }
                        '0'..='9' => {
                            // Decimal escape sequence
                            for _ in 0..2 {
                                if let Some(digit) = chars.peek() {
                                    if !digit.is_digit(10) {
                                        break;
                                    }
                                    chars.next();
                                }
                            }
                        }
                        'z' => {
                            // Skip whitespace
                            while let Some(c) = chars.peek() {
                                if !c.is_whitespace() {
                                    break;
                                }
                                chars.next();
                            }
                        }
                        _ => {
                            // donot check other escape sequence
                        }
                    }
                }
            }
            _ => {
                if c == delimiter {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn check_dots_literal_error(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    dots_token: &LuaSyntaxToken,
) -> Option<()> {
    if let Some(literal_expr) = dots_token.parent() {
        match literal_expr.kind().into() {
            LuaSyntaxKind::LiteralExpr => {
                let literal_expr = LuaLiteralExpr::cast(literal_expr)?;
                let closure_expr = literal_expr.ancestors::<LuaClosureExpr>().next()?;
                let signature_id =
                    LuaSignatureId::from_closure(semantic_model.get_file_id(), &closure_expr);
                let signature = context.db.get_signature_index().get(&signature_id)?;
                if !signature.params.iter().any(|param| param == "...") {
                    context.add_diagnostic(
                        DiagnosticCode::LuaSyntaxError,
                        literal_expr.get_range(),
                        t!("Cannot use `...` outside a vararg function.").to_string(),
                        None,
                    );
                }
            }
            _ => {}
        }
    }

    Some(())
}
