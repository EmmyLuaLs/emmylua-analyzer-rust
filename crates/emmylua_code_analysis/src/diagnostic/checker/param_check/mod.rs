mod call_analysis;
mod param_count;
mod param_type_mismatch;

use emmylua_parser::{LuaAst, LuaAstNode};

use crate::{DiagnosticCode, SemanticModel};

use super::{Checker, DiagnosticContext};
use call_analysis::CallAnalysis;

pub struct ParamCheckChecker;

impl Checker for ParamCheckChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::ParamTypeMismatch,
        DiagnosticCode::AssignTypeMismatch,
        DiagnosticCode::MissingParameter,
        DiagnosticCode::RedundantParameter,
    ];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let missing_enabled = context.is_checker_enable_by_code(&DiagnosticCode::MissingParameter);
        let redundant_enabled =
            context.is_checker_enable_by_code(&DiagnosticCode::RedundantParameter);
        let type_enabled = context.is_checker_enable_by_code(&DiagnosticCode::ParamTypeMismatch)
            || context.is_checker_enable_by_code(&DiagnosticCode::AssignTypeMismatch);
        let call_check_enabled = missing_enabled || redundant_enabled || type_enabled;

        let root = semantic_model.get_root().clone();
        for node in root.descendants::<LuaAst>() {
            match node {
                LuaAst::LuaCallExpr(call_expr) if call_check_enabled => {
                    let Some(call) = CallAnalysis::analyze(semantic_model, call_expr) else {
                        continue;
                    };
                    let arity = param_count::analyze_call_arity(semantic_model, &call);
                    let compatible_candidates = arity.compatible_candidates.as_slice();
                    let arity_diagnostic_reported = if arity.count_is_known
                        && compatible_candidates.is_empty()
                        && (missing_enabled || redundant_enabled)
                    {
                        param_count::add_call_arity_diagnostic(
                            context,
                            semantic_model,
                            &call,
                            &arity,
                            missing_enabled,
                            redundant_enabled,
                        )
                    } else {
                        false
                    };

                    if !type_enabled || arity_diagnostic_reported {
                        continue;
                    }
                    // 候选索引
                    let candidate_indices = if compatible_candidates.is_empty() {
                        None
                    } else {
                        Some(compatible_candidates)
                    };
                    param_type_mismatch::check_param_type_mismatch(
                        context,
                        semantic_model,
                        &call,
                        candidate_indices,
                    );
                }
                LuaAst::LuaClosureExpr(closure_expr) if redundant_enabled => {
                    param_count::check_closure_param_count(context, semantic_model, &closure_expr);
                }
                _ => {}
            }
        }
    }
}
