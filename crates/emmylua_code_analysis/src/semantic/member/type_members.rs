use std::{cell::OnceCell, rc::Rc};

use hashbrown::HashSet;
use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;
use smol_str::SmolStr;

use crate::{
    DbIndex, FileId, GenericResolveMode, LuaDeclId, LuaGenericType, LuaMemberIndexItem,
    LuaMemberKey, LuaMemberOwner, LuaType, LuaTypeDeclId, LuaUnionType, TypeOps, TypeSubstitutor,
    VariadicType, instantiate_type_generic,
};

use super::{get_buildin_type_map_type_id, intersect_member_types};

pub(in crate::semantic) type TypeMembers = IndexMap<LuaMemberKey, MemberSymbol, FxBuildHasher>;

#[derive(Debug)]
enum MemberOrigin {
    Indexed(LuaMemberIndexItem),
    Decl(LuaDeclId),
    Type(LuaType),
    Union(Vec<MemberSymbol>),
    Intersection(Vec<MemberSymbol>),
}

#[derive(Debug)]
pub(in crate::semantic) struct MemberSymbol {
    origin: MemberOrigin,
    substitutor: Option<Rc<TypeSubstitutor>>,
    typ: OnceCell<LuaType>,
}

impl MemberSymbol {
    fn new(origin: MemberOrigin, substitutor: Option<Rc<TypeSubstitutor>>) -> Self {
        Self {
            origin,
            substitutor,
            typ: OnceCell::new(),
        }
    }

    pub(in crate::semantic) fn typ(&self, db: &DbIndex) -> &LuaType {
        self.typ.get_or_init(|| {
            let typ = match &self.origin {
                MemberOrigin::Indexed(item) => item.resolve_type(db).unwrap_or(LuaType::Unknown),
                MemberOrigin::Decl(id) => db
                    .get_type_index()
                    .get_type_cache(&(*id).into())
                    .map(|cache| cache.as_type().clone())
                    .unwrap_or(LuaType::Unknown),
                MemberOrigin::Type(typ) => typ.clone(),
                MemberOrigin::Union(members) => {
                    TypeOps::union_all(db, members.iter().map(|member| member.typ(db).clone()))
                }
                MemberOrigin::Intersection(members) => members
                    .iter()
                    .map(|member| member.typ(db).clone())
                    .reduce(|left, right| intersect_member_types(db, left, right))
                    .unwrap_or(LuaType::Unknown),
            };
            instantiate(db, &typ, self.substitutor.as_deref())
        })
    }
}

/// 收集类型的全部成员.
pub(in crate::semantic) fn collect_type_members(db: &DbIndex, typ: &LuaType) -> TypeMembers {
    let mut visiting = HashSet::default();
    collect_members(db, typ, None, &mut visiting).unwrap_or_default()
}

fn insert_member(
    db: &DbIndex,
    members: &mut TypeMembers,
    key: LuaMemberKey,
    origin: MemberOrigin,
    substitutor: &Option<Rc<TypeSubstitutor>>,
) {
    // 索引键决定属性归属, 建表时确定键, 字段值仍保留声明和替换上下文.
    let key = match key {
        LuaMemberKey::TypeKey(key) => {
            LuaMemberKey::TypeKey(instantiate(db, &key, substitutor.as_deref()))
        }
        key => key,
    };
    members
        .entry(key)
        .or_insert_with(|| MemberSymbol::new(origin, substitutor.clone()));
}

