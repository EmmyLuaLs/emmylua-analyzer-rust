//! Analyze error checker — reads compilation errors from DbIndex diagnostic store.

use crate::semantic_model::SemanticModel;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, _model: &SemanticModel) {
    // let file_id = context.get_file_id();
    // let Some(diagnostics) = context.get_salsa_db().get_diagnostic_index().get_diagnostics(&file_id) else {
    //     return;
    // };
    // for diag in diagnostics {
    //     context.add_diagnostic(diag.kind, diag.range, diag.message.clone(), None);
    // }
}
