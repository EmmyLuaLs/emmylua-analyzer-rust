use crate::{
    LuaLanguageLevel,
    grammar::{ParseFailReason, ParseResult, lua::is_statement_start_token},
    kind::{LuaSyntaxKind, LuaTokenKind},
    parser::{CompleteMarker, LuaParser, MarkerEventContainer},
    parser_error::LuaParseError,
};

use super::{
    expect_token,
    expr::{parse_closure_expr, parse_expr},
    if_token_bump, parse_block,
};

/// Push expression parsing error with lazy error message generation
fn push_expr_error_lazy<F>(p: &mut LuaParser, error_msg_fn: F)
where
    F: FnOnce() -> std::borrow::Cow<'static, str>,
{
    let error_msg = error_msg_fn();
    p.push_error(LuaParseError::syntax_error_from(
        &error_msg,
        p.current_token_range(),
    ));
}

/// Generic keyword expectation with error recovery and lazy error message generation
fn expect_keyword_with_recovery<F>(
    p: &mut LuaParser,
    expected: LuaTokenKind,
    error_msg_fn: F,
) -> bool
where
    F: FnOnce() -> std::borrow::Cow<'static, str>,
{
    if p.current_token() == expected {
        p.bump();
        true
    } else {
        let error_msg = error_msg_fn();
        p.push_error(LuaParseError::syntax_error_from(
            &error_msg,
            p.current_token_range(),
        ));

        // Check if we can continue parsing (assume user forgot the keyword)
        is_statement_start_token(p.current_token())
    }
}

/// Expect 'end' keyword, report error at start keyword location if missing
fn expect_end_keyword<F>(p: &mut LuaParser, start_range: crate::text::SourceRange, error_msg_fn: F)
where
    F: FnOnce() -> std::borrow::Cow<'static, str>,
{
    if p.current_token() == LuaTokenKind::TkEnd {
        p.bump();
    } else {
        let error_msg = error_msg_fn();
        // Report error at the start keyword location
        p.push_error(LuaParseError::syntax_error_from(&error_msg, start_range));

        // Try to recover: look for possible 'end' or other structure terminators
        recover_to_block_end(p);
    }
}

/// Error recovery: skip to block end markers
fn recover_to_block_end(p: &mut LuaParser) {
    let mut depth = 1;

    while p.current_token() != LuaTokenKind::TkEof && depth > 0 {
        match p.current_token() {
            // Nested structure starts
            LuaTokenKind::TkIf
            | LuaTokenKind::TkWhile
            | LuaTokenKind::TkFor
            | LuaTokenKind::TkDo
            | LuaTokenKind::TkFunction => {
                depth += 1;
                p.bump();
            }
            // Structure ends
            LuaTokenKind::TkEnd => {
                depth -= 1;
                if depth == 0 {
                    p.bump(); // Consume the found 'end'
                }
            }
            // Other possible recovery points
            LuaTokenKind::TkElseIf | LuaTokenKind::TkElse => {
                if depth == 1 {
                    // Found same-level elseif/else, can recover
                    break;
                }
                p.bump();
            }
            // Other control flow end markers
            LuaTokenKind::TkUntil => {
                depth -= 1;
                if depth == 0 {
                    // This might be the end of repeat-until
                    break;
                }
                p.bump();
            }
            _ => {
                p.bump();
            }
        }
    }
}

/// Error recovery: skip to specified keywords
fn recover_to_keywords(p: &mut LuaParser, keywords: &[LuaTokenKind]) {
    while p.current_token() != LuaTokenKind::TkEof {
        if keywords.contains(&p.current_token()) {
            break;
        }

        // Also stop recovery if we encounter statement start markers
        if is_statement_start_token(p.current_token()) {
            break;
        }

        p.bump();
    }
}

// Parse a comma-separated list of expressions, returning an error message only if there's an error.
fn parse_expr_list_impl(p: &mut LuaParser) -> Result<(), &'static str> {
    parse_expr(p).map_err(|_| "expected expression")?;

    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        parse_expr(p).map_err(|_| "expected expression after ','")?;
    }

    Ok(())
}

