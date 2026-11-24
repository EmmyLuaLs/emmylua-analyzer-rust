use std::rc::Rc;

use emmylua_parser::{LuaAstNode, LuaExpr, LuaTableExpr, LuaTableField};
use hashbrown::{HashMap, HashSet};

use crate::semantic_db::def::{DeclKind, MemberRef, SemanticId, TypeDef, TypeDefKind};
use crate::semantic_model::infer::unify::{self, TplBindings};
use crate::semantic_model::type_eval::{
    bind_class_doc_tpls, bind_class_generic_names, class_generic_definition, class_generic_map,
    substitute_named_refs, with_expanded_alias,
};
use crate::{FileId, InFiled, LuaMemberKey, LuaObjectType, LuaType, LuaTypeDeclId};
use crate::{Member, semantic_db::facts::FileFacts};
use smol_str::SmolStr;

use super::SemanticModel;
use super::table_constructor::expanded_results;

/// Maximum inheritance/parent-type depth expanded during member lookup.
///
/// Normal class hierarchies are shallow. This cap prevents pathological/recursive parent
/// chains from making `member_info` / `resolve_member` slow-path expansion unbounded.
const MAX_MEMBER_INHERITANCE_DEPTH: usize = 100;

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
    dedup_by_key(member_infos_all(model, prefix_type))
}

/// Declared class fields and index signatures, without initializer values.
pub(crate) fn class_fields(model: &SemanticModel, target: &LuaType) -> LuaObjectType {
    let fields = model
        .member_infos(target)
        .into_iter()
        .filter_map(|mut info| {
            if let Some(id) = info.id.as_ref() {
                let member = info
                    .file_id
                    .and_then(|file| model.file_facts_of(file))?
                    .member_by_id(id)?;
                if !matches!(member.owner, SemanticId::TypeDef(_)) {
                    return None;
                }
                if member.is_nullable && !info.typ.is_nullable() {
                    info.typ = LuaType::from_vec(vec![info.typ, LuaType::Nil]);
                }
            }
            Some((info.key, info.typ))
        })
        .collect();
    let indexes = model
        .index_infos(target)
        .into_iter()
        .map(|info| (info.key_type, info.value_type))
        .collect();
    LuaObjectType::new_with_fields(fields, indexes)
}

