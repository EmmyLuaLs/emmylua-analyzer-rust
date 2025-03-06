use crate::{
    grammar::ParseResult,
    kind::{LuaOpKind, LuaSyntaxKind, LuaTokenKind, LuaTypeBinaryOperator, LuaTypeUnaryOperator},
    lexer::LuaDocLexerState,
    parser::{CompleteMarker, LuaDocParser, MarkerEventContainer},
    parser_error::LuaParseError,
};

use super::{expect_token, if_token_bump, parse_description};

pub fn parse_type(p: &mut LuaDocParser) -> ParseResult {
    if p.current_token() == LuaTokenKind::TkDocContinueOr {
        return parse_multi_line_union_type(p);
    }

    let cm = parse_sub_type(p, 0)?;

    // <type>?
    if p.current_token() == LuaTokenKind::TkDocQuestion {
        let m = cm.precede(p, LuaSyntaxKind::TypeNullable);
        p.bump();
        Ok(m.complete(p))
    }
    // <type> and <true type> or <false type>
    else if p.current_token() == LuaTokenKind::TkAnd {
        let m = cm.precede(p, LuaSyntaxKind::TypeConditional);
        p.bump();
        parse_sub_type(p, 0)?;
        expect_token(p, LuaTokenKind::TkOr)?;
        parse_sub_type(p, 0)?;
        Ok(m.complete(p))
    } else {
        Ok(cm)
    }
}

