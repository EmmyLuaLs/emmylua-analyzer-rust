//! # reference_searcher — pure Salsa reference search
//!
//! The legacy DbIndex version (module export alias following / ctor references / fuzzy / string references)
//! was moved to `handlers/common/legacy_references.rs` for not-yet-migrated handlers (M3 batch 3+),
//! and will be deleted after the compilation layer is retired.

use std::collections::HashSet;

use emmylua_code_analysis::{FileId, SalsaDatabase, SalsaSemanticModel, SemanticId, TypeDef};
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallExpr, LuaIndexExpr, LuaLiteralToken, LuaSyntaxToken,
};
use lsp_types::Location;

use crate::handlers::common::{
    decl_reference_ranges, label_reference_ranges, member_reference_ranges,
    type_def_reference_ranges,
};

/// Entry point: token position → list of reference positions.
pub fn search_references(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    token: LuaSyntaxToken,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    // 1. Label references (same-file pure syntax).
    if let Some(label_ranges) = label_reference_ranges(model, &token, include_declaration) {
        return Some(
            label_ranges
                .into_iter()
                .filter_map(|range| location_of(salsa, model.file_id(), range))
                .collect(),
        );
    }

    // 2. Semantic declaration references (decl / member / typedef).
    if let Some(decl) = model.find_decl(token.into()) {
        let ranges = match &decl {
            SemanticId::Decl(_) => {
                decl_reference_ranges_with_aliases(salsa, &decl, include_declaration)
            }
            SemanticId::Member(_) => member_reference_ranges(salsa, &decl, include_declaration),
            SemanticId::TypeDef(_) => match resolve_type_def_of_id(model, &decl) {
                Some(def) => type_def_reference_ranges(salsa, &def, include_declaration),
                None => Vec::new(),
            },
            _ => Vec::new(),
        };
        return Some(
            ranges
                .into_iter()
                .filter_map(|(file_id, range)| location_of(salsa, file_id, range))
                .collect(),
        );
    }

    // 3. String reference index and fuzzy search retired with the old reference index (no Salsa equivalent).
    Some(Vec::new())
}

/// Type definition identity → TypeDef (after `find_decl` returns `SemanticId::TypeDef`, fetch the definition details).
fn resolve_type_def_of_id(model: &SalsaSemanticModel<'_>, id: &SemanticId) -> Option<TypeDef> {
    let SemanticId::TypeDef(key) = id else {
        return None;
    };
    model
        .type_defs_in_scope(key.scope, &key.full_name)
        .into_iter()
        .find(|def| def.id == *id)
}

/// `require("mod").field` → member identity in the module file facts.
fn require_index_member(
    model: &SalsaSemanticModel<'_>,
    index_expr: &LuaIndexExpr,
) -> Option<SemanticId> {
    let prefix = index_expr.get_prefix_expr()?;
    let call = match prefix {
        emmylua_parser::LuaExpr::CallExpr(call) => call,
        _ => return None,
    };
    if !call.is_require() {
        return None;
    }
    let arg = call.get_args_list()?.get_args().next()?;
    let literal = arg
        .syntax()
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
        .find_map(LuaLiteralToken::cast)?;
    let module_name = match literal {
        LuaLiteralToken::String(string) => string.get_value(),
        _ => return None,
    };
    let key = index_expr.get_index_key()?.get_path_part();
    let module_file = model.module_file_of(&module_name)?;
    let facts = model.file_facts_of(module_file)?;
    facts
        .members
        .iter()
        .find(|member| member.key.to_path() == key)
        .map(|member| member.id.clone())
}

