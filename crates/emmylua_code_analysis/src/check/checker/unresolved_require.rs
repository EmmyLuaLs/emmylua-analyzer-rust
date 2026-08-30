//! # unresolved_require: require module cannot find the file
//!
//! M0: string constant module name in a require call -> `module_file_of` -> no file -> report
//! `UnresolvedRequire`. Module visibility (`RequireModuleNotVisible`) is left for later.

use emmylua_parser::{LuaAstNode, LuaCallExpr};

use crate::DiagnosticCode;
use crate::LuaType;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct UnresolvedRequireChecker;

impl Checker for UnresolvedRequireChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::UnresolvedRequire];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for call_expr in root.descendants().filter_map(LuaCallExpr::cast) {
            if !call_expr.is_require() {
                continue;
            }
            check_require(context, semantic_model, &call_expr);
        }
    }
}

fn check_require(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
) {
    let Some(args_list) = call_expr.get_args_list() else {
        return;
    };
    let Some(arg_expr) = args_list.get_args().next() else {
        return;
    };
    let ty = semantic_model.type_of_expr(arg_expr.get_syntax_id());
    let module_path = match &ty {
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => s.as_ref().to_string(),
        _ => return,
    };
    if semantic_model.require_module_type(&module_path) != LuaType::Unknown {
        return;
    }
    context.add_diagnostic(
        DiagnosticCode::UnresolvedRequire,
        arg_expr.get_range(),
        t!("Cannot resolve module `%{module}`.", module = module_path),
    );
}
