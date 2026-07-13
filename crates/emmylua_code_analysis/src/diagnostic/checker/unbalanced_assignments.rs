//! Unbalanced assignments checker — salsa-native.

use emmylua_parser::{LuaAstNode, LuaExpr, LuaStat};

use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaType};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for stat in root.descendants::<LuaStat>() {
        match stat {
            LuaStat::LocalStat(local) => {
                let vars: Vec<_> = local.get_local_name_list().collect();
                let exprs: Vec<_> = local.get_value_exprs().collect();
                check_unbalanced(context, model, &vars, &exprs);
            }
            LuaStat::AssignStat(assign) => {
                let (vars, exprs) = assign.get_var_and_expr_list();
                let vars: Vec<_> = vars.into_iter().collect();
                let exprs: Vec<_> = exprs.into_iter().collect();
                check_unbalanced(context, model, &vars, &exprs);
            }
            _ => {}
        }
    }
}

fn check_unbalanced(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    vars: &[impl LuaAstNode],
    value_exprs: &[LuaExpr],
) {
    let Some(last_expr) = value_exprs.last() else {
        return;
    };

    // 调用表达式（如 pcall）跳过检查
    if matches!(last_expr, LuaExpr::CallExpr(_)) {
        return;
    }

    if let Ok(value_types) = model.infer_expr_list_types(value_exprs, Some(vars.len())) {
        if value_types.last().is_some_and(|(t, _)| check_last(t)) {
            return;
        }
        let value_len = value_types.len();
        if vars.len() > value_len {
            for var in &vars[value_len..] {
                context.add_diagnostic(
                    DiagnosticCode::UnbalancedAssignments,
                    var.get_range(),
                    t!(
                        "The value is assigned as `nil` because the number of values is not enough."
                    )
                    .to_string(),
                    None,
                );
            }
        }
    }
}

fn check_last(last_type: &LuaType) -> bool {
    match last_type {
        LuaType::Instance(inst) => check_last(inst.get_base()),
        _ => false,
    }
}
