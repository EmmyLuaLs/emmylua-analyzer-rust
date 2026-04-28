#[cfg(test)]
mod tests {
    use internment::ArcIntern;
    use smol_str::SmolStr;
    use std::mem::ManuallyDrop;

    use crate::{GenericTpl, GenericTplId, LuaArrayType, LuaType, TypeVisitTrait, VariadicType};

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
                ArcIntern::new(SmolStr::new("T")),
                None,
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
}
