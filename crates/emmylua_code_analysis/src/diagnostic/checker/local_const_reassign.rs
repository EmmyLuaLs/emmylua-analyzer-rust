//! Local const reassign checker — salsa-native.

use crate::DiagnosticCode;
use crate::compilation::{
    SalsaDeclKindSummary, SalsaLocalAttributeSummary, SalsaUseSiteRoleSummary,
};
use crate::semantic_model::SemanticModel;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let Some(decl_tree) = model.decl_tree() else {
        return;
    };
    for decl in &decl_tree.decls {
        let attrib = match &decl.kind {
            SalsaDeclKindSummary::Local { attrib: Some(a) } => a,
            _ => continue,
        };
        let code = match attrib {
            SalsaLocalAttributeSummary::Const => DiagnosticCode::LocalConstReassign,
            SalsaLocalAttributeSummary::IterConst => DiagnosticCode::IterVariableReassign,
            _ => continue,
        };

        let Some(refs) = model.decl_references(decl.id) else {
            continue;
        };
        for r in &refs {
            if r.role == SalsaUseSiteRoleSummary::Write {
                let message = match code {
                    DiagnosticCode::LocalConstReassign => {
                        t!("Cannot reassign to a constant variable").to_string()
                    }
                    DiagnosticCode::IterVariableReassign => {
                        t!("Should not reassign to iter variable").to_string()
                    }
                    _ => continue,
                };
                context.add_diagnostic(code, r.syntax_id.text_range(), message, None);
            }
        }
    }
}
