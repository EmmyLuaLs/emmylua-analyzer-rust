use std::collections::HashMap;
use std::sync::Arc;

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaTableExpr, LuaTableField};

use crate::salsa_builder::def::{DeclKind, MemberRef, SemanticId, TypeDef, TypeScope};
use crate::semantic_model::infer::unify::{self, TplBindings};
use crate::{
    FileId, GenericTplId, LuaMemberKey, LuaType, LuaTypeDeclId, LuaTypeIdentifier, WorkspaceId,
};
use crate::{Member, salsa_builder::facts::FileFacts};
use smol_str::SmolStr;

use super::SemanticModel;

/// Maximum inheritance/parent-type depth expanded during member lookup.
///
/// Normal class hierarchies are shallow. This cap prevents pathological/recursive parent
/// chains from making `member_info` / `resolve_member` slow-path expansion unbounded.
const MAX_MEMBER_INHERITANCE_DEPTH: usize = 16;

/// Member information (an entry from a prefix-type query).
#[derive(Debug, Clone, PartialEq)]
pub struct MemberInfo {
    /// Member key.
    pub key: LuaMemberKey,
    /// Member type (after projection / generic substitution).
    pub typ: LuaType,
    /// Declaration identity (`SemanticId::Member`; `None` for synthesized members).
    pub id: Option<SemanticId>,
    /// Declaring file.
    pub file_id: Option<FileId>,
    /// `:` method definition.
    pub is_method: bool,
}

/// All members of a prefix type (completion candidates; deduplicated by key, with this class's `@field` before parent/runtime values).
pub fn member_infos(model: &SemanticModel, prefix_type: &LuaType) -> Vec<MemberInfo> {
    let mut out = Vec::new();
    let mut visited = Vec::new();
    collect_members(model, prefix_type, None, None, &mut visited, &mut out);
    dedup_by_key(out)
}

/// Member with the given key of a prefix type (first match).
pub fn member_info(
    model: &SemanticModel,
    prefix_type: &LuaType,
    key: &LuaMemberKey,
) -> Option<MemberInfo> {
    member_info_impl(model, prefix_type, key)
}

fn member_ref_matches_key(
    model: &SemanticModel,
    member_ref: &MemberRef,
    key: &LuaMemberKey,
) -> bool {
    let Some(facts) = model.file_facts_of(member_ref.file_id) else {
        return false;
    };
    facts
        .member_by_id(&member_ref.id)
        .is_some_and(|member| &member.key == key)
}

fn find_member_in_owner(
    model: &SemanticModel,
    owner: &SemanticId,
    key: &LuaMemberKey,
    bindings: Option<&TplBindings>,
    name_bindings: Option<&HashMap<String, LuaType>>,
) -> Option<MemberInfo> {
    if let Some(name) = key.name() {
        for member_ref in model.members_of_owner_named(owner, name) {
            if member_ref_matches_key(model, &member_ref, key) {
                if let Some(info) = member_info_of(model, &member_ref, bindings, name_bindings) {
                    return Some(info);
                }
            }
        }
        return None;
    }
    for member_ref in model.members_of_owner(owner) {
        if member_ref_matches_key(model, &member_ref, key) {
            if let Some(info) = member_info_of(model, &member_ref, bindings, name_bindings) {
                return Some(info);
            }
        }
    }
    None
}

fn find_type_def_member(
    model: &SemanticModel,
    def: &TypeDef,
    key: &LuaMemberKey,
    bindings: Option<&TplBindings>,
    name_bindings: Option<&HashMap<String, LuaType>>,
    visited: &mut Vec<SemanticId>,
) -> Option<MemberInfo> {
    if visited.contains(&def.id) || visited.len() >= MAX_MEMBER_INHERITANCE_DEPTH {
        return None;
    }
    visited.push(def.id.clone());

    if let Some(info) = find_member_in_owner(model, &def.id, key, bindings, name_bindings) {
        visited.pop();
        return Some(info);
    }

    for super_name in &def.super_names {
        if let Some(bindings) = bindings {
            let mut found_super = false;
            for (index, param) in def.generic_params.iter().enumerate() {
                let param_name = param.name.as_str();
                if super_name.as_str() == param_name
                    || super_name.as_str().starts_with(&format!("{param_name}<"))
                {
                    if let Some(bound) = bindings.get(&GenericTplId::Type(index as u32)) {
                        if let Some(info) = member_info_impl(model, bound, key) {
                            visited.pop();
                            return Some(info);
                        }
                        found_super = true;
                    }
                    break;
                }
            }
            if found_super {
                continue;
            }
        }
        if let Some(super_def) = model.resolve_type_def_in(def.file_id, super_name.as_str()) {
            if let Some(info) =
                find_type_def_member(model, &super_def, key, bindings, name_bindings, visited)
            {
                visited.pop();
                return Some(info);
            }
        }
    }

    for owner in model.q().resolve_owner_set(def.id.clone()) {
        if owner == def.id {
            continue;
        }
        if let Some(info) = find_member_in_owner(model, &owner, key, bindings, name_bindings) {
            visited.pop();
            return Some(info);
        }
    }

    visited.pop();
    None
}