/// All inherited and substituted members, before choosing one declaration per key.
pub(super) fn member_infos_all(model: &SemanticModel, prefix_type: &LuaType) -> Vec<MemberInfo> {
    let mut out = Vec::new();
    let mut visited = Vec::new();
    collect_members(
        model,
        prefix_type,
        None,
        None,
        &mut visited,
        &mut out,
        None,
        MemberCollectMode::Effective,
    );
    out
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
        for member_ref in model.members_of_owner_named(owner, name).iter() {
            if member_ref_matches_key(model, &member_ref, key) {
                if let Some(info) = member_info_of(model, &member_ref, bindings, name_bindings) {
                    return Some(info);
                }
            }
        }
        return None;
    }
    for member_ref in model.members_of_owner(owner).iter() {
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

    for super_type in inherited_base_types(model, def, bindings) {
        let mut infos = Vec::new();
        collect_members(
            model,
            &super_type,
            None,
            None,
            visited,
            &mut infos,
            Some(key),
            MemberCollectMode::Effective,
        );
        if let Some(info) = infos.into_iter().next() {
            visited.pop();
            return Some(info);
        }
    }

    for owner in model.resolve_owner_set(def.id.clone()) {
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

/// 按各基类型的声明替换实参.
pub(super) fn inherited_base_types(
    model: &SemanticModel,
    def: &TypeDef,
    bindings: Option<&TplBindings>,
) -> Vec<LuaType> {
    let bindings = bindings.cloned().unwrap_or_default();
    model
        .base_types(&def.type_decl_id())
        .into_iter()
        .map(|ty| {
            let ty = bind_class_doc_tpls(&ty, def);
            unify::substitute(&ty, &bindings)
        })
        .collect()
}

pub(super) fn direct_member_info(
    model: &SemanticModel,
    prefix_type: &LuaType,
    key: &LuaMemberKey,
) -> Option<MemberInfo> {
    match prefix_type {
        LuaType::Ref(id) | LuaType::Def(id) => {
            let def = model.type_def_of(id)?;
            if def.kind == TypeDefKind::Alias {
                return with_expanded_alias(model, prefix_type, |expanded| {
                    direct_member_info(model, expanded, key)
                })
                .flatten();
            }
            let mut visited = Vec::new();
            // Bare generic type (`Box`) uses its declared defaults/constraints for member lookup.
            let mut bindings = TplBindings::new();
            for (index, param) in def.generic_params.iter().enumerate() {
                let ty = param
                    .default
                    .map(|syntax| {
                        let ty = model.doc_type_lua_in(def.file_id, syntax, &def.generic_params);
                        bind_class_doc_tpls(&ty, &def)
                    })
                    .or_else(|| {
                        param.constraint.map(|syntax| {
                            let ty =
                                model.doc_type_lua_in(def.file_id, syntax, &def.generic_params);
                            bind_class_doc_tpls(&ty, &def)
                        })
                    })
                    .unwrap_or(LuaType::Any);
                let ty = unify::substitute(&ty, &bindings);
                bindings.insert(def.generic_tpl_id(index), ty);
            }
            if !bindings.is_empty() {
                find_type_def_member(model, &def, key, Some(&bindings), None, &mut visited)
            } else {
                find_type_def_member(model, &def, key, None, None, &mut visited)
            }
        }
        LuaType::Generic(generic) => {
            let def = model.type_def_of(&generic.get_base_type_id())?;
            if def.kind == TypeDefKind::Alias {
                return with_expanded_alias(model, prefix_type, |expanded| {
                    direct_member_info(model, expanded, key)
                })
                .flatten();
            }
            let bindings: TplBindings = generic
                .get_params()
                .iter()
                .enumerate()
                .map(|(index, ty)| (def.generic_tpl_id(index), ty.clone()))
                .collect();
            let mut visited = Vec::new();
            find_type_def_member(model, &def, key, Some(&bindings), None, &mut visited)
        }
        LuaType::TableConst(table) => {
            let owner = SemanticId::member(table.file_id, table.value);
            if let Some(info) = find_member_in_owner(model, &owner, key, None, None) {
                return Some(info);
            }
            if let Some(decl) = model
                .file_facts_of(table.file_id)
                .and_then(|facts| facts.decl_by_value_range(table.value))
            {
                let mut owners = vec![decl.id.clone()];
                if matches!(decl.kind, DeclKind::Global) {
                    owners.push(SemanticId::name(decl.name.clone()));
                }
                for owner in owners {
                    if let Some(info) = find_member_in_owner(model, &owner, key, None, None) {
                        return Some(info);
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
    if let Some(info) = member_infos_with_key(model, prefix_type, key)
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

/// The effective member of the key for a prefix type (first declaration per key).
///
/// Keyed lookup must not enumerate every member of the owner: assignment-heavy files
/// (`local M = {}; M.a = 1; M.b = 2; ...`) would otherwise become O(N^2).
pub fn member_infos_with_key(
    model: &SemanticModel,
    prefix_type: &LuaType,
    key: &LuaMemberKey,
) -> Vec<MemberInfo> {
    dedup_by_key(member_infos_with_key_mode(
        model,
        prefix_type,
        key,
        MemberCollectMode::Effective,
    ))
}

/// Members with the given key of a prefix type under the given collection mode.
///
/// - `Effective`:  keeps the highest-priority layer's declarations: same-key declarations in later layers
///   are shadowed, while repeated `@field` lines inside the winning layer stay as overloads.
/// - `AllDefinitions`: keeps every definition site, including declarations shadowed by a higher layer.
pub fn member_infos_with_key_mode(
    model: &SemanticModel,
    prefix_type: &LuaType,
    key: &LuaMemberKey,
    mode: MemberCollectMode,
) -> Vec<MemberInfo> {
    let mut out = Vec::new();
    let mut visited = Vec::new();
    collect_members(
        model,
        prefix_type,
        None,
        None,
        &mut visited,
        &mut out,
        Some(key),
        mode,
    );
    out
}

/// Member type for a prefix type + key (old `infer_member_type`).
pub fn member_type(
    model: &SemanticModel,
    prefix_type: &LuaType,
    key: &LuaMemberKey,
) -> Option<LuaType> {
    member_info(model, prefix_type, key).map(|info| info.typ)
}

/// Use the reaching attachments when the read site is available. Type-only
/// callers can still look up a metatable by the identity of its table literal.
pub(crate) fn metatable_type(
    model: &SemanticModel,
    expr: Option<&LuaExpr>,
    table: Option<&InFiled<rowan::TextRange>>,
) -> Option<LuaType> {
    if let Some(metatable) = expr.and_then(|expr| super::flow::metatable_type_at(model, expr)) {
        return Some(metatable);
    }
    let table = table?;
    // Cross-file: a diagnostic/consumer model's `type_of_expr` may return Unknown for expressions in the defining file,
    // so delegate to the model for the file containing the table.
    if model.file_id() != table.file_id {
        let foreign = model.model_for(table.file_id);
        return metatable_type(&foreign, None, Some(table));
    }
    let facts = model.file_facts_of(table.file_id)?;
    for &(first_range, first_syntax, metatable_syntax) in facts.setmetatable_calls() {
        let first_matches = first_range == table.value
            || matches!(
                model.type_of_expr(first_syntax),
                LuaType::TableConst(ft) if ft.file_id == table.file_id && ft.value == table.value
            );
        if first_matches {
            return Some(model.type_of_expr(metatable_syntax));
        }
    }
    None
}

/// If a table literal is the runtime class table for a `---@class Foo` annotation, returns that class's Ref/Def.
pub(crate) fn table_const_class_type(
    model: &SemanticModel,
    table: &InFiled<rowan::TextRange>,
) -> Option<LuaType> {
    if model.file_id() != table.file_id {
        let foreign = model.model_for(table.file_id);
        return table_const_class_type(&foreign, table);
    }
    let facts = model.file_facts_of(table.file_id)?;
    let decl = facts.decl_by_value_range(table.value)?;
    let def = facts.type_def_by_owner_syntax(decl.owner_syntax?)?;
    Some(model.type_def_ref(def))
}

/// Returns the `__index` member info on the metatable corresponding to a table literal.
pub(crate) fn table_metatable_index_info(
    model: &SemanticModel,
    table: &InFiled<rowan::TextRange>,
) -> Option<MemberInfo> {
    let metatable_ty = metatable_type(model, None, Some(table))?;
    let key = LuaMemberKey::Name(SmolStr::new("__index"));
    member_info(model, &metatable_ty, &key)
}

/// Finds a member with a `self` return_cast by method name.
/// Cross-file supported: scans same-named members in the workspace.
pub fn find_self_return_cast_member(model: &SemanticModel, name: &str) -> Option<SemanticId> {
    for file_id in model.file_ids() {
        let Some(facts) = model.file_facts_of(file_id) else {
            continue;
        };
        for member in facts.members_named(name) {
            let Some(value_syntax) = member.value_syntax else {
                continue;
            };
            let Some(signature) = facts.signature_by_closure(value_syntax) else {
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
    None
}

/// Test-only counter guarding the keyed member query against accidentally
/// falling back to full-owner enumeration.
///
/// Thread-local, and test-only. The harness runs tests in parallel, so a
/// process-global counter lets unrelated tests inflate the value between a
/// `reset()` and the assertion, which made the `p4_5` guards fail intermittently
/// (~2 failing runs in 4 with `cargo test -p emmylua_code_analysis --lib`). Test
/// instrumentation has no business in shipped code, so this never reaches a
/// production build.
#[cfg(test)]
pub(crate) mod query_metrics {
    use std::cell::Cell;

    thread_local! {
        static FULL_OWNER_MEMBER_SCANS: Cell<u32> = const { Cell::new(0) };
    }

    pub(crate) fn record_full_owner_member_scan() {
        FULL_OWNER_MEMBER_SCANS.with(|count| count.set(count.get() + 1));
    }

    pub(crate) fn reset() {
        FULL_OWNER_MEMBER_SCANS.with(|count| count.set(0));
    }

    pub(crate) fn full_owner_member_scans() -> u32 {
        FULL_OWNER_MEMBER_SCANS.with(Cell::get)
    }
}

/// Owner member refs for a query.
///
/// For a named key this uses the owner/name bucket (O(matches)); only unkeyed or
/// non-name queries may enumerate the whole owner.
fn owner_member_refs(
    model: &SemanticModel,
    owner: &SemanticId,
    filter: Option<&LuaMemberKey>,
) -> Vec<MemberRef> {
    match filter {
        Some(LuaMemberKey::Name(name)) => model
            .members_of_owner_named(owner, name.as_str())
            .iter()
            .cloned()
            .collect(),
        _ => {
            #[cfg(test)]
            query_metrics::record_full_owner_member_scan();
            model.members_of_owner(owner).iter().cloned().collect()
        }
    }
}

fn push_member_info(out: &mut Vec<MemberInfo>, filter: Option<&LuaMemberKey>, info: MemberInfo) {
    if filter.is_none_or(|key| &info.key == key) {
        out.push(info);
    }
}

/// How member collection treats same-key declarations from different layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberCollectMode {
    /// Effective members: a declaration in an earlier layer shadows the same key in later layers.
    Effective,
    /// All definitions: shadowed declarations are kept so every definition site stays reachable.
    AllDefinitions,
}

fn collect_members(
    model: &SemanticModel,
    prefix_type: &LuaType,
    bindings: Option<&TplBindings>,
    name_bindings: Option<&HashMap<String, LuaType>>,
    visited: &mut Vec<SemanticId>,
    out: &mut Vec<MemberInfo>,
    filter: Option<&LuaMemberKey>,
    mode: MemberCollectMode,
) {
    match prefix_type {
        // Named type: @field + inheritance + runtime value members.
        LuaType::Ref(decl_id) | LuaType::Def(decl_id) => {
            if let Some(def) = model.type_def_of(decl_id) {
                // Keep the alias active through traversal of recursive structures.
                if def.kind == TypeDefKind::Alias {
                    with_expanded_alias(model, prefix_type, |target| {
                        collect_members(
                            model,
                            target,
                            bindings,
                            name_bindings,
                            visited,
                            out,
                            filter,
                            mode,
                        );
                    });
                    return;
                }
                // Bare generic types (`Box`) fill in default generic arguments on member access,
                // so inherited fields from `Box<T = string>`'s `Parent<T>` resolve to `string`.
                if bindings.is_none() && !def.generic_params.is_empty() {
                    let mut default_bindings = TplBindings::new();
                    for (index, param) in def.generic_params.iter().enumerate() {
                        let ty = param
                            .default
                            .or(param.constraint)
                            .map(|syntax| {
                                let ty =
                                    model.doc_type_lua_in(def.file_id, syntax, &def.generic_params);
                                bind_class_doc_tpls(&ty, &def)
                            })
                            .unwrap_or(LuaType::Any);
                        let ty = unify::substitute(&ty, &default_bindings);
                        default_bindings.insert(def.generic_tpl_id(index), ty);
                    }
                    if !default_bindings.is_empty() {
                        collect_type_def_members(
                            model,
                            &def,
                            Some(&default_bindings),
                            name_bindings,
                            visited,
                            out,
                            filter,
                            mode,
                        );
                        return;
                    }
                }
                collect_type_def_members(
                    model,
                    &def,
                    bindings,
                    name_bindings,
                    visited,
                    out,
                    filter,
                    mode,
                );
            }
        }
        // Generic instance: base type members + argument substitution.
        LuaType::Generic(generic) => {
            if let Some(def) = model.type_def_of(&generic.get_base_type_id())
                && def.kind == TypeDefKind::Alias
            {
                with_expanded_alias(model, prefix_type, |expanded| {
                    collect_members(model, expanded, None, None, visited, out, filter, mode);
                });
                return;
            }
            let Some(def) = model.type_def_of(&generic.get_base_type_id()) else {
                return;
            };
            let params = generic.get_params().to_vec();
            let bindings: TplBindings = params
                .iter()
                .enumerate()
                .map(|(index, ty)| (def.generic_tpl_id(index), ty.clone()))
                .collect();
            collect_members(
                model,
                &LuaType::Ref(generic.get_base_type_id()),
                Some(&bindings),
                None,
                visited,
                out,
                filter,
                mode,
            );
        }
        // Anonymous table literal: field members of the synthesized owner (file, range).
        LuaType::TableConst(table) => {
            let owner = SemanticId::member(table.file_id, table.value);
            for member_ref in owner_member_refs(model, &owner, filter) {
                if let Some(info) = member_info_of(model, &member_ref, bindings, name_bindings) {
                    if let Some(expanded) =
                        expand_table_multi_return_member(model, table, &member_ref, &info, filter)
                    {
                        for info in expanded {
                            push_member_info(out, filter, info);
                        }
                    } else {
                        push_member_info(out, filter, info);
                    }
                }
            }
            // `local t = {}; t.a = 1`: the table identity is also bound to the local that declares it,
            // so its runtime members (`t.a = 1`) belong to the table as well.
            if let Some(decl) = model
                .file_facts_of(table.file_id)
                .and_then(|facts| facts.decl_by_value_range(table.value))
            {
                let mut owners = vec![decl.id.clone()];
                // Global table: runtime members are collected by Name key (`string.rep` for `string = {}`).
                if matches!(decl.kind, DeclKind::Global) {
                    owners.push(SemanticId::name(decl.name.clone()));
                }
                for decl_owner in owners {
                    for member_ref in owner_member_refs(model, &decl_owner, filter) {
                        if let Some(info) =
                            member_info_of(model, &member_ref, bindings, name_bindings)
                        {
                            push_member_info(out, filter, info);
                        }
                    }
                }
            }
        }
        // Rich projection object: access field members directly (`{ foo: number }`'s `.foo`).
        LuaType::Object(object) => {
            for (key, ty) in object.get_fields() {
                push_member_info(
                    out,
                    filter,
                    MemberInfo {
                        key: key.clone(),
                        typ: ty.clone(),
                        id: None,
                        file_id: None,
                        is_method: false,
                    },
                );
            }
        }
        // String value: complete members through the string library (`s.sub` / `s:sub`).
        LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_) => {
            let string_owner = SemanticId::name(SmolStr::new("string"));
            for member_ref in owner_member_refs(model, &string_owner, filter) {
                if let Some(info) = member_info_of(model, &member_ref, bindings, name_bindings) {
                    push_member_info(out, filter, info);
                }
            }
        }
        // Intersection: members from all components are visible (`(A & B).x` takes A/B fields).
        LuaType::Intersection(intersection) => {
            for component in intersection.get_types() {
                collect_members(
                    model,
                    component,
                    bindings,
                    name_bindings,
                    visited,
                    out,
                    filter,
                    mode,
                );
            }
        }
        // Union: union of component members; same-key member types merge (`(A|B).x` = `A.x | B.x`).
        LuaType::Union(union) => {
            for component in union.iter() {
                let mut component_members = Vec::new();
                let mut component_visited = visited.clone();
                collect_members(
                    model,
                    component,
                    bindings,
                    name_bindings,
                    &mut component_visited,
                    &mut component_members,
                    filter,
                    mode,
                );
                for info in component_members {
                    if mode == MemberCollectMode::AllDefinitions {
                        push_declaration_info(out, filter, info);
                        continue;
                    }
                    if let Some(existing) = out.iter_mut().find(|existing| existing.key == info.key)
                    {
                        if existing.typ != info.typ {
                            existing.typ = LuaType::from_vec(vec![existing.typ.clone(), info.typ]);
                        }
                    } else {
                        push_member_info(out, filter, info);
                    }
                }
            }
        }
        _ => {}
    }
}

/// If the last unkeyed field of a Lua table constructor supplies multiple values,
/// all returned values expand consecutively into integer-keyed fields (`{ coroutine.resume(...) }` → `[1]`, `[2]`, ...).
fn expand_table_multi_return_member(
    model: &SemanticModel,
    table: &InFiled<rowan::TextRange>,
    member_ref: &MemberRef,
    info: &MemberInfo,
    filter: Option<&LuaMemberKey>,
) -> Option<Vec<MemberInfo>> {
    if !matches!(info.typ, LuaType::Variadic(_)) {
        return None;
    }
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
    // Lua expands calls and varargs only in the last unkeyed field.
    let is_last = table_expr
        .get_fields()
        .last()
        .is_some_and(|last| last.get_range() == field.get_range());
    if !is_last {
        return None;
    }
    // A repeated tail cannot be enumerated. Resolve the requested slot directly.
    let values = if let Some(LuaMemberKey::Integer(index)) = filter {
        let offset = usize::try_from(index.checked_sub(start)?).ok()?;
        vec![(*index, info.typ.get_result_slot_type(offset)?)]
    } else {
        let mut values: Vec<_> = expanded_results(&info.typ, true)
            .filter_map(|(offset, _, _)| {
                let typ = info.typ.get_result_slot_type(offset)?;
                Some((start + offset as i64, typ))
            })
            .collect();
        values.sort_by_key(|(index, _)| *index);
        values.dedup_by_key(|(index, _)| *index);
        values
    };
    if values.is_empty() {
        return None;
    }
    Some(
        values
            .into_iter()
            .map(|(index, ty)| MemberInfo {
                key: LuaMemberKey::Integer(index),
                typ: ty,
                id: None,
                file_id: None,
                is_method: false,
            })
            .collect(),
    )
}

/// Named type members: this class's `@field` → parent type recursion → runtime value members.
///
/// Each layer shadows the layers before it by key, while overloads inside one layer stay complete.
fn collect_type_def_members(
    model: &SemanticModel,
    def: &TypeDef,
    bindings: Option<&TplBindings>,
    name_bindings: Option<&HashMap<String, LuaType>>,
    visited: &mut Vec<SemanticId>,
    out: &mut Vec<MemberInfo>,
    filter: Option<&LuaMemberKey>,
    mode: MemberCollectMode,
) {
    if visited.contains(&def.id) || visited.len() >= MAX_MEMBER_INHERITANCE_DEPTH {
        return;
    }
    visited.push(def.id.clone());
    let mut shadowed = HashSet::new();
    let mut layer = Vec::new();

    // 1. This class's `@field` (workspace member index, cross-file).
    for member_ref in owner_member_refs(model, &def.id, filter) {
        if let Some(info) = member_info_of(model, &member_ref, bindings, name_bindings) {
            push_member_info(&mut layer, filter, info);
        }
    }
    push_member_layer(out, &mut shadowed, layer, mode);

    // 2. Parent types
    for super_type in inherited_base_types(model, def, bindings) {
        let mut layer = Vec::new();
        collect_members(
            model,
            &super_type,
            None,
            None,
            visited,
            &mut layer,
            filter,
            mode,
        );
        push_member_layer(out, &mut shadowed, layer, mode);
    }

    // 3. Runtime value members: members of the type's runtime value decl (same-file same-name `local M = {}`) (`M.x = 1`).
    for owner in model.resolve_owner_set(def.id.clone()) {
        if owner == def.id {
            continue;
        }
        let mut layer = Vec::new();
        for member_ref in owner_member_refs(model, &owner, filter) {
            if let Some(info) = member_info_of(model, &member_ref, bindings, name_bindings) {
                push_member_info(&mut layer, filter, info);
            }
        }
        push_member_layer(out, &mut shadowed, layer, mode);
    }
}

fn push_member_layer(
    out: &mut Vec<MemberInfo>,
    shadowed: &mut HashSet<LuaMemberKey>,
    layer: Vec<MemberInfo>,
    mode: MemberCollectMode,
) {
    if mode == MemberCollectMode::AllDefinitions {
        for info in layer {
            push_declaration_info(out, None, info);
        }
        return;
    }
    let visible: Vec<_> = layer
        .into_iter()
        .filter(|info| !shadowed.contains(&info.key))
        .collect();
    shadowed.extend(visible.iter().map(|info| info.key.clone()));
    out.extend(visible);
}

/// Appends a definition site, skipping declarations that are already collected.
fn push_declaration_info(
    out: &mut Vec<MemberInfo>,
    filter: Option<&LuaMemberKey>,
    info: MemberInfo,
) {
    if filter.is_some_and(|key| &info.key != key) {
        return;
    }
    let duplicate = match (&info.id, info.file_id) {
        (Some(id), Some(file_id)) => out
            .iter()
            .any(|existing| existing.file_id == Some(file_id) && existing.id.as_ref() == Some(id)),
        _ => out.iter().any(|existing| {
            existing.id.is_none() && existing.key == info.key && existing.typ == info.typ
        }),
    };
    if !duplicate {
        out.push(info);
    }
}

/// `---@version` visibility: the member is visible if any version condition on its closure signature matches.
pub(super) fn member_version_visible(
    model: &SemanticModel,
    facts: &FileFacts,
    member: &Member,
) -> bool {
    let Some(value_syntax) = member.value_syntax else {
        return true;
    };
    let Some(signature) = facts.signature_by_closure(value_syntax) else {
        return true;
    };
    signature
        .docs
        .as_ref()
        .map(|docs| model.versions_visible(&docs.versions))
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

/// Member ref → `MemberInfo` (type projection + generic substitution).
///
/// index-signature members and version-hidden members are skipped.
fn member_info_of(
    model: &SemanticModel,
    member_ref: &MemberRef,
    bindings: Option<&TplBindings>,
    name_bindings: Option<&HashMap<String, LuaType>>,
) -> Option<MemberInfo> {
    let facts = model.file_facts_of(member_ref.file_id)?;
    let member = facts.member_by_id(&member_ref.id)?;
    if member.index_key_syntax.is_some() || !member_version_visible(model, facts, &member) {
        return None;
    }
    let typ = projected_member_type(model, member_ref, member, bindings, name_bindings)?;

    Some(MemberInfo {
        key: member.key.clone(),
        typ,
        id: Some(member.id.clone()),
        file_id: Some(member_ref.file_id),
        is_method: member.is_method,
    })
}

fn member_generic_definition(
    model: &SemanticModel,
    file_id: FileId,
    member: &Member,
) -> Option<TypeDef> {
    let facts = model.file_facts_of(file_id)?;
    match &member.owner {
        SemanticId::TypeDef(_) => facts.type_def_by_id(&member.owner).cloned(),
        SemanticId::Decl(_) => facts
            .decl_by_id(&member.owner)
            .and_then(|decl| decl.owner_syntax)
            .and_then(|syntax| facts.type_def_by_owner_syntax(syntax))
            .cloned(),
        SemanticId::Name(name) => model.type_def_of(&LuaTypeDeclId::global(name.as_str())),
        _ => None,
    }
}

/// Resolve the generic scope and arguments for the declaring class of a member candidate.
/// This follows instantiated parents so `Child<number> : Parent<string>` binds Parent's
/// method parameters to `string`, while keeping each class's same-named parameters distinct.
pub(crate) fn class_generic_context_for_member(
    model: &SemanticModel,
    receiver: &LuaType,
    info: &MemberInfo,
) -> Option<(TypeDef, TplBindings)> {
    let member_id = info.id.as_ref()?;
    class_generic_context_for_member_id(model, receiver, member_id)
}

pub(crate) fn class_generic_context_for_member_id(
    model: &SemanticModel,
    receiver: &LuaType,
    member_id: &SemanticId,
) -> Option<(TypeDef, TplBindings)> {
    let SemanticId::Member(key) = member_id else {
        return None;
    };
    let member = model.file_facts_of(key.file_id)?.member_by_id(member_id)?;
    let target = member_generic_definition(model, key.file_id, member)?;
    find_class_generic_context(model, receiver, &target, &mut Vec::new())
}

fn find_class_generic_context(
    model: &SemanticModel,
    ty: &LuaType,
    target: &TypeDef,
    visited: &mut Vec<SemanticId>,
) -> Option<(TypeDef, TplBindings)> {
    let def = class_generic_definition(model, ty)?;
    if visited.contains(&def.id) {
        return None;
    }
    let bindings = class_generic_map(model, ty).unwrap_or_default();
    if def.id == target.id {
        return Some((def, bindings));
    }
    visited.push(def.id.clone());
    for parent in inherited_base_types(model, &def, Some(&bindings)) {
        if let Some(context) = find_class_generic_context(model, &parent, target, visited) {
            visited.pop();
            return Some(context);
        }
    }
    visited.pop();
    None
}

pub(super) fn projected_member_type(
    model: &SemanticModel,
    member_ref: &MemberRef,
    member: &Member,
    bindings: Option<&TplBindings>,
    name_bindings: Option<&HashMap<String, LuaType>>,
) -> Option<LuaType> {
    let mut typ = model.type_of_member(&member_ref.id)?;
    let generic_def = member_generic_definition(model, member_ref.file_id, member);
    // Cross-file projection of runtime function members: `type_of_member` often returns a wide Function,
    // so look up the real signature in the member's declaring file to let `require(...).module.fn()` keep inferring.
    // Standard library members keep the wide Function behavior to avoid disturbing parameter checks / math library overloads.
    if matches!(typ, LuaType::Unknown | LuaType::Function)
        && !model.is_std_file(member_ref.file_id)
        && !is_class_table_member(model, member_ref, &member)
        && let Some(value_syntax) = member.value_syntax
    {
        if member_ref.file_id == model.file_id() {
            let expr_ty = model.type_of_expr(value_syntax);
            // Table literal closure fields need a callable real signature; class methods keep the original wide Function projection.
            if model.is_initializer_table_field(&member)
                && let Some(fun) = model.type_of_signature_in_file(member_ref.file_id, value_syntax)
            {
                typ = LuaType::DocFunction(Rc::new(fun));
            } else if !matches!(expr_ty, LuaType::Unknown) {
                typ = expr_ty;
            }
        } else if let Some(fun) = model.type_of_signature_in_file(member_ref.file_id, value_syntax)
        {
            typ = LuaType::DocFunction(Rc::new(fun));
        }
    }
    // Nullable fields merge nil; function fields stay pure-function shaped to avoid parameter/self inference
    // being disturbed by a `fun | nil` union (flow queries still add nil through flow_member_value_type).
    // Generic aliases stay lazy and can resolve to primitives, so they retain nil.
    if member.is_nullable
        && !typ.is_nullable()
        && !matches!(
            typ,
            LuaType::Function
                | LuaType::DocFunction(_)
                | LuaType::Ref(_)
                | LuaType::Def(_)
                | LuaType::Array(_)
                | LuaType::Object(_)
        )
    {
        typ = LuaType::from_vec(vec![typ, LuaType::Nil]);
    }
    if let Some(def) = &generic_def {
        typ = bind_class_doc_tpls(&typ, def);
        typ = bind_class_generic_names(&typ, def);
    }
    if let Some(bindings) = bindings {
        typ = unify::substitute(&typ, bindings);
    }
    if generic_def.is_none()
        && let Some(name_bindings) = name_bindings
    {
        typ = substitute_named_refs(&typ, name_bindings);
    }

    Some(typ)
}

/// Deduplicate by key (keep first: collection order = this class's @field → parent → runtime values).
pub(super) fn dedup_by_key(infos: Vec<MemberInfo>) -> Vec<MemberInfo> {
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
        Emmyrc, EmmyrcLuaVersion, LuaGenericType, LuaType, LuaTypeDeclId, LuaUnionType,
        SemanticDatabase,
    };

    use super::*;
    use crate::VirtualWorkspace;
    use crate::semantic_model::SemanticModel;

    struct TestModel {
        db: SemanticDatabase,
        file_id: FileId,
    }

    impl TestModel {
        fn model(&self) -> SemanticModel<'_> {
            SemanticModel::new(&self.db, self.file_id)
        }
    }

    fn model_of(source: &str) -> TestModel {
        let emmyrc = Arc::new(Emmyrc::default());
        let mut db = SemanticDatabase::new();
        db.update_config(emmyrc);
        let uri = Uri::from_str("file:///C:/ws/member.lua").unwrap();
        let fid = db.set_file_content(&uri, Some(source.to_string()));

        TestModel { db, file_id: fid }
    }

    /// Named type `@field` members.
    #[test]
    fn test_member_infos_class_fields() {
        let env = model_of("---@class C\n---@field x number\n---@field name string\nlocal C = {}");
        let model = env.model();
        let infos = model.member_infos(&LuaType::Ref(LuaTypeDeclId::global("C")));
        let map = member_map(&infos);
        assert_eq!(map.get("x"), Some(&LuaType::Number));
        assert_eq!(map.get("name"), Some(&LuaType::String));
    }

    #[test]
    fn test_std_member_resolution() {
        use emmylua_parser::{LuaAstNode, LuaIndexExpr};
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def("local x = math.randomseed(os.time())");
        let model = ws.analysis.semantic_model(file_id);
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

    /// Workspace global classes extend matching stdlib modules even when the
    /// stdlib declares the module class under a different name
    /// (`---@class oslib` + `os = {}`); members merge by global name.
    fn check_stdlib_global_class_extension(with_assignment: bool) {
        let modules = [
            ("os", "date"),
            ("table", "insert"),
            ("math", "abs"),
            ("io", "open"),
            ("package", "searchpath"),
            ("coroutine", "create"),
            ("debug", "traceback"),
            ("utf8", "len"),
            ("string", "format"),
        ];
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let mut sources = Vec::new();
        for (module, _) in modules {
            let mut source =
                format!("---@meta\n---@class {module}\n---@field custom_extra fun(): string\n");
            if with_assignment {
                source.push_str(&format!("\n---@type {module}\n{module} = {{}}\n"));
            }
            ws.def_file(&format!("{module}.lua"), &source);
            sources.push((module, source));
        }

        for ((module, std_member), (_, source)) in modules.into_iter().zip(sources) {
            for expected in [std_member, "custom_extra"] {
                let expr = format!("{module}.{expected}");
                let ty = ws.expr_ty(&expr);
                assert!(
                    ty.is_function(),
                    "{expr} must resolve to a function type, got {ty:?} \
                     (assignment: {with_assignment})\n{source}"
                );
            }
        }
    }

    #[test]
    fn test_stdlib_global_class_extension_without_assignment() {
        check_stdlib_global_class_extension(false);
    }

    /// The same extension with an explicit global declaration: workspace members
    /// are added, but stdlib members must stay available.
    #[test]
    fn test_stdlib_global_class_extension_with_assignment() {
        check_stdlib_global_class_extension(true);
    }

    /// LuaJIT workspace: set the runtime version before `init_std_lib`, because
    /// `new_with_init_std_lib` would load the default (non-JIT) std first.
    /// `new()` does not load std, so ffi/jit are registered exactly once.
    fn ffi_workspace() -> VirtualWorkspace {
        let mut ws = VirtualWorkspace::new();
        let mut config = Emmyrc::default();
        config.runtime.version = EmmyrcLuaVersion::LuaJIT;
        ws.update_emmyrc(config);
        ws.analysis.init_std_lib(None);
        ws
    }

    #[test]
    fn test_std_ffi_load_returns_clib() {
        let mut ws = ffi_workspace();

        let ty = ws.expr_ty("require('ffi').load('c')");
        assert_eq!(ws.humanize_type(ty), "ffi.clib*");
    }

    #[test]
    fn test_std_ffi_c_returns_clib() {
        let mut ws = ffi_workspace();

        let ty = ws.expr_ty("require('ffi').C");
        assert_eq!(ws.humanize_type(ty), "ffi.clib*");
    }

    /// String values can complete members through the string library (`s.sub` / `s:sub`).
    #[test]
    fn test_member_infos_string_library() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def("local s = 'abc'");
        let model = ws.analysis.semantic_model(file_id);
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
        let env = model_of(
            "---@class A\n---@field x number\n---@class B : A\n---@field y string\nlocal A = {}\nlocal B = {}",
        );
        let model = env.model();
        let infos = model.member_infos(&LuaType::Ref(LuaTypeDeclId::global("B")));
        let map = member_map(&infos);
        assert_eq!(map.get("x"), Some(&LuaType::Number), "继承 A.x");
        assert_eq!(map.get("y"), Some(&LuaType::String));
    }

    /// Runtime value members: `M.y = 1` merges into members of type M.
    #[test]
    fn test_member_infos_runtime_members() {
        let env = model_of("---@class M\nlocal M = {}\nM.y = 1");
        let model = env.model();
        let infos = model.member_infos(&LuaType::Ref(LuaTypeDeclId::global("M")));
        let map = member_map(&infos);
        assert_eq!(map.get("y"), Some(&LuaType::Integer));
    }

    /// Generic instance: `value` member of `Box<number>` substitutes to `number`.
    #[test]
    fn test_member_infos_generic_instance() {
        let env = model_of("---@class Box<T>\n---@field value T\nlocal Box = {}");
        let model = env.model();
        let generic = LuaType::Generic(Rc::new(LuaGenericType::new(
            LuaTypeDeclId::global("Box"),
            vec![LuaType::Number],
        )));
        let infos = model.member_infos(&generic);
        let map = member_map(&infos);
        assert_eq!(map.get("value"), Some(&LuaType::Number), "T 代入 number");
    }

    /// 数组及其别名不暴露元素字段, 联合分支只贡献各自的成员.
    #[test]
    fn test_array_members_do_not_include_element_fields() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def(
            r#"
---@class Other
---@field label string

---@class C
---@field x number

---@alias CArray C[]
---@alias Array<T> T[]
"#,
        );
        let element = ws.ty("C");
        let array = ws.ty("C[]");
        let alias = ws.ty("CArray");
        let generic_alias = ws.ty("Array<C>");
        let union = ws.ty("Other | C[]");
        let model = ws.analysis.semantic_model(file);
        let key = LuaMemberKey::from("x");
        for (typ, has_label) in [
            (&array, false),
            (&alias, false),
            (&generic_alias, false),
            (&union, true),
        ] {
            let members = member_map(&model.member_infos(typ));
            assert!(!members.contains_key("x"), "{typ:?}: {members:?}");
            assert_eq!(members.contains_key("label"), has_label);
            assert!(model.member_type(typ, &key).is_none(), "{typ:?}");
        }
        assert_eq!(model.member_type(&element, &key), Some(LuaType::Number));
        assert_eq!(model.index_infos(&array)[0].value_type, element);
    }

    #[test]
    fn test_array_inheritance_keeps_element_members_on_elements() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def(
            r#"
---@class Item
---@field name string
---@class Metadata
---@field label string
---@alias ItemArray Item[]
---@class Items: ItemArray, Metadata
"#,
        );
        let items = ws.ty("Items");
        let item = ws.ty("Item");
        let model = ws.analysis.semantic_model(file);
        let name = LuaMemberKey::from("name");
        let members = member_map(&model.member_infos(&items));
        assert!(!members.contains_key("name"));
        assert_eq!(members.get("label"), Some(&LuaType::String));
        assert!(model.member_type(&items, &name).is_none());
        assert_eq!(model.member_type(&item, &name), Some(LuaType::String));
        assert_eq!(model.index_infos(&items)[0].value_type, item);
    }

    /// Query by key: `member_type(prefix, key)`.
    #[test]
    fn test_member_type_by_key() {
        let env = model_of("---@class C\n---@field x number\n---@field name string\nlocal C = {}");
        let model = env.model();
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
        let env = model_of(
            "---@class A\n---@field x number\n---@class B\n---@field y string\nlocal A = {}\nlocal B = {}",
        );
        let model = env.model();
        let union = LuaType::Union(Rc::new(LuaUnionType::from_vec(vec![
            LuaType::Ref(LuaTypeDeclId::global("A")),
            LuaType::Ref(LuaTypeDeclId::global("B")),
        ])));
        let infos = model.member_infos(&union);
        let map = member_map(&infos);
        assert_eq!(map.get("x"), Some(&LuaType::Number));
        assert_eq!(map.get("y"), Some(&LuaType::String));
    }
}
