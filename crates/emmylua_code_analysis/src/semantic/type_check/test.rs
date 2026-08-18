#[cfg(test)]
mod test {
    use crate::{
        DbIndex, DiagnosticCode, GenericTpl, GenericTplId, LuaArrayLen, LuaArrayType,
        LuaGenericType, LuaIndexAccessKey, LuaIntersectionType, LuaObjectType, LuaType,
        LuaTypeDeclId, VirtualWorkspace, is_assignable,
        semantic::type_check::{
            AssignabilityResult, RelationOutcome, check_assignable, probe_assignable,
        },
    };

    #[test]
    fn test_string() {
        let mut ws = VirtualWorkspace::new();

        let string_ty = ws.ty("string");

        let right_ty = ws.ty("'ssss'");
        assert!(ws.check_type(&right_ty, &string_ty));

        let right_ty = ws.ty("number");
        assert!(!ws.check_type(&right_ty, &string_ty));

        let right_ty = ws.ty("string | number");
        assert!(!ws.check_type(&right_ty, &string_ty));

        let right_ty = ws.ty("'a' | 'b' | 'c'");
        assert!(ws.check_type(&right_ty, &string_ty));
    }

    #[test]
    fn test_callable_parameters_remain_contravariant() {
        let mut ws = VirtualWorkspace::new();
        let broad_parameter = ws.ty("fun(value: string | number)");
        let narrow_parameter = ws.ty("fun(value: string)");

        assert!(ws.check_type(&broad_parameter, &narrow_parameter));
        assert!(!ws.check_type(&narrow_parameter, &broad_parameter));
    }

    #[test]
    fn test_callable_declared_parameters_remain_contravariant() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class CallableVarianceParent
            ---@field a string