fn direct_member_info(
    model: &SemanticModel,
    prefix_type: &LuaType,
    key: &LuaMemberKey,
) -> Option<MemberInfo> {
    match prefix_type {
        LuaType::Ref(id) | LuaType::Def(id) => {
            let def = type_def_of(model, id)?;
            let mut visited = Vec::new();
            // Bare generic type (`Box`) uses its declared defaults/constraints for member lookup.
            let mut bindings = TplBindings::new();
            let mut name_bindings = HashMap::new();
            let mut all_bound = true;
            for (index, param) in def.generic_params.iter().enumerate() {
                let ty = param
                    .default
                    .map(|syntax| model.doc_type_lua_in(def.file_id, syntax, &def.generic_params))
                    .or_else(|| {
                        param.constraint.map(|syntax| {
                            model.doc_type_lua_in(def.file_id, syntax, &def.generic_params)
                        })
                    });
                if let Some(ty) = ty {
                    bindings.insert(GenericTplId::Type(index as u32), ty.clone());
                    name_bindings.insert(param.name.to_string(), ty);
                } else {
                    all_bound = false;
                    break;
                }
            }
            if all_bound && !bindings.is_empty() {
                find_type_def_member(
                    model,
                    &def,
                    key,
                    Some(&bindings),
                    Some(&name_bindings),
                    &mut visited,
                )
            } else {
                find_type_def_member(model, &def, key, None, None, &mut visited)
            }
        }
        LuaType::Generic(generic) => {
            let def = type_def_of(model, &generic.get_base_type_id())?;
            let bindings: TplBindings = generic
                .get_params()
                .iter()
                .enumerate()
                .map(|(index, ty)| (GenericTplId::Type(index as u32), ty.clone()))
                .collect();
            let name_map: HashMap<String, LuaType> = def
                .generic_params
                .iter()
                .enumerate()
                .filter_map(|(index, param)| {
                    generic
                        .get_params()
                        .get(index)
                        .map(|value| (param.name.to_string(), value.clone()))
                })
                .collect();
            let mut visited = Vec::new();
            find_type_def_member(
                model,
                &def,
                key,
                Some(&bindings),
                Some(&name_map),
                &mut visited,
            )
        }
        LuaType::TableConst(table) => {
            let owner = SemanticId::member(table.file_id, table.value);
            if let Some(info) = find_member_in_owner(model, &owner, key, None, None) {
                return Some(info);
            }
            if let Some(facts) = model.file_facts_of(table.file_id) {
                for decl in &facts.decls {
                    if decl
                        .value_expr_syntax
                        .is_some_and(|syntax| syntax.get_range() == table.value)
                    {
                        let mut owners = vec![decl.id.clone()];
                        if matches!(decl.kind, DeclKind::Global) {
                            owners.push(SemanticId::name(decl.name.clone()));
                        }
                        for owner in owners {
                            if let Some(info) = find_member_in_owner(model, &owner, key, None, None)
                            {
                                return Some(info);
                            }
                        }
                    }
                }
            }
            None
        }
        LuaType::Object(object) => object.get_fields().get(key).cloned().map(|typ| MemberInfo {
            key: key.clone(),
            typ,
            id: None,
            file_id: None,
            is_method: false,
        }),
        LuaType::Array(array) => direct_member_info(model, array.get_base(), key),
        LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_) => {
            let string_owner = SemanticId::name(SmolStr::new("string"));
            find_member_in_owner(model, &string_owner, key, None, None)
        }
        _ => None,
    }
}

pub(crate) fn member_info_impl(
    model: &SemanticModel,
    prefix_type: &LuaType,
    key: &LuaMemberKey,
) -> Option<MemberInfo> {
    // Single-key direct lookup avoids collecting every member of a type when only one key is needed.
    if let Some(info) = direct_member_info(model, prefix_type, key) {
        return Some(info);
    }
    // Intersection members: same-key members from all components are merged; conflicting types (`number & string`) collapse to `never`.
    if let LuaType::Intersection(intersection) = prefix_type {
        let mut types = Vec::new();
        let mut first = None;
        for component in intersection.get_types() {
            if let Some(info) = member_info(model, component, key) {
                if !types.contains(&info.typ) {
                    types.push(info.typ.clone());
                }
                if first.is_none() {
                    first = Some(info);
                }
            }
        }
        if types.is_empty() {
            return None;
        }
        let typ = if types.len() == 1 {
            types.pop()?
        } else {
            LuaType::Never
        };
        return Some(MemberInfo {
            key: key.clone(),
            typ,
            id: first.as_ref().and_then(|info| info.id.clone()),
            file_id: first.as_ref().and_then(|info| info.file_id),
            is_method: first.as_ref().is_some_and(|info| info.is_method),
        });
    }
    if let Some(info) = member_infos(model, prefix_type)
        .into_iter()
        .find(|info| &info.key == key)
    {
        return Some(info);
    }
    // An anonymous table literal can supply missing members through the
    // `__index` function in `setmetatable(t, { __index = function ... end })`:
    // Lua passes the access key to that function, and its return type is the member type.
    if let LuaType::TableConst(table) = prefix_type
        && let Some(index_info) = table_metatable_index_info(model, table)
        && let Some(ret) = match &index_info.typ {
            LuaType::DocFunction(fun) => Some(fun.get_ret().clone()),
            LuaType::Function | LuaType::Signature(_) => None,
            _ => None,
        }
    {
        return Some(MemberInfo {
            key: key.clone(),
            typ: ret,
            id: None,
            file_id: None,
            is_method: false,
        });
    }
    None
}

