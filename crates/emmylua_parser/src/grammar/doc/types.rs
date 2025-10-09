use crate::{
    UNARY_TYPE_PRIORITY,
    grammar::DocParseResult,
    kind::{LuaOpKind, LuaSyntaxKind, LuaTokenKind, LuaTypeBinaryOperator, LuaTypeUnaryOperator},
    lexer::LuaDocLexerState,
    parser::{CompleteMarker, LuaDocParser, LuaDocParserState, MarkerEventContainer},
    parser_error::LuaParseError,
};

use super::{expect_token, if_token_bump, parse_description};

pub fn parse_type(p: &mut LuaDocParser) -> DocParseResult {
    if p.current_token() == LuaTokenKind::TkDocContinueOr {
        return parse_multi_line_union_type(p);
    }

    let mut cm = parse_sub_type(p, 0)?;

    loop {
        match p.current_token() {
            // <type>?
            LuaTokenKind::TkDocQuestion => {
                let m = cm.precede(p, LuaSyntaxKind::TypeNullable);
                p.bump();
                cm = m.complete(p);
            }
            // <type> and <true type> or <false type>
            LuaTokenKind::TkAnd => {
                let m = cm.precede(p, LuaSyntaxKind::TypeConditional);
                p.bump();
                parse_type(p)?;
                expect_token(p, LuaTokenKind::TkOr)?;
                parse_type(p)?;
                cm = m.complete(p);
                break;
            }
            LuaTokenKind::TkDots => {
                // donot support  'xxx... ...'
                if matches!(cm.kind, LuaSyntaxKind::TypeVariadic) {
                    break;
                }

                let m = cm.precede(p, LuaSyntaxKind::TypeVariadic);
                p.bump();
                cm = m.complete(p);
                break;
            }
            _ => break,
        }
    }

    Ok(cm)
}

// <type>
// keyof <type>, -1
// <type> | <type> , <type> & <type>, <type> extends <type>, <type> in keyof <type>
fn parse_sub_type(p: &mut LuaDocParser, limit: i32) -> DocParseResult {
    let uop = LuaOpKind::to_type_unary_operator(p.current_token());
    let mut cm = if uop != LuaTypeUnaryOperator::None {
        let range = p.current_token_range();
        let m = p.mark(LuaSyntaxKind::TypeUnary);
        p.bump();
        match parse_sub_type(p, UNARY_TYPE_PRIORITY) {
            Ok(_) => {}
            Err(err) => {
                p.push_error(LuaParseError::doc_error_from(
                    &t!("unary operator not followed by type"),
                    range,
                ));
                return Err(err);
            }
        }
        m.complete(p)
    } else {
        parse_simple_type(p)?
    };
    parse_binary_operator(p, &mut cm, limit)?;

    Ok(cm)
}

pub fn parse_binary_operator(
    p: &mut LuaDocParser,
    cm: &mut CompleteMarker,
    limit: i32,
) -> Result<(), LuaParseError> {
    let mut bop = LuaOpKind::to_parse_binary_operator(p.current_token());
    while bop != LuaTypeBinaryOperator::None && bop.get_priority().left > limit {
        let range = p.current_token_range();
        let m = cm.precede(p, LuaSyntaxKind::TypeBinary);
        p.bump();
        if p.current_token() != LuaTokenKind::TkDocQuestion {
            // infer 只有在条件类型中才能被解析为关键词
            let parse_result = if bop == LuaTypeBinaryOperator::Extends {
                p.infer_depth += 1;
                let res = parse_sub_type(p, bop.get_priority().right);
                p.infer_depth = p.infer_depth.saturating_sub(1);
                res
            } else {
                parse_sub_type(p, bop.get_priority().right)
            };
            match parse_result {
                Ok(_) => {}
                Err(err) => {
                    p.push_error(LuaParseError::doc_error_from(
                        &t!("binary operator not followed by type"),
                        range,
                    ));

                    return Err(err);
                }
            }
        } else {
            let m2 = p.mark(LuaSyntaxKind::TypeLiteral);
            p.bump();
            m2.complete(p);
        }

        *cm = m.complete(p);
        bop = LuaOpKind::to_parse_binary_operator(p.current_token());
    }

    Ok(())
}

pub fn parse_type_list(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocTypeList);
    parse_type(p)?;
    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        parse_type(p)?;
    }
    Ok(m.complete(p))
}

