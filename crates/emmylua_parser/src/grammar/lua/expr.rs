use crate::{
    grammar::ParseResult,
    kind::{BinaryOperator, LuaOpKind, LuaSyntaxKind, LuaTokenKind, UnaryOperator, UNARY_PRIORITY},
    parser::{LuaParser, MarkerEventContainer},
    parser_error::LuaParseError,
    SpecialFunction,
};

use super::{expect_token, if_token_bump, parse_block};

pub fn parse_expr(p: &mut LuaParser) -> ParseResult {
    parse_sub_expr(p, 0)
}

fn parse_sub_expr(p: &mut LuaParser, limit: i32) -> ParseResult {
    let uop = LuaOpKind::to_unary_operator(p.current_token());
    let mut cm = if uop != UnaryOperator::OpNop {
        let m = p.mark(LuaSyntaxKind::UnaryExpr);
        let range = p.current_token_range();
        p.bump();
        match parse_sub_expr(p, UNARY_PRIORITY) {
            Ok(_) => {}
            Err(err) => {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("unary operator not followed by expression"),
                    range,
                ));
                return Err(err);
            }
        }
        m.complete(p)
    } else {
        parse_simple_expr(p)?
    };

    let mut bop = LuaOpKind::to_binary_operator(p.current_token());
    while bop != BinaryOperator::OpNop && bop.get_priority().left > limit {
        let range = p.current_token_range();
        let m = cm.precede(p, LuaSyntaxKind::BinaryExpr);
        p.bump();
        match parse_sub_expr(p, bop.get_priority().right) {
            Ok(_) => {}
            Err(err) => {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("binary operator not followed by expression"),
                    range,
                ));

                return Err(err);
            }
        }

        cm = m.complete(p);
        bop = LuaOpKind::to_binary_operator(p.current_token());
    }

    Ok(cm)
}

fn parse_simple_expr(p: &mut LuaParser) -> ParseResult {
    match p.current_token() {
        LuaTokenKind::TkInt
        | LuaTokenKind::TkFloat
        | LuaTokenKind::TkComplex
        | LuaTokenKind::TkNil
        | LuaTokenKind::TkTrue
        | LuaTokenKind::TkFalse
        | LuaTokenKind::TkDots
        | LuaTokenKind::TkString
        | LuaTokenKind::TkLongString => {
            let m = p.mark(LuaSyntaxKind::LiteralExpr);
            p.bump();
            Ok(m.complete(p))
        }
        LuaTokenKind::TkLeftBrace => parse_table_expr(p),
        LuaTokenKind::TkFunction => parse_closure_expr(p),
        _ => parse_suffixed_expr(p),
    }
}

pub fn parse_closure_expr(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::ClosureExpr);

    if_token_bump(p, LuaTokenKind::TkFunction);
    parse_param_list(p)?;

    if p.current_token() != LuaTokenKind::TkEnd {
        parse_block(p)?;
    }

    expect_token(p, LuaTokenKind::TkEnd)?;
    Ok(m.complete(p))
}

fn parse_param_list(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::ParamList);

    expect_token(p, LuaTokenKind::TkLeftParen)?;
    if p.current_token() != LuaTokenKind::TkRightParen {
        parse_param_name(p)?;
        while p.current_token() == LuaTokenKind::TkComma {
            p.bump();
            parse_param_name(p)?;
        }
    }

    expect_token(p, LuaTokenKind::TkRightParen)?;
    Ok(m.complete(p))
}

fn parse_param_name(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::ParamName);

    if p.current_token() == LuaTokenKind::TkName || p.current_token() == LuaTokenKind::TkDots {
        p.bump();
    } else {
        return Err(LuaParseError::syntax_error_from(
            &t!("expect parameter name"),
            p.current_token_range(),
        ));
    }

    Ok(m.complete(p))
}