/// Members with the given key of a prefix type (all matches, overload scenario).
pub fn member_infos_with_key(
    model: &SemanticModel,
    prefix_type: &LuaType,
    key: &LuaMemberKey,
) -> Vec<MemberInfo> {
    member_infos(model, prefix_type)
        .into_iter()
        .filter(|info| &info.key == key)
        .collect()
}

/// Members with the given key of a prefix type (all matches, no dedup; duplicate `@field` lines are kept as overloads).
pub(crate) fn member_infos_with_key_all(
    model: &SemanticModel,
    prefix_type: &LuaType,
    key: &LuaMemberKey,
) -> Vec<MemberInfo> {
    let mut out = Vec::new();
    let mut visited = Vec::new();
    collect_members(model, prefix_type, None, None, &mut visited, &mut out);
    out.into_iter().filter(|info| &info.key == key).collect()
}

/// Member type for a prefix type + key (old `infer_member_type`).
pub fn member_type(
    model: &SemanticModel,
    prefix_type: &LuaType,
    key: &LuaMemberKey,
) -> Option<LuaType> {
    member_info(model, prefix_type, key).map(|info| info.typ)
}

/// Returns the second argument type of `setmetatable(t, mt)` for the given `t` table identity.
/// Supports a table literal as the first argument, and also a local name indirection (`local t = {...}; setmetatable(t, mt)`).
pub(crate) fn table_metatable_type(
    model: &SemanticModel,
    table: &crate::InFiled<rowan::TextRange>,
) -> Option<LuaType> {
    // Cross-file: a diagnostic/consumer model's `type_of_expr` may return Unknown for expressions in the defining file,
    // so delegate to the model for the file containing the table.
    if model.file_id() != table.file_id {
        let foreign = model.model_for(table.file_id)?;
        return table_metatable_type(&foreign, table);
    }
    let facts = model.file_facts_of(table.file_id)?;
    let tree = model.syntax_tree_of(table.file_id)?;
    let root = tree.get_red_root();
    for decl in &facts.decls {
        let value_syntax = decl.value_expr_syntax?;
        let Some(node) = value_syntax.to_node_from_root(&root) else {
            continue;
        };
        let Some(call) = LuaCallExpr::cast(node) else {
            continue;
        };
        let Some(prefix) = call.get_prefix_expr() else {
            continue;
        };
        let is_setmetatable = match prefix {
            LuaExpr::NameExpr(name_expr) => {
                name_expr.get_name_text().as_deref() == Some("setmetatable")
            }
            LuaExpr::IndexExpr(index_expr) => index_expr
                .get_index_name_token()
                .is_some_and(|name| name.text() == "setmetatable"),
            _ => false,
        };
        if !is_setmetatable {
            continue;
        }
        let mut args = call.get_args_list()?.get_args();
        let Some(first) = args.next() else {
            continue;
        };
        let first_matches = match first {
            LuaExpr::TableExpr(table_expr) => table_expr.get_range() == table.value,
            LuaExpr::NameExpr(_) => matches!(
                model.type_of_expr(first.get_syntax_id()),
                LuaType::TableConst(ft) if ft.file_id == table.file_id && ft.value == table.value
            ),
            _ => false,
        };
        if !first_matches {
            continue;
        }
        let metatable_expr = args.next()?;
        return Some(model.type_of_expr(metatable_expr.get_syntax_id()));
    }
    None
}

/// If a table literal is the runtime class table for a `---@class Foo` annotation, returns that class's Ref/Def.
pub(crate) fn table_const_class_type(
    model: &SemanticModel,
    table: &crate::InFiled<rowan::TextRange>,
) -> Option<LuaType> {
    if model.file_id() != table.file_id {
        let foreign = model.model_for(table.file_id)?;
        return table_const_class_type(&foreign, table);
    }
    let facts = model.file_facts_of(table.file_id)?;
    let decl = facts.decls.iter().find(|decl| {
        decl.value_expr_syntax
            .is_some_and(|syntax| syntax.get_range() == table.value)
    })?;
    let owner_syntax = decl.owner_syntax?;
    let def = facts
        .type_defs
        .iter()
        .find(|def| def.owner_syntax == Some(owner_syntax))?;
    Some(model.type_def_ref(def))
}

/// Returns the `__index` member info on the metatable corresponding to a table literal.
pub(crate) fn table_metatable_index_info(
    model: &SemanticModel,
    table: &crate::InFiled<rowan::TextRange>,
) -> Option<MemberInfo> {
    let metatable_ty = table_metatable_type(model, table)?;
    let key = LuaMemberKey::Name(SmolStr::new("__index"));
    member_info(model, &metatable_ty, &key)
}

