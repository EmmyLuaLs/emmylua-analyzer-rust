//! Index signature collection, type projection and inheritance.

use hashbrown::HashMap;

use crate::semantic_db::def::TypeDefKind;
use crate::semantic_model::SemanticModel;
use crate::semantic_model::cache::{CacheEntry, QueryCacheScope, TypeEvaluationScope};
use crate::semantic_model::infer::unify::{self, TplBindings};
use crate::semantic_model::member::{inherited_base_types, projected_member_type};
use crate::semantic_model::type_eval::{
    expand_alias_branches, substitute_named_refs, with_expanded_alias,
};
use crate::{FileId, GenericTplId, LuaType, LuaTypeDeclId, SemanticId};

#[derive(Debug, Clone, PartialEq)]
pub struct IndexInfo {
    pub key_type: LuaType,
    pub value_type: LuaType,
    pub id: Option<SemanticId>,
    pub file_id: Option<FileId>,
}

impl SemanticModel<'_> {
    /// Index signature candidates of `typ` with instantiated key/value types and their declaration source.
    pub fn index_infos(&self, typ: &LuaType) -> Vec<IndexInfo> {
        let key = (self.file_id(), typ.clone());
        {
            let cache = self.cache.borrow();
            if let Some(entry) = cache.index_infos.get(&key) {
                return entry.get(&cache).cloned().unwrap_or_default();
            }
        }
        let _query = QueryCacheScope::new(&self.cache);
        let revision = self.cache.borrow().revision();
        self.cache
            .borrow_mut()
            .index_infos
            .insert(key.clone(), CacheEntry::InProgress);
        let mut infos = Vec::new();
        collect_index_infos(self, typ, &mut Vec::new(), &mut infos);
        // 收集完成后统一去重, 只移除相同声明及相同投影, 保留分支顺序.
        let mut index = 0;
        while index < infos.len() {
            if infos[..index].contains(&infos[index]) {
                infos.remove(index);
            } else {
                index += 1;
            }
        }
        let mut cache = self.cache.borrow_mut();
        let complete = cache.is_complete(revision);
        cache
            .index_infos
            .insert(key, CacheEntry::finish(infos.clone(), complete));
        infos
    }
}

fn collect_index_infos(
    model: &SemanticModel,
    typ: &LuaType,
    path: &mut Vec<LuaType>,
    out: &mut Vec<IndexInfo>,
) {
    // 循环继承中的已知声明只处理一次; 不同泛型实例仍分别处理.
    if path.contains(typ) {
        return;
    }
    let Some(evaluation) = TypeEvaluationScope::new(&model.cache) else {
        return;
    };
    if !evaluation.visit(typ) {
        return;
    }
    path.push(typ.clone());
    match typ {
        LuaType::Ref(id) | LuaType::Def(id) => {
            collect_named_indexes(model, typ, id, &[], path, out);
        }
        LuaType::Generic(generic) => {
            let id = generic.get_base_type_id();
            let params = generic.get_params();
            // 仅标准库 table 或无声明的内建 table 使用字典投影.
            if id.get_name() == "table"
                && params.len() == 2
                && model
                    .type_def_of(&id)
                    .is_none_or(|def| model.is_std_file(def.file_id))
            {
                out.push(IndexInfo {
                    key_type: params[0].clone(),
                    value_type: params[1].clone(),
                    id: None,
                    file_id: None,
                });
            } else {
                collect_named_indexes(model, typ, &id, params, path, out);
            }
        }
        LuaType::Object(object) => {
            out.extend(
                object
                    .get_index_access()
                    .iter()
                    .map(|(key, value)| IndexInfo {
                        key_type: key.clone(),
                        value_type: value.clone(),
                        id: None,
                        file_id: None,
                    }),
            );
        }
        LuaType::TableGeneric(params) => {
            if let [key, value] = params.as_slice() {
                out.push(IndexInfo {
                    key_type: key.clone(),
                    value_type: value.clone(),
                    id: None,
                    file_id: None,
                });
            }
        }
        LuaType::Array(array) => out.push(IndexInfo {
            key_type: LuaType::Integer,
            value_type: array.get_base().clone(),
            id: None,
            file_id: None,
        }),
        LuaType::TplRef(tpl) => {
            if let Some(constraint) = tpl.get_constraint() {
                collect_index_infos(model, constraint, path, out);
            }
        }
        LuaType::Union(union) => {
            for component in union.iter() {
                collect_index_infos(model, component, path, out);
            }
        }
        LuaType::Intersection(intersection) => {
            for component in intersection.get_types() {
                collect_index_infos(model, component, path, out);
            }
        }
        LuaType::Instance(instance) => collect_index_infos(model, instance.get_base(), path, out),
        _ => {}
    }
    path.pop();
}

