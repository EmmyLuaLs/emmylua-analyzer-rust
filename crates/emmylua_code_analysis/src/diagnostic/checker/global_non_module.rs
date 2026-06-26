//! Global in non-module checker — pure salsa.

use emmylua_parser::{LuaAssignStat, LuaAst, LuaAstNode, LuaBlock, LuaVarExpr};

use crate::DiagnosticCode;
use crate::compilation::SalsaDeclKindSummary;
use crate::semantic_model::SemanticModel;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for assign in root.descendants::<LuaAssignStat>() {
        check_assign(context, model, assign);
    }
}

fn check_assign(context: &mut DiagnosticContext, model: &SemanticModel, assign: LuaAssignStat) {
    let _file_id = model.get_file_id();
    let (vars, _) = assign.get_var_and_expr_list();
    let Some(decl_tree) = model.decl_tree() else {
        return;
    };

    for var in vars {
        // Check if this var is in a non-module scope
        let in_module_scope = is_in_module_scope(&var);
        if in_module_scope {
            continue;
        }

        // Find matching declaration
        let decl = decl_tree
            .decls
            .iter()
            .find(|d| d.name.as_str() == var.get_text().as_str());
        let Some(decl) = decl else { continue };
        if matches!(decl.kind, SalsaDeclKindSummary::Global) {
            context.add_diagnostic(
                DiagnosticCode::GlobalInNonModule,
                var.get_range(),
                t!("Global variable should only be defined in module scope").to_string(),
                None,
            );
        }
    }
}

fn is_in_module_scope(var: &LuaVarExpr) -> bool {
    for block in var.ancestors::<LuaBlock>() {
        match block.get_parent::<LuaAst>() {
            Some(LuaAst::LuaChunk(_)) => return true,
            Some(LuaAst::LuaClosureExpr(_)) => break,
            _ => {}
        }
    }
    false
}
