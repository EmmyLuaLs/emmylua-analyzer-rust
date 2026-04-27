use emmylua_parser::{LuaAssignStat, LuaAst, LuaAstNode, LuaBlock, LuaVarExpr};

use crate::{DiagnosticCode, LuaDeclId, SemanticModel, resolve_global_decl_id};

use super::{Checker, DiagnosticContext};

pub struct GlobalInNonModuleChecker;

impl Checker for GlobalInNonModuleChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::GlobalInNonModule];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for assign_stat in root.descendants::<LuaAssignStat>() {
            check_assign_stat(context, semantic_model, assign_stat);
        }
    }
}

fn check_assign_stat(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    assign_stat: LuaAssignStat,
) -> Option<()> {
    let file_id = semantic_model.get_file_id();

    let (vars, _) = assign_stat.get_var_and_expr_list();
    for var in vars {
        let decl_id = LuaDeclId::new(file_id, var.get_position());
        if let Some(decl) = semantic_model.get_decl(&decl_id)
            && decl.is_global()
            && is_global_define_in_non_module_scope(semantic_model, var.clone(), decl_id)
        {
            context.add_diagnostic(
                DiagnosticCode::GlobalInNonModule,
                var.get_range(),
                t!("Global variable should only be defined in module scope").to_string(),
                None,
            );
        }
    }

    Some(())
}

fn is_global_define_in_non_module_scope(
    semantic_model: &SemanticModel,
    var: LuaVarExpr,
    decl_id: LuaDeclId,
) -> bool {
    for block in var.ancestors::<LuaBlock>() {
        let parent = block.get_parent::<LuaAst>();
        match parent {
            Some(LuaAst::LuaChunk(_)) => {
                return false;
            }
            Some(LuaAst::LuaClosureExpr(_)) => {
                break;
            }
            _ => {}
        }
    }

    let name = var.get_text();
    let Some(global_id) = resolve_global_decl_id(
        semantic_model.get_compilation().legacy_db(),
        &mut semantic_model.get_cache().borrow_mut(),
        &name,
        None,
    ) else {
        return true;
    };

    global_id == decl_id
}