fn collect_named_indexes(
    model: &SemanticModel,
    typ: &LuaType,
    id: &LuaTypeDeclId,
    params: &[LuaType],
    path: &mut Vec<LuaType>,
    out: &mut Vec<IndexInfo>,
) {
    let Some(def) = model.type_def_of(id) else {
        model.cache.borrow().mark_incomplete();
        return;
    };
    if def.kind == TypeDefKind::Alias {
        with_expanded_alias(model, typ, |expanded| {
            collect_index_infos(model, expanded, path, out);
        });
        return;
    }
    let mut bindings = TplBindings::new();
    let mut names = HashMap::new();
    for (index, param) in def.generic_params.iter().enumerate() {
        let value = params.get(index).cloned().or_else(|| {
            param.default.or(param.constraint).map(|syntax| {
                let value = model.doc_type_lua_in(def.file_id, syntax, &def.generic_params);
                substitute_named_refs(&unify::substitute(&value, &bindings), &names)
            })
        });
        if let Some(value) = value {
            bindings.insert(GenericTplId::Type(index as u32), value.clone());
            names.insert(param.name.to_string(), value);
        }
    }
    let mut candidates = Vec::new();
    for member_ref in model.members_of_owner(&def.id).iter() {
        let Some(facts) = model.file_facts_of(member_ref.file_id) else {
            model.cache.borrow().mark_incomplete();
            continue;
        };
        let Some(member) = facts.member_by_id(&member_ref.id) else {
            model.cache.borrow().mark_incomplete();
            continue;
        };
        let Some(key_syntax) = member.index_key_syntax else {
            continue;
        };
        let generics = facts
            .type_def_by_id(&member.owner)
            .map(|declaration| declaration.generic_params.as_slice())
            .unwrap_or_default();
        let declaration_names = generics
            .iter()
            .enumerate()
            .filter_map(|(index, param)| {
                bindings
                    .get(&GenericTplId::Type(index as u32))
                    .map(|value| (param.name.to_string(), value.clone()))
            })
            .collect();
        let key = model.doc_type_lua_in(member_ref.file_id, key_syntax, generics);
        let key = substitute_named_refs(&unify::substitute(&key, &bindings), &declaration_names);
        let Some(key_type) = expand_alias_branches(model, &key) else {
            continue;
        };
        let Some(value_type) = projected_member_type(
            model,
            member_ref,
            member,
            Some(&bindings),
            Some(&declaration_names),
        ) else {
            model.cache.borrow().mark_incomplete();
            continue;
        };
        candidates.push(IndexInfo {
            key_type,
            value_type,
            id: Some(member.id.clone()),
            file_id: Some(member_ref.file_id),
        });
    }
    for base in inherited_base_types(model, &def, Some(&bindings), Some(&names)) {
        collect_index_infos(model, &base, path, &mut candidates);
    }
    // 同一具名类型的声明与父类按键域选取优先项, 不影响外层分支.
    let mut domains = Vec::new();
    for mut info in candidates {
        let Some(key) = expand_alias_branches(model, &info.key_type) else {
            continue;
        };
        info.key_type = key;
        if !domains.contains(&info.key_type) {
            domains.push(info.key_type.clone());
            out.push(info);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use crate::semantic_model::cache::{CacheEntry, QueryCacheScope};
    use crate::semantic_model::type_check::TypeCheckFailReason;
    use crate::{GenericTpl, GenericTplId, LuaMemberKey, LuaType, VirtualWorkspace};

    /// 索引签名不遮蔽同名静态成员: `[integer]`/`[1]`/`['one']` 独立存在, `integer` 仍是普通字段.
    #[test]
    fn index_signatures_do_not_shadow_named_members() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def(
            r#"
---@class Record
---@field [integer] string
---@field integer boolean
---@field [1] number
---@field ['one'] boolean
"#,
        );
        let typ = ws.ty("Record");
        let model = ws.analysis.semantic_model(file);
        let members = model.member_infos(&typ);
        assert_eq!(members.len(), 3);
        assert_eq!(
            model.member_type(&typ, &LuaMemberKey::from("integer")),
            Some(LuaType::Boolean)
        );
        let infos = model.index_infos(&typ);
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].key_type, LuaType::Integer);
        assert_eq!(infos[0].value_type, LuaType::String);
        let def = model.resolve_type_def("Record").unwrap();
        assert_eq!(model.members_of_owner_named(&def.id, "integer").len(), 1);
        assert!(model.members_of_owner_named(&def.id, "").is_empty());
        assert!(
            !model
                .member_keys_of_owner(&def.id)
                .iter()
                .any(|key| key.is_empty())
        );
    }

    /// 跨文件泛型默认值、显式实例化与子类继承都能得到正确的键域、值类型和声明来源.
    #[test]
    fn index_signatures_substitute_cross_file_defaults_and_inheritance() {
        let mut ws = VirtualWorkspace::new();
        let base_file = ws.def_file(
            "base.lua",
            r#"
---@class Dictionary<K = integer, V = string>
---@field [K] V
"#,
        );
        let file = ws.def_file(
            "child.lua",
            r#"
---@class Child: Dictionary<integer, boolean>
---@field [integer] string
---@class Both: Dictionary<integer, string>, Dictionary<string, boolean>
"#,
        );
        let default = ws.ty("Dictionary");
        let instance = ws.ty("Dictionary<string, boolean>");
        let child = ws.ty("Child");
        let both = ws.ty("Both");
        let model = ws.analysis.semantic_model(file);
        for (typ, key, value) in [
            (&default, LuaType::Integer, LuaType::String),
            (&instance, LuaType::String, LuaType::Boolean),
            (&child, LuaType::Integer, LuaType::String),
        ] {
            let infos = model.index_infos(typ);
            assert_eq!(infos.len(), 1, "{typ:?}: {infos:?}");
            assert_eq!(infos[0].key_type, key);
            assert_eq!(infos[0].value_type, value);
        }
        assert_eq!(model.index_infos(&instance)[0].file_id, Some(base_file));
        let infos = model.index_infos(&both);
        assert_eq!(infos.len(), 2);
        assert_eq!(infos[0].key_type, LuaType::Integer);
        assert_eq!(infos[1].key_type, LuaType::String);
    }

    /// 对象类型、别名键域与联合分支都保留各自的索引签名候选.
    #[test]
    fn index_signatures_keep_objects_aliases_and_branch_candidates() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def(
            r#"
---@alias Key integer
---@class Base
---@field [Key] boolean
---@class Child: Base
---@field [integer] string
---@class Other
---@field [integer] number
"#,
        );
        let object = ws.ty("{[integer]: string, [string]: boolean}");
        let child = ws.ty("Child");
        let union = ws.ty("Child | Other");
        let model = ws.analysis.semantic_model(file);
        assert!(model.member_infos(&object).is_empty());
        assert_eq!(model.index_infos(&object).len(), 2);
        assert_eq!(model.index_infos(&child).len(), 1);
        assert_eq!(model.index_infos(&child)[0].value_type, LuaType::String);
        assert_eq!(model.index_infos(&union).len(), 2);
    }

    /// 数组、`table<K, V>`、泛型约束和 `TableGeneric` 分别投影出整数/字符串键域.
    #[test]
    fn index_signatures_project_arrays_tables_and_constraints() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def("");
        let array = ws.ty("string[]");
        let table = ws.ty("table<string, boolean>");
        let constrained = LuaType::TplRef(Rc::new(GenericTpl::new(
            GenericTplId::Type(0),
            "T".into(),
            Some(array.clone()),
            None,
            false,
            None,
        )));
        let model = ws.analysis.semantic_model(file);
        assert_eq!(model.index_infos(&array)[0].key_type, LuaType::Integer);
        assert_eq!(model.index_infos(&array)[0].value_type, LuaType::String);
        assert_eq!(model.index_infos(&table)[0].key_type, LuaType::String);
        assert_eq!(model.index_infos(&table)[0].value_type, LuaType::Boolean);
        assert_eq!(model.index_infos(&constrained), model.index_infos(&array));
        let table_type = LuaType::TableGeneric(Rc::new(vec![LuaType::String, LuaType::Boolean]));
        assert_eq!(model.index_infos(&table_type), model.index_infos(&table));
    }

    /// 父类是泛型对象类型 `{[K]: V}` 时, 子类按继承实参替换索引签名的键值.
    #[test]
    fn index_signatures_keep_generic_object_parents() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def(
            r#"
---@class Parent<K, V>: {[K]: V}
---@class Child: Parent<string, boolean>
"#,
        );
        let typ = ws.ty("Child");
        let model = ws.analysis.semantic_model(file);
        let infos = model.index_infos(&typ);
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].key_type, LuaType::String);
        assert_eq!(infos[0].value_type, LuaType::Boolean);
    }

    /// 递归继承展开超出上限时返回 `TooComplex`, 且不把失败结果写入缓存.
    #[test]
    fn index_signatures_report_exhausted_inheritance() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def(
            r#"
---@class Growing<T>: Growing<T[]>
---@field [integer] T
"#,
        );
        let typ = ws.ty("Growing<string>");
        let model = ws.analysis.semantic_model(file);
        assert_eq!(
            model.applicable_index_infos(&typ, &LuaType::Integer),
            Err(TypeCheckFailReason::TooComplex)
        );
        assert!(!model.cache.borrow().index_infos.contains_key(&(file, typ)));
    }

    /// 未完成的收集结果不能作为完整候选列表, 最外层查询结束后允许重试.
    #[test]
    fn index_signatures_reentry_does_not_cache_absence() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def(
            r#"
---@class Record
---@field [integer] string
"#,
        );
        let typ = ws.ty("Record");
        let model = ws.analysis.semantic_model(file);
        let expected = model.index_infos(&typ);
        for entry in [
            CacheEntry::InProgress,
            CacheEntry::Provisional(Vec::new()),
            CacheEntry::Provisional(expected.clone()),
        ] {
            let _query = QueryCacheScope::new(&model.cache);
            model
                .cache
                .borrow_mut()
                .index_infos
                .insert((file, typ.clone()), entry);
            assert_eq!(
                model.applicable_index_infos(&typ, &LuaType::Integer),
                Err(TypeCheckFailReason::Unresolved)
            );
        }
        assert_eq!(model.index_infos(&typ).len(), 1);
        assert_eq!(
            model
                .applicable_index_infos(&typ, &LuaType::Integer)
                .unwrap()
                .len(),
            1
        );
    }

    /// 循环继承不会无限展开, 两侧各保留签名, 且结果与查询顺序无关.
    #[test]
    fn index_signatures_preserve_cycles_and_query_order() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def(
            r#"
---@class A: B
---@field [integer] string
---@class B: A
---@field [string] boolean
"#,
        );
        let a = ws.ty("A");
        let b = ws.ty("B");
        for types in [[&a, &b], [&b, &a]] {
            let model = ws.analysis.semantic_model(file);
            for typ in types {
                assert_eq!(model.index_infos(typ).len(), 2);
                assert_eq!(
                    model
                        .applicable_index_infos(typ, &LuaType::Integer)
                        .unwrap()
                        .len(),
                    1
                );
            }
        }
    }

    /// 文件更新后键域随之变化: 索引签名不进入具名桶, 改成普通字段后才进入.
    #[test]
    fn index_signatures_update_key_domains_and_named_buckets() {
        let mut ws = VirtualWorkspace::new();
        let typ = ws.ty("Record");
        for (field, key) in [
            ("[integer]", Some(LuaType::Integer)),
            ("[boolean]", Some(LuaType::Boolean)),
            ("integer", None),
            ("[string]", Some(LuaType::String)),
        ] {
            let file = ws.def_file(
                "record.lua",
                &format!(
                    r#"
---@class Record
---@field {field} string
"#
                ),
            );
            let model = ws.analysis.semantic_model(file);
            let infos = model.index_infos(&typ);
            assert_eq!(infos.first().map(|info| info.key_type.clone()), key);
            for candidate in [LuaType::Integer, LuaType::Boolean, LuaType::String] {
                let expected = if key.as_ref() == Some(&candidate) {
                    infos.clone()
                } else {
                    Vec::new()
                };
                assert_eq!(model.applicable_index_infos(&typ, &candidate), Ok(expected));
            }
            let def = model.resolve_type_def("Record").unwrap();
            assert_eq!(
                model.members_of_owner_named(&def.id, "integer").len(),
                usize::from(key.is_none())
            );
            assert_eq!(model.member_infos(&typ).len(), usize::from(key.is_none()));
        }
    }

    /// 键语法 id 不变(等长改动)时, `surface_eq` 仍能通过键域文本感知变化.
    #[test]
    fn index_signatures_export_surface_tracks_same_length_key_edits() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def_file(
            "record.lua",
            r#"
---@class Record
---@field [integer] string
"#,
        );
        let before = ws
            .analysis
            .semantic_model(file)
            .file_exports(file)
            .unwrap()
            .clone();
        ws.def_file(
            "record.lua",
            r#"
---@class Record
---@field [boolean] string
"#,
        );
        let model = ws.analysis.semantic_model(file);
        let after = model.file_exports(file).unwrap();
        assert_eq!(
            before.members[0].index_key_syntax,
            after.members[0].index_key_syntax
        );
        assert!(!before.surface_eq(after));
    }

    /// 同名类型跨文件分别声明时, 每个索引签名按各自声明的泛型参数名完成替换.
    #[test]
    fn index_signatures_use_each_merged_declarations_generic_names() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "first.lua",
            r#"
