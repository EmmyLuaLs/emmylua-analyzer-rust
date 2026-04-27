use crate::{DiagnosticCode, LocalAttribute, LuaDeclExtra, LuaDeclId, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct LocalConstReassignChecker;

impl Checker for LocalConstReassignChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::LocalConstReassign,
        DiagnosticCode::IterVariableReassign,
    ];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let file_id = semantic_model.get_file_id();
        let Some(decl_tree) = semantic_model
            .get_compilation()
            .legacy_db()
            .get_decl_index()
            .get_decl_tree(&file_id)
        else {
            return;
        };
        for (decl_id, decl) in decl_tree.get_decls() {
            if let LuaDeclExtra::Local {
                attrib: Some(attrib @ (LocalAttribute::Const | LocalAttribute::IterConst)),
                ..
            } = &decl.extra
            {
                check_local_const_reassign(context, semantic_model, decl_id, attrib);
            }
        }
    }
}

fn check_local_const_reassign(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    decl_id: &LuaDeclId,
    attrib: &LocalAttribute,
) -> Option<()> {
    let file_id = semantic_model.get_file_id();
    let refs_index = semantic_model.get_compilation().legacy_db().get_reference_index();
    let local_refs = refs_index.get_local_reference(&file_id)?;
    let decl_refs = local_refs.get_decl_references(decl_id)?;
    for decl_ref in &decl_refs.cells {
        if decl_ref.is_write {
            match attrib {
                LocalAttribute::Const => {
                    context.add_diagnostic(
                        DiagnosticCode::LocalConstReassign,
                        decl_ref.range,
                        t!("Cannot reassign to a constant variable").to_string(),
                        None,
                    );
                }
                LocalAttribute::IterConst => {
                    context.add_diagnostic(
                        DiagnosticCode::IterVariableReassign,
                        decl_ref.range,
                        t!("Should not reassign to iter variable").to_string(),
                        None,
                    );
                }
                _ => {}
            }
        }
    }

    Some(())
}