// Parse a comma-separated list of variable names.
fn parse_variable_name_list(p: &mut LuaParser, support_attrib: bool) -> ParseResult {
    parse_local_name(p, support_attrib)?;

    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        match parse_local_name(p, support_attrib) {
            Ok(_) => {}
            Err(_) => {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("expected variable name after ','"),
                    p.current_token_range(),
                ));
            }
        }
    }

    Ok(CompleteMarker::empty())
}

fn parse_global_name_list(p: &mut LuaParser) -> ParseResult {
    parse_local_name(p, true)?;

    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        match parse_local_name(p, true) {
            Ok(_) => {}
            Err(_) => {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("expected variable name after ','"),
                    p.current_token_range(),
                ));
            }
        }
    }

    if p.current_token() == LuaTokenKind::TkEq {
        p.bump();
        match parse_expr_list_impl(p) {
            Ok(_) => {}
            Err(_) => {
                push_expr_error_lazy(p, || t!("expected expression after '='"));
            }
        }
    }

    Ok(CompleteMarker::empty())
}

pub fn parse_stats(p: &mut LuaParser) {
    while !block_follow(p) {
        let level = p.get_mark_level();
        match parse_stat(p) {
            Ok(_) => {}
            Err(_) => {
                let current_level = p.get_mark_level();
                for _ in 0..(current_level - level) {
                    p.push_node_end();
                }

                let mut can_continue = false;
                // error recover
                while p.current_token() != LuaTokenKind::TkEof {
                    if is_statement_start_token(p.current_token()) {
                        can_continue = true;
                        break;
                    }

                    p.bump();
                }

                if can_continue {
                    continue;
                }
                break;
            }
        }
    }
}

fn block_follow(p: &LuaParser) -> bool {
    matches!(
        p.current_token(),
        LuaTokenKind::TkElse
            | LuaTokenKind::TkElseIf
            | LuaTokenKind::TkEnd
            | LuaTokenKind::TkEof
            | LuaTokenKind::TkUntil
    )
}

fn parse_stat(p: &mut LuaParser) -> ParseResult {
    let cm = match p.current_token() {
        LuaTokenKind::TkIf => parse_if(p)?,
        LuaTokenKind::TkWhile => parse_while(p)?,
        LuaTokenKind::TkFor => parse_for(p)?,
        LuaTokenKind::TkFunction => parse_function(p)?,
        LuaTokenKind::TkLocal => parse_local(p)?,
        LuaTokenKind::TkReturn => parse_return(p)?,
        LuaTokenKind::TkBreak => parse_break(p)?,
        LuaTokenKind::TkDo => parse_do(p)?,
        LuaTokenKind::TkRepeat => parse_repeat(p)?,
        LuaTokenKind::TkGoto => parse_goto(p)?,
        LuaTokenKind::TkDbColon => parse_label_stat(p)?,
        LuaTokenKind::TkSemicolon => parse_empty_stat(p)?,
        _ => parse_assign_or_expr_or_global_stat(p)?,
    };

    Ok(cm)
}

fn parse_if(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::IfStat);
    let if_start_range = p.current_token_range();
    p.bump(); // consume 'if'

    // Parse condition expression
    if parse_expr(p).is_err() {
        push_expr_error_lazy(p, || t!("expected condition expression after 'if'"));
        // 尝试恢复到 'then' 或语句开始
        recover_to_keywords(p, &[LuaTokenKind::TkThen, LuaTokenKind::TkEnd]);
    }

    // Expect 'then'
    if !expect_keyword_with_recovery(p, LuaTokenKind::TkThen, || {
        t!("expected 'then' after if condition")
    }) {
        // 如果没有找到 'then'，尝试恢复
        recover_to_keywords(
            p,
            &[
                LuaTokenKind::TkEnd,
                LuaTokenKind::TkElseIf,
                LuaTokenKind::TkElse,
            ],
        );
    }

    // 只有在找到合适的恢复点时才解析块
    if !matches!(
        p.current_token(),
        LuaTokenKind::TkEnd | LuaTokenKind::TkElseIf | LuaTokenKind::TkElse | LuaTokenKind::TkEof
    ) {
        parse_block(p)?;
    }

    while p.current_token() == LuaTokenKind::TkElseIf {
        parse_elseif_clause(p)?;
    }

    if p.current_token() == LuaTokenKind::TkElse {
        parse_else_clause(p)?;
    }

    // Use new end expectation function to associate error with 'if' keyword
    expect_end_keyword(p, if_start_range, || {
        t!("expected 'end' to close if statement")
    });

    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_elseif_clause(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::ElseIfClauseStat);
    p.bump();

    if parse_expr(p).is_err() {
        push_expr_error_lazy(p, || t!("expected condition expression after 'elseif'"));
    }

    expect_keyword_with_recovery(p, LuaTokenKind::TkThen, || {
        t!("expected 'then' after 'elseif' condition")
    });

    parse_block(p)?;

    Ok(m.complete(p))
}