// None 表示 never, 空表则表示没有已知属性.
fn collect_members(
    db: &DbIndex,
    typ: &LuaType,
    substitutor: Option<Rc<TypeSubstitutor>>,
    visiting: &mut HashSet<LuaType>,
) -> Option<TypeMembers> {
    let mut members = TypeMembers::default();
    match typ {
        LuaType::Never => return None,
        LuaType::Ref(id) | LuaType::Def(id) => {
            return collect_declared_members(db, id, None, None, visiting);
        }
        LuaType::Generic(generic) => {
            return collect_declared_members(
                db,
                generic.get_base_type_id_ref(),
                Some(generic.get_params()),
                substitutor,
                visiting,
            );
        }
        LuaType::Object(object) => {
            for (key, typ) in object.get_fields() {
                insert_member(
                    db,
                    &mut members,
                    key.clone(),
                    MemberOrigin::Type(typ.clone()),
                    &substitutor,
                );
            }
            for (key, typ) in object.get_index_access() {
                insert_member(
                    db,
                    &mut members,
                    LuaMemberKey::TypeKey(key.clone()),
                    MemberOrigin::Type(typ.clone()),
                    &substitutor,
                );
            }
        }
        LuaType::Tuple(tuple) => {
            for (index, typ) in tuple.get_types().iter().enumerate() {
                if let (LuaType::Variadic(variadic), Some(substitutor)) = (typ, &substitutor) {
                    if let VariadicType::Base(LuaType::TplRef(tpl)) = variadic.as_ref() {
                        let typ = substitutor
                            .resolve_type(
                                tpl.get_tpl_id(),
                                GenericResolveMode::Value,
                                tpl.is_const(),
                            )
                            .unwrap_or(typ);
                        insert_member(
                            db,
                            &mut members,
                            LuaMemberKey::Integer(index as i64 + 1),
                            MemberOrigin::Type(typ.clone()),
                            &None,
                        );
                    }
                    break;
                }
                insert_member(
                    db,
                    &mut members,
                    LuaMemberKey::Integer(index as i64 + 1),
                    MemberOrigin::Type(typ.clone()),
                    &substitutor,
                );
            }
        }
        LuaType::TableGeneric(params) if params.len() == 2 => {
            insert_member(
                db,
                &mut members,
                LuaMemberKey::TypeKey(params[0].clone()),
                MemberOrigin::Type(params[1].clone()),
                &substitutor,
            );
        }
        LuaType::Array(array) => {
            insert_member(
                db,
                &mut members,
                LuaMemberKey::TypeKey(LuaType::Integer),
                MemberOrigin::Type(array.get_base().clone()),
                &substitutor,
            );
        }
        LuaType::TableConst(range) => {
            return Some(collect_owner_members(
                db,
                &LuaMemberOwner::Element(range.clone()),
                &substitutor,
            ));
        }
        LuaType::Union(union) => return collect_union_members(db, union, substitutor, visiting),
        LuaType::Intersection(intersection) => {
            let mut combined: IndexMap<_, Vec<_>, FxBuildHasher> = IndexMap::default();
            for typ in intersection.get_types() {
                for (key, member) in collect_members(db, typ, substitutor.clone(), visiting)? {
                    combined.entry(key).or_default().push(member);
                }
            }
            return Some(
                combined
                    .into_iter()
                    .map(|(key, members)| {
                        (
                            key,
                            MemberSymbol::new(MemberOrigin::Intersection(members), None),
                        )
                    })
                    .collect(),
            );
        }
        LuaType::MultiLineUnion(union) => {
            return collect_members(db, &union.to_union(), substitutor, visiting);
        }
        LuaType::ModuleRef(file_id) => {
            if let Some(typ) = db
                .get_module_index()
                .get_module(*file_id)
                .and_then(|module| module.export_type.as_ref())
            {
                return collect_members(db, typ, None, visiting);
            }
        }
        LuaType::Instance(instance) => {
            members = collect_owner_members(
                db,
                &LuaMemberOwner::Element(instance.get_range().clone()),
                &substitutor,
            );
            merge_members(
                &mut members,
                collect_members(db, instance.get_base(), substitutor, visiting).unwrap_or_default(),
            );
        }
        LuaType::Global => {
            for id in db.get_global_index().get_all_global_decl_ids() {
                if let Some(decl) = db.get_decl_index().get_decl(&id) {
                    insert_member(
                        db,
                        &mut members,
                        LuaMemberKey::Name(decl.get_name().into()),
                        MemberOrigin::Decl(id),
                        &substitutor,
                    );
                }
            }
        }
        LuaType::Namespace(namespace) => {
            let prefix = format!("{namespace}.");
            for (name, id) in db.get_type_index().find_type_decls(
                FileId::VIRTUAL,
                &prefix,
                db.resolve_workspace_id(FileId::VIRTUAL),
            ) {
                let typ = match id {
                    Some(id) => LuaType::Def(id),
                    None => LuaType::Namespace(SmolStr::new(format!("{namespace}.{name}")).into()),
                };
                insert_member(
                    db,
                    &mut members,
                    LuaMemberKey::Name(name.into()),
                    MemberOrigin::Type(typ),
                    &substitutor,
                );
            }
        }
        LuaType::TplRef(_) | LuaType::Call(_) | LuaType::Conditional(_) | LuaType::Mapped(_) => {
            // 属性集合依赖类型表达式时, 先求出结构, 再为其中的字段建表.
            let resolved = instantiate(db, typ, substitutor.as_deref());
            if resolved != *typ {
                return collect_members(db, &resolved, None, visiting);
            }
        }
        _ => {
            if let Some(id) = get_buildin_type_map_type_id(typ) {
                return collect_declared_members(db, &id, None, None, visiting);
            }
        }
    }
    Some(members)
}