/// Reference ranges for require module path strings (the legacy reference index also counted the
/// `"mod"` in `require("mod").field` as a member alias reference).
fn require_module_literal_ranges(
    salsa: &SalsaDatabase,
    member: &SemanticId,
) -> Vec<(FileId, rowan::TextRange)> {
    let mut out = Vec::new();
    for file_id in salsa.file_ids() {
        let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
            continue;
        };
        let Some(chunk) = model.chunk() else {
            continue;
        };
        for index_expr in chunk.descendants::<LuaIndexExpr>() {
            if require_index_member(&model, &index_expr).as_ref() != Some(member) {
                continue;
            }
            let Some(prefix) = index_expr.get_prefix_expr() else {
                continue;
            };
            let emmylua_parser::LuaExpr::CallExpr(call) = prefix else {
                continue;
            };
            let Some(arg) = call.get_args_list().and_then(|list| list.get_args().next()) else {
                continue;
            };
            push_unique(&mut out, (file_id, arg.get_range()));
        }
    }
    out
}

/// Member reference ranges + `require("mod").member` usage sites (`resolve_member` does not yet give
/// alias identities for require-module exported members; here they are filled in by module file + member key).
fn member_reference_ranges_with_require(
    salsa: &SalsaDatabase,
    member: &SemanticId,
    include_declaration: bool,
) -> Vec<(FileId, rowan::TextRange)> {
    let mut out = member_reference_ranges(salsa, member, include_declaration);
    for file_id in salsa.file_ids() {
        let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
            continue;
        };
        let Some(chunk) = model.chunk() else {
            continue;
        };
        for index_expr in chunk.descendants::<LuaIndexExpr>() {
            if require_index_member(&model, &index_expr).as_ref() == Some(member)
                && let Some(key) = index_expr.get_index_key()
                && let Some(range) = key.get_range()
            {
                push_unique(&mut out, (file_id, range));
            }
        }
    }
    out
}

/// Declaration references + module export alias chains:
/// Declarations in `local function flush() ...; return { flush = flush }` are included, and cross-file
/// usage sites like `local f = require("mod").flush; f()` must also be counted.
fn decl_reference_ranges_with_aliases(
    salsa: &SalsaDatabase,
    decl: &SemanticId,
    include_declaration: bool,
) -> Vec<(FileId, rowan::TextRange)> {
    let mut out = decl_reference_ranges(salsa, decl, include_declaration);
    let mut visited_members: HashSet<SemanticId> = HashSet::new();
    let mut visited_decls: HashSet<SemanticId> = HashSet::new();
    let mut member_work = alias_members_of_decl(salsa, decl);
    let mut decl_work = Vec::new();
    let mut module_alias_work = module_alias_decls_of_decl(salsa, decl);

    while !member_work.is_empty() || !decl_work.is_empty() || !module_alias_work.is_empty() {
        for (module_decl, literal_ranges) in module_alias_work.drain(..) {
            if visited_decls.insert(module_decl.clone()) {
                out.extend(decl_reference_ranges(
                    salsa,
                    &module_decl,
                    include_declaration,
                ));
            }
            out.extend(literal_ranges);
            member_work.extend(alias_members_of_decl(salsa, &module_decl));
        }
        for member in member_work.drain(..) {
            if !visited_members.insert(member.clone()) {
                continue;
            }
            out.extend(member_reference_ranges_with_require(
                salsa,
                &member,
                include_declaration,
            ));
            out.extend(require_module_literal_ranges(salsa, &member));
            decl_work.extend(decls_aliased_to_member(salsa, &member));
        }
        for alias_decl in decl_work.drain(..) {
            if !visited_decls.insert(alias_decl.clone()) {
                continue;
            }
            out.extend(decl_reference_ranges(
                salsa,
                &alias_decl,
                include_declaration,
            ));
            member_work.extend(alias_members_of_decl(salsa, &alias_decl));
        }
    }

    out.sort_by_key(|(file_id, range)| {
        (file_id.id, u32::from(range.start()), u32::from(range.end()))
    });
    out.dedup();
    out
}