fn parse_simple_type(p: &mut LuaDocParser) -> DocParseResult {
    let cm = parse_primary_type(p)?;

    parse_suffixed_type(p, cm)
}

fn parse_primary_type(p: &mut LuaDocParser) -> DocParseResult {
    match p.current_token() {
        LuaTokenKind::TkLeftBrace => {
            if is_mapped_type_start(p) {
                parse_mapped_type(p)
            } else {
                parse_object_type(p)
            }
        }
        LuaTokenKind::TkLeftBracket => parse_tuple_type(p),
        LuaTokenKind::TkLeftParen => parse_paren_type(p),
        LuaTokenKind::TkString
        | LuaTokenKind::TkInt
        | LuaTokenKind::TkTrue
        | LuaTokenKind::TkFalse => parse_literal_type(p),
        LuaTokenKind::TkName => {
            if p.is_infer_context() && p.current_token_text() == "infer" {
                parse_infer_type(p)
            } else {
                parse_name_or_func_type(p)
            }
        }
        LuaTokenKind::TkStringTemplateType => parse_string_template_type(p),
        LuaTokenKind::TkDots => parse_vararg_type(p),
        _ => Err(LuaParseError::doc_error_from(
            &t!("expect type"),
            p.current_token_range(),
        )),
    }
}

/// 判断是否是映射类型.
///
/// 这里与`TS`保持一致, 即第一个 key 必须为`[]` 且内部必须包含 `in`.
fn is_mapped_type_start(p: &LuaDocParser) -> bool {
    let text = p.origin_text();
    let start = p.current_token_range().end_offset();
    let rest = match text.get(start..) {
        Some(value) => value,
        None => return false,
    };

    let mut trimmed = rest.trim_start_matches(char::is_whitespace);

    if let Some(after_dashes) = trimmed.strip_prefix("---") {
        trimmed = after_dashes.trim_start_matches(char::is_whitespace);
    }

    if !trimmed.starts_with('[') {
        return false;
    }

    let after_left_bracket = &trimmed[1..];
    let rb_pos = match after_left_bracket.find(']') {
        Some(pos) => pos,
        None => return false,
    };

    let content = &after_left_bracket[..rb_pos];
    let mut seen_identifier = false;
    let mut token_start: Option<usize> = None;

    for (idx, ch) in content.char_indices() {
        if ch == '_' || ch.is_alphanumeric() {
            if token_start.is_none() {
                token_start = Some(idx);
            }
            continue;
        }

        if let Some(start_idx) = token_start.take() {
            let token = &content[start_idx..idx];
            if token == "in" {
                return seen_identifier;
            }
            seen_identifier = true;
        }
    }

    if let Some(start_idx) = token_start {
        if &content[start_idx..] == "in" {
            return seen_identifier;
        }
    }

    false
}

// [Property in Type]: Type;
// [Property in keyof Type]: Type;
fn parse_mapped_type(p: &mut LuaDocParser) -> DocParseResult {
    p.set_parser_state(LuaDocParserState::Mapped);
    let m = p.mark(LuaSyntaxKind::TypeMapped);
    p.bump();

    if p.current_token() != LuaTokenKind::TkLeftBracket {
        return Err(LuaParseError::doc_error_from(
            &t!("expect mapped field"),
            p.current_token_range(),
        ));
    }
    // key
    parse_mapped_key(p)?;
    if p.current_token() == LuaTokenKind::TkDocQuestion {
        p.bump();
    }
    expect_token(p, LuaTokenKind::TkColon)?;

    // value
    parse_type(p)?;

    // end
    expect_token(p, LuaTokenKind::TkSemicolon)?;
    expect_token(p, LuaTokenKind::TkRightBrace)?;

    p.set_parser_state(LuaDocParserState::Normal);
    Ok(m.complete(p))
}

// [Property in Type]
// [Property in keyof Type]
fn parse_mapped_key(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocMappedKey);
    expect_token(p, LuaTokenKind::TkLeftBracket)?;

    let param = p.mark(LuaSyntaxKind::DocGenericParameter);
    expect_token(p, LuaTokenKind::TkName)?;
    expect_token(p, LuaTokenKind::TkIn)?;
    parse_type(p)?;
    param.complete(p);

    if p.current_token() == LuaTokenKind::TkDocAs {
        p.bump();
        parse_type(p)?;
    }
    expect_token(p, LuaTokenKind::TkRightBracket)?;
    Ok(m.complete(p))
}