fn parse_else_clause(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::ElseClauseStat);
    p.bump();
    parse_block(p)?;

    Ok(m.complete(p))
}

fn parse_while(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::WhileStat);
    let while_start_range = p.current_token_range();
    p.bump(); // consume 'while'

    // Parse condition expression
    if parse_expr(p).is_err() {
        push_expr_error_lazy(p, || t!("expected condition expression after 'while'"));
        recover_to_keywords(p, &[LuaTokenKind::TkDo, LuaTokenKind::TkEnd]);
    }

    // Expect 'do'
    if !expect_keyword_with_recovery(p, LuaTokenKind::TkDo, || {
        t!("expected 'do' after while condition")
    }) {
        recover_to_keywords(p, &[LuaTokenKind::TkEnd]);
    }

    // 只有在找到合适的恢复点时才解析块
    if p.current_token() != LuaTokenKind::TkEnd && p.current_token() != LuaTokenKind::TkEof {
        parse_block(p)?;
    }

    // Use new end expectation function to associate error with 'while' keyword
    expect_end_keyword(p, while_start_range, || {
        t!("expected 'end' to close while statement")
    });

    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_do(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::DoStat);
    let do_start_range = p.current_token_range();
    p.bump();

    parse_block(p)?;

    expect_end_keyword(p, do_start_range, || t!("expected 'end' after 'do' block"));

    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_for(p: &mut LuaParser) -> ParseResult {
    let mut m = p.mark(LuaSyntaxKind::ForStat);
    let for_start_range = p.current_token_range();
    p.bump(); // consume 'for'

    // Expect variable name
    if p.current_token() == LuaTokenKind::TkName {
        p.bump();
    } else {
        p.push_error(LuaParseError::syntax_error_from(
            &t!("expected variable name after 'for'"),
            p.current_token_range(),
        ));
        // Try to recover: skip to '=' or 'in'
        recover_to_keywords(
            p,
            &[
                LuaTokenKind::TkAssign,
                LuaTokenKind::TkIn,
                LuaTokenKind::TkComma,
                LuaTokenKind::TkDo,
                LuaTokenKind::TkEnd,
            ],
        );
    }

    match p.current_token() {
        LuaTokenKind::TkAssign => {
            // Numeric for loop
            p.bump();
            // Start value
            if parse_expr(p).is_err() {
                push_expr_error_lazy(p, || {
                    t!("expected start value expression in numeric for loop")
                });
            }

            if p.current_token() == LuaTokenKind::TkComma {
                p.bump();
            } else {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("expected ',' after start value in numeric for loop"),
                    p.current_token_range(),
                ));
            }

            // End value
            if parse_expr(p).is_err() {
                push_expr_error_lazy(p, || {
                    t!("expected end value expression in numeric for loop")
                });
            }

            // Optional step value
            if p.current_token() == LuaTokenKind::TkComma {
                p.bump();
                if parse_expr(p).is_err() {
                    push_expr_error_lazy(p, || {
                        t!("expected step value expression in numeric for loop")
                    });
                }
            }
        }
        LuaTokenKind::TkComma | LuaTokenKind::TkIn => {
            // Generic for loop
            m.set_kind(p, LuaSyntaxKind::ForRangeStat);
            while p.current_token() == LuaTokenKind::TkComma {
                p.bump();
                if p.current_token() == LuaTokenKind::TkName {
                    p.bump();
                } else {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("expected variable name after ','"),
                        p.current_token_range(),
                    ));
                }
            }

            if p.current_token() == LuaTokenKind::TkIn {
                p.bump();
            } else {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("expected 'in' after variable list in generic for loop"),
                    p.current_token_range(),
                ));
            }

            // Iterator expression list
            if parse_expr_list_impl(p).is_err() {
                push_expr_error_lazy(p, || t!("expected iterator expression after 'in'"));
            }
        }
        _ => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected '=' for numeric for loop or ',' or 'in' for generic for loop"),
                p.current_token_range(),
            ));
        }
    }

    // Expect 'do'
    if !expect_keyword_with_recovery(p, LuaTokenKind::TkDo, || {
        t!("expected 'do' in for statement")
    }) {
        recover_to_keywords(p, &[LuaTokenKind::TkEnd]);
    }

    // 只有在找到合适的恢复点时才解析块
    if p.current_token() != LuaTokenKind::TkEnd && p.current_token() != LuaTokenKind::TkEof {
        parse_block(p)?;
    }

    expect_end_keyword(p, for_start_range, || {
        t!("expected 'end' to close for statement")
    });

    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_function(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::FuncStat);
    p.bump();
    parse_func_name(p)?;
    parse_closure_expr(p)?;
    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_func_name(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::NameExpr);
    match expect_token(p, LuaTokenKind::TkName) {
        Ok(_) => {}
        Err(_) => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected function name after 'function'"),
                p.current_token_range(),
            ));
            return Err(ParseFailReason::UnexpectedToken);
        }
    }

    let cm =
        if p.current_token() == LuaTokenKind::TkDot || p.current_token() == LuaTokenKind::TkColon {
            let mut cm = m.complete(p);
            while p.current_token() == LuaTokenKind::TkDot {
                let m = cm.precede(p, LuaSyntaxKind::IndexExpr);
                p.bump();
                match expect_token(p, LuaTokenKind::TkName) {
                    Ok(_) => {}
                    Err(_) => {
                        p.push_error(LuaParseError::syntax_error_from(
                            &t!("expected name after '.'"),
                            p.current_token_range(),
                        ));
                    }
                }
                cm = m.complete(p);
            }

            if p.current_token() == LuaTokenKind::TkColon {
                let m = cm.precede(p, LuaSyntaxKind::IndexExpr);
                p.bump();
                match expect_token(p, LuaTokenKind::TkName) {
                    Ok(_) => {}
                    Err(_) => {
                        p.push_error(LuaParseError::syntax_error_from(
                            &t!("expected name after ':'"),
                            p.current_token_range(),
                        ));
                    }
                }
                cm = m.complete(p);
            }

            cm
        } else {
            m.complete(p)
        };

    Ok(cm)
}