/// Finds a member with a `self` return_cast by method name.
/// Cross-file supported: iterates files in the workspace and aggregates owners from type definitions/local declarations.
pub fn find_self_return_cast_member(model: &SemanticModel, name: &str) -> Option<SemanticId> {
    let mut seen = std::collections::HashSet::new();
    for file_id in model.db().file_ids() {
        let Some(facts) = model.file_facts_of(file_id) else {
            continue;
        };
        let mut owners: Vec<SemanticId> = Vec::new();
        for def in &facts.type_defs {
            owners.push(def.id.clone());
            for decl in &facts.decls {
                if decl.owner_syntax == def.owner_syntax {
                    owners.push(decl.id.clone());
                }
            }
        }
        for decl in &facts.decls {
            owners.push(decl.id.clone());
        }
        for owner in owners {
            if !seen.insert(owner.clone()) {
                continue;
            }
            for member in model.members_of_owner(&owner) {
                if member.name.as_str() != name {
                    continue;
                }
                let Some(member_facts) = model.file_facts_of(member.file_id) else {
                    continue;
                };
                let Some(member_def) = member_facts.member_by_id(&member.id) else {
                    continue;
                };
                let Some(value_syntax) = member_def.value_syntax else {
                    continue;
                };
                let Some(signature) = member_facts.signature_by_closure(value_syntax) else {
                    continue;
                };
                let Some(docs) = signature.docs.as_ref() else {
                    continue;
                };
                if docs
                    .return_cast
                    .as_ref()
                    .is_some_and(|cast| cast.name == "self")
                {
                    return Some(member.id.clone());
                }
            }
        }
    }
    None
}

