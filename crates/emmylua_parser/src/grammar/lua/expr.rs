use crate::{
    SpecialFunction,
    grammar::{ParseFailReason, ParseResult, lua::is_statement_start_token},
    kind::{BinaryOperator, LuaOpKind, LuaSyntaxKind, LuaTokenKind, UNARY_PRIORITY, UnaryOperator},
    parser::{LuaParser, MarkerEventContainer},
    parser_error::LuaParseError,
};

use super::{expect_token, if_token_bump, parse_block};

pub fn parse_expr(p: &mut LuaParser) -> ParseResult {
    parse_sub_expr(p, 0)
}

fn parse_sub_expr(p: &mut LuaParser, limit: i32) -> ParseResult {
    let uop = LuaOpKind::to_unary_operator(p.current_token());
    let mut cm = if uop != UnaryOperator::OpNop {
        let m = p.mark(LuaSyntaxKind::UnaryExpr);
        let op_range = p.current_token_range();
        let op_token = p.current_token();
        p.bump();
        match parse_sub_expr(p, UNARY_PRIORITY) {
            Ok(_) => {}
            Err(_) => {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!(
                        "unary operator '%{op}' is not followed by an expression",
                        op = op_token
                    ),
                    op_range,
                ));
            }
        };
        m.complete(p)
    } else {
        parse_simple_expr(p)?
    };

    let mut bop = LuaOpKind::to_binary_operator(p.current_token());
    while bop != BinaryOperator::OpNop && bop.get_priority().left > limit {
        let op_range = p.current_token_range();
        let op_token = p.current_token();
        let m = cm.precede(p, LuaSyntaxKind::BinaryExpr);
        p.bump();
        match parse_sub_expr(p, bop.get_priority().right) {
            Ok(_) => {}
            Err(err) => {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!(
                        "binary operator '%{op}' is not followed by an expression",
                        op = op_token
                    ),
                    op_range,
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
        LuaTokenKind::TkName | LuaTokenKind::TkLeftParen => parse_suffixed_expr(p),
        _ => {
            // Provide more specific error information
            let error_msg = match p.current_token() {
                LuaTokenKind::TkEof => t!("unexpected end of file, expected expression"),
                LuaTokenKind::TkRightParen => t!("unexpected ')', expected expression"),
                LuaTokenKind::TkRightBrace => t!("unexpected '}', expected expression"),
                LuaTokenKind::TkRightBracket => t!("unexpected ']', expected expression"),
                LuaTokenKind::TkComma => t!("unexpected ',', expected expression"),
                LuaTokenKind::TkSemicolon => t!("unexpected ';', expected expression"),
                LuaTokenKind::TkEnd => t!("unexpected 'end', expected expression"),
                LuaTokenKind::TkElse => t!("unexpected 'else', expected expression"),
                LuaTokenKind::TkElseIf => t!("unexpected 'elseif', expected expression"),
                LuaTokenKind::TkThen => t!("unexpected 'then', expected expression"),
                LuaTokenKind::TkDo => t!("unexpected 'do', expected expression"),
                LuaTokenKind::TkUntil => t!("unexpected 'until', expected expression"),
                _ => t!(
                    "unexpected token '%{token}', expected expression",
                    token = p.current_token()
                ),
            };

            p.push_error(LuaParseError::syntax_error_from(
                &error_msg,
                p.current_token_range(),
            ));
            Err(ParseFailReason::UnexpectedToken)
        }
    }
}

pub fn parse_closure_expr(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::ClosureExpr);

    if_token_bump(p, LuaTokenKind::TkFunction);

    parse_param_list(p)?;

    if p.current_token() != LuaTokenKind::TkEnd {
        parse_block(p)?;
    }

    if p.current_token() == LuaTokenKind::TkEnd {
        p.bump();
    } else {
        p.push_error(LuaParseError::syntax_error_from(
            &t!("expected 'end' to close function definition"),
            p.current_token_range(),
        ));
    }

    Ok(m.complete(p))
}

fn parse_param_list(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::ParamList);

    if p.current_token() == LuaTokenKind::TkLeftParen {
        p.bump();
    } else {
        p.push_error(LuaParseError::syntax_error_from(
            &t!("expected '(' to start parameter list"),
            p.current_token_range(),
        ));
    }

    if p.current_token() != LuaTokenKind::TkRightParen {
        loop {
            match parse_param_name(p) {
                Ok(_) => {}
                Err(_) => {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("expected parameter name"),
                        p.current_token_range(),
                    ));
                    // Try to recover to next comma or right parenthesis
                    while !matches!(
                        p.current_token(),
                        LuaTokenKind::TkComma
                            | LuaTokenKind::TkRightParen
                            | LuaTokenKind::TkEof
                            | LuaTokenKind::TkEnd
                    ) && !is_statement_start_token(p.current_token())
                    {
                        p.bump();
                    }
                }
            }

            if p.current_token() == LuaTokenKind::TkComma {
                p.bump();
                // Check if there is a parameter after comma
                if p.current_token() == LuaTokenKind::TkRightParen {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("expected parameter name after ','"),
                        p.current_token_range(),
                    ));
                    break;
                }
            } else {
                break;
            }
        }
    }

    if p.current_token() == LuaTokenKind::TkRightParen {
        p.bump();
    } else {
        p.push_error(LuaParseError::syntax_error_from(
            &t!("expected ')' to close parameter list"),
            p.current_token_range(),
        ));
    }

    Ok(m.complete(p))
}

