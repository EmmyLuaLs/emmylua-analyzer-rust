use emmylua_parser::{LuaExpr, LuaIndexExpr, LuaIndexMemberExpr};

use crate::{
    DbIndex, InferFailReason, LuaInferCache,
    semantic::infer::{
        VarRefId,
        narrow::{
            condition_flow::{ConditionFlowAction, InferConditionFlow, PendingConditionNarrow},
            var_ref_id::get_var_expr_var_ref_id,
        },
    },
};

pub fn get_type_at_index_expr(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    var_ref_id: &VarRefId,
    index_expr: LuaIndexExpr,
    condition_flow: InferConditionFlow,
) -> Result<ConditionFlowAction, InferFailReason> {
    let Some(name_var_ref_id) =
        get_var_expr_var_ref_id(db, cache, LuaExpr::IndexExpr(index_expr.clone()))
    else {
        return Ok(ConditionFlowAction::Continue);
    };

    if name_var_ref_id == *var_ref_id {
        return Ok(ConditionFlowAction::Pending(
            PendingConditionNarrow::Truthiness(condition_flow),
        ));
    }

    let Some(prefix_expr) = index_expr.get_prefix_expr() else {
        return Ok(ConditionFlowAction::Continue);
    };

    let Some(maybe_var_ref_id) = get_var_expr_var_ref_id(db, cache, prefix_expr.clone()) else {
        // If we cannot find a reference declaration ID, we cannot narrow it
        return Ok(ConditionFlowAction::Continue);
    };

    if maybe_var_ref_id != *var_ref_id {
        return Ok(ConditionFlowAction::Continue);
    }

    Ok(ConditionFlowAction::Pending(
        PendingConditionNarrow::FieldTruthy {
            idx: LuaIndexMemberExpr::IndexExpr(index_expr),
            condition_flow,
        },
    ))
}
