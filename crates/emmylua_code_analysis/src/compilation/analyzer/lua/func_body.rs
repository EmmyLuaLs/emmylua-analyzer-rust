use emmylua_parser::{
    LuaBlock, LuaCallExprStat, LuaDoStat, LuaExpr, LuaForRangeStat, LuaForStat, LuaIfClauseStat,
    LuaIfStat, LuaRepeatStat, LuaReturnStat, LuaStat, LuaWhileStat,
};

use crate::{InferFailReason, LuaType};

#[derive(Debug, Clone)]
pub enum LuaReturnPoint {
    Expr(LuaExpr),
    MuliExpr(Vec<LuaExpr>),
    Nil,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConditionState {
    Dynamic,
    Truthy,
    Falsy,
    Never,
}

#[derive(Debug, Default)]
struct ReturnFlow {
    return_points: Vec<LuaReturnPoint>,
    can_continue: bool,
    can_break: bool,
    // `can_stall` means control can get stuck without returning, e.g. `while true do end`.
    can_stall: bool,
}

impl ReturnFlow {
    fn continue_flow() -> Self {
        Self {
            can_continue: true,
            ..Default::default()
        }
    }

    fn merge_choice(&mut self, mut other: Self) {
        self.return_points.append(&mut other.return_points);
        self.can_continue |= other.can_continue;
        self.can_break |= other.can_break;
        self.can_stall |= other.can_stall;
    }
}

pub fn analyze_func_body_returns_with<F>(
    body: LuaBlock,
    infer_expr_type: &mut F,
) -> Result<Vec<LuaReturnPoint>, InferFailReason>
where
    F: FnMut(&LuaExpr) -> Result<LuaType, InferFailReason>,
{
    let mut flow = analyze_block_returns(body, infer_expr_type)?;
    if flow.can_continue || flow.can_break {
        flow.return_points.push(LuaReturnPoint::Nil);
    }

    Ok(flow.return_points)
}

pub fn does_func_body_always_return_or_exit<F>(
    body: LuaBlock,
    infer_expr_type: &mut F,
) -> Result<bool, InferFailReason>
where
    F: FnMut(&LuaExpr) -> Result<LuaType, InferFailReason>,
{
    let flow = analyze_block_returns(body, infer_expr_type)?;
    Ok(!flow.can_continue && !flow.can_break && !flow.can_stall)
}

fn analyze_block_returns<F>(
    block: LuaBlock,
    infer_expr_type: &mut F,
) -> Result<ReturnFlow, InferFailReason>
where
    F: FnMut(&LuaExpr) -> Result<LuaType, InferFailReason>,
{
    let mut flow = ReturnFlow::default();
    let mut can_continue = true;

    for stat in block.get_stats() {
        if !can_continue {
            break;
        }

        let stat_flow = analyze_stat_returns(stat, infer_expr_type)?;
        flow.return_points.extend(stat_flow.return_points);
        flow.can_break |= stat_flow.can_break;
        flow.can_stall |= stat_flow.can_stall;
        can_continue = stat_flow.can_continue;
    }

    flow.can_continue = can_continue;
    Ok(flow)
}

fn analyze_optional_block_returns<F>(
    block: Option<LuaBlock>,
    infer_expr_type: &mut F,
) -> Result<ReturnFlow, InferFailReason>
where
    F: FnMut(&LuaExpr) -> Result<LuaType, InferFailReason>,
{
    match block {
        Some(block) => analyze_block_returns(block, infer_expr_type),
        None => Ok(ReturnFlow::continue_flow()),
    }
}

fn analyze_stat_returns<F>(
    stat: LuaStat,
    infer_expr_type: &mut F,
) -> Result<ReturnFlow, InferFailReason>
where
    F: FnMut(&LuaExpr) -> Result<LuaType, InferFailReason>,
{
    match stat {
        LuaStat::DoStat(do_stat) => analyze_do_stat_returns(do_stat, infer_expr_type),
        LuaStat::WhileStat(while_stat) => analyze_while_stat_returns(while_stat, infer_expr_type),
        LuaStat::RepeatStat(repeat_stat) => {
            analyze_repeat_stat_returns(repeat_stat, infer_expr_type)
        }
        LuaStat::IfStat(if_stat) => analyze_if_stat_returns(if_stat, infer_expr_type),
        LuaStat::ForStat(for_stat) => analyze_for_stat_returns(for_stat, infer_expr_type),
        LuaStat::ForRangeStat(for_range_stat) => {
            analyze_for_range_stat_returns(for_range_stat, infer_expr_type)
        }
        LuaStat::CallExprStat(call_expr) => analyze_call_expr_stat_returns(call_expr),
        LuaStat::BreakStat(_) => Ok(ReturnFlow {
            can_break: true,
            ..Default::default()
        }),
        LuaStat::ReturnStat(return_stat) => Ok(analyze_return_stat_returns(return_stat)),
        _ => Ok(ReturnFlow::continue_flow()),
    }
}

fn analyze_do_stat_returns<F>(
    do_stat: LuaDoStat,
    infer_expr_type: &mut F,
) -> Result<ReturnFlow, InferFailReason>
where
    F: FnMut(&LuaExpr) -> Result<LuaType, InferFailReason>,
{
    analyze_optional_block_returns(do_stat.get_block(), infer_expr_type)
}

fn analyze_while_stat_returns<F>(
    while_stat: LuaWhileStat,
    infer_expr_type: &mut F,
) -> Result<ReturnFlow, InferFailReason>
where
    F: FnMut(&LuaExpr) -> Result<LuaType, InferFailReason>,
{
    let condition_state =
        analyze_condition_state(while_stat.get_condition_expr(), infer_expr_type)?;
    match condition_state {
        ConditionState::Falsy => Ok(ReturnFlow::continue_flow()),
        ConditionState::Never => Ok(ReturnFlow::default()),
        ConditionState::Truthy | ConditionState::Dynamic => {
            let body = analyze_optional_block_returns(while_stat.get_block(), infer_expr_type)?;
            Ok(ReturnFlow {
                return_points: body.return_points,
                can_continue: matches!(condition_state, ConditionState::Dynamic) || body.can_break,
                can_break: false,
                can_stall: body.can_continue || body.can_stall,
            })
        }
    }
}

fn analyze_repeat_stat_returns<F>(
    repeat_stat: LuaRepeatStat,
    infer_expr_type: &mut F,
) -> Result<ReturnFlow, InferFailReason>
where
    F: FnMut(&LuaExpr) -> Result<LuaType, InferFailReason>,
{
    let body = analyze_optional_block_returns(repeat_stat.get_block(), infer_expr_type)?;
    let mut flow = ReturnFlow {
        return_points: body.return_points,
        can_continue: body.can_break,
        can_break: false,
        can_stall: body.can_stall,
    };

    if !body.can_continue {
        return Ok(flow);
    }

    match analyze_condition_state(repeat_stat.get_condition_expr(), infer_expr_type)? {
        ConditionState::Truthy => {
            flow.can_continue = true;
        }
        ConditionState::Falsy => {
            flow.can_stall = true;
        }
        ConditionState::Dynamic => {
            flow.can_continue = true;
            flow.can_stall = true;
        }
        ConditionState::Never => {}
    }

    Ok(flow)
}

fn analyze_for_stat_returns<F>(
    for_stat: LuaForStat,
    infer_expr_type: &mut F,
) -> Result<ReturnFlow, InferFailReason>
where
    F: FnMut(&LuaExpr) -> Result<LuaType, InferFailReason>,
{
    let mut flow = analyze_optional_block_returns(for_stat.get_block(), infer_expr_type)?;
    flow.can_continue = true;
    flow.can_break = false;
    Ok(flow)
}

fn analyze_if_stat_returns<F>(
    if_stat: LuaIfStat,
    infer_expr_type: &mut F,
) -> Result<ReturnFlow, InferFailReason>
where
    F: FnMut(&LuaExpr) -> Result<LuaType, InferFailReason>,
{
    let mut flow = ReturnFlow::default();
    let mut can_reach_next_clause = true;

    match analyze_condition_state(if_stat.get_condition_expr(), infer_expr_type)? {
        ConditionState::Truthy => {
            return analyze_optional_block_returns(if_stat.get_block(), infer_expr_type);
        }
        ConditionState::Falsy => {}
        ConditionState::Dynamic => {
            flow.merge_choice(analyze_optional_block_returns(
                if_stat.get_block(),
                infer_expr_type,
            )?);
        }
        ConditionState::Never => {
            return Ok(flow);
        }
    }

    for clause in if_stat.get_all_clause() {
        if !can_reach_next_clause {
            break;
        }

        match clause {
            LuaIfClauseStat::ElseIf(clause) => {
                match analyze_condition_state(clause.get_condition_expr(), infer_expr_type)? {
                    ConditionState::Truthy => {
                        flow.merge_choice(analyze_optional_block_returns(
                            clause.get_block(),
                            infer_expr_type,
                        )?);
                        can_reach_next_clause = false;
                    }
                    ConditionState::Falsy => {}
                    ConditionState::Dynamic => {
                        flow.merge_choice(analyze_optional_block_returns(
                            clause.get_block(),
                            infer_expr_type,
                        )?);
                    }
                    ConditionState::Never => {
                        can_reach_next_clause = false;
                    }
                }
            }
            LuaIfClauseStat::Else(clause) => {
                flow.merge_choice(analyze_optional_block_returns(
                    clause.get_block(),
                    infer_expr_type,
                )?);
                can_reach_next_clause = false;
            }
        }
    }

    if can_reach_next_clause {
        flow.can_continue = true;
    }

    Ok(flow)
}

fn analyze_for_range_stat_returns<F>(
    for_range_stat: LuaForRangeStat,
    infer_expr_type: &mut F,
) -> Result<ReturnFlow, InferFailReason>
where
    F: FnMut(&LuaExpr) -> Result<LuaType, InferFailReason>,
{
    let mut flow = analyze_optional_block_returns(for_range_stat.get_block(), infer_expr_type)?;
    flow.can_continue = true;
    flow.can_break = false;
    Ok(flow)
}

fn analyze_call_expr_stat_returns(
    call_expr_stat: LuaCallExprStat,
) -> Result<ReturnFlow, InferFailReason> {
    let Some(call_expr) = call_expr_stat.get_call_expr() else {
        return Ok(ReturnFlow::continue_flow());
    };

    if call_expr.is_error() {
        return Ok(ReturnFlow {
            return_points: vec![LuaReturnPoint::Error],
            ..Default::default()
        });
    }

    Ok(ReturnFlow::continue_flow())
}

fn analyze_return_stat_returns(return_stat: LuaReturnStat) -> ReturnFlow {
    let exprs: Vec<LuaExpr> = return_stat.get_expr_list().collect();
    let return_point = match exprs.len() {
        0 => LuaReturnPoint::Nil,
        1 => LuaReturnPoint::Expr(exprs[0].clone()),
        _ => LuaReturnPoint::MuliExpr(exprs),
    };

    ReturnFlow {
        return_points: vec![return_point],
        ..Default::default()
    }
}

fn analyze_condition_state<F>(
    condition: Option<LuaExpr>,
    infer_expr_type: &mut F,
) -> Result<ConditionState, InferFailReason>
where
    F: FnMut(&LuaExpr) -> Result<LuaType, InferFailReason>,
{
    let Some(condition) = condition else {
        return Ok(ConditionState::Dynamic);
    };

    if !can_analyze_condition(&condition) {
        return Ok(ConditionState::Dynamic);
    }

    let condition_type = match infer_expr_type(&condition) {
        Ok(condition_type) => condition_type,
        Err(InferFailReason::None | InferFailReason::RecursiveInfer) => {
            return Ok(ConditionState::Dynamic);
        }
        Err(reason) => return Err(reason),
    };

    Ok(condition_state_from_type(&condition_type))
}

fn condition_state_from_type(condition_type: &LuaType) -> ConditionState {
    if condition_type.is_never() {
        return ConditionState::Never;
    }

    if condition_type.is_always_truthy() {
        return ConditionState::Truthy;
    }

    if condition_type.is_always_falsy() {
        return ConditionState::Falsy;
    }

    ConditionState::Dynamic
}

fn can_analyze_condition(expr: &LuaExpr) -> bool {
    match expr {
        LuaExpr::LiteralExpr(_) | LuaExpr::TableExpr(_) | LuaExpr::ClosureExpr(_) => true,
        // Return-flow analysis keeps call conditions dynamic. A call like `pred()`
        // can change after rebinding or depend on another file, so we only fold
        // self-contained expressions here.
        LuaExpr::CallExpr(_) => false,
        LuaExpr::ParenExpr(paren_expr) => paren_expr
            .get_expr()
            .is_some_and(|expr| can_analyze_condition(&expr)),
        LuaExpr::UnaryExpr(unary_expr) => unary_expr
            .get_expr()
            .is_some_and(|expr| can_analyze_condition(&expr)),
        LuaExpr::BinaryExpr(binary_expr) => binary_expr.get_exprs().is_some_and(|(left, right)| {
            can_analyze_condition(&left) && can_analyze_condition(&right)
        }),
        LuaExpr::NameExpr(_) | LuaExpr::IndexExpr(_) => false,
    }
}
