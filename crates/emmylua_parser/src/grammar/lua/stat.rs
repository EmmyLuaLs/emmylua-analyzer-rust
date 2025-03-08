use crate::{
    grammar::ParseResult,
    kind::{LuaSyntaxKind, LuaTokenKind},
    parser::{LuaParser, MarkerEventContainer},
    parser_error::LuaParseError,
};

use super::{
    expect_token,
    expr::{parse_closure_expr, parse_expr},
    if_token_bump, parse_block,
};

pub fn parse_stats(p: &mut LuaParser) {
    while !block_follow(p) {
        let level = p.get_mark_level();
        match parse_stat(p) {
            Ok(_) => {}
            Err(err) => {
                p.errors.push(err);
                let current_level = p.get_mark_level();
                for _ in 0..(current_level - level) {
                    p.push_node_end();
                }

                break;
            }
        }
    }
}

fn block_follow(p: &LuaParser) -> bool {
    match p.current_token() {
        LuaTokenKind::TkElse
        | LuaTokenKind::TkElseIf
        | LuaTokenKind::TkEnd
        | LuaTokenKind::TkEof
        | LuaTokenKind::TkUntil => true,
        _ => false,
    }
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
        _ => parse_assign_or_expr_stat(p)?,
    };

    Ok(cm)
}