fn parse_param_name(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::ParamName);
    let token = p.current_token();
    match token {
        LuaTokenKind::TkName | LuaTokenKind::TkDots => {
            p.bump();
            if token == LuaTokenKind::TkDots
                && p.parse_config.support_named_var_args()
                && p.current_token() == LuaTokenKind::TkName
            {
                p.bump();
            }
        }
        _ => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected parameter name or '...' (vararg)"),
                p.current_token_range(),
            ));
            return Err(ParseFailReason::UnexpectedToken);
        }
    }

    Ok(m.complete(p))
}

fn parse_table_expr(p: &mut LuaParser) -> ParseResult {
    let mut m = p.mark(LuaSyntaxKind::TableEmptyExpr);
    p.bump(); // consume '{'

    if p.current_token() == LuaTokenKind::TkRightBrace {
        p.bump();
        return Ok(m.complete(p));
    }

    // Parse first field
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
            // If first field parsing failed, continue trying to recover
            recover_to_table_boundary(p);
        }
    }

    // Parse remaining fields
    while matches!(
        p.current_token(),
        LuaTokenKind::TkComma | LuaTokenKind::TkSemicolon
    ) {
        let separator_token = p.current_token();
        p.bump(); // consume separator

        if p.current_token() == LuaTokenKind::TkRightBrace {
            // Allow trailing separator
            break;
        }

        match parse_field_with_recovery(p) {
            Ok(cm) => {
                if cm.kind == LuaSyntaxKind::TableFieldAssign {
                    m.set_kind(p, LuaSyntaxKind::TableObjectExpr);
                }
            }
            Err(_) => {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("invalid table field after '%{sep}'", sep = separator_token),
                    p.current_token_range(),
                ));
                // Recover to next field boundary
                recover_to_table_boundary(p);
                if p.current_token() == LuaTokenKind::TkRightBrace {
                    break;
                }
            }
        }
    }

    // Handle closing brace
    if p.current_token() == LuaTokenKind::TkRightBrace {
        p.bump();
    } else {
        p.push_error(LuaParseError::syntax_error_from(
            &t!("expected '}' to close table constructor"),
            p.current_token_range(),
        ));

        // Try to recover: look for possible closing brace
        let mut found_brace = false;
        let mut brace_count = 1; // 我们已经在表中
        let mut lookahead_count = 0;
        const MAX_LOOKAHEAD: usize = 50; // 限制向前查看的token数量

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
                // 如果遇到看起来像是表外部的token，停止寻找
                LuaTokenKind::TkEnd
                | LuaTokenKind::TkElse
                | LuaTokenKind::TkElseIf
                | LuaTokenKind::TkUntil
                | LuaTokenKind::TkThen
                | LuaTokenKind::TkDo => {
                    break;
                }
                _ => {
                    p.bump();
                }
            }
            lookahead_count += 1;
        }

        if !found_brace {
            // 如果没有找到闭合括号，在当前位置创建一个错误标记
            p.push_error(LuaParseError::syntax_error_from(
                &t!("table constructor was not properly closed"),
                p.current_token_range(),
            ));
        }
    }

    Ok(m.complete(p))
}

fn parse_field_with_recovery(p: &mut LuaParser) -> ParseResult {
    let mut m = p.mark(LuaSyntaxKind::TableFieldValue);

    match p.current_token() {
        LuaTokenKind::TkLeftBracket => {
            // [expr] = expr 形式
            m.set_kind(p, LuaSyntaxKind::TableFieldAssign);
            p.bump(); // consume '['

            match parse_expr(p) {
                Ok(_) => {}
                Err(_) => {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("expected expression inside table index brackets"),
                        p.current_token_range(),
                    ));
                    // 恢复到边界
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
                    &t!("expected ']' to close table index"),
                    p.current_token_range(),
                ));
            }

            if p.current_token() == LuaTokenKind::TkAssign {
                p.bump();
            } else {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("expected '=' after table index"),
                    p.current_token_range(),
                ));
            }

            match parse_expr(p) {
                Ok(_) => {}
                Err(_) => {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("expected value expression after '='"),
                        p.current_token_range(),
                    ));
                }
            }
        }
        LuaTokenKind::TkName => {
            // 可能是 name = expr 或者只是 expr
            if p.peek_next_token() == LuaTokenKind::TkAssign {
                m.set_kind(p, LuaSyntaxKind::TableFieldAssign);
                p.bump(); // consume name
                p.bump(); // consume '='
                match parse_expr(p) {
                    Ok(_) => {}
                    Err(_) => {
                        p.push_error(LuaParseError::syntax_error_from(
                            &t!("expected value expression after field name"),
                            p.current_token_range(),
                        ));
                    }
                }
            } else {
                // 作为表达式解析
                match parse_expr(p) {
                    Ok(_) => {}
                    Err(_) => {
                        p.push_error(LuaParseError::syntax_error_from(
                            &t!("invalid table field expression"),
                            p.current_token_range(),
                        ));
                    }
                }
            }
        }
        // 表示表实际上已经结束的token
        LuaTokenKind::TkEof | LuaTokenKind::TkLocal => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("unexpected end of table field"),
                p.current_token_range(),
            ));
        }
        _ => {
            // 尝试解析为普通表达式
            match parse_expr(p) {
                Ok(_) => {}
                Err(_) => {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("invalid table field, expected expression, field assignment, or table end"),
                        p.current_token_range(),
                    ));
                }
            }
        }
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
            let paren_range = p.current_token_range();
            p.bump();
            match parse_expr(p) {
                Ok(_) => {}
                Err(err) => {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("expected expression inside parentheses"),
                        paren_range,
                    ));
                    return Err(err);
                }
            }
            if p.current_token() == LuaTokenKind::TkRightParen {
                p.bump();
            } else {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("expected ')' to close parentheses"),
                    paren_range,
                ));
            }
            m.complete(p)
        }
        _ => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expect primary expression (identifier or parenthesized expression)"),
                p.current_token_range(),
            ));
            return Err(ParseFailReason::UnexpectedToken);
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
        SpecialFunction::Setmetaatable => LuaSyntaxKind::SetmetatableCallExpr,
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

