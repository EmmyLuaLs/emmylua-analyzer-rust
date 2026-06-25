mod call_facts;
mod param_count;
mod type_mismatch;

use std::collections::HashSet;

use emmylua_parser::{LuaAst, LuaAstNode};
use rowan::TextRange;

use crate::{DiagnosticCode, SemanticModel};

use super::{Checker, DiagnosticContext};
use call_facts::CallFacts;

pub(super) type ParamCountDiagnosticRanges = HashSet<TextRange>;

pub struct ParamCheckChecker;

impl Checker for ParamCheckChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::ParamTypeMismatch,
        DiagnosticCode::AssignTypeMismatch,
        DiagnosticCode::MissingParameter,
        DiagnosticCode::RedundantParameter,
    ];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let check_param_count = context
            .is_checker_enable_by_code(&DiagnosticCode::MissingParameter)
            || context.is_checker_enable_by_code(&DiagnosticCode::RedundantParameter);
        let check_param_type = context
            .is_checker_enable_by_code(&DiagnosticCode::ParamTypeMismatch)
            || context.is_checker_enable_by_code(&DiagnosticCode::AssignTypeMismatch);

        let root = semantic_model.get_root().clone();
        for node in root.descendants::<LuaAst>() {
            match node {
                LuaAst::LuaCallExpr(call_expr) if check_param_count || check_param_type => {
                    let Some(facts) = CallFacts::new(semantic_model, call_expr) else {
                        continue;
                    };

                    let mut param_count_diagnostic_ranges = ParamCountDiagnosticRanges::new();
                    let count_compatible_funcs = if check_param_count {
                        Some(param_count::check_call_param_count(
                            context,
                            semantic_model,
                            &facts,
                            &mut param_count_diagnostic_ranges,
                        ))
                    } else {
                        None
                    };

                    if should_check_param_type(check_param_type, &param_count_diagnostic_ranges) {
                        let candidates = match count_compatible_funcs.as_ref() {
                            Some(funcs) => funcs.as_slice(),
                            None => facts.funcs(),
                        };
                        type_mismatch::check_param_types(
                            context,
                            semantic_model,
                            &facts,
                            candidates,
                        );
                    }
                }
                LuaAst::LuaClosureExpr(closure_expr) if check_param_count => {
                    param_count::check_closure_param_count(context, semantic_model, &closure_expr);
                }
                _ => {}
            }
        }
    }
}

fn should_check_param_type(
    check_param_type: bool,
    param_count_diagnostic_ranges: &ParamCountDiagnosticRanges,
) -> bool {
    check_param_type && param_count_diagnostic_ranges.is_empty()
}
