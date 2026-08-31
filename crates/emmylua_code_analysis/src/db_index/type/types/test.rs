#[cfg(test)]
mod tests {

    use smol_str::SmolStr;
    use std::mem::ManuallyDrop;

    use crate::{
        GenericTpl, GenericTplId, LuaArrayType, LuaIndexAccessKey, LuaMemberKey, LuaObjectType,
        LuaType, TypeVisitTrait, VariadicType,
    };

    #[test]
    fn test_object_member_lookup_includes_exact_index_access() {
        let field_key = LuaMemberKey::Name(SmolStr::new("name"));
        let index_key = LuaMemberKey::TypeKey(LuaType::String);
        let object = LuaObjectType::new(vec![
            (
                LuaIndexAccessKey::String(SmolStr::new("name")),
                LuaType::String,
            ),
            (LuaIndexAccessKey::Type(LuaType::String), LuaType::Number),
        ]);

        assert_eq!(object.get_member_type(&field_key), Some(LuaType::String));
        assert_eq!(object.get_field(&index_key), None);
        assert_eq!(object.get_member_type(&index_key), Some(LuaType::Number));
    }

    #[test]
    fn test_union_with_variadic_uses_result_slot_extraction() {
        let variadic = LuaType::Variadic(VariadicType::Multi(vec![LuaType::String]).into());
        let optional_variadic = LuaType::from_vec(vec![variadic.clone(), LuaType::Nil]);

        assert_eq!(variadic.get_result_slot_type(0), Some(LuaType::String));
        assert!(!optional_variadic.is_multi_return());
        assert!(optional_variadic.contain_multi_return());
        assert_eq!(
            optional_variadic.get_result_slot_type(0),
            Some(LuaType::from_vec(vec![LuaType::String, LuaType::Nil]))
        );
    }

    #[test]
    fn test_deep_contain_tpl_uses_iterative_walk() {
        let mut ty = LuaType::TplRef(
            GenericTpl::new(
                GenericTplId::Type(0),
                SmolStr::new("T"),
                None,
                None,
                false,
                None,
            )
            .into(),
        );

        for _ in 0..20_000 {
            ty = LuaType::Array(LuaArrayType::from_base_type(ty).into());
        }

        let ty = ManuallyDrop::new(ty);
        assert!(ty.contain_tpl());
    }

    #[test]
    fn test_deep_visit_type_uses_iterative_walk() {
        let depth = 20_000;
        let mut ty = LuaType::String;

        for _ in 0..depth {
            ty = LuaType::Array(LuaArrayType::from_base_type(ty).into());
        }

        let ty = ManuallyDrop::new(ty);
        let mut visited = 0;
        ty.visit_type(&mut |_| {
            visited += 1;
        });

        assert_eq!(visited, depth + 1);
    }

    #[test]
    fn test_union_iterator() {
        use crate::{BasicTypeKind, BasicTypeUnion, LuaMultiLineUnion, LuaUnionType};

        // 基础类型联合
        let mut basic = BasicTypeUnion::new();
        basic.add(BasicTypeKind::Boolean);
        basic.add(BasicTypeKind::String);
        let union_basic = LuaUnionType::Basic(basic);
        let members: Vec<LuaType> = union_basic.iter().map(|c| c.into_owned()).collect();
        assert_eq!(members, vec![LuaType::Boolean, LuaType::String]);
        assert_eq!(union_basic.iter().len(), 2);

        // 可空联合
        let union_nullable = LuaUnionType::Nullable(LuaType::Integer);
        let mut iter = union_nullable.iter();
        assert_eq!(
            iter.next(),
            Some(std::borrow::Cow::Borrowed(&LuaType::Integer))
        );
        assert_eq!(iter.next(), Some(std::borrow::Cow::Owned(LuaType::Nil)));
        assert_eq!(iter.next(), None);
        assert_eq!(union_nullable.iter().len(), 2);

        // 多成员联合
        let union_multi =
            LuaUnionType::Multi(vec![LuaType::Integer, LuaType::String, LuaType::Boolean]);
        let mut iter = union_multi.iter();
        assert_eq!(
            iter.next(),
            Some(std::borrow::Cow::Borrowed(&LuaType::Integer))
        );
        assert_eq!(
            iter.next(),
            Some(std::borrow::Cow::Borrowed(&LuaType::String))
        );
        assert_eq!(
            iter.next(),
            Some(std::borrow::Cow::Borrowed(&LuaType::Boolean))
        );
        assert_eq!(iter.next(), None);
        assert_eq!(union_multi.iter().len(), 3);

        // 多行联合
        let multi_line = LuaMultiLineUnion::new(vec![
            (LuaType::Number, Some("doc1".to_string())),
            (LuaType::String, None),
        ]);
        let types: Vec<&LuaType> = multi_line.iter().collect();
        assert_eq!(types, vec![&LuaType::Number, &LuaType::String]);
        assert_eq!(multi_line.iter().len(), 2);
    }
}
