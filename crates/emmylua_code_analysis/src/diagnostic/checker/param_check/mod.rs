mod call_facts;
mod param_count;
mod type_mismatch;

use emmylua_parser::{LuaAst, LuaAstNode};

use crate::{DiagnosticCode, SemanticModel};

use super::{Checker, DiagnosticContext};
use call_facts::CallFacts;

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

                    if check_param_count {
                        param_count::check_call_param_count(context, semantic_model, &facts);
                    }

                    if check_param_type {
                        type_mismatch::check_param_types(
                            context,
                            semantic_model,
                            &facts,
                            check_param_count,
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
