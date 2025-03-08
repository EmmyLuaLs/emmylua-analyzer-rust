use emmylua_parser::{
    LuaBlock, LuaCallExprStat, LuaDoStat, LuaExpr, LuaForRangeStat, LuaForStat, LuaIfStat,
    LuaRepeatStat, LuaReturnStat, LuaStat, LuaWhileStat,
};

#[derive(Debug, Clone)]
pub enum LuaReturnPoint {
    Expr(LuaExpr),
    MuliExpr(Vec<LuaExpr>),
    Nil,
    Error,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ChangeFlow {
    None,
    Break,
    Error,
    Return,
}

pub fn analyze_func_body_returns(body: LuaBlock) -> Vec<LuaReturnPoint> {
    let mut returns = Vec::new();

    let flow = analyze_block_returns(body, &mut returns);
    match flow {
        Some(ChangeFlow::Break) | Some(ChangeFlow::None) => {
            returns.push(LuaReturnPoint::Nil);
        }
        _ => {}
    }

    returns
}

fn analyze_block_returns(block: LuaBlock, returns: &mut Vec<LuaReturnPoint>) -> Option<ChangeFlow> {
    for stat in block.get_stats() {
        match stat {
            LuaStat::DoStat(do_stat) => {
                let flow = analyze_do_stat_returns(do_stat, returns);
                match flow {
                    Some(ChangeFlow::None) => {}
                    _ => return flow,
                }
            }
            LuaStat::WhileStat(while_stat) => {
                analyze_while_stat_returns(while_stat, returns);
            }
            LuaStat::RepeatStat(repeat_stat) => {
                analyze_repeat_stat_returns(repeat_stat, returns);
            }
            LuaStat::IfStat(if_stat) => {
                analyze_if_stat_returns(if_stat, returns);
            }
            LuaStat::ForStat(for_stat) => {
                analyze_for_stat_returns(for_stat, returns);
            }
            LuaStat::ForRangeStat(for_range_stat) => {
                analyze_for_range_stat_returns(for_range_stat, returns);
            }
            LuaStat::CallExprStat(call_expr) => {
                let flow = analyze_call_expr_stat_returns(call_expr, returns);
                match flow {
                    Some(ChangeFlow::Error) => return Some(ChangeFlow::Error),
                    _ => {}
                }
            }
            LuaStat::BreakStat(_) => {
                return Some(ChangeFlow::Break);
            }
            LuaStat::ReturnStat(return_stat) => {
                return analyze_return_stat_returns(return_stat, returns);
            }
            _ => {}
        };
    }

    Some(ChangeFlow::None)
}

fn analyze_do_stat_returns(
    do_stat: LuaDoStat,
    returns: &mut Vec<LuaReturnPoint>,
) -> Option<ChangeFlow> {
    analyze_block_returns(do_stat.get_block()?, returns)
}

fn analyze_while_stat_returns(
    while_stat: LuaWhileStat,
    returns: &mut Vec<LuaReturnPoint>,
) -> Option<ChangeFlow> {
    let flow = analyze_block_returns(while_stat.get_block()?, returns);
    match flow {
        Some(ChangeFlow::Break) => Some(ChangeFlow::None),
        _ => flow,
    }
}

fn analyze_repeat_stat_returns(
    repeat_stat: LuaRepeatStat,
    returns: &mut Vec<LuaReturnPoint>,
) -> Option<ChangeFlow> {
    let flow = analyze_block_returns(repeat_stat.get_block()?, returns);
    match flow {
        Some(ChangeFlow::Break) => Some(ChangeFlow::None),
        _ => flow,
    }
}

fn analyze_for_stat_returns(
    for_stat: LuaForStat,
    returns: &mut Vec<LuaReturnPoint>,
) -> Option<ChangeFlow> {
    let flow = analyze_block_returns(for_stat.get_block()?, returns);
    match flow {
        Some(ChangeFlow::Break) => Some(ChangeFlow::None),
        _ => flow,
    }
}

// todo
fn analyze_if_stat_returns(
    if_stat: LuaIfStat,
    returns: &mut Vec<LuaReturnPoint>,
) -> Option<ChangeFlow> {
    analyze_block_returns(if_stat.get_block()?, returns);
    for clause in if_stat.get_all_clause() {
        analyze_block_returns(clause.get_block()?, returns);
    }

    Some(ChangeFlow::None)
}

fn analyze_for_range_stat_returns(
    for_range_stat: LuaForRangeStat,
    returns: &mut Vec<LuaReturnPoint>,
) -> Option<ChangeFlow> {
    let flow = analyze_block_returns(for_range_stat.get_block()?, returns);
    match flow {
        Some(ChangeFlow::Break) => Some(ChangeFlow::None),
        _ => flow,
    }
}

fn analyze_call_expr_stat_returns(
    call_expr_stat: LuaCallExprStat,
    returns: &mut Vec<LuaReturnPoint>,
) -> Option<ChangeFlow> {
    let prefix_expr = call_expr_stat.get_call_expr()?.get_prefix_expr()?;
    if let LuaExpr::NameExpr(name) = prefix_expr {
        if name.get_name_text()? == "error" {
            returns.push(LuaReturnPoint::Error);
            return Some(ChangeFlow::Error);
        }
    }
    Some(ChangeFlow::None)
}

fn analyze_return_stat_returns(
    return_stat: LuaReturnStat,
    returns: &mut Vec<LuaReturnPoint>,
) -> Option<ChangeFlow> {
    let exprs: Vec<LuaExpr> = return_stat.get_expr_list().collect();
    match exprs.len() {
        0 => returns.push(LuaReturnPoint::Nil),
        1 => returns.push(LuaReturnPoint::Expr(exprs[0].clone())),
        _ => returns.push(LuaReturnPoint::MuliExpr(exprs)),
    }

    Some(ChangeFlow::Return)
}