fn parse_local(p: &mut LuaParser) -> ParseResult {
    let mut m = p.mark(LuaSyntaxKind::LocalStat);
    p.bump(); // consume 'local'

    match p.current_token() {
        LuaTokenKind::TkFunction => {
            p.bump();
            m.set_kind(p, LuaSyntaxKind::LocalFuncStat);

            match parse_local_name(p, false) {
                Ok(_) => {}
                Err(_) => {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("expected function name after 'local function'"),
                        p.current_token_range(),
                    ));
                }
            }

            match parse_closure_expr(p) {
                Ok(_) => {}
                Err(_) => {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("invalid function definition"),
                        p.current_token_range(),
                    ));
                }
            }
        }
        LuaTokenKind::TkName => {
            parse_variable_name_list(p, true)?;

            // 可选的初始化表达式
            if p.current_token().is_assign_op() {
                p.bump();
                if parse_expr_list_impl(p).is_err() {
                    push_expr_error_lazy(p, || t!("expected initialization expression after '='"));
                }
            }
        }
        LuaTokenKind::TkLt => {
            if p.parse_config.level >= LuaLanguageLevel::Lua55 {
                match parse_attrib(p) {
                    Ok(_) => {}
                    Err(_) => {
                        p.push_error(LuaParseError::syntax_error_from(
                            &t!("invalid attribute syntax"),
                            p.current_token_range(),
                        ));
                    }
                }

                parse_variable_name_list(p, true)?;

                if p.current_token().is_assign_op() {
                    p.bump();
                    if parse_expr_list_impl(p).is_err() {
                        push_expr_error_lazy(p, || {
                            t!("expected initialization expression after '='")
                        });
                    }
                }
            } else {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!(
                        "local attributes are not supported in Lua version %{level}",
                        level = p.parse_config.level
                    ),
                    p.current_token_range(),
                ));

                return Err(ParseFailReason::UnexpectedToken);
            }
        }
        _ => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected 'function', variable name, or attribute after 'local'"),
                p.current_token_range(),
            ));

            return Err(ParseFailReason::UnexpectedToken);
        }
    }

    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_local_name(p: &mut LuaParser, support_attrib: bool) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::LocalName);
    match expect_token(p, LuaTokenKind::TkName) {
        Ok(_) => {}
        Err(_) => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected variable name after 'local'"),
                p.current_token_range(),
            ));
        }
    }
    if support_attrib && p.current_token() == LuaTokenKind::TkLt {
        parse_attrib(p)?;
    }

    Ok(m.complete(p))
}

