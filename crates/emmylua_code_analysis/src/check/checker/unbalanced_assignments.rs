//! unbalanced_assignments: not enough values for `local a, b = 1` / `a, b = 1`.

use emmylua_parser::{LuaAstNode, LuaExpr, LuaStat};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct UnbalancedAssignmentsChecker;

impl Checker for UnbalancedAssignmentsChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::UnbalancedAssignments];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(chunk) = semantic_model.chunk() else {
            return;
        };
        for stat in chunk.descendants::<LuaStat>() {
            match stat {
                LuaStat::LocalStat(local) => {
                    let vars = local.get_local_name_list().collect::<Vec<_>>();
                    let values = local.get_value_exprs().collect::<Vec<_>>();
                    check_unbalanced(context, &vars, &values);
                }
                LuaStat::AssignStat(assign) => {
                    let (vars, values) = assign.get_var_and_expr_list();
                    check_unbalanced(context, &vars, &values);
                }
                _ => {}
            }
        }
    }
}

fn check_unbalanced<T: LuaAstNode>(
    context: &mut CheckContext<'_>,
    vars: &[T],
    value_exprs: &[LuaExpr],
) {
    // No values (`local a, b`) is valid; if the last value is a call (may return multiple values), skip in M0.
    let Some(last) = value_exprs.last() else {
        return;
    };
    if matches!(last, LuaExpr::CallExpr(_)) {
        return;
    }
    let value_len = value_exprs.len();
    if vars.len() > value_len {
        for var in &vars[value_len..] {
            context.add_diagnostic(
                DiagnosticCode::UnbalancedAssignments,
                var.get_range(),
                t!("The value is assigned as `nil` because the number of values is not enough."),
            );
        }
    }
}