// { <name>: <type>, ... }
// { <name> : <type>, ... }
fn parse_object_type(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::TypeObject);
    p.bump();

    if p.current_token() != LuaTokenKind::TkRightBrace {
        parse_typed_field(p)?;
        while p.current_token() == LuaTokenKind::TkComma {
            p.bump();
            if p.current_token() == LuaTokenKind::TkRightBrace {
                break;
            }
            parse_typed_field(p)?;
        }
    }

    expect_token(p, LuaTokenKind::TkRightBrace)?;

    Ok(m.complete(p))
}

// <name> : <type>
// [<number>] : <type>
// [<string>] : <type>
// [<type>] : <type>
// <name>? : <type>
fn parse_typed_field(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocObjectField);
    match p.current_token() {
        LuaTokenKind::TkName => {
            p.bump();
            if_token_bump(p, LuaTokenKind::TkDocQuestion);
        }
        LuaTokenKind::TkLeftBracket => {
            p.bump();

            parse_type(p)?;

            expect_token(p, LuaTokenKind::TkRightBracket)?;
            if_token_bump(p, LuaTokenKind::TkDocQuestion);
        }
        _ => {
            return Err(LuaParseError::doc_error_from(
                &t!("expect name or [<number>] or [<string>]"),
                p.current_token_range(),
            ));
        }
    }

    if p.current_token() == LuaTokenKind::TkColon {
        p.bump();
        parse_type(p)?;
    }
    Ok(m.complete(p))
}

// [ <type> , <type>  ...]
// [ string, number ]
fn parse_tuple_type(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::TypeTuple);
    p.bump();
    if p.current_token() != LuaTokenKind::TkRightBracket {
        parse_type(p)?;
        while p.current_token() == LuaTokenKind::TkComma {
            p.bump();
            parse_type(p)?;
        }
    }

    expect_token(p, LuaTokenKind::TkRightBracket)?;
    Ok(m.complete(p))
}

// ( <type> )
fn parse_paren_type(p: &mut LuaDocParser) -> DocParseResult {
    p.bump();
    let cm = parse_type(p)?;
    expect_token(p, LuaTokenKind::TkRightParen)?;
    Ok(cm)
}

// <string> | <integer> | <bool>
fn parse_literal_type(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::TypeLiteral);
    p.bump();
    Ok(m.complete(p))
}

fn parse_name_or_func_type(p: &mut LuaDocParser) -> DocParseResult {
    let text = p.current_token_text();
    match text {
        "fun" | "async" | "sync" => parse_fun_type(p),
        _ => parse_name_type(p),
    }
}

// fun ( <name>: <type>, ... ): <type>, ...
// async fun ( <name>: <type>, ... ) <type>, ...
pub fn parse_fun_type(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::TypeFun);
    if matches!(p.current_token_text(), "async" | "sync") {
        p.bump();
    }

    if p.current_token_text() != "fun" {
        return Err(LuaParseError::doc_error_from(
            &t!("expect fun"),
            p.current_token_range(),
        ));
    }

    p.bump();
    expect_token(p, LuaTokenKind::TkLeftParen)?;

    if p.current_token() != LuaTokenKind::TkRightParen {
        parse_typed_param(p)?;
        while p.current_token() == LuaTokenKind::TkComma {
            p.bump();
            parse_typed_param(p)?;
        }
    }

    expect_token(p, LuaTokenKind::TkRightParen)?;

    if p.current_token() == LuaTokenKind::TkColon {
        p.bump();

        // compact luals return type (number, integer)
        parse_fun_return_list(p)?;
    }

    Ok(m.complete(p))
}

fn parse_fun_return_list(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocTypeList);
    // compact luals return type (number, integer)
    let parse_paren = if p.current_token() == LuaTokenKind::TkLeftParen {
        p.bump();
        true
    } else {
        false
    };

    parse_fun_return_type(p)?;

    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        parse_fun_return_type(p)?;
    }

    if parse_paren {
        expect_token(p, LuaTokenKind::TkRightParen)?;
    }

    Ok(m.complete(p))
}