fn parse_attrib(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::Attribute);
    let range = p.current_token_range();
    p.bump();
    match expect_token(p, LuaTokenKind::TkName) {
        Ok(_) => {}
        Err(_) => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected attribute name after '<'"),
                p.current_token_range(),
            ));
        }
    }
    match expect_token(p, LuaTokenKind::TkGt) {
        Ok(_) => {}
        Err(_) => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected '>' after attribute name"),
                p.current_token_range(),
            ));
        }
    }
    if !p.parse_config.support_local_attrib() {
        p.errors.push(LuaParseError::syntax_error_from(
            &t!(
                "local attribute is not supported for current version: %{level}",
                level = p.parse_config.level
            ),
            range,
        ));
    }

    Ok(m.complete(p))
}

fn parse_return(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::ReturnStat);
    p.bump();
    if !block_follow(p)
        && p.current_token() != LuaTokenKind::TkSemicolon
        && parse_expr_list_impl(p).is_err()
    {
        push_expr_error_lazy(p, || t!("expected expression in return statement"));
    }

    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_break(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::BreakStat);
    p.bump();
    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_repeat(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::RepeatStat);
    p.bump();
    parse_block(p)?;
    match expect_token(p, LuaTokenKind::TkUntil) {
        Ok(_) => {}
        Err(_) => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected 'until' after repeat block"),
                p.current_token_range(),
            ));
        }
    }
    if parse_expr(p).is_err() {
        push_expr_error_lazy(p, || t!("expected condition expression after 'until'"));
    }
    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_goto(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::GotoStat);
    p.bump();
    match expect_token(p, LuaTokenKind::TkName) {
        Ok(_) => {}
        Err(_) => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected label name after 'goto'"),
                p.current_token_range(),
            ));
        }
    }
    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_empty_stat(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::EmptyStat);
    p.bump();
    Ok(m.complete(p))
}