fn parse_if(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::IfStat);
    p.bump();
    parse_expr(p)?;
    expect_token(p, LuaTokenKind::TkThen)?;
    parse_block(p)?;

    while p.current_token() == LuaTokenKind::TkElseIf {
        parse_elseif_clause(p)?;
    }

    if p.current_token() == LuaTokenKind::TkElse {
        parse_else_clause(p)?;
    }

    expect_token(p, LuaTokenKind::TkEnd)?;

    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_elseif_clause(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::ElseIfClauseStat);
    p.bump();
    parse_expr(p)?;
    expect_token(p, LuaTokenKind::TkThen)?;
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
    p.bump();
    parse_expr(p)?;
    expect_token(p, LuaTokenKind::TkDo)?;
    parse_block(p)?;

    expect_token(p, LuaTokenKind::TkEnd)?;
    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_do(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::DoStat);
    p.bump();
    parse_block(p)?;
    expect_token(p, LuaTokenKind::TkEnd)?;

    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_for(p: &mut LuaParser) -> ParseResult {
    let mut m = p.mark(LuaSyntaxKind::ForStat);
    p.bump();
    expect_token(p, LuaTokenKind::TkName)?;
    match p.current_token() {
        LuaTokenKind::TkAssign => {
            p.bump();
            parse_expr(p)?;
            expect_token(p, LuaTokenKind::TkComma)?;
            parse_expr(p)?;
            if p.current_token() == LuaTokenKind::TkComma {
                p.bump();
                parse_expr(p)?;
            }
        }
        LuaTokenKind::TkComma | LuaTokenKind::TkIn => {
            m.set_kind(p, LuaSyntaxKind::ForRangeStat);
            while p.current_token() == LuaTokenKind::TkComma {
                p.bump();
                expect_token(p, LuaTokenKind::TkName)?;
            }

            expect_token(p, LuaTokenKind::TkIn)?;
            parse_expr(p)?;
            while p.current_token() == LuaTokenKind::TkComma {
                p.bump();
                parse_expr(p)?;
            }
        }
        _ => {
            return Err(LuaParseError::from_source_range(
                &t!("unexpected token"),
                p.current_token_range(),
            ));
        }
    }
    expect_token(p, LuaTokenKind::TkDo)?;
    parse_block(p)?;
    expect_token(p, LuaTokenKind::TkEnd)?;

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
    expect_token(p, LuaTokenKind::TkName)?;

    let cm =
        if p.current_token() == LuaTokenKind::TkDot || p.current_token() == LuaTokenKind::TkColon {
            let mut cm = m.complete(p);
            while p.current_token() == LuaTokenKind::TkDot {
                let m = cm.precede(p, LuaSyntaxKind::IndexExpr);
                p.bump();
                expect_token(p, LuaTokenKind::TkName)?;
                cm = m.complete(p);
            }

            if p.current_token() == LuaTokenKind::TkColon {
                let m = cm.precede(p, LuaSyntaxKind::IndexExpr);
                p.bump();
                expect_token(p, LuaTokenKind::TkName)?;
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
    p.bump();
    match p.current_token() {
        LuaTokenKind::TkFunction => {
            p.bump();
            m.set_kind(p, LuaSyntaxKind::LocalFuncStat);
            parse_local_name(p, false)?;
            parse_closure_expr(p)?;
        }
        LuaTokenKind::TkName => {
            parse_local_name(p, true)?;
            while p.current_token() == LuaTokenKind::TkComma {
                p.bump();
                parse_local_name(p, true)?;
            }

            if p.current_token() == LuaTokenKind::TkAssign {
                p.bump();
                parse_expr(p)?;
                while p.current_token() == LuaTokenKind::TkComma {
                    p.bump();
                    parse_expr(p)?;
                }
            }
        }
        _ => {
            return Err(LuaParseError::from_source_range(
                &t!("unexpected token %{token}", token = p.current_token()),
                p.current_token_range(),
            ));
        }
    }

    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_local_name(p: &mut LuaParser, support_attrib: bool) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::LocalName);
    expect_token(p, LuaTokenKind::TkName)?;
    if support_attrib && p.current_token() == LuaTokenKind::TkLt {
        parse_attrib(p)?;
    }

    Ok(m.complete(p))
}

fn parse_attrib(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::Attribute);
    let range: crate::text::SourceRange = p.current_token_range();
    p.bump();
    expect_token(p, LuaTokenKind::TkName)?;
    expect_token(p, LuaTokenKind::TkGt)?;
    if !p.parse_config.support_local_attrib() {
        p.errors.push(LuaParseError::from_source_range(
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
    if !block_follow(p) && p.current_token() != LuaTokenKind::TkSemicolon {
        parse_expr(p)?;
        while p.current_token() == LuaTokenKind::TkComma {
            p.bump();
            parse_expr(p)?;
        }
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
    expect_token(p, LuaTokenKind::TkUntil)?;
    parse_expr(p)?;
    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_goto(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::GotoStat);
    p.bump();
    expect_token(p, LuaTokenKind::TkName)?;
    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_empty_stat(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::EmptyStat);
    p.bump();
    Ok(m.complete(p))
}

fn parse_assign_or_expr_stat(p: &mut LuaParser) -> ParseResult {
    let mut m = p.mark(LuaSyntaxKind::AssignStat);
    let range = p.current_token_range();
    let mut cm = parse_expr(p)?;
    if cm.kind == LuaSyntaxKind::CallExpr {
        m.set_kind(p, LuaSyntaxKind::CallExprStat);
        if_token_bump(p, LuaTokenKind::TkSemicolon);
        return Ok(m.complete(p));
    }

    if cm.kind != LuaSyntaxKind::NameExpr && cm.kind != LuaSyntaxKind::IndexExpr {
        return Err(LuaParseError::from_source_range(
            &t!("unexpected expr for varList"),
            range,
        ));
    }

    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        cm = parse_expr(p)?;
        if cm.kind != LuaSyntaxKind::NameExpr && cm.kind != LuaSyntaxKind::IndexExpr {
            return Err(LuaParseError::from_source_range(
                &t!("unexpected expr for varList"),
                range,
            ));
        }
    }

    if p.current_token() == LuaTokenKind::TkAssign {
        p.bump();
        parse_expr(p)?;
        while p.current_token() == LuaTokenKind::TkComma {
            p.bump();
            parse_expr(p)?;
        }
    } else {
        return Err(LuaParseError::from_source_range(
            &t!("unfinished stat"),
            range,
        ));
    }

    if_token_bump(p, LuaTokenKind::TkSemicolon);
    Ok(m.complete(p))
}

fn parse_label_stat(p: &mut LuaParser) -> ParseResult {
    let m = p.mark(LuaSyntaxKind::LabelStat);
    p.bump();
    expect_token(p, LuaTokenKind::TkName)?;
    expect_token(p, LuaTokenKind::TkDbColon)?;
    Ok(m.complete(p))
}