            ---@class CallableVarianceChild: CallableVarianceParent
            ---@field b number
            "#,
        );
        let parent = ws.ty("fun(value: CallableVarianceParent)");
        let child = ws.ty("fun(value: CallableVarianceChild)");

        assert!(ws.check_type(&parent, &child));
        assert!(!ws.check_type(&child, &parent));
    }

    #[test]
    fn test_callable_union_parameters_remain_contravariant() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class CallableUnionVarianceParent
            ---@field a string

            ---@class CallableUnionVarianceChild: CallableUnionVarianceParent
            ---@field b number

            ---@class CallableUnionVarianceOther
            ---@field c boolean
            "#,
        );
        let parent = ws.ty("fun(value: CallableUnionVarianceParent | CallableUnionVarianceOther)");
        let child = ws.ty("fun(value: CallableUnionVarianceChild | CallableUnionVarianceOther)");

        assert!(ws.check_type(&parent, &child));
        assert!(!ws.check_type(&child, &parent));
    }

    #[test]
    fn test_generic_class_call_operator_uses_instantiated_parameter_types() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class GenericCallable<T>
            ---@operator call(T): T
            "#,
        );
        let callable = ws.ty("GenericCallable<string>");
        let compatible = ws.ty("fun(value: string): string");
        let incompatible_parameter = ws.ty("fun(value: number): string");

        assert!(ws.check_type(&callable, &LuaType::Function));
        assert!(ws.check_type(&callable, &compatible));
        assert!(!ws.check_type(&callable, &incompatible_parameter));
        assert!(ws.check_type(&compatible, &callable));
        assert!(!ws.check_type(&incompatible_parameter, &callable));
    }

    #[test]
    fn test_callable_declared_sources_still_check_required_members() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class CallableShapeLeft
            ---@field left string
            ---@operator call(string): string

            ---@class CallableShapeRight
            ---@field right number
            ---@operator call(string): string
            "#,
        );
        let left = ws.ty("CallableShapeLeft");
        let right = ws.ty("CallableShapeRight");
        let function = ws.ty("fun(value: string): string");

        assert!(!ws.check_type(&left, &right));
        assert!(!ws.check_type(&right, &left));
        assert!(ws.check_type(&left, &function));
        assert!(ws.check_type(&function, &left));
    }

    #[test]
    fn test_inherited_generic_call_operator_participates_in_callable_relation() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class GenericCallableParent<T>
            ---@operator call(T): T

            ---@class StringCallableChild: GenericCallableParent<string>
            "#,
        );
        let callable = ws.ty("StringCallableChild");
        let compatible = ws.ty("fun(value: string)");
        let incompatible = ws.ty("fun(value: number)");
        let db = ws.analysis.compilation.get_db();

        assert!(matches!(
            check_assignable(db, &callable, &compatible),
            AssignabilityResult::Assignable
        ));
        assert!(matches!(
            check_assignable(db, &compatible, &callable),
            AssignabilityResult::Assignable
        ));
        assert!(matches!(
            check_assignable(db, &callable, &incompatible),
            AssignabilityResult::NotAssignable(_)
        ));
        assert!(matches!(
            check_assignable(db, &incompatible, &callable),
            AssignabilityResult::NotAssignable(_)
        ));
    }

    #[test]
    fn test_declared_call_operator_overrides_inherited_constructor_signature() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class CallableParent
            ---@operator call(string): string

            ---@class CallableChild: CallableParent
            ---@operator call(integer): integer
            "#,
        );
        let callable = ws.ty("CallableChild");
        let own_signature = ws.ty("fun(value: integer)");
        let inherited_signature = ws.ty("fun(value: string)");
        let db = ws.analysis.compilation.get_db();

        assert!(matches!(
            check_assignable(db, &callable, &own_signature),
            AssignabilityResult::Assignable
        ));
        assert!(matches!(
            check_assignable(db, &own_signature, &callable),
            AssignabilityResult::Assignable
        ));
        assert!(matches!(
            check_assignable(db, &callable, &inherited_signature),
            AssignabilityResult::NotAssignable(_)
        ));
        assert!(matches!(
            check_assignable(db, &inherited_signature, &callable),
            AssignabilityResult::NotAssignable(_)
        ));
    }

    #[test]
    fn test_metatable_call_operator_participates_in_callable_relation() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            metatable_callable = setmetatable({}, {
                ---@param value string
                __call = function(self, value) end,
            })
            "#,
        );
        let callable = ws.expr_ty("metatable_callable");
        let compatible = ws.ty("fun(value: string)");
        let incompatible = ws.ty("fun(value: number)");
        let db = ws.analysis.compilation.get_db();

        assert!(matches!(
            check_assignable(db, &callable, &compatible),
            AssignabilityResult::Assignable
        ));
        assert!(matches!(
            check_assignable(db, &compatible, &callable),
            AssignabilityResult::Assignable
        ));
        assert!(matches!(
            check_assignable(db, &callable, &incompatible),
            AssignabilityResult::NotAssignable(_)
        ));
        assert!(matches!(
            check_assignable(db, &incompatible, &callable),
            AssignabilityResult::NotAssignable(_)
        ));
    }

    #[test]
    fn test_field_fast_path_preserves_alias_source_expansion() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias FieldTextAlias string
            ---@alias FieldShapeAlias { value: string }
            ---@alias GenericFieldShapeAlias<T> { value: T }
            "#,
        );
        let empty_object = ws.ty("{}");
        let shape_target = ws.ty("{ value: string }");
        let text_alias = ws.ty("FieldTextAlias");
        let shape_alias = ws.ty("FieldShapeAlias");
        let generic_shape_alias = ws.ty("GenericFieldShapeAlias<string>");
        let nested_text_alias = ws.ty("{ item: FieldTextAlias }");
        let nested_shape_alias = ws.ty("{ item: FieldShapeAlias }");
        let nested_generic_shape_alias = ws.ty("{ item: GenericFieldShapeAlias<string> }");
        let nested_empty_object = ws.ty("{ item: {} }");
        let nested_shape_target = ws.ty("{ item: { value: string } }");

        assert!(!ws.check_type(&text_alias, &empty_object));
        assert!(ws.check_type(&shape_alias, &shape_target));
        assert!(ws.check_type(&generic_shape_alias, &shape_target));
        assert!(!ws.check_type(&nested_text_alias, &nested_empty_object));
        assert!(ws.check_type(&nested_shape_alias, &nested_shape_target));
        assert!(ws.check_type(&nested_generic_shape_alias, &nested_shape_target));
    }

    #[test]
    fn test_field_fast_path_preserves_enum_source_value_domain() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@enum FieldTextEnum
            local FieldTextEnum = { First = "first", Second = "second" }
            "#,
        );
        let nested_source = ws.ty("{ item: FieldTextEnum }");
        let nested_string_target = ws.ty("{ item: string }");
        let nested_empty_target = ws.ty("{ item: {} }");

        assert!(ws.check_type(&nested_source, &nested_string_target));
        assert!(!ws.check_type(&nested_source, &nested_empty_target));
    }

    #[test]
    fn test_enum_source_uses_value_domain_and_minimal_definition_table_rules() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@enum DeclaredTextEnum
            local DeclaredTextEnum = { First = "first", Second = "second" }
            ---@enum DeclaredIntegerEnum
            local DeclaredIntegerEnum = { First = 1, Second = 2 }
            ---@enum DeclaredTextEnumCopy
            local DeclaredTextEnumCopy = { First = "first", Second = "second" }
            ---@enum DeclaredTextEnumNarrow
            local DeclaredTextEnumNarrow = { First = "first" }
            ---@enum DeclaredTableEnum
            local DeclaredTableEnum = {
                First = { value = "first" },
                Second = { value = "second" },
            }
            ---@class PlainDeclaredClass
            "#,
        );
        let text_enum = ws.ty("DeclaredTextEnum");
        let integer_enum = ws.ty("DeclaredIntegerEnum");
        let text_enum_copy = ws.ty("DeclaredTextEnumCopy");
        let text_enum_narrow = ws.ty("DeclaredTextEnumNarrow");
        let table_enum = ws.ty("DeclaredTableEnum");
        let text_enum_def = LuaType::Def(LuaTypeDeclId::global("DeclaredTextEnum"));
        let text_enum_copy_def = LuaType::Def(LuaTypeDeclId::global("DeclaredTextEnumCopy"));
        let class = ws.ty("PlainDeclaredClass");
        let empty_object = ws.ty("{}");
        let tuple = ws.ty("[string]");
        let array = ws.ty("string[]");
        let table_generic = ws.ty("table<string, string>");
        let table_shape = ws.ty("{ value: string }");

        assert!(ws.check_type(&text_enum, &LuaType::String));
        assert!(!ws.check_type(&text_enum, &LuaType::Integer));
        assert!(ws.check_type(&integer_enum, &LuaType::Integer));
        assert!(!ws.check_type(&integer_enum, &LuaType::String));
        assert!(!ws.check_type(&class, &LuaType::String));
        assert!(!ws.check_type(&class, &LuaType::Integer));
        assert!(ws.check_type(&text_enum, &text_enum_copy));
        assert!(ws.check_type(&text_enum_def, &text_enum_copy_def));
        assert!(!ws.check_type(&text_enum, &text_enum_narrow));
        assert!(ws.check_type(&text_enum_narrow, &text_enum));
        assert!(!ws.check_type(&text_enum, &class));
        assert!(!ws.check_type(&class, &text_enum));
        assert!(!ws.check_type(&text_enum, &LuaType::Table));
        assert!(!ws.check_type(&text_enum, &LuaType::Userdata));
        assert!(!ws.check_type(&text_enum, &empty_object));
        assert!(!ws.check_type(&text_enum, &tuple));
        assert!(!ws.check_type(&text_enum, &array));
        assert!(!ws.check_type(&text_enum, &table_generic));
        assert!(ws.check_type(&text_enum_def, &LuaType::Table));
        assert!(ws.check_type(&text_enum_def, &table_generic));
        assert!(ws.check_type(&table_enum, &LuaType::Table));
        assert!(ws.check_type(&table_enum, &table_shape));
    }

    #[test]
    fn test_indeterminate_is_conservative_only_for_plain_assignability() {
        let db = DbIndex::new();
        let mut source = LuaType::String;
        let mut target = LuaType::Number;
        for _ in 0..101 {
            source = LuaType::Array(LuaArrayType::from_base_type(source).into());
            target = LuaType::Array(LuaArrayType::from_base_type(target).into());
        }

        assert!(is_assignable(&db, &source, &target));
        assert!(matches!(
            probe_assignable(&db, &source, &target),
            RelationOutcome::Indeterminate(_)
        ));
        assert!(matches!(
            check_assignable(&db, &source, &target),
            AssignabilityResult::Indeterminate(_)
        ));
    }

    #[test]
    fn test_target_intersection_unrelated_member_overrides_indeterminate_member() {
        let db = DbIndex::new();
        let mut source = LuaType::String;
        let mut deep_target = LuaType::Number;
        for _ in 0..101 {
            source = LuaType::Array(LuaArrayType::from_base_type(source).into());
            deep_target = LuaType::Array(LuaArrayType::from_base_type(deep_target).into());
        }
        let target = LuaType::Intersection(
            LuaIntersectionType::new(vec![deep_target, LuaType::Boolean]).into(),
        );

        assert_eq!(
            probe_assignable(&db, &source, &target),
            RelationOutcome::Unrelated
        );
    }

    #[test]
    fn test_generic_super_probe_preserves_indeterminate_outcome() {
        // 深度超出限制时应返回无法完成, 而不是直接判定为不可赋值
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class GenericProbeParent<T>
            ---@class GenericProbeChild<T>: GenericProbeParent<T>
            "#,
        );
        let mut source_param = LuaType::String;
        let mut target_param = LuaType::Number;
        for _ in 0..101 {
            source_param = LuaType::Array(LuaArrayType::from_base_type(source_param).into());
            target_param = LuaType::Array(LuaArrayType::from_base_type(target_param).into());
        }
        let source = LuaType::Generic(
            LuaGenericType::new(
                LuaTypeDeclId::global("GenericProbeChild"),
                vec![source_param],
            )
            .into(),
        );
        let target = LuaType::Generic(
            LuaGenericType::new(
                LuaTypeDeclId::global("GenericProbeParent"),
                vec![target_param],
            )
            .into(),
        );

        assert!(matches!(
            probe_assignable(ws.analysis.compilation.get_db(), &source, &target,),
            RelationOutcome::Indeterminate(_)
        ));
    }

    #[test]
    fn test_same_family_generic_mismatch_does_not_probe_super_types() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class GenericMismatchParent
            ---@class GenericMismatchChild<T>: GenericMismatchParent
            "#,
        );
        let source = ws.ty("GenericMismatchChild<string>");
        let target = ws.ty("GenericMismatchChild<number>");
        assert!(!ws.check_type(&source, &target));
    }

    #[test]
    fn test_nullable_ref_fast_path() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@class LegacyFastPathRef");
        let source = ws.ty("LegacyFastPathRef?");
        let target = ws.ty("LegacyFastPathRef");

        assert!(matches!(&source, LuaType::Union(_)));
        assert!(matches!(&target, LuaType::Ref(_)));
        assert!(!ws.check_type(&source, &target));
    }

    #[test]
    fn test_generic_target_completes_ref_source_default_arguments() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@class DefaultGeneric<T = string>");

        let source = LuaType::Ref(LuaTypeDeclId::global("DefaultGeneric"));
        let compatible = ws.ty("DefaultGeneric<string>");
        let incompatible = ws.ty("DefaultGeneric<number>");

        assert!(ws.check_type(&source, &compatible));
        assert!(!ws.check_type(&source, &incompatible));
    }

    #[test]
    fn test_uninstantiated_generic_target_matches_nested_field_relation() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class NestedTemplateTarget<T>
            ---@field required string
            ---@class NestedMissingRequired
            ---@class NestedMatchingRequired
            ---@field required string
            "#,
        );
        let target_item = LuaType::Generic(
            LuaGenericType::new(
                LuaTypeDeclId::global("NestedTemplateTarget"),
                vec![LuaType::TplRef(
                    GenericTpl::new(GenericTplId::Type(0), "T".into(), None, None, false, None)
                        .into(),
                )],
            )
            .into(),
        );
        let target = LuaType::Object(
            LuaObjectType::new(vec![(
                LuaIndexAccessKey::String("item".into()),
                target_item,
            )])
            .into(),
        );
        let missing = ws.ty("{ item: NestedMissingRequired }");
        let matching = ws.ty("{ item: NestedMatchingRequired }");

        assert!(!ws.check_type(&missing, &target));
        assert!(ws.check_type(&matching, &target));
    }

    #[test]
    fn test_number_types() {
        let mut ws = VirtualWorkspace::new();

        let number_ty = ws.ty("number");
        let integer_ty = ws.ty("integer");

        let number_expr1 = ws.expr_ty("1");
        assert!(ws.check_type(&number_expr1, &number_ty));
        let number_expr2 = ws.expr_ty("1.5");
        assert!(ws.check_type(&number_expr2, &number_ty));

        assert!(ws.check_type(&integer_ty, &number_ty));
        assert!(!ws.check_type(&number_ty, &integer_ty));

        let number_union = ws.ty("1 | 2 | 3");
        assert!(ws.check_type(&number_union, &number_ty));
        assert!(ws.check_type(&number_union, &integer_ty));
    }

    #[test]
    fn test_union_types() {
        let mut ws = VirtualWorkspace::new();

        let ty_union = ws.ty("number | string");
        let ty_number = ws.ty("number");
        let ty_string = ws.ty("string");
        let ty_boolean = ws.ty("boolean");

        assert!(ws.check_type(&ty_number, &ty_union));
        assert!(ws.check_type(&ty_string, &ty_union));
        assert!(!ws.check_type(&ty_boolean, &ty_union));
        assert!(ws.check_type(&ty_union, &ty_union));

        let ty_union2 = ws.ty("number | string | boolean");
        assert!(ws.check_type(&ty_number, &ty_union2));
        assert!(ws.check_type(&ty_string, &ty_union2));
        assert!(ws.check_type(&ty_union, &ty_union2));
        assert!(ws.check_type(&ty_union2, &ty_union2));

        let ty_union3 = ws.ty("1 | 2 | 3");
        let ty_union4 = ws.ty("1 | 2");

        assert!(ws.check_type(&ty_union4, &ty_union3));
        assert!(!ws.check_type(&ty_union3, &ty_union4));
        assert!(ws.check_type(&ty_union3, &ty_union3));
    }

    #[test]
    fn test_recursive_alias_accepts_expanded_origin_members() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@alias Recursive string | (Recursive[])
        "#,
        );

        let recursive_ty = ws.ty("Recursive");
        let expanded_ty = ws.ty("string | Recursive[]");
        let invalid_ty = ws.ty("boolean | Recursive[]");

        assert!(ws.check_type(&expanded_ty, &recursive_ty));
        assert!(!ws.check_type(&invalid_ty, &recursive_ty));
    }

    #[test]
    fn test_generic_recursive_alias_accepts_expanded_origin_members() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@alias Recursive<T> T | (Recursive<T>[])
        "#,
        );

        let recursive_ty = ws.ty("Recursive<string>");
        let expanded_ty = ws.ty("string | Recursive<string>[]");
        let invalid_ty = ws.ty("boolean | Recursive<string>[]");

        assert!(ws.check_type(&expanded_ty, &recursive_ty));
        assert!(!ws.check_type(&invalid_ty, &recursive_ty));
    }

    #[test]
    fn test_mutually_recursive_generic_aliases_close_by_active_relation() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias RecursiveA<T> { value: T, next: RecursiveA<T> }
            ---@alias RecursiveB<T> { value: T, next: RecursiveB<T> }
            "#,
        );

        let source = ws.ty("RecursiveA<string>");
        let compatible = ws.ty("RecursiveB<string>");
        let incompatible = ws.ty("RecursiveB<number>");

        assert!(ws.check_type(&source, &compatible));
        assert!(!ws.check_type(&source, &incompatible));
    }

    #[test]
    fn test_object_types() {
        let mut ws = VirtualWorkspace::new();

        // case 1
        {
            let object_ty = ws.ty("{ x: number, y: string }");
            let matched_object_ty2 = ws.ty("{ x: 1, y: 'test' }");
            let mismatch_object_ty2 = ws.ty("{ x: 2, y: 3 }");
            let matched_table_ty = ws.expr_ty("{ x = 1, y = 'test' }");
            let mismatch_table_ty = ws.expr_ty("{ x = 2, y = 3 }");

            assert!(ws.check_type(&matched_object_ty2, &object_ty));
            assert!(!ws.check_type(&mismatch_object_ty2, &object_ty));
            assert!(ws.check_type(&matched_table_ty, &object_ty));
            assert!(!ws.check_type(&mismatch_table_ty, &object_ty));
        }

        // case for tuple, object, and table
        {
            let object_ty = ws.ty("{ [1]: string, [2]: number }");
            let matched_tulple_ty = ws.ty("[string, number");
            let matched_object_ty = ws.ty("{ [1]: 'test', [2]: 1 }");

            assert!(ws.check_type(&matched_tulple_ty, &object_ty));
            assert!(ws.check_type(&matched_object_ty, &object_ty));
            let mismatch_tulple_ty = ws.ty("[number, string]");
            assert!(!ws.check_type(&mismatch_tulple_ty, &object_ty));

            let matched_table_ty = ws.expr_ty("{ [1] = 'test', [2] = 1 }");
            assert!(ws.check_type(&matched_table_ty, &object_ty));
        }

        // issue #69
        {
            let object_ty = ws.ty("{ [1]: number, [2]: integer }?");

            assert!(ws.check_type(&object_ty, &object_ty));
        }
    }

    #[test]
    fn test_declared_targets_use_effective_inherited_members() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class StructuralTarget: { required: string }

            ---@class GenericTargetBase<T>
            ---@field value T

            ---@class GenericTarget: GenericTargetBase<string>
            "#,
        );

        let empty = ws.ty("{}");
        let structural_source = ws.ty("{ required: string }");
        let structural_target = ws.ty("StructuralTarget");
        assert!(!ws.check_type(&empty, &structural_target));
        assert!(ws.check_type(&structural_source, &structural_target));

        let matching_generic_source = ws.ty("{ value: string }");
        let mismatch_generic_source = ws.ty("{ value: number }");
        let generic_target = ws.ty("GenericTarget");
        assert!(ws.check_type(&matching_generic_source, &generic_target));
        assert!(!ws.check_type(&mismatch_generic_source, &generic_target));
    }

    #[test]
    fn test_declared_targets_instantiate_effective_index_members() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class GenericIndexBase<T>
            ---@field [T] string

            ---@class GenericIndexTarget: GenericIndexBase<integer>

            ---@class StructuralIndexTarget<T>: table<integer, T>
            "#,
        );

        let integer_string_table = ws.ty("table<integer, string>");
        let integer_number_table = ws.ty("table<integer, number>");
        let generic_index_target = ws.ty("GenericIndexTarget");
        let structural_index_target = ws.ty("StructuralIndexTarget<string>");

        assert!(ws.check_type(&integer_string_table, &generic_index_target));
        assert!(!ws.check_type(&integer_number_table, &generic_index_target));
        assert!(ws.check_type(&integer_string_table, &structural_index_target));
        assert!(!ws.check_type(&integer_number_table, &structural_index_target));
    }

    #[test]
    fn test_array_types() {
        let mut ws = VirtualWorkspace::new();

        let array_ty = ws.ty("number[]");
        let matched_tuple_ty = ws.ty("[1, 2, 3]");
        let mismatch_array_ty = ws.ty("['a', 'b', 'c']");

        assert!(ws.check_type(&matched_tuple_ty, &array_ty));
        assert!(!ws.check_type(&mismatch_array_ty, &array_ty));

        let array_ty2 = ws.ty("integer[]");
        assert!(ws.check_type(&array_ty2, &array_ty));
        assert!(!ws.check_type(&array_ty, &array_ty2));
    }

    #[test]
    fn test_structured_sequence_relation_keeps_source_direction() {
        let mut ws = VirtualWorkspace::new();
        let tuple_source = ws.ty("[integer, integer]");
        let array_target = ws.ty("integer[]");
        let number_target = ws.ty("number[]");

        assert!(ws.check_type(&tuple_source, &array_target));
        assert!(!ws.check_type(&array_target, &tuple_source)); // 数量不匹配
        assert!(!ws.check_type(&number_target, &tuple_source));
    }

    #[test]
    fn test_array_to_tuple_rejects_unknown_length_in_non_strict_mode() {
        let mut ws = VirtualWorkspace::new();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.strict.array_index = false;
        ws.update_emmyrc(emmyrc);
        let array_source = ws.ty("integer[]");
        let tuple_target = ws.ty("[integer, integer]");
        let variadic_target = ws.ty("[integer...]");

        assert!(ws.check_type(&array_source, &variadic_target)); // 数量不匹配
        assert!(!ws.check_type(&array_source, &tuple_target));
    }

    #[test]
    fn test_array_to_tuple_accepts_guaranteed_prefix_length() {
        let mut ws = VirtualWorkspace::new();
        let array_source =
            LuaType::Array(LuaArrayType::new(LuaType::Integer, LuaArrayLen::Max(2)).into());
        let tuple_target = ws.ty("[integer, integer]");

        assert!(ws.check_type(&array_source, &tuple_target));
    }

    #[test]
    fn test_structured_table_generic_relation_keeps_source_direction() {
        let mut ws = VirtualWorkspace::new();
        let narrow_key = ws.ty("table<integer, string>");
        let wide_key = ws.ty("table<number, string>");

        assert!(ws.check_type(&narrow_key, &wide_key));
        assert!(!ws.check_type(&wide_key, &narrow_key));
    }

    #[test]
    fn test_wide_table_accepts_class_targets_without_accepting_other_declared_types() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class WideTableClass
            ---@field value string
            ---@class WideTableGeneric<T>
            ---@field value T
            "#,
        );
        ws.def(
            r#"
            ---@alias WideTableAlias string
            "#,
        );
        ws.def(
            r#"
            ---@enum WideTableEnum
            local WideTableEnum = { Value = "value" }
            "#,
        );
        let wide_table = LuaType::Table;
        let class_target = ws.ty("WideTableClass");
        let generic_target = ws.ty("WideTableGeneric<string>");
        let alias_target = ws.ty("WideTableAlias");
        let enum_target = ws.ty("WideTableEnum");

        assert!(ws.check_type(&wide_table, &class_target));
        assert!(ws.check_type(&wide_table, &generic_target));
        assert!(!ws.check_type(&wide_table, &alias_target));
        assert!(!ws.check_type(&wide_table, &enum_target)); // 这里将 enum 视为 field 而不是 class, 因此不接受 table
    }

    #[test]
    fn test_object_source_explicitly_relates_to_generic_target() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class RequiredGeneric<T>
            ---@field value T
            "#,
        );
        let shape_source = ws.ty("{ value: string, extra: integer }");
        let generic_target = ws.ty("RequiredGeneric<string>");
        let mismatched_source = ws.ty("{ value: number }");

        assert!(ws.check_type(&shape_source, &generic_target));
        assert!(!ws.check_type(&generic_target, &shape_source));
        assert!(!ws.check_type(&mismatched_source, &generic_target));
    }

    #[test]
    fn test_structured_member_obligation_keeps_source_direction() {
        let mut ws = VirtualWorkspace::new();
        let source_with_extra = ws.ty("{ value: string, extra: integer }");
        let target_without_extra = ws.ty("{ value: string }");

        assert!(ws.check_type(&source_with_extra, &target_without_extra));
        assert!(!ws.check_type(&target_without_extra, &source_with_extra));
    }

    #[test]
    fn test_structured_target_requires_explicit_source_dispatch() {
        let mut ws = VirtualWorkspace::new();
        let scalar_source = ws.ty("string");
        let array_source = ws.ty("string[]");
        let empty_object_target = ws.ty("{}");
        let empty_table_target = ws.expr_ty("{}");

        assert!(!ws.check_type(&scalar_source, &empty_object_target));
        assert!(ws.check_type(&array_source, &empty_object_target));
        assert!(!ws.check_type(&scalar_source, &empty_table_target));
        assert!(ws.check_type(&array_source, &empty_table_target));
    }

    #[test]
    fn test_structured_index_obligation_keeps_source_direction() {
        let mut ws = VirtualWorkspace::new();
        let named_field_source = ws.ty("{ value: integer }");
        let string_index_target = ws.ty("{ [string]: number }");

        assert!(ws.check_type(&named_field_source, &string_index_target));
        assert!(!ws.check_type(&string_index_target, &named_field_source));
    }

    #[test]
    fn test_tuple_types() {
        let mut ws = VirtualWorkspace::new();

        let tuple_ty = ws.ty("[number, string]");
        let matched_tuple_ty = ws.ty("[1, 'test']");
        let mismatch_tuple_ty = ws.ty("['a', 1]");

        assert!(ws.check_type(&matched_tuple_ty, &tuple_ty));
        assert!(!ws.check_type(&mismatch_tuple_ty, &tuple_ty));

        let tuple_ty2 = ws.ty("[integer, string]");
        assert!(ws.check_type(&tuple_ty2, &tuple_ty));
        assert!(!ws.check_type(&tuple_ty, &tuple_ty2));
    }

    #[test]
    fn test_tuple_source_expands_target_variadic_elements() {
        let mut ws = VirtualWorkspace::new();
        let compatible = ws.ty("[string, string]");
        let incompatible = ws.ty("[string, boolean]");
        let target = ws.ty("[string...]");

        assert!(ws.check_type(&compatible, &target));
        assert!(!ws.check_type(&incompatible, &target));
    }

    #[test]
    fn test_issue_86() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        let ty = ws.ty("string?");
        let ty2 = ws.expr_ty("(\"hello\"):match(\".*\")");
        assert!(ws.check_type(&ty2, &ty));
    }

    #[test]
    fn test_issue_634() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            --- @class A
            --- @field a integer

            --- @param x table<integer,string>
            local function foo(x) end

            local y --- @type A
            foo(y) -- should error
        "#
        ));
    }

    #[test]
    fn test_issue_790() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@class Holder<T>

        ---@class StringHolder: Holder<string>

        ---@class NumberHolder: Holder<number>

        ---@class StringHolderWith<T>: Holder<string>

        ---@generic T
        ---@param a T
        ---@param b T
        function test(a, b) end
        "#,
        );

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Holder<string>, NumberHolder
            local a, b
            test(a, b)
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Holder<string>, StringHolderWith<table>
            local a, b
            test(a, b)
        "#
        ));
    }

    #[test]
    fn test_intersection_is_table_subtype() {
        let mut ws = VirtualWorkspace::new();

        // [integer] & { n: integer } should be assignable to table
        let intersection_ty = ws.ty("integer[] & { n: integer }");
        let table_ty = ws.ty("table");
        assert!(
            ws.check_type(&intersection_ty, &table_ty),
            "integer[] & {{ n: integer }} should be a subtype of table"
        );

        // Verify via diagnostic: passing intersection type to a table parameter should not error
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@param t table
            local function foo(t) end

            ---@type integer[] & { n: integer }
            local packed
            foo(packed)
            "#
        ));

        // Also verify: assigning intersection to table should not error
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type integer[] & { n: integer }
            local packed

            ---@type table
            local t = packed
            "#
        ));

        // Intersection type should be assignable to an array type (non-generic)
        let array_ty = ws.ty("integer[]");
        assert!(
            ws.check_type(&intersection_ty, &array_ty),
            "integer[] & {{ n: integer }} should be assignable to integer[]"
        );

        // Intersection type should be assignable to an array parameter (non-generic)
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@param t integer[]
            local function foo2(t) end

            ---@type integer[] & { n: integer }
            local packed
            foo2(packed)
            "#
        ));

        // Intersection type should be assignable to a generic array parameter
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@generic V
            ---@param t V[]
            ---@return fun(): integer, V
            local function my_ipairs(t) end

            ---@type integer[] & { n: integer }
            local packed
            my_ipairs(packed)
            "#
        ));

        // Intersection type should be assignable to table<int, V>
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@generic V
            ---@param t table<integer, V>
            ---@return fun(): integer, V
            local function my_iter(t) end

            ---@type integer[] & { n: integer }
            local packed
            my_iter(packed)
            "#
        ));
    }

    #[test]
    fn test_nested_semantic_accept_on_recursive_relate() {
        let mut ws = VirtualWorkspace::new();

        // Nested target any: string <: any only via recursive semantic accept.
        let source = ws.ty("{ value: string }");
        let target = ws.ty("{ value: any }");
        assert!(ws.check_type(&source, &target));

        let source = ws.ty("{ nested: { value: integer } }");
        let target = ws.ty("{ nested: { value: any } }");
        assert!(ws.check_type(&source, &target));

        let source = ws.ty("string[]");
        let target = ws.ty("any[]");
        assert!(ws.check_type(&source, &target));

        let source = ws.ty("fun(x: string): integer");
        let target = ws.ty("fun(x: any): any");
        assert!(ws.check_type(&source, &target));

        // Nested target unknown.
        let source = ws.ty("{ value: string }");
        let target = ws.ty("{ value: unknown }");
        assert!(ws.check_type(&source, &target));

        // Source any nested under structure still assigns to concrete target fields
        // (source Any is accepted against anything).
        let source = ws.ty("{ value: any }");
        let target = ws.ty("{ value: string }");
        assert!(ws.check_type(&source, &target));

        // Diagnostic path: assigning concrete table into any-field should not error.
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@type { value: any }
            local sink = { value = "ok" }
            "#,
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@param opts { flag: any }
            local function take(opts) end
            take({ flag = true })
            "#,
        ));
    }
}