// <type>
// keyof <type>, -1
// <type> | <type> , <type> & <type>, <type> extends <type>, <type> in keyof <type>
fn parse_sub_type(p: &mut LuaDocParser, limit: i32) -> ParseResult {
    let uop = LuaOpKind::to_type_unary_operator(p.current_token());
    let mut cm = if uop != LuaTypeUnaryOperator::None {
        let range = p.current_token_range();
        let m = p.mark(LuaSyntaxKind::TypeUnary);
        p.bump();
        match parse_sub_type(p, 0) {
            Ok(_) => {}
            Err(err) => {
                p.push_error(LuaParseError::from_source_range(
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

    let mut bop = LuaOpKind::to_parse_binary_operator(p.current_token());
    while bop != LuaTypeBinaryOperator::None && bop.get_priority().left > limit {
        let range = p.current_token_range();
        let m = cm.precede(p, LuaSyntaxKind::TypeBinary);
        p.bump();
        if p.current_token() != LuaTokenKind::TkDocQuestion {
            match parse_sub_type(p, bop.get_priority().right) {
                Ok(_) => {}
                Err(err) => {
                    p.push_error(LuaParseError::from_source_range(
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

        cm = m.complete(p);
        bop = LuaOpKind::to_parse_binary_operator(p.current_token());
    }

    Ok(cm)
}

pub fn parse_type_list(p: &mut LuaDocParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::DocTypeList);
    parse_type(p)?;
    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        parse_type(p)?;
    }
    Ok(m.complete(p))
}

fn parse_simple_type(p: &mut LuaDocParser) -> ParseResult {
    let cm = parse_primary_type(p)?;

    parse_suffixed_type(p, cm)
}

fn parse_primary_type(p: &mut LuaDocParser) -> ParseResult {
    match p.current_token() {
        LuaTokenKind::TkLeftBrace => parse_object_or_mapped_type(p),
        LuaTokenKind::TkLeftBracket => parse_tuple_type(p),
        LuaTokenKind::TkLeftParen => parse_paren_type(p),
        LuaTokenKind::TkString | LuaTokenKind::TkInt | LuaTokenKind::TkDocBoolean => {
            parse_literal_type(p)
        }
        LuaTokenKind::TkName => parse_name_or_func_type(p),
        LuaTokenKind::TkStringTemplateType => parse_string_template_type(p),
        LuaTokenKind::TkDots => parse_vararg_type(p),
        _ => Err(LuaParseError::from_source_range(
            &t!("expect type"),
            p.current_token_range(),
        )),
    }
}

// { <name>: <type>, ... }
// { <name> : <type>, ... }
fn parse_object_or_mapped_type(p: &mut LuaDocParser) -> ParseResult {
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
fn parse_typed_field(p: &mut LuaDocParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::DocObjectField);
    match p.current_token() {
        LuaTokenKind::TkName => {
            p.bump();
            if_token_bump(p, LuaTokenKind::TkDocQuestion);
        }
        LuaTokenKind::TkLeftBracket => {
            p.bump();
            if p.current_token() == LuaTokenKind::TkInt
                || p.current_token() == LuaTokenKind::TkString
            {
                p.bump();
            } else {
                parse_type(p)?;
            }
            expect_token(p, LuaTokenKind::TkRightBracket)?;
            if_token_bump(p, LuaTokenKind::TkDocQuestion);
        }
        _ => {
            return Err(LuaParseError::from_source_range(
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
fn parse_tuple_type(p: &mut LuaDocParser) -> ParseResult {
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
fn parse_paren_type(p: &mut LuaDocParser) -> ParseResult {
    p.bump();
    let cm = parse_type(p)?;
    expect_token(p, LuaTokenKind::TkRightParen)?;
    Ok(cm)
}

// <string> | <integer> | <bool>
fn parse_literal_type(p: &mut LuaDocParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::TypeLiteral);
    p.bump();
    Ok(m.complete(p))
}

fn parse_name_or_func_type(p: &mut LuaDocParser) -> ParseResult {
    let text = p.current_token_text();
    match text {
        "fun" | "async" => parse_fun_type(p),
        _ => parse_name_type(p),
    }
}

// fun ( <name>: <type>, ... ): <type>, ...
// async fun ( <name>: <type>, ... ) <type>, ...
pub fn parse_fun_type(p: &mut LuaDocParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::TypeFun);
    if p.current_token_text() == "async" {
        p.bump();
    }

    if p.current_token_text() != "fun" {
        return Err(LuaParseError::from_source_range(
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
        if p.current_token() == LuaTokenKind::TkLeftParen {
            p.bump();
            parse_type_list(p)?;
            expect_token(p, LuaTokenKind::TkRightParen)?;
        } else {
            parse_type_list(p)?;
        };
    }

    Ok(m.complete(p))
}

// <name> : <type>
// ... : <type>
// <name>
// ...
fn parse_typed_param(p: &mut LuaDocParser) -> ParseResult {
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
            return Err(LuaParseError::from_source_range(
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
fn parse_name_type(p: &mut LuaDocParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::TypeName);
    p.bump();
    Ok(m.complete(p))
}

// `<name type>`
fn parse_string_template_type(p: &mut LuaDocParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::TypeStringTemplate);
    p.bump();
    Ok(m.complete(p))
}

// just compact luals, trivia type
// ...<name type>
fn parse_vararg_type(p: &mut LuaDocParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::TypeName);
    p.bump();
    parse_name_type(p)?;
    Ok(m.complete(p))
}

// <type>[]
// <name type> < <type_list> >
// <name type> ...
// <prefix name type>`T`
fn parse_suffixed_type(p: &mut LuaDocParser, cm: CompleteMarker) -> ParseResult {
    let mut only_continue_array = false;
    let mut cm = cm;
    loop {
        match p.current_token() {
            LuaTokenKind::TkLeftBracket => {
                let mut m = cm.precede(p, LuaSyntaxKind::TypeArray);
                p.bump();
                if matches!(
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
            LuaTokenKind::TkStringTemplateType => {
                if only_continue_array {
                    return Ok(cm);
                }
                if cm.kind != LuaSyntaxKind::TypeName {
                    return Ok(cm);
                }

                let m = cm.precede(p, LuaSyntaxKind::TypeStringTemplate);
                p.bump();
                cm = m.complete(p);
                return Ok(cm);
            }
            _ => return Ok(cm),
        }
    }
}

fn parse_multi_line_union_type(p: &mut LuaDocParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::TypeMultiLineUnion);

    while p.current_token() == LuaTokenKind::TkDocContinueOr {
        p.bump();
        parse_one_line_type(p)?;
    }

    Ok(m.complete(p))
}

fn parse_one_line_type(p: &mut LuaDocParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::DocOneLineField);

    parse_simple_type(p)?;
    if p.current_token() != LuaTokenKind::TkDocContinueOr {
        p.set_state(LuaDocLexerState::Description);
        parse_description(p);
        p.set_state(LuaDocLexerState::Normal);
    }

    Ok(m.complete(p))
}
