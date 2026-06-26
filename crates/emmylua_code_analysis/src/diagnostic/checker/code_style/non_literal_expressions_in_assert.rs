//! Non-literal expressions in assert — hybrid.

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaLocalStat};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for call in root.descendants::<LuaCallExpr>() {
        if call.is_assert() {
            check_assert(context, model, call);
        }
    }
}

fn check_assert(context: &mut DiagnosticContext, model: &SemanticModel, call: LuaCallExpr) {
    let Some(_) = call.get_parent::<LuaLocalStat>() else {
        return;
    };
    let Some(args) = call.get_args_list() else {
        return;
    };
    let args: Vec<_> = args.get_args().collect();
    if args.len() < 2 {
        return;
    }
    let second = &args[1];
    match second {
        LuaExpr::LiteralExpr(_) | LuaExpr::IndexExpr(_) => return,
        LuaExpr::NameExpr(name) => {
            let Some(name_text) = name.get_name_text() else {
                return;
            };
            let Some(decl_tree) = model.decl_tree() else {
                return;
            };
            let Some(decl) = decl_tree
                .decls
                .iter()
                .find(|d| d.name.as_str() == name_text)
            else {
                return;
            };
            if matches!(
                decl.kind,
                crate::compilation::SalsaDeclKindSummary::Local { .. }
            ) {
                return;
            }
        }
        _ => {}
    }
    context.add_diagnostic(
        DiagnosticCode::NonLiteralExpressionsInAssert,
        second.get_range(),
        t!("codestyle.NonLiteralExpressionsInAssert").to_string(),
        None,
    );
}