---@class Record<K, V>
---@field [K] V
"#,
        );
        let file = ws.def_file(
            "second.lua",
            r#"
---@class Record<A, B>
---@field [B] A
"#,
        );
        let typ = ws.ty("Record<integer, string>");
        let model = ws.analysis.semantic_model(file);
        let infos = model.index_infos(&typ);
        assert_eq!(infos.len(), 2);
        assert!(infos.iter().any(|info| info.key_type == LuaType::Integer && info.value_type == LuaType::String));
        assert!(infos.iter().any(|info| info.key_type == LuaType::String && info.value_type == LuaType::Integer));
    }

    /// 泛型索引签名实例化后仍参与结构化类型检查, 由值域决定兼容性.
    #[test]
    fn index_signatures_preserve_generic_structural_checks() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def(
            r#"
---@class Record<K, V>
---@field [K] V
"#,
        );
        let target = ws.ty("Record<string, boolean>");
        let accepted = ws.ty("{[string]: boolean}");
        let rejected = ws.ty("{[string]: number}");
        let model = ws.analysis.semantic_model(file);
        assert!(model.type_check(&accepted, &target));
        assert!(!model.type_check(&rejected, &target));
    }

    /// 联合中重复出现的同一声明按键域去重, 不同实例化各自保留.
    #[test]
    fn index_signatures_deduplicate_shared_declarations_without_losing_projections() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def(
            r#"
---@class Base<V>
---@field [integer] V
---@class Left: Base<string>
---@class Right: Base<string>
---@class Other
---@field [integer] string
"#,
        );
        let base_string = ws.ty("Base<string>");
        let base_number = ws.ty("Base<number>");
        let other = ws.ty("Other");
        let union = ws.ty("Left | Other | Right | Base<number>");
        let model = ws.analysis.semantic_model(file);
        let mut expected = model.index_infos(&base_string);
        expected.extend(model.index_infos(&other));
        expected.extend(model.index_infos(&base_number));
        assert_eq!(expected.len(), 3);
        assert_eq!(model.index_infos(&union), expected);
    }
}
