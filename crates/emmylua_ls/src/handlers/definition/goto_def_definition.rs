//! # goto_def_definition — pure-salsa declaration lookup
//!
//! `SemanticId::Decl` → declaration name; `Member` → definition site + same-key members (prefix type /
//! table field's table and declaration type); `TypeDef` → all definition positions.
//! The old DbIndex implementation (overload matching / accessor properties / attribute source lookup)
//! is retired; see `docs/SALSA_FROM_SCRATCH.md` §M3.

use emmylua_code_analysis::{
    Emmyrc, LuaMemberKey, SalsaDatabase, SalsaMemberInfo, SalsaSemanticModel, SemanticId,
    WorkspaceId,
};
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaComment, LuaDocDescription, LuaExpr, LuaIndexExpr, LuaLocalStat,
    LuaNameExpr, LuaSyntaxToken, LuaTableExpr, LuaTableField, LuaTokenKind,
};
use lsp_types::{GotoDefinitionResponse, Location};
use rowan::TextSize;

use crate::handlers::common::resolve_alias_origin;
use crate::util::parse_desc;

pub fn goto_def_definition(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    decl: &SemanticId,
    token: &LuaSyntaxToken,
) -> Option<GotoDefinitionResponse> {
    match decl {
        SemanticId::Decl(key) => {
            // Value alias: `local f = t.func` / `local a = b` jump to the real function/member definition.
            if let Some(origin) = resolve_alias_origin(model, decl)
                && origin != *decl
            {
                return goto_def_definition(model, salsa, &origin, token);
            }
            location_of(salsa, key.file_id, key.name_range).map(GotoDefinitionResponse::Scalar)
        }
        SemanticId::Member(_) => goto_member_definition(model, salsa, decl, token),
        SemanticId::TypeDef(key) => {
            let mut locations = Vec::new();
            for def in model.type_defs_in_scope(key.scope, &key.full_name) {
                if let Some(location) = location_of(salsa, def.file_id, def.name_range) {
                    locations.push(location);
                }
            }
            (!locations.is_empty()).then_some(GotoDefinitionResponse::Array(locations))
        }
        _ => None,
    }
}

fn goto_member_definition(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    decl: &SemanticId,
    token: &LuaSyntaxToken,
) -> Option<GotoDefinitionResponse> {
    let mut locations = Vec::new();
    let parent = token.parent();
    let is_usage = parent
        .as_ref()
        .is_some_and(|p| LuaIndexExpr::can_cast(p.kind().into()));

    // 1. Same-key members: usage site → prefix's **declared type**; definition site (table field) → outer declaration's declared type.
    let key = member_key_of(model, decl)?;
    let mut prefix_types = Vec::new();
    if let Some(index_expr) = parent.as_ref().and_then(|p| LuaIndexExpr::cast(p.clone()))
        && let Some(prefix) = index_expr.get_prefix_expr()
    {
        let prefix_decl_type = if let LuaExpr::NameExpr(name_expr) = &prefix {
            model
                .resolve_name(name_expr.get_position())
                .and_then(|decl_id| model.type_of_decl(&decl_id))
        } else {
            None
        };
        match prefix_decl_type {
            // `---@type T` annotation takes priority (mirrors the old `t:func()` behavior of locating only class @field).
            Some(ty) => prefix_types.push(ty),
            None => prefix_types.push(model.type_of_expr(prefix.get_syntax_id())),
        }
    } else if let Some(table_field) = parent.as_ref().and_then(|p| LuaTableField::cast(p.clone()))
        && let Some(table) = table_field.get_parent::<LuaTableExpr>()
    {
        // `---@type T local t = { ... }`: @field member of declared type T.
        if let Some(local_name) = table
            .get_parent::<LuaLocalStat>()
            .and_then(|stat| stat.get_local_name_list().next())
            && let Some(name_token) = local_name.get_name_token()
            && let Some(decl_id) = model.decl_by_offset(name_token.get_position())
            && let Some(ty) = model.type_of_decl(&decl_id)
        {
            prefix_types.push(ty);
        }
    }

    for prefix_type in &prefix_types {
        let infos = model.member_infos_with_key_all(&prefix_type, &key);
        let infos = filter_overload_infos(model, token, &infos);
        for info in infos {
            if let Some(id) = info.id
                && let Some(range) = id.member_key_range()
                && let SemanticId::Member(member_key) = &id
                && let Some(location) = location_of(salsa, member_key.file_id, range)
                && !locations.contains(&location)
            {
                locations.push(location);
            }
        }
    }
    // Field accessor: fields with `---@[field_accessor]` also jump to get/set methods.
    add_field_accessor_locations(model, salsa, &prefix_types, &key, &mut locations);
    // Usage site: return on type-level member match (old semantics no longer mix in runtime table members).
    if is_usage && !locations.is_empty() {
        return Some(GotoDefinitionResponse::Array(locations));
    }

    // 2. Member definition site (runtime member / table field definition).
    if let SemanticId::Member(key) = decl
        && let Some(location) = location_of(salsa, key.file_id, key.key_range)
        && !locations.contains(&location)
    {
        locations.push(location);
    }

    (!locations.is_empty()).then_some(GotoDefinitionResponse::Array(locations))
}