fn parse_table_expr(p: &mut LuaParser) -> ParseResult {
    let mut m = p.mark(LuaSyntaxKind::TableEmptyExpr);
    p.bump();

    if p.current_token() == LuaTokenKind::TkRightBrace {
        p.bump();
        return Ok(m.complete(p));
    }

    match parse_field_with_recovery(p) {
        Ok(cm) => match cm.kind {
            LuaSyntaxKind::TableFieldAssign => {
                m.set_kind(p, LuaSyntaxKind::TableObjectExpr);
            }
            LuaSyntaxKind::TableFieldValue => {
                m.set_kind(p, LuaSyntaxKind::TableArrayExpr);
            }
            _ => {}
        },
        Err(_) => {
            //  即使字段解析失败, 我们也不中止解析
            recover_to_table_boundary(p);
        }
    }

    while p.current_token() == LuaTokenKind::TkComma
        || p.current_token() == LuaTokenKind::TkSemicolon
    {
        p.bump();
        if p.current_token() == LuaTokenKind::TkRightBrace {
            break;
        }

        match parse_field_with_recovery(p) {
            Ok(cm) => {
                if cm.kind == LuaSyntaxKind::TableFieldAssign {
                    m.set_kind(p, LuaSyntaxKind::TableObjectExpr);
                }
            }
            Err(_) => {
                // 即使字段解析失败, 我们也不中止解析
                recover_to_table_boundary(p);
                if p.current_token() == LuaTokenKind::TkRightBrace {
                    break;
                }
            }
        }
    }

    // 处理闭合括号
    if p.current_token() == LuaTokenKind::TkRightBrace {
        p.bump();
    } else {
        // 表可能是错的, 但可以继续尝试解析
        let mut found_brace = false;
        let mut brace_count = 1; // 我们已经在表中
        let mut lookahead_count = 0;
        const MAX_LOOKAHEAD: usize = 50; // 限制令牌数避免无休止的解析

        while p.current_token() != LuaTokenKind::TkEof && lookahead_count < MAX_LOOKAHEAD {
            match p.current_token() {
                LuaTokenKind::TkRightBrace => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        p.bump(); // 消费闭合括号
                        found_brace = true;
                        break;
                    }
                    p.bump();
                }
                LuaTokenKind::TkLeftBrace => {
                    brace_count += 1;
                    p.bump();
                }
                // 如果遇到则认为已经是表的边界
                LuaTokenKind::TkLocal
                | LuaTokenKind::TkFunction
                | LuaTokenKind::TkIf
                | LuaTokenKind::TkWhile
                | LuaTokenKind::TkFor
                | LuaTokenKind::TkReturn => {
                    break;
                }
                _ => {
                    p.bump();
                }
            }
            lookahead_count += 1;
        }

        if !found_brace {
            // 没有找到闭合括号, 报告错误
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected '}' to close table"),
                p.current_token_range(),
            ));
        }
    }

    Ok(m.complete(p))
}

fn parse_field_with_recovery(p: &mut LuaParser) -> ParseResult {
    let mut m = p.mark(LuaSyntaxKind::TableFieldValue);
    // 即使字段解析失败, 我们也不会中止解析
    match p.current_token() {
        LuaTokenKind::TkLeftBracket => {
            m.set_kind(p, LuaSyntaxKind::TableFieldAssign);
            p.bump();
            match parse_expr(p) {
                Ok(_) => {}
                Err(err) => {
                    p.push_error(err);
                    // 找到边界
                    while !matches!(
                        p.current_token(),
                        LuaTokenKind::TkRightBracket
                            | LuaTokenKind::TkAssign
                            | LuaTokenKind::TkComma
                            | LuaTokenKind::TkSemicolon
                            | LuaTokenKind::TkRightBrace
                            | LuaTokenKind::TkEof
                    ) {
                        p.bump();
                    }
                }
            }
            if p.current_token() == LuaTokenKind::TkRightBracket {
                p.bump();
            } else {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("expected ']'"),
                    p.current_token_range(),
                ));
            }
            if p.current_token() == LuaTokenKind::TkAssign {
                p.bump();
            } else {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("expected '='"),
                    p.current_token_range(),
                ));
            }
            match parse_expr(p) {
                Ok(_) => {}
                Err(err) => {
                    p.push_error(err);
                }
            }
        }
        LuaTokenKind::TkName => {
            if p.peek_next_token() == LuaTokenKind::TkAssign {
                m.set_kind(p, LuaSyntaxKind::TableFieldAssign);
                p.bump(); // consume name
                p.bump(); // consume '='
                match parse_expr(p) {
                    Ok(_) => {}
                    Err(err) => {
                        p.push_error(err);
                    }
                }
            } else {
                match parse_expr(p) {
                    Ok(_) => {}
                    Err(err) => {
                        p.push_error(err);
                    }
                }
            }
        }
        // 一些表示`table`实际上已经结束的令牌
        LuaTokenKind::TkEof | LuaTokenKind::TkLocal => {}
        _ => match parse_expr(p) {
            Ok(_) => {}
            Err(err) => {
                p.push_error(err);
            }
        },
    }

    Ok(m.complete(p))
}

fn recover_to_table_boundary(p: &mut LuaParser) {
    // 跳过直到找到表边界或字段分隔符
    while !matches!(
        p.current_token(),
        LuaTokenKind::TkComma
            | LuaTokenKind::TkSemicolon
            | LuaTokenKind::TkRightBrace
            | LuaTokenKind::TkEof
    ) {
        p.bump();
    }
}