fn try_parse_global_stat(p: &mut LuaParser) -> ParseResult {
    let mut m = p.mark(LuaSyntaxKind::GlobalStat);
    match p.peek_next_token() {
        LuaTokenKind::TkName => {
            p.set_current_token_kind(LuaTokenKind::TkGlobal);
            p.bump();
            parse_global_name_list(p)?;
        }
        LuaTokenKind::TkLt => {
            p.set_current_token_kind(LuaTokenKind::TkGlobal);
            p.bump();
            parse_attrib(p)?;
            parse_global_name_list(p)?;
        }
        // global function
        LuaTokenKind::TkFunction => {
            p.set_current_token_kind(LuaTokenKind::TkGlobal);
            p.bump(); // consume 'global'
            m.set_kind(p, LuaSyntaxKind::FuncStat);
            p.bump(); // consume 'function'
            let m2 = p.mark(LuaSyntaxKind::NameExpr);
            match expect_token(p, LuaTokenKind::TkName) {
                Ok(_) => {}
                Err(_) => {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!("expected function name after 'global function'"),
                        p.current_token_range(),
                    ));
                    return Err(ParseFailReason::UnexpectedToken);
                }
            }
            m2.complete(p);
            parse_closure_expr(p)?;
        }
        // global *
        LuaTokenKind::TkMul => {
            p.set_current_token_kind(LuaTokenKind::TkGlobal);
            p.bump(); // consume 'global'
            p.bump(); // consume '*'
        }
        _ => {
            return Ok(m.undo(p));
        }
    }

    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_assign_or_expr_or_global_stat(p: &mut LuaParser) -> ParseResult {
    if p.parse_config.level >= LuaLanguageLevel::Lua55 && p.current_token() == LuaTokenKind::TkName
    {
        let token_text = p.current_token_text();
        if token_text == "global" {
            let cm = try_parse_global_stat(p)?;
            if !cm.is_invalid() {
                return Ok(cm);
            }
        }
    }

    let mut m = p.mark(LuaSyntaxKind::AssignStat);
    let range = p.current_token_range();

    // 解析第一个表达式
    let cm = match parse_expr(p) {
        Ok(cm) => cm,
        Err(err) => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected expression in assignment or statement"),
                range,
            ));
            return Err(err);
        }
    };

    // 检查是否是函数调用语句
    if matches!(
        cm.kind,
        LuaSyntaxKind::CallExpr
            | LuaSyntaxKind::AssertCallExpr
            | LuaSyntaxKind::ErrorCallExpr
            | LuaSyntaxKind::RequireCallExpr
            | LuaSyntaxKind::TypeCallExpr
            | LuaSyntaxKind::SetmetatableCallExpr
    ) {
        m.set_kind(p, LuaSyntaxKind::CallExprStat);
        if_token_bump(p, LuaTokenKind::TkSemicolon);
        return Ok(m.complete(p));
    }

    // 验证左值
    if !matches!(cm.kind, LuaSyntaxKind::NameExpr | LuaSyntaxKind::IndexExpr) {
        p.push_error(LuaParseError::syntax_error_from(
            &t!("invalid left-hand side in assignment (expected variable or table index)"),
            range,
        ));

        return Err(ParseFailReason::UnexpectedToken);
    }

    // 解析更多左值（如果有逗号）
    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        match parse_expr(p) {
            Ok(expr_cm) => {
                if !matches!(
                    expr_cm.kind,
                    LuaSyntaxKind::NameExpr | LuaSyntaxKind::IndexExpr
                ) {
                    p.push_error(LuaParseError::syntax_error_from(
                        &t!(
                            "invalid left-hand side in assignment (expected variable or table index)"
                        ),
                        p.current_token_range(),
                    ));
                    return Err(ParseFailReason::UnexpectedToken);
                }
            }
            Err(_) => {
                p.push_error(LuaParseError::syntax_error_from(
                    &t!("expected variable after ',' in assignment"),
                    p.current_token_range(),
                ));
            }
        }
    }

    // 期望赋值操作符
    if p.current_token().is_assign_op() {
        p.bump();

        // 解析右值表达式列表
        if parse_expr_list_impl(p).is_err() {
            push_expr_error_lazy(p, || t!("expected expression after '=' in assignment"));
        }
    } else {
        p.push_error(LuaParseError::syntax_error_from(
            &t!("expected '=' for assignment or this is an incomplete statement"),
            p.current_token_range(),
        ));

        return Err(ParseFailReason::UnexpectedToken);
    }

    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_label_stat(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::LabelStat);
    p.bump();
    match expect_token(p, LuaTokenKind::TkName) {
        Ok(_) => {}
        Err(_) => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected label name after 'goto'"),
                p.current_token_range(),
            ));
        }
    }
    match expect_token(p, LuaTokenKind::TkDbColon) {
        Ok(_) => {}
        Err(_) => {
            p.push_error(LuaParseError::syntax_error_from(
                &t!("expected '::' after label name"),
                p.current_token_range(),
            ));
        }
    }
    Ok(m.complete(p))
}