fn parse_fun_return_type(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocNamedReturnType);
    let cm = parse_type(p)?;
    if cm.kind == LuaSyntaxKind::TypeName && p.current_token() == LuaTokenKind::TkColon {
        p.bump();
        parse_type(p)?;
    }
    Ok(m.complete(p))
}

// <name> : <type>
// ... : <type>
// <name>
// ...
fn parse_typed_param(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocTypedParameter);
    match p.current_token() {
        LuaTokenKind::TkName => {
            p.bump();
            if_token_bump(p, LuaTokenKind::TkDocQuestion);
        }
        LuaTokenKind::TkDots => {
            p.bump();
            if_token_bump(p, LuaTokenKind::TkDocQuestion);
        }
        _ => {
            return Err(LuaParseError::doc_error_from(
                &t!("expect name or ..."),
                p.current_token_range(),
            ));
        }
    }

    if p.current_token() == LuaTokenKind::TkColon {
        p.bump();
        parse_type(p)?;
    }

    Ok(m.complete(p))
}

// <name type>
fn parse_name_type(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::TypeName);
    p.bump();
    Ok(m.complete(p))
}

fn parse_infer_type(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::TypeInfer);
    p.bump();
    let param = p.mark(LuaSyntaxKind::DocGenericParameter);
    expect_token(p, LuaTokenKind::TkName)?;
    param.complete(p);
    Ok(m.complete(p))
}

// `<name type>`
fn parse_string_template_type(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::TypeStringTemplate);
    p.bump();
    Ok(m.complete(p))
}

// just compact luals, trivia type
// ...<name type>
fn parse_vararg_type(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::TypeName);
    p.bump();
    parse_name_type(p)?;
    Ok(m.complete(p))
}

// <type>[]
// <name type> < <type_list> >
// <name type> ...
// <prefix name type>`T`
fn parse_suffixed_type(p: &mut LuaDocParser, cm: CompleteMarker) -> DocParseResult {
    let mut only_continue_array = false;
    let mut cm = cm;
    loop {
        match p.current_token() {
            LuaTokenKind::TkLeftBracket => {
                let mut m = cm.precede(p, LuaSyntaxKind::TypeArray);
                p.bump();
                if p.state == LuaDocParserState::Mapped {
                    if p.current_token() != LuaTokenKind::TkRightBracket {
                        m.set_kind(p, LuaSyntaxKind::TypeIndexAccess);
                        parse_type(p)?;
                    }
                } else {
                    if matches!(
                        p.current_token(),
                        LuaTokenKind::TkString | LuaTokenKind::TkInt | LuaTokenKind::TkName
                    ) {
                        m.set_kind(p, LuaSyntaxKind::IndexExpr);
                        p.bump();
                    }
                }

                expect_token(p, LuaTokenKind::TkRightBracket)?;
                cm = m.complete(p);
                only_continue_array = true;
            }
            LuaTokenKind::TkLt => {
                if only_continue_array {
                    return Ok(cm);
                }
                if cm.kind != LuaSyntaxKind::TypeName {
                    return Ok(cm);
                }

                let m = cm.precede(p, LuaSyntaxKind::TypeGeneric);
                p.bump();
                parse_type_list(p)?;
                expect_token(p, LuaTokenKind::TkGt)?;
                cm = m.complete(p);
            }
            LuaTokenKind::TkDots => {
                if only_continue_array {
                    return Ok(cm);
                }
                if cm.kind != LuaSyntaxKind::TypeName {
                    return Ok(cm);
                }

                let m = cm.precede(p, LuaSyntaxKind::TypeVariadic);
                p.bump();
                cm = m.complete(p);
                return Ok(cm);
            }
            _ => return Ok(cm),
        }
    }
}

fn parse_multi_line_union_type(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::TypeMultiLineUnion);

    while p.current_token() == LuaTokenKind::TkDocContinueOr {
        p.bump();
        parse_one_line_type(p)?;
    }

    Ok(m.complete(p))
}

fn parse_one_line_type(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocOneLineField);

    parse_simple_type(p)?;
    if p.current_token() != LuaTokenKind::TkDocContinueOr {
        p.set_lexer_state(LuaDocLexerState::Description);
        parse_description(p);
        p.set_lexer_state(LuaDocLexerState::Normal);
    }

    Ok(m.complete(p))
}