fn collect_members(
    model: &SemanticModel,
    prefix_type: &LuaType,
    bindings: Option<&TplBindings>,
    name_bindings: Option<&HashMap<String, LuaType>>,
    visited: &mut Vec<SemanticId>,
    out: &mut Vec<MemberInfo>,
) {
    match prefix_type {
        // Named type: @field + inheritance + runtime value members.
        LuaType::Ref(decl_id) | LuaType::Def(decl_id) => {
            if let Some(def) = type_def_of(model, decl_id) {
                // Bare generic types (`Box`) fill in default generic arguments on member access,
                // so inherited fields from `Box<T = string>`'s `Parent<T>` resolve to `string`.
                if bindings.is_none() && !def.generic_params.is_empty() {
                    let mut default_bindings = TplBindings::new();
                    let mut all_defaults = true;
                    for (index, param) in def.generic_params.iter().enumerate() {
                        if let Some(default_syntax) = param.default {
                            let ty = model.doc_type_lua_in(
                                def.file_id,
                                default_syntax,
                                &def.generic_params,
                            );
                            if matches!(ty, LuaType::Unknown) {
                                all_defaults = false;
                                break;
                            }
                            default_bindings.insert(GenericTplId::Type(index as u32), ty);
                        } else if let Some(constraint) = param.constraint {
                            let ty =
                                model.doc_type_lua_in(def.file_id, constraint, &def.generic_params);
                            if matches!(ty, LuaType::Unknown) {
                                all_defaults = false;
                                break;
                            }
                            default_bindings.insert(GenericTplId::Type(index as u32), ty);
                        } else {
                            all_defaults = false;
                            break;
                        }
                    }
                    if all_defaults && !default_bindings.is_empty() {
                        collect_type_def_members(
                            model,
                            &def,
                            Some(&default_bindings),
                            name_bindings,
                            visited,
                            out,
                        );
                        return;
                    }
                }
                collect_type_def_members(model, &def, bindings, name_bindings, visited, out);
            }
        }
        // Generic instance: base type members + argument substitution.
        LuaType::Generic(generic) => {
            let def = type_def_of(model, &generic.get_base_type_id());
            let params = generic.get_params().to_vec();
            let bindings: TplBindings = params
                .iter()
                .enumerate()
                .map(|(index, ty)| (GenericTplId::Type(index as u32), ty.clone()))
                .collect();
            let generic_name_bindings: HashMap<String, LuaType> = def
                .iter()
                .flat_map(|def| {
                    def.generic_params
                        .iter()
                        .enumerate()
                        .filter_map(|(index, param)| {
                            params
                                .get(index)
                                .map(|value| (param.name.to_string(), value.clone()))
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            collect_members(
                model,
                &LuaType::Ref(generic.get_base_type_id()),
                Some(&bindings),
                Some(&generic_name_bindings),
                visited,
                out,
            );
        }
        // Array: base type members.
        LuaType::Array(array) => {
            collect_members(
                model,
                array.get_base(),
                bindings,
                name_bindings,
                visited,
                out,
            );
        }
        // Anonymous table literal: field members of the synthesized owner (file, range).
        LuaType::TableConst(table) => {
            let owner = SemanticId::member(table.file_id, table.value);
            for member_ref in model.members_of_owner(&owner) {
                if let Some(info) = member_info_of(model, &member_ref, bindings, name_bindings) {
                    if let Some(expanded) =
                        expand_table_multi_return_member(model, table, &member_ref, &info)
                    {
                        out.extend(expanded);
                    } else {
                        out.push(info);
                    }
                }
            }
            // `local t = {}; t.a = 1`: the table identity is also bound to the local that declares it,
            // so its runtime members (`t.a = 1`) belong to the table as well.
            if let Some(facts) = model.file_facts_of(table.file_id) {
                let mut owners: Vec<SemanticId> = Vec::new();
                for decl in &facts.decls {
                    if decl
                        .value_expr_syntax
                        .is_some_and(|syntax| syntax.get_range() == table.value)
                    {
                        owners.push(decl.id.clone());
                        // Global table: runtime members are collected by Name key (`string.rep` for `string = {}`).
                        if matches!(decl.kind, DeclKind::Global) {
                            owners.push(SemanticId::name(decl.name.clone()));
                        }
                    }
                }
                for decl_owner in owners {
                    for member_ref in model.members_of_owner(&decl_owner) {
                        if let Some(info) =
                            member_info_of(model, &member_ref, bindings, name_bindings)
                        {
                            out.push(info);
                        }
                    }
                }
            }
        }
        // Rich projection object: access field members directly (`{ foo: number }`'s `.foo`).
        LuaType::Object(object) => {
            for (key, ty) in object.get_fields() {
                out.push(MemberInfo {
                    key: key.clone(),
                    typ: ty.clone(),
                    id: None,
                    file_id: None,
                    is_method: false,
                });
            }
            for (_key, ty) in object.get_index_access() {
                out.push(MemberInfo {
                    key: LuaMemberKey::Name("[index]".into()),
                    typ: ty.clone(),
                    id: None,
                    file_id: None,
                    is_method: false,
                });
            }
        }
        // String value: complete members through the string library (`s.sub` / `s:sub`).
        LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_) => {
            let string_owner = SemanticId::name(SmolStr::new("string"));
            for member_ref in model.members_of_owner(&string_owner) {
                if let Some(info) = member_info_of(model, &member_ref, bindings, name_bindings) {
                    out.push(info);
                }
            }
        }
        // Intersection: members from all components are visible (`(A & B).x` takes A/B fields).
        LuaType::Intersection(intersection) => {
            for component in intersection.get_types() {
                collect_members(model, component, bindings, name_bindings, visited, out);
            }
        }
        // Union: union of component members; same-key member types merge (`(A|B).x` = `A.x | B.x`).
        LuaType::Union(union) => {
            for component in union.into_vec() {
                let mut component_members = Vec::new();
                collect_members(
                    model,
                    &component,
                    bindings,
                    name_bindings,
                    visited,
                    &mut component_members,
                );
                for info in component_members {
                    if let Some(existing) = out.iter_mut().find(|existing| existing.key == info.key)
                    {
                        if existing.typ != info.typ {
                            existing.typ = LuaType::from_vec(vec![existing.typ.clone(), info.typ]);
                        }
                    } else {
                        out.push(info);
                    }
                }
            }
        }
        _ => {}
    }
}

/// If the last unkeyed field of a Lua table constructor is a multi-return function call,
/// all returned values expand consecutively into integer-keyed fields (`{ coroutine.resume(...) }` → `[1]`, `[2]`, ...).
fn expand_table_multi_return_member(
    model: &SemanticModel,
    table: &crate::InFiled<rowan::TextRange>,
    member_ref: &MemberRef,
    info: &MemberInfo,
) -> Option<Vec<MemberInfo>> {
    let LuaType::Variadic(variadic) = &info.typ else {
        return None;
    };
    let LuaMemberKey::Integer(start) = info.key else {
        return None;
    };
    let facts = model.file_facts_of(member_ref.file_id)?;
    let member_def = facts.member_by_id(&member_ref.id)?;
    let value_syntax = member_def.value_syntax?;
    let tree = model.syntax_tree_of(member_ref.file_id)?;
    let value_node = value_syntax.to_node_from_root(&tree.get_red_root())?;
    let field = value_node.ancestors().find_map(LuaTableField::cast)?;
    let table_node = field.syntax().parent()?;
    let table_expr = LuaTableExpr::cast(table_node)?;
    if table_expr.get_range() != table.value {
        return None;
    }
    // Only unkeyed value fields trigger multi-return expansion; `Idx` is the key the parser synthesizes for implicit array items.
    if field
        .get_field_key()
        .as_ref()
        .is_some_and(|key| !matches!(key, emmylua_parser::LuaIndexKey::Idx(_)))
    {
        return None;
    }
    // Lua expands only the function call in the last field.
    let is_last = table_expr
        .get_fields()
        .last()
        .is_some_and(|last| last.get_range() == field.get_range());
    if !is_last {
        return None;
    }
    let types = flatten_table_multi_return(variadic);
    if types.is_empty() {
        return None;
    }
    Some(
        types
            .into_iter()
            .enumerate()
            .map(|(index, ty)| MemberInfo {
                key: LuaMemberKey::Integer(start + index as i64),
                typ: ty,
                id: None,
                file_id: None,
                is_method: false,
            })
            .collect(),
    )
}

/// Flattens a multi-return type such as `(boolean, any...)` into a concrete slot list.
fn flatten_table_multi_return(variadic: &crate::VariadicType) -> Vec<LuaType> {
    use crate::VariadicType;
    match variadic {
        VariadicType::Multi(types) => types
            .iter()
            .flat_map(|ty| match ty {
                LuaType::Variadic(inner) => match inner.as_ref() {
                    VariadicType::Base(base) => vec![base.clone()],
                    VariadicType::Multi(inner_types) => inner_types.clone(),
                },
                other => vec![other.clone()],
            })
            .collect(),
        VariadicType::Base(base) => vec![base.clone()],
    }
}

/// Named type members: this class's `@field` → parent type recursion → runtime value members.
fn collect_type_def_members(
    model: &SemanticModel,
    def: &TypeDef,
    bindings: Option<&TplBindings>,
    name_bindings: Option<&HashMap<String, LuaType>>,
    visited: &mut Vec<SemanticId>,
    out: &mut Vec<MemberInfo>,
) {
    if visited.contains(&def.id) || visited.len() >= MAX_MEMBER_INHERITANCE_DEPTH {
        return;
    }
    visited.push(def.id.clone());

    // 1. This class's `@field` (workspace member index, cross-file).
    for member_ref in model.members_of_owner(&def.id) {
        if let Some(info) = member_info_of(model, &member_ref, bindings, name_bindings) {
            out.push(info);
        }
    }

    // 2. Parent types (resolved in the defining file's scope: Private/Internal/Global all supported; recursion guarded by visited).
    for super_name in &def.super_names {
        // Generic inheritance `class foo<T>: T` / `class buz<T>: foo<T>`:
        // if the parent type is a generic parameter of the current class, expand it with the substituted arguments (`foo<{a:string}>`).
        if let Some(bindings) = bindings {
            let mut found_super = false;
            for (index, param) in def.generic_params.iter().enumerate() {
                let param_name = param.name.as_str();
                if super_name.as_str() == param_name
                    || super_name.as_str().starts_with(&format!("{param_name}<"))
                {
                    if let Some(bound) = bindings.get(&GenericTplId::Type(index as u32)) {
                        collect_members(model, bound, None, None, visited, out);
                        found_super = true;
                    }
                    break;
                }
            }
            if found_super {
                continue;
            }
        }
        if let Some(super_def) = model.resolve_type_def_in(def.file_id, super_name.as_str()) {
            collect_type_def_members(model, &super_def, bindings, name_bindings, visited, out);
        }
    }

    // 3. Runtime value members: members of the type's runtime value decl (same-file same-name `local M = {}`) (`M.x = 1`).
    for owner in model.q().resolve_owner_set(def.id.clone()) {
        if owner == def.id {
            continue;
        }
        for member_ref in model.members_of_owner(&owner) {
            if let Some(info) = member_info_of(model, &member_ref, bindings, name_bindings) {
                out.push(info);
            }
        }
    }
}

/// `---@version` visibility: the member is visible if any version condition on its closure signature matches.
fn member_version_visible(model: &SemanticModel, facts: &FileFacts, member: &Member) -> bool {
    let Some(version) = model.lua_version() else {
        return true;
    };
    let Some(value_syntax) = member.value_syntax else {
        return true;
    };
    let Some(signature) = facts.signature_by_closure(value_syntax) else {
        return true;
    };
    signature
        .docs
        .as_ref()
        .map(|docs| {
            docs.versions.is_empty()
                || docs
                    .versions
                    .iter()
                    .any(|condition| condition.check(&version))
        })
        .unwrap_or(true)
}

/// Determines whether a member is attached to a class table (`---@class Foo` + `local Foo = {}`);
/// class methods already have a full type chain, and a cross-file wide Function projection would interfere with generic/parameter checks.
fn is_class_table_member(
    model: &SemanticModel<'_>,
    member_ref: &MemberRef,
    member: &Member,
) -> bool {
    let Some(facts) = model.file_facts_of(member_ref.file_id) else {
        return false;
    };
    match &member.owner {
        SemanticId::Decl(decl_key) => {
            let Some(decl) = facts.decl_by_id(&SemanticId::Decl(decl_key.clone())) else {
                return false;
            };
            facts
                .type_defs
                .iter()
                .any(|def| def.name == decl.name && def.owner_syntax == decl.owner_syntax)
        }
        SemanticId::Name(name) => facts
            .type_defs
            .iter()
            .any(|def| def.name.as_str() == name.as_str()),
        SemanticId::Member(table_key) => facts
            .decls
            .iter()
            .find(|decl| {
                decl.value_expr_syntax
                    .map(|syntax| syntax.get_range())
                    .is_some_and(|range| range == table_key.key_range)
            })
            .is_some_and(|decl| {
                facts
                    .type_defs
                    .iter()
                    .any(|def| def.name == decl.name && def.owner_syntax == decl.owner_syntax)
            }),
        _ => false,
    }
}

/// Determines whether a file belongs to the STD workspace.
fn is_std_file(model: &SemanticModel<'_>, file_id: FileId) -> bool {
    let Some(workspace) = model.db().workspace_input() else {
        return false;
    };
    crate::salsa_builder::query::file_workspace_id(model.db(), workspace, file_id)
        == Some(WorkspaceId::STD)
}

/// Member reference → member info (type projection + generic substitution).
fn member_info_of(
    model: &SemanticModel,
    member_ref: &MemberRef,
    bindings: Option<&TplBindings>,
    name_bindings: Option<&HashMap<String, LuaType>>,
) -> Option<MemberInfo> {
    let facts = model.file_facts_of(member_ref.file_id)?;
    let member = facts.member_by_id(&member_ref.id)?;
    if !member_version_visible(model, facts, &member) {
        return None;
    }
    let mut typ = model.type_of_member(&member_ref.id)?;
    // Cross-file projection of runtime function members: `type_of_member` often returns a wide Function,
    // so look up the real signature in the member's declaring file to let `require(...).module.fn()` keep inferring.
    // Standard library members keep the wide Function behavior to avoid disturbing parameter checks / math library overloads.
    if matches!(typ, LuaType::Unknown | LuaType::Function)
        && !is_std_file(model, member_ref.file_id)
        && !is_class_table_member(model, member_ref, &member)
        && let Some(value_syntax) = member.value_syntax
    {
        if member_ref.file_id == model.file_id() {
            let expr_ty = model.type_of_expr(value_syntax);
            // Table literal closure fields need a callable real signature; class methods keep the original wide Function projection.
            if model.is_initializer_table_field(&member_ref.id, &member)
                && let Some(fun) = model.type_of_signature_in_file(member_ref.file_id, value_syntax)
            {
                typ = LuaType::DocFunction(Arc::new(fun));
            } else if !matches!(expr_ty, LuaType::Unknown) {
                typ = expr_ty;
            }
        } else if let Some(fun) = model.type_of_signature_in_file(member_ref.file_id, value_syntax)
        {
            typ = LuaType::DocFunction(Arc::new(fun));
        }
    }
    // Nullable fields merge nil; function fields stay pure-function shaped to avoid parameter/self inference
    // being disturbed by a `fun | nil` union (flow queries still add nil through flow_member_value_type).
    if member.is_nullable
        && !typ.is_nullable()
        && !matches!(
            typ,
            LuaType::Function
                | LuaType::DocFunction(_)
                | LuaType::Ref(_)
                | LuaType::Def(_)
                | LuaType::Generic(_)
                | LuaType::Array(_)
                | LuaType::Object(_)
        )
    {
        typ = LuaType::from_vec(vec![typ, LuaType::Nil]);
    }
    // A method function's own generics (`---@generic T`) need a separate `Func` ID on member access,
    // to prevent `Test<integer>` class generics from substituting the method-level T with integer.
    if let LuaType::DocFunction(fun) = &typ
        && !fun.get_generic_params().is_empty()
    {
        let fun = super::infer::vm::reassign_function_generics_to_func_ids(fun.as_ref().clone());
        typ = LuaType::DocFunction(Arc::new(fun));
    }
    if let Some(bindings) = bindings {
        typ = unify::substitute(&typ, bindings);
    }
    if let Some(name_bindings) = name_bindings {
        typ = crate::semantic_model::type_eval::substitute_named_refs(&typ, name_bindings);
    }
    // After substituting class/generic arguments, member types still need alias expansion and conditional evaluation:
    // `Mock<fun(...)>.ctx.calls`'s field is `MockContextCalls<MockParameters<T>>[]`;
    // without expansion it stays `Alias<...>[]`, so rendering/diagnostics/later access cannot see `any[]`.
    typ = crate::semantic_model::type_eval::expand_alias_generic(model, &typ);
    typ = crate::semantic_model::type_eval::eval_conditionals(model, &typ);

    Some(MemberInfo {
        key: member_key_to_lua(&member.key),
        typ,
        id: Some(member.id.clone()),
        file_id: Some(member_ref.file_id),
        is_method: member.is_method,
    })
}

/// Deduplicate by key (keep first: collection order = this class's @field → parent → runtime values).
fn dedup_by_key(infos: Vec<MemberInfo>) -> Vec<MemberInfo> {
    let mut seen: Vec<LuaMemberKey> = Vec::new();
    let mut out = Vec::new();
    for info in infos {
        if !seen.contains(&info.key) {
            seen.push(info.key.clone());
            out.push(info);
        }
    }
    out
}

/// `LuaTypeDeclId` → type definition (look up the workspace type index by identifier scope + full name).
pub(crate) fn type_def_of(model: &SemanticModel, decl_id: &LuaTypeDeclId) -> Option<TypeDef> {
    let (scope, name) = match decl_id.get_id() {
        LuaTypeIdentifier::Global(name) => (TypeScope::Global, name.as_str()),
        LuaTypeIdentifier::Internal(workspace_id, name) => (
            TypeScope::Internal(WorkspaceId {
                id: workspace_id.id,
            }),
            name.as_str(),
        ),
        LuaTypeIdentifier::File(file_id, name) => {
            (TypeScope::File(FileId::new(file_id.id)), name.as_str())
        }
    };
    model.q().type_defs_in_scope(scope, name).into_iter().next()
}

/// `def::LuaMemberKey` → value-domain `LuaMemberKey` (already unified; passed through directly).
fn member_key_to_lua(key: &LuaMemberKey) -> LuaMemberKey {
    key.clone()
}

/// For test assertions: convert collected members to a map keyed by name.
#[cfg(test)]
fn member_map(infos: &[MemberInfo]) -> HashMap<String, LuaType> {
    infos
        .iter()
        .filter_map(|info| match &info.key {
            LuaMemberKey::Name(name) => Some((name.to_string(), info.typ.clone())),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;

    use lsp_types::Uri;

    use crate::{
        Emmyrc, LuaArrayType, LuaGenericType, LuaType, LuaTypeDeclId, LuaUnionType, SalsaDatabase,
    };

    use super::super::SemanticModel;
    use super::*;

    fn model_of(source: &str) -> &'static SemanticModel<'static> {
        let emmyrc = Arc::new(Emmyrc::default());
        let mut db = SalsaDatabase::new();
        db.update_config(emmyrc);
        let uri = Uri::from_str("file:///C:/ws/member.lua").unwrap();
        let fid = db.set_file_content(&uri, Some(source.to_string()));
        // Leak: for tests.
        let db: &'static SalsaDatabase = Box::leak(Box::new(db));
        Box::leak(Box::new(SemanticModel::new(db, fid).unwrap()))
    }

    /// Named type `@field` members.
    #[test]
    fn test_member_infos_class_fields() {
        let model =
            model_of("---@class C\n---@field x number\n---@field name string\nlocal C = {}");
        let infos = model.member_infos(&LuaType::Ref(LuaTypeDeclId::global("C")));
        let map = member_map(&infos);
        assert_eq!(map.get("x"), Some(&LuaType::Number));
        assert_eq!(map.get("name"), Some(&LuaType::String));
    }

    #[test]
    fn test_std_member_resolution() {
        use emmylua_parser::{LuaAstNode, LuaIndexExpr};
        let mut ws = crate::VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def("local x = math.randomseed(os.time())");
        let model = ws.analysis.semantic_model(file_id).unwrap();
        let chunk = model.chunk().unwrap();

        let mut found_randomseed = false;
        let mut found_time = false;
        for index_expr in chunk.descendants::<LuaIndexExpr>() {
            let Some(name) = index_expr.get_index_name_token() else {
                continue;
            };
            let resolved = model.resolve_member(&index_expr).expect("resolved");
            match name.text() {
                "randomseed" => {
                    assert!(resolved.member_id.is_some(), "math.randomseed 应解析到成员");
                    found_randomseed = true;
                }
                "time" => {
                    assert!(resolved.member_id.is_some(), "os.time 应解析到成员");
                    found_time = true;
                }
                _ => {}
            }
        }
        assert!(found_randomseed, "未找到 math.randomseed");
        assert!(found_time, "未找到 os.time");
    }

    /// String values can complete members through the string library (`s.sub` / `s:sub`).
    #[test]
    fn test_member_infos_string_library() {
        let mut ws = crate::VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def("local s = 'abc'");
        let model = ws.analysis.semantic_model(file_id).expect("model");
        let infos = model.member_infos(&LuaType::String);
        let map = member_map(&infos);
        assert!(
            map.contains_key("sub") || map.contains_key("gsub"),
            "string 成员缺失: {:?}",
            map.keys().take(20).collect::<Vec<_>>()
        );
    }

    /// Inheritance chain: B : A includes A's `@field`.
    #[test]
    fn test_member_infos_inheritance() {
        let model = model_of(
            "---@class A\n---@field x number\n---@class B : A\n---@field y string\nlocal A = {}\nlocal B = {}",
        );
        let infos = model.member_infos(&LuaType::Ref(LuaTypeDeclId::global("B")));
        let map = member_map(&infos);
        assert_eq!(map.get("x"), Some(&LuaType::Number), "继承 A.x");
        assert_eq!(map.get("y"), Some(&LuaType::String));
    }

    /// Runtime value members: `M.y = 1` merges into members of type M.
    #[test]
    fn test_member_infos_runtime_members() {
        let model = model_of("---@class M\nlocal M = {}\nM.y = 1");
        let infos = model.member_infos(&LuaType::Ref(LuaTypeDeclId::global("M")));
        let map = member_map(&infos);
        assert_eq!(map.get("y"), Some(&LuaType::Number));
    }

    /// Generic instance: `value` member of `Box<number>` substitutes to `number`.
    #[test]
    fn test_member_infos_generic_instance() {
        let model = model_of("---@class Box<T>\n---@field value T\nlocal Box = {}");
        let generic = LuaType::Generic(Arc::new(LuaGenericType::new(
            LuaTypeDeclId::global("Box"),
            vec![LuaType::Number],
        )));
        let infos = model.member_infos(&generic);
        let map = member_map(&infos);
        assert_eq!(map.get("value"), Some(&LuaType::Number), "T 代入 number");
    }

    /// Array forwarding: `C[]` includes C's members.
    #[test]
    fn test_member_infos_array_base() {
        let model = model_of("---@class C\n---@field x number\nlocal C = {}");
        let array = LuaType::Array(Arc::new(LuaArrayType::from_base_type(LuaType::Ref(
            LuaTypeDeclId::global("C"),
        ))));
        let infos = model.member_infos(&array);
        let map = member_map(&infos);
        assert_eq!(map.get("x"), Some(&LuaType::Number));
    }

    /// Query by key: `member_type(prefix, key)`.
    #[test]
    fn test_member_type_by_key() {
        let model =
            model_of("---@class C\n---@field x number\n---@field name string\nlocal C = {}");
        let ty = model
            .member_type(
                &LuaType::Ref(LuaTypeDeclId::global("C")),
                &LuaMemberKey::Name("x".into()),
            )
            .expect("x member");
        assert_eq!(ty, LuaType::Number);
        assert!(
            model
                .member_type(
                    &LuaType::Ref(LuaTypeDeclId::global("C")),
                    &LuaMemberKey::Name("missing".into()),
                )
                .is_none()
        );
    }

    /// Union: union of component members.
    #[test]
    fn test_member_infos_union() {
        let model = model_of(
            "---@class A\n---@field x number\n---@class B\n---@field y string\nlocal A = {}\nlocal B = {}",
        );
        let union = LuaType::Union(Arc::new(LuaUnionType::from_vec(vec![
            LuaType::Ref(LuaTypeDeclId::global("A")),
            LuaType::Ref(LuaTypeDeclId::global("B")),
        ])));
        let infos = model.member_infos(&union);
        let map = member_map(&infos);
        assert_eq!(map.get("x"), Some(&LuaType::Number));
        assert_eq!(map.get("y"), Some(&LuaType::String));
    }
}
