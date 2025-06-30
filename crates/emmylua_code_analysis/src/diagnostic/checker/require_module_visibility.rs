use emmylua_parser::{LuaAstNode, LuaCallExpr};

use crate::{check_export_visibility, DiagnosticCode, LuaType, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct RequireModuleVisibilityChecker;

impl Checker for RequireModuleVisibilityChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::RequireModuleNotVisible];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for call_expr in root.descendants::<LuaCallExpr>() {
            if call_expr.is_require() {
                check_require_call_expr(context, semantic_model, call_expr);
            }
        }
    }
}

fn check_require_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let args_list = call_expr.get_args_list()?;
    let arg_expr = args_list.get_args().next()?;

    // 获取模块路径
    let ty = semantic_model
        .infer_expr(arg_expr.clone())
        .unwrap_or(LuaType::Any);
    let module_path = match ty {
        LuaType::StringConst(s) => s.as_ref().to_string(),
        _ => return Some(()),
    };

    // 查找模块信息
    let module_info = semantic_model
        .get_db()
        .get_module_index()
        .find_module(&module_path)?;

    // 检查可见性
    if !check_export_visibility(semantic_model, module_info).unwrap_or(false) {
        context.add_diagnostic(
            DiagnosticCode::RequireModuleNotVisible,
            arg_expr.get_range(),
            t!(
                "Module '%{module}' is not visible. It has @export restrictions.",
                module = module_info.full_module_name
            )
            .to_string(),
            None,
        );
    }

    Some(())
}
