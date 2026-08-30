//! syntax_error: parser-stage errors + syntax errors that need self-analysis.
//!
//! Four parts:
//! 1. Parser-stage errors (already recorded on `LuaSyntaxTree`) -> SyntaxError / DocSyntaxError.
//! 2. Numeric literal validity (`int_token_value` / `float_token_value`).
//! 3. String escape validity (hex / unicode / decimal / `\z`).
//! 4. `...` (vararg) and `goto` usage validity.

use std::collections::HashMap;

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaClosureExpr, LuaGotoStat, LuaLabelStat, LuaLiteralExpr,
    LuaParseErrorKind, LuaSyntaxKind, LuaSyntaxToken, LuaTokenKind, float_token_value,
    int_token_value,
};
use rowan::TextRange;

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct SyntaxErrorChecker;

impl Checker for SyntaxErrorChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::SyntaxError, DiagnosticCode::DocSyntaxError];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        // 1. Parser-stage errors.
        if let Some(parse_errors) = semantic_model.parse_errors() {
            for parse_error in parse_errors {
                let code = match parse_error.kind {
                    LuaParseErrorKind::SyntaxError => DiagnosticCode::SyntaxError,
                    LuaParseErrorKind::DocError => DiagnosticCode::DocSyntaxError,
                };
                context.add_diagnostic(code, parse_error.range, parse_error.message);
            }
        }

        // 2-4. Token-level syntax errors requiring self-analysis.
        let Some(chunk) = semantic_model.chunk() else {
            return;
        };
        for node_or_token in chunk.syntax().descendants_with_tokens() {
            let Some(token) = node_or_token.into_token() else {
                continue;
            };
            match token.kind().into() {
                LuaTokenKind::TkInt => {
                    if let Err(err) = int_token_value(&token) {
                        context.add_diagnostic(DiagnosticCode::SyntaxError, err.range, err.message);
                    }
                }
                LuaTokenKind::TkFloat => {
                    if let Err(err) = float_token_value(&token) {
                        context.add_diagnostic(DiagnosticCode::SyntaxError, err.range, err.message);
                    }
                }
                LuaTokenKind::TkString => {
                    if let Err(message) = check_normal_string_error(&token) {
                        context.add_diagnostic(
                            DiagnosticCode::SyntaxError,
                            token.text_range(),
                            message,
                        );
                    }
                }
                LuaTokenKind::TkDots => {
                    check_dots_literal_error(context, semantic_model, &token);
                }
                _ => {}
            }
        }

        check_goto_labels(context, semantic_model);
    }
}

/// String escape validity (`\x`/`\u`/decimal/`\z`).
fn check_normal_string_error(string_token: &LuaSyntaxToken) -> Result<(), String> {
    let text = string_token.text();
    if text.len() < 2 {
        return Ok(());
    }

    let mut chars = text.chars().peekable();
    let delimiter = match chars.next() {
        Some(c) => c,
        None => return Ok(()),
    };

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next_char) = chars.next() {
                    match next_char {
                        'a' | 'b' | 'f' | 'n' | 'r' | 't' | 'v' | '\\' | '\'' | '\"' | '\r'
                        | '\n' => {}
                        'x' => {
                            let hex = chars.by_ref().take(2).collect::<String>();
                            if hex.len() == 2 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                                if u8::from_str_radix(&hex, 16).is_err() {
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
                            if let Some('{') = chars.next() {
                                let unicode_hex =
                                    chars.by_ref().take_while(|c| *c != '}').collect::<String>();
                                if let Ok(code_point) = u32::from_str_radix(&unicode_hex, 16)
                                    && std::char::from_u32(code_point).is_none()
                                {
                                    return Err(t!(
                                        "Invalid unicode escape sequence '\\u{{%{unicode_hex}}}'",
                                        unicode_hex = unicode_hex
                                    )
                                    .to_string());
                                }
                            }
                        }
                        '0'..='9' => {
                            for _ in 0..2 {
                                if let Some(digit) = chars.peek() {
                                    if !digit.is_ascii_digit() {
                                        break;
                                    }
                                    chars.next();
                                }
                            }
                        }
                        'z' => {
                            while let Some(c) = chars.peek() {
                                if !c.is_whitespace() {
                                    break;
                                }
                                chars.next();
                            }
                        }
                        _ => {}
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

/// `...` may only be used in a vararg function body (the innermost closure must declare a `...` parameter).
fn check_dots_literal_error(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    dots_token: &LuaSyntaxToken,
) {
    let Some(parent) = dots_token.parent() else {
        return;
    };
    if parent.kind() != LuaSyntaxKind::LiteralExpr.into() {
        return;
    }
    let Some(literal_expr) = LuaLiteralExpr::cast(parent) else {
        return;
    };
    let Some(closure_expr) = literal_expr.ancestors::<LuaClosureExpr>().next() else {
        // Top-level `...`: not checked in M0 (no enclosing function).
        return;
    };
    let closure_syntax = closure_expr.get_syntax_id();
    let is_vararg = semantic_model
        .signatures()
        .map(|sigs| {
            sigs.iter().any(|sig| {
                sig.closure_syntax == closure_syntax && sig.param_names.iter().any(|p| p == "...")
            })
        })
        .unwrap_or(false);
    if !is_vararg {
        context.add_diagnostic(
            DiagnosticCode::SyntaxError,
            literal_expr.get_range(),
            t!("Cannot use `...` outside a vararg function."),
        );
    }
}

/// `goto` target label must exist in the same function scope; at top level the chunk is the scope.
fn check_goto_labels(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
    let Some(chunk) = semantic_model.chunk() else {
        return;
    };
    // Label names per scope.
    let chunk_range = chunk.get_range();
    let mut labels: HashMap<TextRange, Vec<String>> = HashMap::new();
    for label in chunk.descendants::<LuaLabelStat>() {
        let Some(label_token) = label.get_label_name_token() else {
            continue;
        };
        let scope = enclosing_scope_range(&label, chunk_range);
        labels
            .entry(scope)
            .or_default()
            .push(label_token.get_name_text().to_string());
    }
    for goto in chunk.descendants::<LuaGotoStat>() {
        let Some(label_token) = goto.get_label_name_token() else {
            continue;
        };
        let name = label_token.get_name_text();
        let scope = enclosing_scope_range(&goto, chunk_range);
        if !labels
            .get(&scope)
            .is_some_and(|names| names.iter().any(|n| n == name))
        {
            context.add_diagnostic(
                DiagnosticCode::SyntaxError,
                label_token.get_range(),
                t!("goto label '%{name}' not found", name = name),
            );
        }
    }
}

/// Scope key: the innermost closure range, or the chunk range when there is no closure.
fn enclosing_scope_range<N: LuaAstNode>(node: &N, chunk_range: TextRange) -> TextRange {
    node.ancestors::<LuaClosureExpr>()
        .next()
        .map(|closure| closure.get_range())
        .unwrap_or(chunk_range)
}
