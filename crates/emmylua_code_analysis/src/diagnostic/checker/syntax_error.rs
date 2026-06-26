//! Syntax error checker — salsa-native.
//!
//! 检查语法错误 + 文档语法错误 + 整数/浮点/字符串字面量溢出 + `...` 误用。

use emmylua_parser::{
    LuaAstNode, LuaClosureExpr, LuaLiteralExpr, LuaParseErrorKind, LuaSyntaxKind, LuaSyntaxToken,
    LuaTokenKind, float_token_value, int_token_value,
};

use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaSignatureId};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    // 解析错误
    if let Some(parse_errors) = model.get_file_parse_error() {
        for err in parse_errors {
            let code = match err.kind {
                LuaParseErrorKind::SyntaxError => DiagnosticCode::SyntaxError,
                LuaParseErrorKind::DocError => DiagnosticCode::DocSyntaxError,
            };
            context.add_diagnostic(code, err.range, err.message, None);
        }
    }

    // 字面量溢出检查
    let root = model.get_root();
    for node_or_token in root.syntax().descendants_with_tokens() {
        let Some(token) = node_or_token.into_token() else {
            continue;
        };
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
                if let Err(err) = check_string(&token) {
                    context.add_diagnostic(
                        DiagnosticCode::SyntaxError,
                        token.text_range(),
                        err,
                        None,
                    );
                }
            }
            LuaTokenKind::TkDots => {
                check_dots(context, model, &token);
            }
            _ => {}
        }
    }
}

fn check_string(token: &LuaSyntaxToken) -> Result<(), String> {
    let text = token.text();
    if text.len() < 2 {
        return Ok(());
    }
    let mut chars = text.chars().peekable();
    let delimiter = chars.next().unwrap_or('\"');
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let Some(next) = chars.next() else { break };
                match next {
                    'x' => {
                        let hex: String = chars.by_ref().take(2).collect();
                        if hex.len() != 2
                            || !hex.chars().all(|c| c.is_ascii_hexdigit())
                            || u8::from_str_radix(&hex, 16).is_err()
                        {
                            return Err(t!("Invalid hex escape sequence '\\x%{hex}'", hex = hex)
                                .to_string());
                        }
                    }
                    'u' => {
                        if chars.next() == Some('{') {
                            let unicode: String =
                                chars.by_ref().take_while(|c| *c != '}').collect();
                            if u32::from_str_radix(&unicode, 16)
                                .map_or(true, |cp| std::char::from_u32(cp).is_none())
                            {
                                return Err(t!(
                                    "Invalid unicode escape sequence '\\u{{%{unicode}}}'",
                                    unicode = unicode
                                )
                                .to_string());
                            }
                        }
                    }
                    '0'..='9' => {
                        for _ in 0..2 {
                            if !chars.peek().is_some_and(|d| d.is_ascii_digit()) {
                                break;
                            }
                            chars.next();
                        }
                    }
                    'z' => {
                        while chars.peek().is_some_and(|c| c.is_whitespace()) {
                            chars.next();
                        }
                    }
                    _ => {}
                }
            }
            _ if c == delimiter => break,
            _ => {}
        }
    }
    Ok(())
}

fn check_dots(context: &mut DiagnosticContext, model: &SemanticModel, dots_token: &LuaSyntaxToken) {
    let Some(parent) = dots_token.parent() else {
        return;
    };
    if parent.kind() != LuaSyntaxKind::LiteralExpr.into() {
        return;
    }
    let Some(literal) = LuaLiteralExpr::cast(parent) else {
        return;
    };
    let Some(closure) = literal.ancestors::<LuaClosureExpr>().next() else {
        return;
    };
    let file_id = model.get_file_id();
    let sig_id = LuaSignatureId::from_closure(file_id, &closure);
    let is_vararg = model
        .get_signature(file_id, sig_id.get_position())
        .is_some_and(|s| s.is_variadic());
    if !is_vararg {
        context.add_diagnostic(
            DiagnosticCode::SyntaxError,
            literal.get_range(),
            t!("Cannot use `...` outside a vararg function.").to_string(),
            None,
        );
    }
}
