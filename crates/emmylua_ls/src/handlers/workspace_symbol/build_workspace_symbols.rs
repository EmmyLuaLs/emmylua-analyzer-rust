use emmylua_code_analysis::{DeclKind, EmmyLuaAnalysis, SalsaSemanticModel};
use lsp_types::{SymbolKind, WorkspaceSymbol, WorkspaceSymbolResponse};
use tokio_util::sync::CancellationToken;

/// if query contains uppercase, do case-sensitive match; otherwise, ignore case
fn match_symbol(text: &str, query: &str) -> bool {
    if query.chars().any(|c| c.is_uppercase()) {
        text.contains(query)
    } else {
        text.to_lowercase().contains(&query.to_lowercase())
    }
}

pub fn build_workspace_symbols(
    analysis: &EmmyLuaAnalysis,
    query: String,
    cancel_token: CancellationToken,
) -> Option<WorkspaceSymbolResponse> {
    let mut symbols = Vec::new();
    let salsa = analysis.salsa_snapshot();
    for file_id in salsa.file_ids() {
        if cancel_token.is_cancelled() {
            break;
        }
        let Some(model) = SalsaSemanticModel::new(&salsa, file_id) else {
            continue;
        };
        let Some(document) = salsa.document(file_id) else {
            continue;
        };
        let Some(uri) = document.get_uri() else {
            continue;
        };
        if let Some(decls) = model.decls() {
            for decl in decls.iter() {
                if !matches!(decl.kind, DeclKind::Global) {
                    continue;
                }
                if !query.is_empty() && !match_symbol(&decl.name, &query) {
                    continue;
                }
                let Some(lsp_range) = document.to_lsp_range(decl.name_range) else {
                    continue;
                };
                symbols.push(WorkspaceSymbol {
                    name: decl.name.to_string(),
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    location: lsp_types::OneOf::Left(lsp_types::Location {
                        uri: uri.clone(),
                        range: lsp_range,
                    }),
                    container_name: None,
                    data: None,
                });
            }
        }
        if let Some(facts) = model.file_facts() {
            for def in facts.type_defs.iter() {
                if !query.is_empty() && !match_symbol(&def.full_name, &query) {
                    continue;
                }
                let Some(lsp_range) = document.to_lsp_range(def.name_range) else {
                    continue;
                };
                symbols.push(WorkspaceSymbol {
                    name: def.full_name.to_string(),
                    kind: SymbolKind::CLASS,
                    tags: None,
                    location: lsp_types::OneOf::Left(lsp_types::Location {
                        uri: uri.clone(),
                        range: lsp_range,
                    }),
                    container_name: None,
                    data: None,
                });
            }
        }
    }
    Some(WorkspaceSymbolResponse::Nested(symbols))
}
