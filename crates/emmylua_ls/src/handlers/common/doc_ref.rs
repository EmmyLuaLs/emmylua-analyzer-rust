//! Shared resolution for symbol references inside doc comments.
//!
//! Both goto-definition and `textDocument/documentLink` need to turn a doc
//! reference path (`Widget`, `Widget.name`, `module.fn`, ...) into source
//! locations.

use emmylua_code_analysis::{
    FileId, LuaMemberKey, LuaType, MemberCollectMode, MemberInfo, SemanticDatabase, SemanticId,
    SemanticModel,
};
use emmylua_parser::{LuaAstNode, LuaComment, LuaNameExpr};
use lsp_types::{Location, Position, Range};
use rowan::TextRange;

pub(crate) fn location_of(
    db: &SemanticDatabase,
    file_id: FileId,
    range: TextRange,
) -> Option<Location> {
    let document = db.document(file_id)?;
    let uri = document.get_uri()?;
    Some(Location {
        uri,
        range: document.to_lsp_range(range)?,
    })
}

/// Location at the start of a file, used for module links.
fn file_location_of(db: &SemanticDatabase, file_id: FileId) -> Option<Location> {
    let document = db.document(file_id)?;
    Some(Location {
        uri: document.get_uri()?,
        range: Range::new(Position::new(0, 0), Position::new(0, 0)),
    })
}

/// Declaration/member location for a semantic id.
fn semantic_id_location(
    model: &SemanticModel<'_>,
    db: &SemanticDatabase,
    id: &SemanticId,
) -> Option<Location> {
    let file_id = match id {
        SemanticId::Decl(key) => key.file_id,
        SemanticId::Member(key) => key.file_id,
        _ => return None,
    };
    let model = model.model_for(file_id);
    let facts = model.file_facts()?;
    match id {
        SemanticId::Decl(_) => {
            let decl = facts.decl_by_id(id)?;
            location_of(db, file_id, decl.name_range)
        }
        SemanticId::Member(_) => {
            let member = facts.member_by_id(id)?;
            location_of(db, file_id, member.id.member_key_range()?)
        }
        _ => None,
    }
}

/// Usable types involved in the comment's owner (mirrors the scoped member
/// source used by desc completion).
pub(crate) fn comment_scope_types(model: &SemanticModel<'_>, comment: &LuaComment) -> Vec<LuaType> {
    let Some(owner) = comment.get_owner() else {
        return Vec::new();
    };
    let mut types = Vec::new();
    for name in owner.syntax().descendants().filter_map(LuaNameExpr::cast) {
        let ty = model
            .resolve_name(name.get_position())
            .and_then(|decl| model.type_of_decl(&decl))
            .or_else(|| {
                let ty = model.type_of_expr(name.get_syntax_id());
                (!matches!(ty, LuaType::Unknown)).then_some(ty)
            });
        if let Some(ty) = ty {
            types.push(ty);
        }
        if let Some(name_text) = name.get_name_text()
            && let Some(def) = model.resolve_type_def(&name_text)
        {
            types.push(model.type_def_ref(&def));
        }
    }
    types
}

/// Resolves a dotted doc-reference path to the source locations it points at.
///
/// `cursor_index` is the path segment under the cursor; it decides whether a
/// bare name refers to a type, a member of a scoped type, or an enclosing
/// module.
pub(crate) fn resolve_doc_path_locations(
    model: &SemanticModel<'_>,
    db: &SemanticDatabase,
    names: &[String],
    cursor_index: usize,
    scope_types: &[LuaType],
) -> Option<Vec<Location>> {
    let full_name = names.join(".");
    if let Some(def) = model.resolve_type_def(&full_name) {
        return location_of(db, def.file_id, def.name_range).map(|location| vec![location]);
    }

    if cursor_index > 0 {
        let prefix = names[..cursor_index].join(".");
        if let Some(def) = model.resolve_type_def(&prefix) {
            let ty = model.type_def_ref(&def);
            if let Some(locations) = member_locations(model, db, &[(&ty, &names[cursor_index])]) {
                return Some(locations);
            }
        }
        if model.module_file_of(&prefix).is_some() {
            let module_ty = model.require_module_type(&prefix);
            let infos = model.member_infos_with_key_mode(
                &module_ty,
                &LuaMemberKey::Name(names[cursor_index].clone().into()),
                MemberCollectMode::AllDefinitions,
            );
            if let Some(locations) = infos_to_locations(db, &infos) {
                return Some(locations);
            }
        }
        if let Some(global) = model.global_decl(&prefix)
            && let Some(ty) = model.type_of_decl(&global)
            && let Some(locations) = member_locations(model, db, &[(&ty, &names[cursor_index])])
        {
            return Some(locations);
        }
    }

    let name = names.get(cursor_index)?;
    if cursor_index == 0 {
        if let Some(def) = model.resolve_type_def(name) {
            return location_of(db, def.file_id, def.name_range).map(|location| vec![location]);
        }
        if let Some(file_id) = model.module_file_of(name) {
            return file_location_of(db, file_id).map(|location| vec![location]);
        }
        if let Some(global) = model.global_decl(name) {
            return semantic_id_location(model, db, &global).map(|location| vec![location]);
        }
        let pairs = scope_types
            .iter()
            .map(|ty| (ty, name.as_str()))
            .collect::<Vec<_>>();
        if let Some(locations) = member_locations(model, db, &pairs) {
            return Some(locations);
        }
        // Bare names in doc references also often point to fields on the current
        // file's types (`@class Z` + `c`).
        if let Some(facts) = model.file_facts() {
            for def in &facts.type_defs {
                let ty = model.type_def_ref(def);
                if let Some(locations) = member_locations(model, db, &[(&ty, name.as_str())]) {
                    return Some(locations);
                }
            }
        }
    }
    None
}

fn member_locations(
    model: &SemanticModel<'_>,
    db: &SemanticDatabase,
    pairs: &[(&LuaType, &str)],
) -> Option<Vec<Location>> {
    let mut infos = Vec::new();
    for (ty, name) in pairs {
        let key = LuaMemberKey::Name((*name).to_string().into());
        for info in model.member_infos_with_key_mode(ty, &key, MemberCollectMode::AllDefinitions) {
            if !infos.iter().any(|existing: &MemberInfo| {
                existing.id == info.id && existing.file_id == info.file_id
            }) {
                infos.push(info);
            }
        }
    }
    infos_to_locations(db, &infos)
}

fn infos_to_locations(db: &SemanticDatabase, infos: &[MemberInfo]) -> Option<Vec<Location>> {
    let mut locations = Vec::new();
    for info in infos {
        if let Some(id) = &info.id
            && let Some(range) = id.member_key_range()
            && let Some(file_id) = info.file_id
            && let Some(location) = location_of(db, file_id, range)
            && !locations.contains(&location)
        {
            locations.push(location);
        }
    }
    (!locations.is_empty()).then_some(locations)
}

/// Extracts a doc-reference path from a comment's `@see` content without
/// parsing markup.
pub(crate) fn see_path_names(text: &str) -> Option<(Vec<String>, usize)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let names = text
        .split('.')
        .map(|part| part.to_string())
        .collect::<Vec<_>>();
    let cursor_index = names.len().saturating_sub(1);
    Some((names, cursor_index))
}