fn filter_overload_infos(
    model: &SalsaSemanticModel<'_>,
    token: &LuaSyntaxToken,
    infos: &[SalsaMemberInfo],
) -> Vec<SalsaMemberInfo> {
    let Some(call) = token.parent().and_then(|parent| {
        parent
            .ancestors()
            .find_map(emmylua_parser::LuaCallExpr::cast)
    }) else {
        return infos.to_vec();
    };
    let Some(args) = call.get_args_list() else {
        return infos.to_vec();
    };
    let args = args.get_args().collect::<Vec<_>>();
    if args.is_empty() {
        return infos.to_vec();
    }
    let actual_types = args
        .iter()
        .map(|arg| model.type_of_expr(arg.get_syntax_id()))
        .collect::<Vec<_>>();
    // With actual arguments, only locate "doc @field overload groups": in declaration order, take groups
    // from the first up to the first overload matching the argument types; actual method implementations (`is_method`) are not part of doc-overload display.
    let doc_infos = infos
        .iter()
        .filter(|info| {
            !info.is_method && matches!(info.typ, emmylua_code_analysis::LuaType::DocFunction(_))
        })
        .cloned()
        .collect::<Vec<_>>();
    let match_index = doc_infos.iter().position(|info| {
        let emmylua_code_analysis::LuaType::DocFunction(func) = &info.typ else {
            return false;
        };
        let params = func.get_params();
        params
            .iter()
            .zip(actual_types.iter())
            .all(|((_, param), actual)| {
                param
                    .as_ref()
                    .is_none_or(|param| model.type_check(actual, param))
            })
    });
    match match_index {
        Some(index) => doc_infos.into_iter().take(index + 1).collect(),
        None => doc_infos,
    }
}

/// Member declaration key lookup (passes through directly after normalizing to `LuaMemberKey`).
fn member_key_of(model: &SalsaSemanticModel<'_>, decl: &SemanticId) -> Option<LuaMemberKey> {
    let SemanticId::Member(key) = decl else {
        return None;
    };
    let member_model = model.model_for(key.file_id)?;
    let member = member_model.members()?.iter().find(|m| &m.id == decl)?;
    Some(member.key.clone())
}