fn collect_declared_members(
    db: &DbIndex,
    id: &LuaTypeDeclId,
    params: Option<&[LuaType]>,
    context: Option<Rc<TypeSubstitutor>>,
    visiting: &mut HashSet<LuaType>,
) -> Option<TypeMembers> {
    let type_index = db.get_type_index();
    let Some(decl) = type_index.get_type_decl(id) else {
        return Some(TypeMembers::default());
    };
    let params: Option<Vec<_>> = params.map(|params| {
        params
            .iter()
            .map(|typ| instantiate(db, typ, context.as_deref()))
            .collect()
    });
    let visiting_type = match &params {
        Some(params) if decl.is_alias() => {
            LuaType::Generic(LuaGenericType::new(id.clone(), params.clone()).into())
        }
        _ => LuaType::Ref(id.clone()),
    };
    // 别名按实参区分嵌套展开, 同时限制实参不断增长的递归.
    if visiting.len() >= 128 || !visiting.insert(visiting_type.clone()) {
        return Some(TypeMembers::default());
    }
    let substitutor = params.map(|params| {
        Rc::new(if decl.is_alias() {
            TypeSubstitutor::from_alias(params, id.clone())
        } else {
            TypeSubstitutor::from_type_array(params)
        })
    });

    let members = if decl.is_alias() {
        match decl.get_alias_ref() {
            Some(origin) => collect_members(db, origin, substitutor, visiting),
            None => Some(TypeMembers::default()),
        }
    } else {
        let mut members =
            collect_owner_members(db, &LuaMemberOwner::Type(id.clone()), &substitutor);
        if decl.is_class()
            && let Some(supers) = type_index.get_super_types_iter(id)
        {
            for typ in supers {
                merge_members(
                    &mut members,
                    collect_members(db, typ, substitutor.clone(), visiting).unwrap_or_default(),
                );
            }
        }
        Some(members)
    };
    visiting.remove(&visiting_type);
    members
}

fn collect_owner_members(
    db: &DbIndex,
    owner: &LuaMemberOwner,
    substitutor: &Option<Rc<TypeSubstitutor>>,
) -> TypeMembers {
    let mut members = TypeMembers::default();
    if let Some(items) = db.get_member_index().get_member_items(owner) {
        for (key, item) in items {
            insert_member(
                db,
                &mut members,
                key.clone(),
                MemberOrigin::Indexed(item.clone()),
                substitutor,
            );
        }
    }
    members
}

fn collect_union_members(
    db: &DbIndex,
    union: &LuaUnionType,
    substitutor: Option<Rc<TypeSubstitutor>>,
    visiting: &mut HashSet<LuaType>,
) -> Option<TypeMembers> {
    let mut common: Option<IndexMap<_, Vec<_>, FxBuildHasher>> = None;
    for branch in union.iter() {
        let Some(mut members) = collect_members(db, &branch, substitutor.clone(), visiting) else {
            continue;
        };
        if let Some(common) = &mut common {
            // 联合只保留共有属性, 成员类型在访问时再合并.
            common.retain(|key, common_members| {
                if let Some(member) = members.swap_remove(key) {
                    common_members.push(member);
                    true
                } else {
                    false
                }
            });
        } else {
            common = Some(
                members
                    .into_iter()
                    .map(|(key, member)| (key, vec![member]))
                    .collect(),
            );
        }
        if common.as_ref().is_some_and(IndexMap::is_empty) {
            break;
        }
    }
    Some(
        common?
            .into_iter()
            .map(|(key, members)| (key, MemberSymbol::new(MemberOrigin::Union(members), None)))
            .collect(),
    )
}

fn instantiate(db: &DbIndex, typ: &LuaType, substitutor: Option<&TypeSubstitutor>) -> LuaType {
    match substitutor {
        Some(substitutor) => instantiate_type_generic(db, typ, substitutor),
        None => typ.clone(),
    }
}

fn merge_members(members: &mut TypeMembers, inherited: TypeMembers) {
    for (key, member) in inherited {
        members.entry(key).or_insert(member);
    }
}

#[cfg(test)]
mod test {
    use std::{ptr, sync::Arc};

    use crate::{
        LuaIndex, LuaMemberIndexItem, LuaMemberKey, LuaMemberOwner, LuaMultiLineUnion, LuaType,
        LuaTypeDeclId, LuaUnionMembers, LuaUnionType, VirtualWorkspace, find_members_with_key,
        semantic::{cache::SemanticLocalCache, type_check::is_assignable},
    };

    use super::{MemberOrigin, collect_type_members};