fn parse_suffixed_expr(p: &mut LuaParser) -> ParseResult {
    let mut cm = match p.current_token() {
        LuaTokenKind::TkName => parse_name_or_special_function(p)?,
        LuaTokenKind::TkLeftParen => {
            let m = p.mark(LuaSyntaxKind::ParenExpr);
            p.bump();
            parse_expr(p)?;
            expect_token(p, LuaTokenKind::TkRightParen)?;
            m.complete(p)
        }
        _ => {
            return Err(LuaParseError::syntax_error_from(
                &t!("expect primary expression"),
                p.current_token_range(),
            ))
        }
    };

    loop {
        match p.current_token() {
            LuaTokenKind::TkDot | LuaTokenKind::TkColon | LuaTokenKind::TkLeftBracket => {
                let m = cm.precede(p, LuaSyntaxKind::IndexExpr);
                parse_index_struct(p)?;
                cm = m.complete(p);
            }
            LuaTokenKind::TkLeftParen
            | LuaTokenKind::TkLongString
            | LuaTokenKind::TkString
            | LuaTokenKind::TkLeftBrace => {
                let m = cm.precede(p, LuaSyntaxKind::CallExpr);
                parse_args(p)?;
                cm = m.complete(p);
            }
            _ => {
                return Ok(cm);
            }
        }
    }
}

fn parse_name_or_special_function(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::NameExpr);
    let special_kind = match p.parse_config.get_special_function(p.current_token_text()) {
        SpecialFunction::Require => LuaSyntaxKind::RequireCallExpr,
        SpecialFunction::Assert => LuaSyntaxKind::AssertCallExpr,
        SpecialFunction::Error => LuaSyntaxKind::ErrorCallExpr,
        SpecialFunction::Type => LuaSyntaxKind::TypeCallExpr,
        SpecialFunction::Setmatable => LuaSyntaxKind::SetmetatableCallExpr,
        _ => LuaSyntaxKind::None,
    };
    p.bump();
    let mut cm = m.complete(p);
    if special_kind == LuaSyntaxKind::None {
        return Ok(cm);
    }

    if matches!(
        p.current_token(),
        LuaTokenKind::TkLeftParen
            | LuaTokenKind::TkLongString
            | LuaTokenKind::TkString
            | LuaTokenKind::TkLeftBrace
    ) {
        let m1 = cm.precede(p, special_kind);
        parse_args(p)?;
        cm = m1.complete(p);
    }

    Ok(cm)
}

fn parse_index_struct(p: &mut LuaParser) -> Result<(), LuaParseError> {
    match p.current_token() {
        LuaTokenKind::TkLeftBracket => {
            p.bump();
            parse_expr(p)?;
            expect_token(p, LuaTokenKind::TkRightBracket)?;
        }
        LuaTokenKind::TkDot => {
            p.bump();
            expect_token(p, LuaTokenKind::TkName)?;
        }
        LuaTokenKind::TkColon => {
            p.bump();
            expect_token(p, LuaTokenKind::TkName)?;
            if !matches!(
                p.current_token(),
                LuaTokenKind::TkLeftParen
                    | LuaTokenKind::TkLeftBrace
                    | LuaTokenKind::TkString
                    | LuaTokenKind::TkLongString
            ) {
                return Err(LuaParseError::syntax_error_from(
                    &t!("colon accessor must be followed by a function call or table constructor or string literal"),
                    p.current_token_range(),
                ));
            }
        }
        _ => {
            return Err(LuaParseError::syntax_error_from(
                &t!("expect index struct"),
                p.current_token_range(),
            ));
        }
    }

    Ok(())
}

fn parse_args(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::CallArgList);
    match p.current_token() {
        LuaTokenKind::TkLeftParen => {
            p.bump();
            if p.current_token() != LuaTokenKind::TkRightParen {
                parse_expr(p)?;
                while p.current_token() == LuaTokenKind::TkComma {
                    p.bump();
                    if p.current_token() == LuaTokenKind::TkRightParen {
                        p.push_error(LuaParseError::syntax_error_from(
                            &t!("expect expression"),
                            p.current_token_range(),
                        ));
                        break;
                    }
                    parse_expr(p)?;
                }
            }
            expect_token(p, LuaTokenKind::TkRightParen)?;
        }
        LuaTokenKind::TkLeftBrace => {
            parse_table_expr(p)?;
        }
        LuaTokenKind::TkString | LuaTokenKind::TkLongString => {
            let m1 = p.mark(LuaSyntaxKind::LiteralExpr);
            p.bump();
            m1.complete(p);
        }
        _ => {
            return Err(LuaParseError::syntax_error_from(
                &t!("expect args"),
                p.current_token_range(),
            ));
        }
    }

    Ok(m.complete(p))
}
