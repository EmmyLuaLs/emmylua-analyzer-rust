use emmylua_code_analysis::{DbIndex, LuaCompilation, LuaSemanticDeclId, LuaType};
use lsp_types::{OneOf, SymbolKind, SymbolTag, WorkspaceSymbol, WorkspaceSymbolResponse};
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
    compilation: &LuaCompilation,
    query: String,
    cancel_token: CancellationToken,
) -> Option<WorkspaceSymbolResponse> {
    let mut symbols = Vec::new();
    add_global_variable_symbols(&mut symbols, compilation, &query, &cancel_token)?;
    add_type_symbols(&mut symbols, compilation, &query, &cancel_token)?;
    Some(WorkspaceSymbolResponse::Nested(symbols))
}

fn add_global_variable_symbols(
    symbols: &mut Vec<WorkspaceSymbol>,
    compilation: &LuaCompilation,
    query: &str,
    cancel_token: &CancellationToken,
) -> Option<()> {
    if cancel_token.is_cancelled() {
        return None;
    }

    let db = compilation.get_db();
    let global_index = db.get_global_index();
    let global_decl_ids = global_index.get_all_global_decl_ids();
    for decl_id in global_decl_ids {
        let decl = db.get_decl_index().get_decl(&decl_id)?;
        if cancel_token.is_cancelled() {
            return None;
        }

        if match_symbol(decl.get_name(), query) {
            let typ = db
                .get_type_index()
                .get_type_cache(&decl_id.into())
                .map(|cache| cache.as_type())
                .unwrap_or(&LuaType::Unknown);
            let property_owner_id = LuaSemanticDeclId::LuaDecl(decl_id);
            let document = db.get_vfs().get_document(&decl.get_file_id())?;
            let location = document.to_lsp_location(decl.get_range())?;
            let symbol = WorkspaceSymbol {
                name: decl.get_name().to_string(),
                kind: get_symbol_kind(typ),
                tags: if is_deprecated(db, property_owner_id) {
                    Some(vec![SymbolTag::DEPRECATED])
                } else {
                    None
                },
                container_name: None,
                location: OneOf::Left(location),
                data: None,
            };
            symbols.push(symbol);
        }
    }

    Some(())
}

fn add_type_symbols(
    symbols: &mut Vec<WorkspaceSymbol>,
    compilation: &LuaCompilation,
    query: &str,
    cancel_token: &CancellationToken,
) -> Option<()> {
    if cancel_token.is_cancelled() {
        return None;
    }

    let db = compilation.get_db();
    let decl_index = db.get_type_index();
    let types = decl_index.get_all_types();
    for typ in types {
        if cancel_token.is_cancelled() {
            return None;
        }

        if match_symbol(typ.get_full_name(), query) {
            let property_owner_id = LuaSemanticDeclId::TypeDecl(typ.get_id());
            let location = typ.get_locations().first()?;
            let document = db.get_vfs().get_document(&location.file_id)?;
            let location = document.to_lsp_location(location.range)?;
            let symbol = WorkspaceSymbol {
                name: typ.get_full_name().to_string(),
                kind: SymbolKind::CLASS,
                tags: if is_deprecated(db, property_owner_id) {
                    Some(vec![SymbolTag::DEPRECATED])
                } else {
                    None
                },
                container_name: None,
                location: OneOf::Left(location),
                data: None,
            };
            symbols.push(symbol);
        }
    }

    Some(())
}

fn get_symbol_kind(typ: &LuaType) -> SymbolKind {
    if typ.is_function() {
        return SymbolKind::FUNCTION;
    } else if typ.is_const() {
        return SymbolKind::CONSTANT;
    } else if typ.is_def() {
        return SymbolKind::CLASS;
    }

    SymbolKind::VARIABLE
}

fn is_deprecated(db: &DbIndex, id: LuaSemanticDeclId) -> bool {
    let property = db.get_property_index().get_property(&id);
    property.is_some_and(|prop| prop.deprecated().is_some())
}
