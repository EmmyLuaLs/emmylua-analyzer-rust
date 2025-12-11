use crate::{
    UNARY_TYPE_PRIORITY,
    grammar::DocParseResult,
    kind::{LuaOpKind, LuaSyntaxKind, LuaTokenKind, LuaTypeBinaryOperator, LuaTypeUnaryOperator},
    lexer::LuaDocLexerState,
    parser::{CompleteMarker, LuaDocParser, LuaDocParserState, Marker, MarkerEventContainer},
    parser_error::LuaParseError,
};

use super::tag::parse_generic_decl_list;
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

        if bop == LuaTypeBinaryOperator::Extends {
            let prev_lexer_state = p.lexer.state;
            p.set_lexer_state(LuaDocLexerState::Extends);
            p.bump();
            p.set_lexer_state(prev_lexer_state);
        } else {
            p.bump();
        }
        if p.current_token() != LuaTokenKind::TkDocQuestion {
            // infer 只有在条件类型中才能被解析为关键词
            let parse_result = if bop == LuaTypeBinaryOperator::Extends {
                let prev_state = p.state;
                p.set_parser_state(LuaDocParserState::Extends);
                let res = parse_sub_type(p, bop.get_priority().right);
                p.set_parser_state(prev_state);
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
        LuaTokenKind::TkLeftBrace => parse_object_or_mapped_type(p),
        LuaTokenKind::TkLeftBracket => parse_tuple_type(p),
        LuaTokenKind::TkLeftParen => parse_paren_type(p),
        LuaTokenKind::TkString
        | LuaTokenKind::TkInt
        | LuaTokenKind::TkTrue
        | LuaTokenKind::TkFalse => parse_literal_type(p),
        LuaTokenKind::TkName => {
            if p.state == LuaDocParserState::Extends && p.current_token_text() == "infer" {
                parse_infer_type(p)
            } else {
                parse_name_or_func_type(p)
            }
        }
        LuaTokenKind::TkStringTemplateType => parse_string_template_type(p),
        LuaTokenKind::TkDots => parse_vararg_type(p),
        LuaTokenKind::TkDocNew => parse_constructor_type(p),
        _ => Err(LuaParseError::doc_error_from(
            &t!("expect type"),
            p.current_token_range(),
        )),
    }
}

// [Property in Type]: Type;
// [Property in keyof Type]: Type;
fn parse_mapped_type(p: &mut LuaDocParser, m: Marker) -> DocParseResult {
    p.set_parser_state(LuaDocParserState::Mapped);

    match p.current_token() {
        LuaTokenKind::TkPlus | LuaTokenKind::TkMinus => {
            p.bump();
            expect_token(p, LuaTokenKind::TkDocReadonly)?;
        }
        LuaTokenKind::TkDocReadonly => {
            p.bump();
        }
        LuaTokenKind::TkLeftBracket => {}
        _ => {
            return Err(LuaParseError::doc_error_from(
                &t!("expect mapped field"),
                p.current_token_range(),
            ));
        }
    }

    parse_mapped_key(p)?;

    match p.current_token() {
        LuaTokenKind::TkPlus | LuaTokenKind::TkMinus => {
            p.bump();
            expect_token(p, LuaTokenKind::TkDocQuestion)?;
        }
        LuaTokenKind::TkDocQuestion => {
            p.bump();
        }
        _ => {}
    }

    expect_token(p, LuaTokenKind::TkColon)?;

    parse_type(p)?;

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
fn parse_object_or_mapped_type(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Mapped);
    let mut m = p.mark(LuaSyntaxKind::TypeObject);
    p.bump();
    p.set_lexer_state(LuaDocLexerState::Normal);

    if p.current_token() != LuaTokenKind::TkRightBrace {
        match p.current_token() {
            LuaTokenKind::TkPlus | LuaTokenKind::TkMinus | LuaTokenKind::TkDocReadonly => {
                m.set_kind(p, LuaSyntaxKind::TypeMapped);
                return parse_mapped_type(p, m);
            }
            LuaTokenKind::TkLeftBracket => {
                if is_mapped_type(p) {
                    m.set_kind(p, LuaSyntaxKind::TypeMapped);
                    return parse_mapped_type(p, m);
                }
            }
            _ => {}
        }

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

/// 判断是否为 mapped type
fn is_mapped_type(p: &LuaDocParser) -> bool {
    let mut lexer = p.lexer.clone();

    loop {
        let kind = lexer.lex();
        match kind {
            LuaTokenKind::TkIn => return true,
            LuaTokenKind::TkLeftBracket | LuaTokenKind::TkRightBracket => return false,
            LuaTokenKind::TkEof => return false,
            LuaTokenKind::TkWhitespace
            | LuaTokenKind::TkDocContinue
            | LuaTokenKind::TkEndOfLine => {}
            _ => {}
        }
    }
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
// fun <T>( <name>: <type>, ... ): <type>, ...
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

    if p.current_token() == LuaTokenKind::TkLt {
        parse_generic_decl_list(p, true)?;
    }

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
pub fn parse_typed_param(p: &mut LuaDocParser) -> DocParseResult {
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
    p.set_current_token_kind(LuaTokenKind::TkDocInfer);
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
                } else if matches!(
                    p.current_token(),
                    LuaTokenKind::TkString | LuaTokenKind::TkInt | LuaTokenKind::TkName
                ) {
                    m.set_kind(p, LuaSyntaxKind::IndexExpr);
                    p.bump();
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

    parse_sub_type(p, 1)?;
    if p.current_token() != LuaTokenKind::TkDocContinueOr {
        p.set_lexer_state(LuaDocLexerState::Description);
        parse_description(p);
        p.set_lexer_state(LuaDocLexerState::Normal);
    }

    Ok(m.complete(p))
}

fn parse_constructor_type(p: &mut LuaDocParser) -> DocParseResult {
    let new_range = p.current_token_range();
    expect_token(p, LuaTokenKind::TkDocNew)?;

    let cm = match parse_sub_type(p, 0) {
        Ok(cm) => {
            if cm.kind != LuaSyntaxKind::TypeFun {
                let err = LuaParseError::doc_error_from(
                    &t!("new keyword must be followed by function type"),
                    new_range,
                );
                p.push_error(err.clone());
                return Err(err);
            }
            cm
        }
        Err(err) => {
            return Err(err);
        }
    };
    Ok(cm)
}