fn add_field_accessor_locations(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    prefix_types: &[emmylua_code_analysis::LuaType],
    key: &LuaMemberKey,
    locations: &mut Vec<Location>,
) {
    let LuaMemberKey::Name(field) = key else {
        return;
    };
    let field = field.as_str();
    let mut chars = field.chars();
    let Some(first) = chars.next() else {
        return;
    };
    let rest = chars.as_str();
    let capitalized = format!("{}{}", first.to_uppercase(), rest);
    for method_name in [format!("get{}", capitalized), format!("set{}", capitalized)] {
        let method_key = LuaMemberKey::Name(method_name.clone().into());
        for prefix_type in prefix_types {
            let type_id = match prefix_type {
                emmylua_code_analysis::LuaType::Ref(id)
                | emmylua_code_analysis::LuaType::Def(id) => Some(id),
                emmylua_code_analysis::LuaType::Generic(generic) => {
                    Some(generic.get_base_type_id_ref())
                }
                _ => None,
            };
            let Some(type_id) = type_id else {
                continue;
            };
            let Some(def) = model.type_def_of(type_id) else {
                continue;
            };
            if !type_has_field_accessor(model, &def) {
                continue;
            }
            for info in model.member_infos_with_key_all(prefix_type, &method_key) {
                if let Some(id) = info.id
                    && let Some(range) = id.member_key_range()
                    && let Some(file_id) = info.file_id
                    && let Some(location) = location_of(salsa, file_id, range)
                    && !locations.contains(&location)
                {
                    locations.push(location);
                }
            }
        }
    }
}

fn type_has_field_accessor(
    model: &SalsaSemanticModel<'_>,
    def: &emmylua_code_analysis::TypeDef,
) -> bool {
    let Some(owner_syntax) = def.owner_syntax else {
        return false;
    };
    let owner_range = owner_syntax.get_range();
    let Some(def_model) = model.model_for(def.file_id) else {
        return false;
    };
    let Some(chunk) = def_model.chunk() else {
        return false;
    };
    chunk.descendants::<LuaComment>().any(|comment| {
        comment
            .syntax()
            .text()
            .to_string()
            .contains("field_accessor")
            && comment.get_owner().is_some_and(|owner| {
                let owner_range_of_comment = owner.syntax().text_range();
                owner_range_of_comment.contains_range(owner_range)
                    || owner_range_of_comment.start() == owner_range.start()
            })
    })
}

fn location_of(
    salsa: &SalsaDatabase,
    file_id: emmylua_code_analysis::FileId,
    range: rowan::TextRange,
) -> Option<Location> {
    let document = salsa.document(file_id)?;
    let uri = document.get_uri()?;
    Some(Location {
        uri,
        range: document.to_lsp_range(range)?,
    })
}

/// Goto definition for doc description references (`:lua:obj:` / `{lua:obj}` / `--- @see`).
pub(super) fn goto_doc_definition(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    token: &LuaSyntaxToken,
    offset: TextSize,
    emmyrc: &Emmyrc,
) -> Option<GotoDefinitionResponse> {
    let is_see = token.kind() == LuaTokenKind::TkDocSeeContent.into();
    let description = if is_see {
        None
    } else {
        Some(
            token
                .parent()
                .and_then(|parent| parent.ancestors().find_map(LuaDocDescription::cast))?,
        )
    };
    let comment = if is_see {
        token
            .parent()
            .and_then(|parent| parent.ancestors().find_map(LuaComment::cast))
    } else {
        description.as_ref()?.get_parent::<LuaComment>()
    };
    let scope_types = comment
        .as_ref()
        .map(|comment| comment_scope_types(model, comment))
        .unwrap_or_default();

    let (names, cursor_index) = if token.kind() == LuaTokenKind::TkDocSeeContent.into() {
        let text = token.text().trim().to_string();
        if text.is_empty() {
            return None;
        }
        let names = text
            .split('.')
            .map(|part| part.to_string())
            .collect::<Vec<_>>();
        let cursor_index = names.len().saturating_sub(1);
        (names, cursor_index)
    } else {
        let document = salsa.document(model.file_id())?;
        let workspace_id = salsa
            .workspace_id_of(model.file_id())
            .unwrap_or(WorkspaceId::MAIN);
        let description = description?;
        let items = parse_desc(
            workspace_id,
            emmyrc,
            document.get_text(),
            description,
            Some(offset.into()),
        );
        let ref_range = items
            .iter()
            .find(|item| {
                item.kind == emmylua_parser_desc::DescItemKind::Ref
                    && item.range.contains_inclusive(offset)
            })?
            .range;
        if ref_range.is_empty() {
            return None;
        }
        let path = emmylua_parser_desc::parse_ref_target(document.get_text(), ref_range, offset)?;
        let names = path
            .iter()
            .map(|(item, _)| item.get_name().map(str::to_string))
            .collect::<Option<Vec<_>>>()?;
        let cursor_index = path
            .iter()
            .position(|(_, range)| range.contains(offset))
            .unwrap_or(names.len().saturating_sub(1));
        (names, cursor_index)
    };

    resolve_doc_path(model, salsa, &names, cursor_index, &scope_types)
}