/// `return init` module export: other files alias the declaration to the target via `local f = require("mod")`.
fn module_alias_decls_of_decl(
    salsa: &SalsaDatabase,
    decl: &SemanticId,
) -> Vec<(SemanticId, Vec<(FileId, rowan::TextRange)>)> {
    let mut out = Vec::new();
    let mut module_files = Vec::new();
    for file_id in salsa.file_ids() {
        let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
            continue;
        };
        let Some(exports) = model.file_exports(file_id) else {
            continue;
        };
        if matches!(
            &exports.module,
            Some(emmylua_code_analysis::ModuleExport::Decl {
                decl: exported,
                ..
            }) if exported == decl
        ) {
            module_files.push(file_id);
        }
    }

    for module_file in module_files {
        let Some(module_name) = salsa.module_name_of(module_file) else {
            continue;
        };
        for file_id in salsa.file_ids() {
            let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
                continue;
            };
            let Some(decls) = model.decls() else {
                continue;
            };
            for alias_decl in decls.iter() {
                let Some(value_syntax) = alias_decl.value_expr_syntax else {
                    continue;
                };
                let Some(require_name) = require_module_name_at(&model, value_syntax) else {
                    continue;
                };
                if require_name == module_name {
                    out.push((alias_decl.id.clone(), Vec::new()));
                }
            }
        }
    }
    out
}

fn require_module_name_at(
    model: &SalsaSemanticModel<'_>,
    call_syntax: emmylua_parser::LuaSyntaxId,
) -> Option<String> {
    let chunk = model.chunk()?;
    let call = chunk
        .descendants::<LuaCallExpr>()
        .find(|call| call.get_syntax_id() == call_syntax)?;
    if !call.is_require() {
        return None;
    }
    let arg = call.get_args_list()?.get_args().next()?;
    let literal = arg
        .syntax()
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
        .find_map(LuaLiteralToken::cast)?;
    match literal {
        LuaLiteralToken::String(string) => Some(string.get_value()),
        _ => None,
    }
}

/// Which member aliases reference a declaration (the `flush` member in `export.flush = flush`).
fn alias_members_of_decl(salsa: &SalsaDatabase, decl: &SemanticId) -> Vec<SemanticId> {
    let mut out = Vec::new();
    for file_id in salsa.file_ids() {
        let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
            continue;
        };
        let Some(members) = model.members() else {
            continue;
        };
        for member in members.iter() {
            let Some(value_syntax) = member.value_syntax else {
                continue;
            };
            let resolved = model.resolve_name(value_syntax.get_range().start());
            if resolved == Some(decl.clone()) {
                out.push(member.id.clone());
            }
        }
    }
    out
}

/// Which declarations are initialized from a member alias (the `f` in `local f = require("mod").flush`).
fn decls_aliased_to_member(salsa: &SalsaDatabase, member: &SemanticId) -> Vec<SemanticId> {
    let mut out = Vec::new();
    for file_id in salsa.file_ids() {
        let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
            continue;
        };
        let Some(decls) = model.decls() else {
            continue;
        };
        let Some(chunk) = model.chunk() else {
            continue;
        };
        for decl in decls.iter() {
            let Some(value_syntax) = decl.value_expr_syntax else {
                continue;
            };
            for index_expr in chunk.descendants::<LuaIndexExpr>() {
                if index_expr.get_syntax_id() != value_syntax {
                    continue;
                }
                let resolved = model
                    .resolve_member(&index_expr)
                    .and_then(|resolved| resolved.member_id)
                    .or_else(|| require_index_member(&model, &index_expr));
                if resolved.as_ref() == Some(member) {
                    out.push(decl.id.clone());
                }
                break;
            }
        }
    }
    out
}

fn push_unique(out: &mut Vec<(FileId, rowan::TextRange)>, item: (FileId, rowan::TextRange)) {
    if !out.contains(&item) {
        out.push(item);
    }
}

fn location_of(
    salsa: &SalsaDatabase,
    file_id: FileId,
    range: rowan::TextRange,
) -> Option<Location> {
    let document = salsa.document(file_id)?;
    let uri = document.get_uri()?;
    Some(Location {
        uri,
        range: document.to_lsp_range(range)?,
    })
}
