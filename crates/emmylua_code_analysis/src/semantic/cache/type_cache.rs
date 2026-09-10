use std::{cell::OnceCell, rc::Rc};

use crate::{
    DbIndex, LuaMemberKey, LuaType,
    semantic::member::{TypeMembers, collect_type_members},
};

#[derive(Debug, Default)]
pub(in crate::semantic) struct TypeCacheEntry {
    members: OnceCell<TypeMembers>,
    pub(in crate::semantic) call_signatures: OnceCell<Option<Rc<[LuaType]>>>,
}

impl TypeCacheEntry {
    pub(in crate::semantic) fn members(&self, db: &DbIndex, typ: &LuaType) -> &TypeMembers {
        self.members.get_or_init(|| collect_type_members(db, typ))
    }

    pub(in crate::semantic) fn member_type(
        &self,
        db: &DbIndex,
        typ: &LuaType,
        key: &LuaMemberKey,
    ) -> Option<&LuaType> {
        self.members(db, typ).get(key).map(|member| member.typ(db))
    }
}

#[cfg(test)]
mod test {
    use std::ptr;

    use crate::{
        LuaMemberKey, LuaType, VirtualWorkspace,
        semantic::{
            cache::SemanticLocalCache,
            type_check::{AssignabilityResult, check_assignable, is_assignable},
        },
    };

    #[test]
    fn union_cache_preserves_branch_relations_and_diagnostics() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias UnionCacheParent { common: string, left: boolean } | { common: number, right: boolean }
            ---@class UnionCacheInherited: UnionCacheParent
        "#,
        );
        let cases = [
            ("UnionCacheInherited", "{ common: string | number }", true),
            ("UnionCacheInherited", "{ common: string }", false),
            ("UnionCacheInherited", "{ left: boolean }", false),
            (
                "UnionCacheInherited & { extra: boolean }",
                "{ common: string, extra: boolean }",
                false,
            ),
            ("{ left: string } | { right: number }", "{}", true),
            ("{}", "{ left: string } | { right: number }", false),
            (
                "{ tag: 'a', value: string } | { tag: 'b', value: number }",
                "{ tag: 'a', value: number } | { tag: 'b', value: string }",
                false,
            ),
            ("fun(x: string) | fun(x: number)", "function", true),
            ("fun(x: string) | string", "function", false),
        ]
        .map(|(source, target, related)| (ws.ty(source), ws.ty(target), related));
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::default();
        for (source, target, related) in cases {
            let expected = check_assignable(db, &source, &target, None);
            assert_eq!(
                matches!(expected, AssignabilityResult::Assignable),
                related,
                "{source:?} -> {target:?}, {expected:?}"
            );
            for _ in 0..2 {
                assert_eq!(
                    is_assignable(db, &source, &target, Some(&mut cache)),
                    related
                );
                assert_eq!(
                    check_assignable(db, &source, &target, Some(&mut cache)),
                    expected
                );
                cache.type_entry(&source).members(db, &source);
                cache.type_entry(&target).members(db, &target);
            }
        }
    }

    #[test]
    fn generic_members_and_call_signatures_initialize_on_demand() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class LazyCacheBase<T>
            ---@field used T
            ---@field unused T[]
            ---@field [T] T
            ---@operator call(T): T
            ---@class LazyCacheChild: LazyCacheBase<string>
        "#,
        );
        let typ = ws.ty("LazyCacheChild");
        let callable = ws.ty("fun(value: string): string");
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::default();
        let entry = cache.type_entry(&typ);
        assert!(entry.members.get().is_none());
        assert!(entry.call_signatures.get().is_none());

        let key = LuaMemberKey::Name("used".into());
        assert_eq!(
            entry.member_type(db, &typ, &key).cloned(),
            Some(LuaType::String)
        );
        let members = entry.members.get().unwrap();
        assert_eq!(members.len(), 3);
        let (_, used) = members.iter().find(|(field, _)| **field == key).unwrap();
        assert!(ptr::eq(used, &members[&key]));
        assert_eq!(used.typ(db), &LuaType::String);
        assert!(entry.call_signatures.get().is_none());
        assert!(is_assignable(db, &typ, &callable, Some(&mut cache)));
        assert!(entry.call_signatures.get().unwrap().is_some());

        let mut call_cache = SemanticLocalCache::default();
        let call_entry = call_cache.type_entry(&typ);
        assert!(is_assignable(db, &typ, &callable, Some(&mut call_cache)));
        assert!(call_entry.members.get().is_none());
        assert!(call_entry.call_signatures.get().unwrap().is_some());
    }

    #[test]
    fn rebuilding_semantic_model_drops_previous_db_results() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def_file(
            "cache_model.lua",
            "---@class CachedModel\n---@field value string",
        );
        let source = ws.ty("CachedModel");
        let target = ws.ty("{ value: string }");
        {
            let model = ws.analysis.compilation.get_semantic_model(file).unwrap();
            assert!(model.is_assignable(&source, &target));
            assert!(model.is_assignable(&source, &target));
        }
        ws.def_file(
            "cache_model.lua",
            "---@class CachedModel\n---@field value number",
        );
        let model = ws.analysis.compilation.get_semantic_model(file).unwrap();
        assert!(!model.is_assignable(&source, &target));
        assert!(!model.is_assignable(&source, &target));
    }
}
