use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaLocalStat};

use crate::{
    DiagnosticCode, SemanticModel,
    diagnostic::checker::{Checker, DiagnosticContext},
};

pub struct NonLiteralExpressionsInAssertChecker;

impl Checker for NonLiteralExpressionsInAssertChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::NonLiteralExpressionsInAssert];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for call_expr in root.descendants::<LuaCallExpr>() {
            if call_expr.is_assert() {
                check_assert_rule(context, semantic_model, call_expr);
            }
        }
    }
}

fn check_assert_rule(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    // only check local a = assert(b, msg)
    call_expr.get_parent::<LuaLocalStat>()?;

    let args = call_expr.get_args_list()?;
    let arg_exprs = args.get_args().collect::<Vec<_>>();
    if arg_exprs.len() > 1 {
        let second_expr = &arg_exprs[1];
        match second_expr {
            LuaExpr::LiteralExpr(_) | LuaExpr::IndexExpr(_) => {
                return Some(());
            }
            LuaExpr::NameExpr(name_expr) => {
                let name = name_expr.get_name_text()?;
                let decl_tree = semantic_model
                    .get_compilation()
                    .legacy_db()
                    .get_decl_index()
                    .get_decl_tree(&semantic_model.get_file_id())?;
                if let Some(decl) = decl_tree.find_local_decl(&name, name_expr.get_position())
                    && decl.is_local()
                {
                    return Some(());
                }
            }
            _ => {}
        }

        let range = second_expr.get_range();
        context.add_diagnostic(
            DiagnosticCode::NonLiteralExpressionsInAssert,
            range,
            t!("codestyle.NonLiteralExpressionsInAssert").to_string(),
            None,
        );
    }

    Some(())
}