    #[test]
    fn collected_lookup_preserves_inheritance_and_composite_members() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class CacheBase<T>
            ---@field inherited T
            ---@field shared T
            ---@field [integer] T
            ---@class CacheOther
            ---@field other boolean
            ---@field shared number
            ---@class CacheChild<T>: CacheBase<T>, CacheOther
            ---@field own T
            ---@field shared string
            ---@class CacheDiamond: CacheChild<string>, CacheBase<number>
            ---@class CacheCycleA: CacheCycleB
            ---@field a string
            ---@class CacheCycleB: CacheCycleA
            ---@field b number
            ---@alias CacheShape<T> { alias: T }
            ---@class CacheAliasChild<T>: CacheShape<T>
            ---@alias CacheIntersection { overlap: string | number } & { overlap: number }
            ---@class CacheIntersectionChild: CacheIntersection
            ---@class CacheObjectChild<T>: { direct: T }
            ---@class CacheTupleChild<T>: [T, boolean]
            ---@class CacheTableChild<T>: table<string, T>
        "#,
        );
        let types = [
            "CacheChild<string>",
            "CacheChild<number>",
            "CacheDiamond",
            "CacheCycleA",
            "CacheCycleB",
            "CacheAliasChild<string>",
            "CacheIntersectionChild",
            "CacheObjectChild<number>",
            "CacheTupleChild<string>",
            "CacheTableChild<number>",
        ]
        .map(|typ| ws.ty(typ));
        let mut keys = [
            "own",
            "shared",
            "inherited",
            "other",
            "a",
            "b",
            "alias",
            "overlap",
            "direct",
            "missing",
        ]
        .map(|key| LuaMemberKey::Name(key.into()))
        .to_vec();
        keys.push(LuaMemberKey::Integer(1));
        keys.push(LuaMemberKey::TypeKey(LuaType::Integer));
        keys.push(LuaMemberKey::TypeKey(LuaType::String));
        let db = ws.analysis.compilation.get_db();
        for typ in &types {
            let members = collect_type_members(db, typ);
            for key in &keys {
                let expected = find_members_with_key(db, typ, key.clone(), false)
                    .and_then(|members| members.into_iter().next())
                    .map(|member| member.typ);
                assert_eq!(
                    members.get(key).map(|member| member.typ(db).clone()),
                    expected,
                    "{typ:?}, {key:?}"
                );
            }
        }
    }

    #[test]
    fn union_lookup_keeps_common_keys_and_unions_their_types() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class UnionCacheBase<T>
            ---@field common T
            ---@field [1] T
            ---@class UnionCacheLeft: UnionCacheBase<string>
            ---@field left boolean
            ---@class UnionCacheRight: UnionCacheBase<number>
            ---@field right boolean
            ---@alias UnionCacheAlias UnionCacheLeft | UnionCacheRight
            ---@class UnionCacheChild: UnionCacheAlias
            ---@class UnionCacheOverride: UnionCacheAlias
            ---@field common boolean
        "#,
        );
        let left = ws.ty("UnionCacheLeft");
        let right = ws.ty("UnionCacheRight");
        let union = ws.ty("UnionCacheLeft | UnionCacheRight");
        let mut types = vec![
            union.clone(),
            ws.ty("UnionCacheRight | UnionCacheLeft"),
            ws.ty("UnionCacheAlias"),
            ws.ty("UnionCacheChild"),
        ];
        types.push(LuaType::MultiLineUnion(
            LuaMultiLineUnion::new(vec![(left.clone(), None), (right, None)]).into(),
        ));
        types.push(LuaType::Union(
            LuaUnionType::Multi(LuaUnionMembers::new(vec![left, union.clone()])).into(),
        ));
        types.push(LuaType::Union(
            LuaUnionType::Multi(LuaUnionMembers::new(vec![
                LuaType::Never,
                union,
                LuaType::Never,
            ]))
            .into(),
        ));
        let override_type = ws.ty("UnionCacheOverride");
        let expected = ws.ty("string | number");
        let db = ws.analysis.compilation.get_db();
        for typ in &types {
            let members = collect_type_members(db, typ);
            assert_eq!(members.len(), 2, "{typ:?}");
            assert!(
                members
                    .keys()
                    .all(|key| !matches!(key, LuaMemberKey::TypeKey(_)))
            );
            for key in [
                LuaMemberKey::Name("common".into()),
                LuaMemberKey::Integer(1),
            ] {
                assert_eq!(
                    members.get(&key).map(|member| member.typ(db).clone()),
                    Some(expected.clone())
                );
            }
            for key in ["left", "right"] {
                assert_eq!(
                    members
                        .get(&LuaMemberKey::Name(key.into()))
                        .map(|member| member.typ(db).clone()),
                    None
                );
            }
        }
        assert_eq!(
            collect_type_members(db, &override_type)
                .get(&LuaMemberKey::Name("common".into()))
                .map(|member| member.typ(db).clone()),
            Some(LuaType::Boolean)
        );
    }

    #[test]
    fn union_indexes_compare_instantiated_keys() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class UnionCacheMap<K, V>
            ---@field [K] V
            ---@class UnionCacheMapChild<K, V>: UnionCacheMap<K, V>
        "#,
        );
        let cases = [
            (
                "UnionCacheMap<string, string> | UnionCacheMap<string, number>",
                Some(("string", "string | number")),
            ),
            (
                "UnionCacheMapChild<string, string> | UnionCacheMapChild<string, number>",
                Some(("string", "string | number")),
            ),
            (
                "UnionCacheMap<string, string> | UnionCacheMap<integer, number>",
                None,
            ),
            (
                "{ [string]: string, [integer]: boolean } | { [string]: number }",
                Some(("string", "string | number")),
            ),
            ("string[] | number[]", Some(("integer", "string | number"))),
            ("table<string, string> | table<integer, number>", None),
            (
                "table<string, string> | { [string]: number }",
                Some(("string", "string | number")),
            ),
            ("{ [string]: string } | {}", None),
        ]
        .map(|(typ, index)| {
            (
                ws.ty(typ),
                index.map(|(key, value)| (ws.ty(key), ws.ty(value))),
            )
        });
        let db = ws.analysis.compilation.get_db();
        for (typ, index) in cases {
            let members = collect_type_members(db, &typ);
            assert!(
                members
                    .keys()
                    .all(|key| matches!(key, LuaMemberKey::TypeKey(_)))
            );
            if let Some((key, value)) = index {
                assert_eq!(members.len(), 1, "{typ:?}");
                assert_eq!(
                    members
                        .get(&LuaMemberKey::TypeKey(key))
                        .map(|member| member.typ(db).clone()),
                    Some(value),
                    "{typ:?}"
                );
            } else {
                assert!(members.is_empty(), "{typ:?}");
            }
        }
    }

    #[test]
    fn union_members_instantiate_only_when_read() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@class UnionCacheLazy<T>\n---@field used T[]\n---@field unused T");
        let source = ws.ty("UnionCacheLazy<string> | UnionCacheLazy<number>");
        let expected = ws.ty("string[] | number[]");
        let db = ws.analysis.compilation.get_db();
        let members = collect_type_members(db, &source);
        for member in members.values() {
            let MemberOrigin::Union(branches) = &member.origin else {
                panic!("expected a union member");
            };
            assert!(member.typ.get().is_none());
            assert!(branches.iter().all(|member| member.typ.get().is_none()));
        }
        let key = LuaMemberKey::Name("used".into());
        assert_eq!(
            members.get(&key).map(|member| member.typ(db).clone()),
            Some(expected)
        );
        let used = &members[&key];
        let MemberOrigin::Union(branches) = &used.origin else {
            panic!("expected a union member");
        };
        assert!(used.typ.get().is_some());
        assert!(branches.iter().all(|member| member.typ.get().is_some()));
        let unused = &members[&LuaMemberKey::Name("unused".into())];
        let MemberOrigin::Union(branches) = &unused.origin else {
            panic!("expected a union member");
        };
        assert!(unused.typ.get().is_none());
        assert!(branches.iter().all(|member| member.typ.get().is_none()));
    }

    #[test]
    fn union_members_handle_empty_branches_cycles_and_duplicate_declarations() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class UnionCacheCycleA: UnionCacheCycleB
            ---@field a string
            ---@class UnionCacheCycleB: UnionCacheCycleA
            ---@field b number
            ---@class UnionCacheDuplicate
            ---@field value string
            ---@field value boolean
            ---@class UnionCacheNumber
            ---@field value number
        "#,
        );
        let empty_types = [
            "UnionCacheCycleA | {}",
            "{} | UnionCacheCycleA",
            "UnionCacheCycleA?",
            "UnionCacheCycleA | number",
            "{ left: string } | { right: number }",
        ]
        .map(|typ| ws.ty(typ));
        let cycle = ws.ty("UnionCacheCycleA | UnionCacheCycleB");
        let duplicate = ws.ty("UnionCacheDuplicate | UnionCacheNumber");
        let expected = ws.ty("string | boolean | number");
        let db = ws.analysis.compilation.get_db();
        for typ in empty_types {
            let members = collect_type_members(db, &typ);
            assert!(members.is_empty(), "{typ:?}");
        }
        // 索引已经过滤循环继承边, 两个分支只剩各自声明, 没有共有成员.
        assert!(collect_type_members(db, &cycle).is_empty());
        assert_eq!(
            collect_type_members(db, &duplicate)
                .get(&LuaMemberKey::Name("value".into()))
                .map(|member| member.typ(db).clone()),
            Some(expected)
        );
    }

    #[test]
    fn generic_member_reuses_instantiated_type_in_either_order() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@class SharedCacheMember<T>\n---@field values T[]");
        let typ = ws.ty("SharedCacheMember<string>");
        let db = ws.analysis.compilation.get_db();
        let key = LuaMemberKey::Name("values".into());
        for lookup_first in [false, true] {
            let members = collect_type_members(db, &typ);
            let (_, enumerated) = members.iter().next().unwrap();
            assert!(ptr::eq(enumerated, &members[&key]));
            assert!(enumerated.typ.get().is_none());
            let (first, second) = if lookup_first {
                (members[&key].typ(db).clone(), enumerated.typ(db).clone())
            } else {
                (enumerated.typ(db).clone(), members[&key].typ(db).clone())
            };
            let (LuaType::Array(first), LuaType::Array(second)) = (first, second) else {
                panic!("expected an instantiated array member");
            };
            assert!(Arc::ptr_eq(&first, &second));
            assert_eq!(first.get_base(), &LuaType::String);
        }
    }

    #[test]
    fn non_generic_parent_keeps_its_own_member_context() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class CacheMethodParent
            local parent = {}
            function parent:run() end

            ---@class CacheMethodChild<T>: CacheMethodParent
            local child = {}
        "#,
        );
        let typ = ws.ty("CacheMethodChild<string>");
        let parent = ws.ty("CacheMethodParent");
        let db = ws.analysis.compilation.get_db();
        let key = LuaMemberKey::Name("run".into());
        let members = collect_type_members(db, &typ);
        let (_, enumerated) = members.iter().next().unwrap();
        assert!(ptr::eq(enumerated, &members[&key]));
        assert!(enumerated.substitutor.is_none());
        assert!(matches!(enumerated.typ(db), LuaType::Signature(_)));
        let expected = find_members_with_key(db, &parent, key.clone(), false)
            .unwrap()
            .remove(0)
            .typ;
        assert_eq!(enumerated.typ(db), &expected);
        assert_eq!(members[&key].typ(db), &expected);
    }

    #[test]
    fn lookup_and_enumeration_share_duplicate_declaration_type() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class CacheDuplicate
            ---@field value string
            ---@field value number
        "#,
        );
        let typ = ws.ty("CacheDuplicate");
        let string_field = ws.ty("{ value: string }");
        let number_field = ws.ty("{ value: number }");
        let union = ws.ty("string | number");
        let db = ws.analysis.compilation.get_db();
        let key = LuaMemberKey::Name("value".into());
        let owner = LuaMemberOwner::Type(LuaTypeDeclId::global("CacheDuplicate"));
        assert!(matches!(
            db.get_member_index().get_member_item(&owner, &key),
            Some(LuaMemberIndexItem::Many(_))
        ));
        let mut cache = SemanticLocalCache::default();
        let members = collect_type_members(db, &typ);
        assert_eq!(members.len(), 1);
        assert!(members[&key].typ.get().is_none());
        assert_eq!(
            members.get(&key).map(|member| member.typ(db).clone()),
            Some(union.clone())
        );
        let (_, member) = members.iter().next().unwrap();
        assert_eq!(member.typ(db), &union);
        assert!(!is_assignable(db, &typ, &string_field, Some(&mut cache)));
        assert!(is_assignable(db, &number_field, &typ, Some(&mut cache)));
    }

    #[test]
    fn unresolved_members_keep_their_symbols_and_override_parents() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class CacheResolvedParent<T>
            ---@field value T
            ---@class CacheUnresolvedPlain: CacheResolvedParent<string>
            ---@class CacheUnresolvedGeneric<T>: CacheResolvedParent<T>
        "#,
        );
        let fields = ws.def(
            r#"
            ---@class (partial) CacheUnresolvedPlain
            ---@field value integer
            ---@class (partial) CacheUnresolvedGeneric<T>
            ---@field value T
        "#,
        );
        let plain = ws.ty("CacheUnresolvedPlain");
        let generic = ws.ty("CacheUnresolvedGeneric<string>");
        let empty = ws.ty("{}");
        ws.analysis
            .compilation
            .get_db_mut()
            .get_type_index_mut()
            .remove(fields);
        let db = ws.analysis.compilation.get_db();
        let key = LuaMemberKey::Name("value".into());
        let mut cache = SemanticLocalCache::default();
        for typ in [&plain, &generic] {
            let members = collect_type_members(db, typ);
            assert_eq!(members.len(), 1);
            assert!(members[&key].typ.get().is_none());
            assert_eq!(
                members.get(&key).map(|member| member.typ(db).clone()),
                Some(LuaType::Unknown)
            );
            let (_, enumerated) = members.iter().next().unwrap();
            assert_eq!(enumerated.typ(db), &LuaType::Unknown);
            assert!(is_assignable(db, &empty, typ, Some(&mut cache)));
        }
    }

    #[test]
    fn inherited_members_use_the_same_override_order() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class CacheFirstParent
            ---@field value string
            ---@class CacheSecondParent
            ---@field value number
            ---@class CacheEmptyChild: CacheFirstParent, CacheSecondParent
        "#,
        );
        let typ = ws.ty("CacheEmptyChild");
        let source = ws.ty("{ value: string }");
        let mismatch = ws.ty("{ value: number }");
        let db = ws.analysis.compilation.get_db();
        let members = collect_type_members(db, &typ);
        let types = members
            .values()
            .map(|member| member.typ(db).clone())
            .collect::<Vec<_>>();
        assert_eq!(types, vec![LuaType::String]);
        assert_eq!(
            members
                .get(&LuaMemberKey::Name("value".into()))
                .map(|member| member.typ(db).clone()),
            Some(LuaType::String)
        );
        assert!(is_assignable(db, &source, &typ, None));
        assert!(!is_assignable(db, &mismatch, &typ, None));
    }

    #[test]
    fn missing_lookup_builds_all_symbols_without_instantiating_fields() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias LazySymbolShape<K, V> { used: V[], unused: { nested: V }, [K]: V }
            ---@class LazySymbolChild<T>: LazySymbolShape<string, T>
            ---@field own T
        "#,
        );
        let typ = ws.ty("LazySymbolChild<number>");
        let expected = ws.ty("number[]");
        let db = ws.analysis.compilation.get_db();
        let members = collect_type_members(db, &typ);
        assert!(members.get(&LuaMemberKey::Name("missing".into())).is_none());
        assert_eq!(members.len(), 4);
        assert!(members.contains_key(&LuaMemberKey::TypeKey(LuaType::String)));
        assert!(members.values().all(|member| member.typ.get().is_none()));
        let used_key = LuaMemberKey::Name("used".into());
        let MemberOrigin::Type(LuaType::Array(raw)) = &members[&used_key].origin else {
            panic!("expected an uninstantiated alias field");
        };
        assert!(matches!(raw.get_base(), LuaType::TplRef(_)));
        assert_eq!(
            members.get(&used_key).map(|member| member.typ(db).clone()),
            Some(expected)
        );
        assert!(members[&used_key].typ.get().is_some());
        for (key, member) in members {
            if key != used_key {
                assert!(member.typ.get().is_none(), "{key:?}");
            }
        }
    }

    #[test]
    fn intersection_members_merge_only_when_read() {
        let mut ws = VirtualWorkspace::new();
        let typ =
            ws.ty("{ value: string | number, left: boolean } & { value: number, right: string }");
        let db = ws.analysis.compilation.get_db();
        let members = collect_type_members(db, &typ);
        assert_eq!(members.len(), 3);
        assert!(members.values().all(|member| member.typ.get().is_none()));
        let key = LuaMemberKey::Name("value".into());
        assert_eq!(
            members.get(&key).map(|member| member.typ(db).clone()),
            Some(LuaType::Number)
        );
        let MemberOrigin::Intersection(branches) = &members[&key].origin else {
            panic!("expected an intersection member");
        };
        assert!(branches.iter().all(|member| member.typ.get().is_some()));
        for name in ["left", "right"] {
            assert!(
                members[&LuaMemberKey::Name(name.into())]
                    .typ
                    .get()
                    .is_none()
            );
        }
        let (_, enumerated) = members.iter().find(|(field, _)| **field == key).unwrap();
        assert_eq!(enumerated.typ(db), &LuaType::Number);
    }

    #[test]
    fn repeated_generic_parents_keep_distinct_index_symbols() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class CacheIndexParent<K, V>
            ---@field [K] V
            ---@class CacheIndexChild: CacheIndexParent<string, boolean>, CacheIndexParent<integer, number>
        "#,
        );
        let typ = ws.ty("CacheIndexChild");
        let db = ws.analysis.compilation.get_db();
        let members = collect_type_members(db, &typ);
        assert_eq!(members.len(), 2);
        assert!(members.values().all(|member| member.typ.get().is_none()));
        assert_eq!(
            members
                .get(&LuaMemberKey::TypeKey(LuaType::String))
                .map(|member| member.typ(db).clone()),
            Some(LuaType::Boolean)
        );
        let integer_key = LuaMemberKey::TypeKey(LuaType::Integer);
        assert!(members[&integer_key].typ.get().is_none());
        assert_eq!(
            members
                .get(&integer_key)
                .map(|member| member.typ(db).clone()),
            Some(LuaType::Number)
        );
    }

    #[test]
    fn recursive_alias_fields_stay_lazy() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@alias CacheLazyNode<T> { value: T, next: CacheLazyNode<T>? }");
        let typ = ws.ty("CacheLazyNode<string>");
        let expected = ws.ty("CacheLazyNode<string>?");
        let db = ws.analysis.compilation.get_db();
        let members = collect_type_members(db, &typ);
        assert_eq!(members.len(), 2);
        assert!(members.values().all(|member| member.typ.get().is_none()));
        let value_key = LuaMemberKey::Name("value".into());
        assert_eq!(
            members.get(&value_key).map(|member| member.typ(db).clone()),
            Some(LuaType::String)
        );
        let next_key = LuaMemberKey::Name("next".into());
        assert!(members[&next_key].typ.get().is_none());
        assert_eq!(
            members.get(&next_key).map(|member| member.typ(db).clone()),
            Some(expected)
        );
    }

    #[test]
    fn global_symbols_resolve_only_the_requested_declaration() {
        let mut ws = VirtualWorkspace::new();
        ws.def("cache_global_used = 'value'\ncache_global_unused = { value = 1 }");
        let db = ws.analysis.compilation.get_db();
        let typ = LuaType::Global;
        let members = collect_type_members(db, &typ);
        assert!(members.values().all(|member| member.typ.get().is_none()));
        let used = LuaMemberKey::Name("cache_global_used".into());
        let expected = find_members_with_key(db, &typ, used.clone(), false)
            .unwrap()
            .remove(0)
            .typ;
        assert_eq!(
            members.get(&used).map(|member| member.typ(db).clone()),
            Some(expected)
        );
        let unused = &members[&LuaMemberKey::Name("cache_global_unused".into())];
        assert!(matches!(unused.origin, MemberOrigin::Decl(_)));
        assert!(unused.typ.get().is_none());
    }

    #[test]
    fn nested_alias_arguments_keep_their_members() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias CacheIdentity<T> T
            ---@alias CacheExtension<T> T & { flag: boolean }
        "#,
        );
        let types = [
            ws.ty("CacheIdentity<CacheIdentity<{ value: string }>>"),
            ws.ty("CacheExtension<CacheExtension<{ value: string }>>"),
        ];
        let db = ws.analysis.compilation.get_db();
        for typ in types {
            assert_eq!(
                collect_type_members(db, &typ)
                    .get(&LuaMemberKey::Name("value".into()))
                    .map(|member| member.typ(db).clone()),
                Some(LuaType::String)
            );
        }
    }

    #[test]
    fn union_ignores_branches_instantiated_to_never() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias CacheMaybe<T> T | { value: string }
            ---@alias CacheImpossible<T> never
        "#,
        );
        let types = [
            ws.ty("CacheMaybe<never>"),
            ws.ty("CacheMaybe<CacheImpossible<string>>"),
        ];
        let db = ws.analysis.compilation.get_db();
        for typ in types {
            assert_eq!(
                collect_type_members(db, &typ)
                    .get(&LuaMemberKey::Name("value".into()))
                    .map(|member| member.typ(db).clone()),
                Some(LuaType::String)
            );
        }
    }

    #[test]
    fn generic_tuple_alias_resolves_variadic_slots() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@alias CacheTupleTail<T> [T[], T...]");
        let typ = ws.ty("CacheTupleTail<string>");
        let db = ws.analysis.compilation.get_db();
        let members = collect_type_members(db, &typ);
        assert_eq!(
            members
                .get(&LuaMemberKey::Integer(2))
                .map(|member| member.typ(db).clone()),
            Some(LuaType::String)
        );
        let unused = &members[&LuaMemberKey::Integer(1)];
        assert!(unused.typ.get().is_none());
        let MemberOrigin::Type(LuaType::Array(raw)) = &unused.origin else {
            panic!("expected an uninstantiated tuple field");
        };
        assert!(matches!(raw.get_base(), LuaType::TplRef(_)));
    }

    #[test]
    fn recursive_alias_shapes_stop_when_arguments_keep_growing() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@alias CacheGrowing<T> CacheGrowing<T[]>");
        let typ = ws.ty("CacheGrowing<string>");
        let db = ws.analysis.compilation.get_db();
        let members = collect_type_members(db, &typ);
        assert!(members.is_empty());
        assert!(members.get(&LuaMemberKey::Integer(1)).is_none());
    }
}
