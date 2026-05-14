use emmylua_parser::{
    LuaBlock, LuaCallExprStat, LuaDoStat, LuaExpr, LuaForRangeStat, LuaForStat, LuaIfStat,
    LuaRepeatStat, LuaReturnStat, LuaStat, LuaWhileStat,
};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ChangeFlow {
    None,
    Break,
    Error,
    Return,
}

pub fn analyze_module_return_points(body: LuaBlock) -> Vec<LuaExpr> {
    let mut returns = Vec::new();
    let _ = analyze_block_returns(body, &mut returns, false);
    returns
}

pub fn analyze_signature_return_points(body: LuaBlock) -> Vec<LuaExpr> {
    let mut returns = Vec::new();
    let _ = analyze_block_returns(body, &mut returns, true);
    returns
}

fn analyze_block_returns(
    block: LuaBlock,
    returns: &mut Vec<LuaExpr>,
    collect_all_return_exprs: bool,
) -> Option<ChangeFlow> {
    for stat in block.get_stats() {
        match stat {
            LuaStat::DoStat(do_stat) => {
                let flow = analyze_do_stat_returns(do_stat, returns, collect_all_return_exprs);
                match flow {
                    Some(ChangeFlow::None) => {}
                    _ => return flow,
                }
            }
            LuaStat::WhileStat(while_stat) => {
                analyze_while_stat_returns(while_stat, returns, collect_all_return_exprs);
            }
            LuaStat::RepeatStat(repeat_stat) => {
                analyze_repeat_stat_returns(repeat_stat, returns, collect_all_return_exprs);
            }
            LuaStat::IfStat(if_stat) => {
                analyze_if_stat_returns(if_stat, returns, collect_all_return_exprs);
            }
            LuaStat::ForStat(for_stat) => {
                analyze_for_stat_returns(for_stat, returns, collect_all_return_exprs);
            }
            LuaStat::ForRangeStat(for_range_stat) => {
                analyze_for_range_stat_returns(for_range_stat, returns, collect_all_return_exprs);
            }
            LuaStat::CallExprStat(call_expr) => {
                let flow = analyze_call_expr_stat_returns(call_expr, returns);
                if let Some(ChangeFlow::Error) = flow {
                    return Some(ChangeFlow::Error);
                }
            }
            LuaStat::BreakStat(_) => return Some(ChangeFlow::Break),
            LuaStat::ReturnStat(return_stat) => {
                return analyze_return_stat_returns(return_stat, returns, collect_all_return_exprs);
            }
            _ => {}
        }
    }

    Some(ChangeFlow::None)
}

fn analyze_do_stat_returns(
    do_stat: LuaDoStat,
    returns: &mut Vec<LuaExpr>,
    collect_all_return_exprs: bool,
) -> Option<ChangeFlow> {
    analyze_block_returns(do_stat.get_block()?, returns, collect_all_return_exprs)
}

fn analyze_while_stat_returns(
    while_stat: LuaWhileStat,
    returns: &mut Vec<LuaExpr>,
    collect_all_return_exprs: bool,
) -> Option<ChangeFlow> {
    let flow = analyze_block_returns(while_stat.get_block()?, returns, collect_all_return_exprs);
    match flow {
        Some(ChangeFlow::Break) => Some(ChangeFlow::None),
        _ => flow,
    }
}

fn analyze_repeat_stat_returns(
    repeat_stat: LuaRepeatStat,
    returns: &mut Vec<LuaExpr>,
    collect_all_return_exprs: bool,
) -> Option<ChangeFlow> {
    let flow = analyze_block_returns(repeat_stat.get_block()?, returns, collect_all_return_exprs);
    match flow {
        Some(ChangeFlow::Break) => Some(ChangeFlow::None),
        _ => flow,
    }
}

fn analyze_for_stat_returns(
    for_stat: LuaForStat,
    returns: &mut Vec<LuaExpr>,
    collect_all_return_exprs: bool,
) -> Option<ChangeFlow> {
    let flow = analyze_block_returns(for_stat.get_block()?, returns, collect_all_return_exprs);
    match flow {
        Some(ChangeFlow::Break) => Some(ChangeFlow::None),
        _ => flow,
    }
}

fn analyze_if_stat_returns(
    if_stat: LuaIfStat,
    returns: &mut Vec<LuaExpr>,
    collect_all_return_exprs: bool,
) -> Option<ChangeFlow> {
    analyze_block_returns(if_stat.get_block()?, returns, collect_all_return_exprs);
    for clause in if_stat.get_all_clause() {
        analyze_block_returns(clause.get_block()?, returns, collect_all_return_exprs);
    }

    Some(ChangeFlow::None)
}

fn analyze_for_range_stat_returns(
    for_range_stat: LuaForRangeStat,
    returns: &mut Vec<LuaExpr>,
    collect_all_return_exprs: bool,
) -> Option<ChangeFlow> {
    let flow = analyze_block_returns(
        for_range_stat.get_block()?,
        returns,
        collect_all_return_exprs,
    );
    match flow {
        Some(ChangeFlow::Break) => Some(ChangeFlow::None),
        _ => flow,
    }
}

fn analyze_call_expr_stat_returns(
    call_expr_stat: LuaCallExprStat,
    returns: &mut Vec<LuaExpr>,
) -> Option<ChangeFlow> {
    let call_expr = call_expr_stat.get_call_expr()?;
    if call_expr.is_error() {
        returns.clear();
        return Some(ChangeFlow::Error);
    }
    Some(ChangeFlow::None)
}

fn analyze_return_stat_returns(
    return_stat: LuaReturnStat,
    returns: &mut Vec<LuaExpr>,
    collect_all_return_exprs: bool,
) -> Option<ChangeFlow> {
    let exprs: Vec<LuaExpr> = return_stat.get_expr_list().collect();
    if collect_all_return_exprs {
        returns.extend(exprs);
    } else if let Some(expr) = exprs.into_iter().next() {
        returns.push(expr);
    }
    Some(ChangeFlow::Return)
}
