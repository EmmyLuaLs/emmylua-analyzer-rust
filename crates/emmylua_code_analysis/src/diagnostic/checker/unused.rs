//! Unused checker — salsa-native.
//!
//! 检查当前文件中声明但未被引用的变量/函数。

use rowan::TextRange;

use crate::compilation::{SalsaDeclId, SalsaDeclKindSummary};
use crate::semantic_model::SemanticModel;
use crate::DiagnosticCode;

use super::DiagnosticContext;

struct DeclInfo {
    id: SalsaDeclId,
    name: String,
    range: TextRange,
    kind: DeclKind,
}

enum DeclKind {
    Local,
    Param,
    ImplicitSelf,
    Global,
}

enum UnusedResult {
    Unused(TextRange),
    UnusedSelf(TextRange),
}

pub fn check_unused(context: &mut DiagnosticContext, model: &SemanticModel) {
    let decl_tree = match model.decl_tree() {
        Some(dt) => dt,
        None => return,
    };

    let mut decls: Vec<DeclInfo> = Vec::new();
    for decl in &decl_tree.decls {
        let kind = match &decl.kind {
            SalsaDeclKindSummary::Global => DeclKind::Global,
            SalsaDeclKindSummary::Param { .. } => DeclKind::Param,
            SalsaDeclKindSummary::ImplicitSelf => DeclKind::ImplicitSelf,
            SalsaDeclKindSummary::Local { .. } => DeclKind::Local,
        };

        decls.push(DeclInfo {
            id: decl.id,
            name: decl.name.to_string(),
            range: TextRange::new(decl.start_offset, decl.end_offset),
            kind,
        });
    }

    for decl in &decls {
        if matches!(decl.kind, DeclKind::Global) {
            continue;
        }
        if matches!(decl.kind, DeclKind::Param) && decl.name == "..." {
            continue;
        }
        if let Err(result) = check_decl_unused(model, decl) {
            if decl.name.starts_with('_') {
                continue;
            }
            match result {
                UnusedResult::Unused(range) => {
                    context.add_diagnostic(
                        DiagnosticCode::Unused,
                        range,
                        t!("%{name} is never used, if this is intentional, prefix it with an underscore: _%{name}",
                            name = decl.name
                        ).to_string(),
                        None,
                    );
                }
                UnusedResult::UnusedSelf(range) => {
                    context.add_diagnostic(
                        DiagnosticCode::Unused,
                        range,
                        t!("Implicit self is never used, if this is intentional, please use '.' instead of ':' to define the method")
                            .to_string(),
                        None,
                    );
                }
            }
        }
    }
}

fn check_decl_unused(model: &SemanticModel, decl: &DeclInfo) -> Result<(), UnusedResult> {
    let refs = model.decl_references(decl.id);
    match refs {
        Some(ref_list) if !ref_list.is_empty() => Ok(()),
        _ => {
            if matches!(decl.kind, DeclKind::ImplicitSelf) {
                Err(UnusedResult::UnusedSelf(decl.range))
            } else {
                Err(UnusedResult::Unused(decl.range))
            }
        }
    }
}