fn resolve_doc_path(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    names: &[String],
    cursor_index: usize,
    scope_types: &[emmylua_code_analysis::LuaType],
) -> Option<GotoDefinitionResponse> {
    let full_name = names.join(".");
    if let Some(def) = model.resolve_type_def(&full_name) {
        return Some(GotoDefinitionResponse::Scalar(location_of(
            salsa,
            def.file_id,
            def.name_range,
        )?));
    }

    if cursor_index > 0 {
        let prefix = names[..cursor_index].join(".");
        if let Some(def) = model.resolve_type_def(&prefix) {
            let ty = model.type_def_ref(&def);
            if let Some(response) =
                member_definition_response(model, salsa, &[(&ty, &names[cursor_index])])
            {
                return Some(response);
            }
        }
        if model.module_file_of(&prefix).is_some() {
            let module_ty = model.require_module_type(&prefix);
            let infos = model.member_infos_with_key_all(
                &module_ty,
                &LuaMemberKey::Name(names[cursor_index].clone().into()),
            );
            if let Some(response) = infos_to_locations(salsa, &infos) {
                return Some(response);
            }
        }
    }

    let name = names.get(cursor_index)?;
    if cursor_index == 0 {
        if let Some(def) = model.resolve_type_def(name) {
            return Some(GotoDefinitionResponse::Scalar(location_of(
                salsa,
                def.file_id,
                def.name_range,
            )?));
        }
        let pairs = scope_types
            .iter()
            .map(|ty| (ty, name.as_str()))
            .collect::<Vec<_>>();
        if let Some(response) = member_definition_response(model, salsa, &pairs) {
            return Some(response);
        }
        // Bare names in doc references also often point to fields on the current file's types (`@class Z` + `c`).
        if let Some(facts) = model.file_facts()
            && let Some(response) = facts.type_defs.iter().find_map(|def| {
                let ty = model.type_def_ref(def);
                member_definition_response(model, salsa, &[(&ty, name.as_str())])
            })
        {
            return Some(response);
        }
    }
    None
}

fn member_definition_response(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    pairs: &[(&emmylua_code_analysis::LuaType, &str)],
) -> Option<GotoDefinitionResponse> {
    let mut infos = Vec::new();
    for (ty, name) in pairs {
        let key = LuaMemberKey::Name((*name).to_string().into());
        for info in model.member_infos_with_key_all(ty, &key) {
            if !infos.iter().any(|existing: &SalsaMemberInfo| {
                existing.id == info.id && existing.file_id == info.file_id
            }) {
                infos.push(info);
            }
        }
    }
    infos_to_locations(salsa, &infos)
}

/// Usable types involved in the comment's owner (mirrors the scoped member source used by desc completion).
fn comment_scope_types(
    model: &SalsaSemanticModel<'_>,
    comment: &LuaComment,
) -> Vec<emmylua_code_analysis::LuaType> {
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
                (!matches!(ty, emmylua_code_analysis::LuaType::Unknown)).then_some(ty)
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

fn infos_to_locations(
    salsa: &SalsaDatabase,
    infos: &[SalsaMemberInfo],
) -> Option<GotoDefinitionResponse> {
    let mut locations = Vec::new();
    for info in infos {
        if let Some(id) = &info.id
            && let Some(range) = id.member_key_range()
            && let Some(file_id) = info.file_id
            && let Some(location) = location_of(salsa, file_id, range)
            && !locations.contains(&location)
        {
            locations.push(location);
        }
    }
    (!locations.is_empty()).then_some(GotoDefinitionResponse::Array(locations))
}