fn parse_index_struct(p: &mut LuaParser) -> Result<(), ParseFailReason> {
    let index_op_range = p.current_token_range();
    match p.current_token() {
        LuaTokenKind::TkLeftBracket => {
            p.bump();
            match parse_expr(p) {
                Ok(_) => {}
                Err(err) => {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("expected expression inside table index brackets"),
                        index_op_range,
                    ));
                    return Err(err);
                }
            }
            match expect_token(p, LuaTokenKind::TkRightBracket) {
                Ok(_) => {}
                Err(err) => {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("expected ']' to close table index"),
                        index_op_range,
                    ));
                    return Err(err);
                }
            }
        }
        LuaTokenKind::TkDot => {
            p.bump();
            match expect_token(p, LuaTokenKind::TkName) {
                Ok(_) => {}
                Err(err) => {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("expected field name after '.'"),
                        index_op_range,
                    ));
                    return Err(err);
                }
            }
        }
        LuaTokenKind::TkColon => {
            p.bump();
            let name_token_range = p.current_token_range();
            match expect_token(p, LuaTokenKind::TkName) {
                Ok(_) => {}
                Err(err) => {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("expected method name after ':'"),
                        index_op_range,
                    ));
                    return Err(err);
                }
            }
            if !matches!(
                p.current_token(),
                LuaTokenKind::TkLeftParen
                    | LuaTokenKind::TkLeftBrace
                    | LuaTokenKind::TkString
                    | LuaTokenKind::TkLongString
            ) {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!(
                        "colon accessor must be followed by a function call or table constructor or string literal"
                    ),
                    name_token_range,
                ));

                return Err(ParseFailReason::UnexpectedToken);
            }
        }
        _ => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expect index struct"),
                p.current_token_range(),
            ));

            return Err(ParseFailReason::UnexpectedToken);
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
                loop {
                    match parse_expr(p) {
                        Ok(_) => {}
                        Err(_) => {
                            p.push_error(LuaParseError::syntax_error_from(
                                &t!("expected argument expression"),
                                p.current_token_range(),
                            ));
                            // 跳过到下一个逗号或右括号
                            while !matches!(
                                p.current_token(),
                                LuaTokenKind::TkComma
                                    | LuaTokenKind::TkRightParen
                                    | LuaTokenKind::TkEof
                            ) && !is_statement_start_token(p.current_token())
                            {
                                p.bump();
                            }

                            if p.current_token() == LuaTokenKind::TkComma {
                                p.bump();
                                continue;
                            }
                            break;
                        }
                    }

                    if p.current_token() == LuaTokenKind::TkComma {
                        p.bump();
                        if p.current_token() == LuaTokenKind::TkRightParen {
                            p.push_error(LuaParseError::syntax_error_from(
                                &t!("expected expression after ','"),
                                p.current_token_range(),
                            ));
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }

            if p.current_token() == LuaTokenKind::TkRightParen {
                p.bump();
            } else {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("expected ')' to close argument list"),
                    p.current_token_range(),
                ));
            }
        }
        LuaTokenKind::TkLeftBrace => match parse_table_expr(p) {
            Ok(_) => {}
            Err(err) => {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("invalid table constructor in function call"),
                    p.current_token_range(),
                ));
                return Err(err);
            }
        },
        LuaTokenKind::TkString | LuaTokenKind::TkLongString => {
            let m1 = p.mark(LuaSyntaxKind::LiteralExpr);
            p.bump();
            m1.complete(p);
        }
        _ => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected '(', string, or table constructor for function call"),
                p.current_token_range(),
            ));

            return Err(ParseFailReason::UnexpectedToken);
        }
    }

    Ok(m.complete(p))
}
