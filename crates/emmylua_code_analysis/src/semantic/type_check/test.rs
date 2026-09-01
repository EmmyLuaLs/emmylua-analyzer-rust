#[cfg(test)]
mod test {
    use crate::{
        ChainMessage, DbIndex, DiagnosticCode, GenericTpl, GenericTplId, LuaArrayLen, LuaArrayType,
        LuaGenericType, LuaIndexAccessKey, LuaIntersectionType, LuaObjectType, LuaType,
        LuaTypeDeclId, LuaUnionType, VirtualWorkspace, is_assignable,
        semantic::type_check::{
            AssignabilityResult, ErrorChain, RelationOutcome, check_assignable, probe_assignable,
        },
    };

    fn chain_messages(chain: &ErrorChain) -> Vec<ChainMessage> {
        let mut result = Vec::new();
        let mut current = Some(chain);
        while let Some(node) = current {
            result.push(node.message().clone());
            current = node.next();
        }
        result
    }

    // 两个无字段的类结构等价
    #[test]
    fn test_structural_class_relation_empty_classes() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class StructuralEmptyA
            ---@class StructuralEmptyB: StructuralEmptyA
            "#,
        );
        let a = ws.ty("StructuralEmptyA");
        let b = ws.ty("StructuralEmptyB");

        assert!(ws.check_type(&a, &b));
        assert!(ws.check_type(&b, &a));
    }

    #[test]
    fn test_structural_class_relation_missing_fields() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class StructuralBaseA
            ---@field name string

            ---@class StructuralBaseB: StructuralBaseA
            ---@field extra integer
            "#,
        );
        let a = ws.ty("StructuralBaseA");
        let b = ws.ty("StructuralBaseB");

        assert!(!ws.check_type(&a, &b));
        assert!(ws.check_type(&b, &a));
    }

    #[test]
    fn test_child_override_field_mismatch_reports_on_child_to_parent() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class OverrideParent
            ---@field value string

            ---@class OverrideChild: OverrideParent
            ---@field value integer
            "#,
        );
        let child = ws.ty("OverrideChild");
        let parent = ws.ty("OverrideParent");

        assert!(!ws.check_type(&child, &parent));
    }

    // 原始值类型与类之间不按空结构互通.
    #[test]
    fn test_primitive_source_to_unrelated_class_rejected() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class PrimitiveTargetEmpty
            ---@class PrimitiveTargetWithField
            ---@field value string
            "#,
        );
        let empty = ws.ty("PrimitiveTargetEmpty");
        let with_field = ws.ty("PrimitiveTargetWithField");
        assert!(!ws.check_type(&LuaType::Integer, &empty));
        assert!(!ws.check_type(&LuaType::String, &with_field));
    }

    // TODO: 应该给这些内置类型添加一个隐式的成员而不是特例
    // userdata | Thread | Global 这些基础类型不能按结构对比
    #[test]
    fn test_userdata_source_keeps_nominal_pass_to_subclass() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class UserDataChild: userdata
            ---@field tag string

            ---@class UnrelatedEmptyZ
            "#,
        );
        let child = ws.ty("UserDataChild");
        let unrelated = ws.ty("UnrelatedEmptyZ");
        assert!(ws.check_type(&LuaType::Userdata, &child));
        assert!(!ws.check_type(&LuaType::Userdata, &unrelated));
    }

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
    fn test_same_family_generic_mismatch_does_not_probe_super_types() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class GenericMismatchParent
            ---@class GenericMismatchChild<T>: GenericMismatchParent
            ---@field value T
            "#,
        );
        let source = ws.ty("GenericMismatchChild<string>");
        let target = ws.ty("GenericMismatchChild<number>");
        assert!(!ws.check_type(&source, &target));
    }

    #[test]
    fn test_same_family_generic_alias_direct_argument_mismatch() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Box<T> { value: T }
            ---@alias DeepBox<T> Box<Box<Box<T>>>
            "#,
        );
        let source = ws.ty("DeepBox<string>");
        let target = ws.ty("DeepBox<number>");

        assert!(!ws.check_type(&source, &target));

        // 别名逐层展开后折叠为点路径, 最深层失败对收尾.
        let chain = match check_assignable(ws.get_db_mut(), &source, &target) {
            AssignabilityResult::NotAssignable(chain) => chain.expect("chain must exist"),
            other => panic!("expected not assignable, got {:?}", other),
        };

        let messages = chain_messages(&chain);
        assert_eq!(
            messages,
            vec![
                ChainMessage::Field {
                    name: "value.value.value".to_string()
                },
                ChainMessage::NotAssignable {
                    source: "string".to_string(),
                    target: "number".to_string(),
                },
            ]
        );
    }

    #[test]
    fn test_same_family_generic_alias_keeps_contravariant_positions() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Handler<T> fun(value: T)

            ---@class HandlerVarianceParent
            ---@field a string

            ---@class HandlerVarianceChild : HandlerVarianceParent
            ---@field b number
            "#,
        );
        // fun(parent) 可赋给 fun(child)
        let broad = ws.ty("Handler<HandlerVarianceParent>");
        let narrow = ws.ty("Handler<HandlerVarianceChild>");
        assert!(ws.check_type(&broad, &narrow));
        // fun(child) 不可赋给 fun(parent)
        assert!(!ws.check_type(&narrow, &broad));
    }

    #[test]
    fn test_same_family_generic_alias_nullable_contravariant_position() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class NullableVarianceBase
            ---@alias NullableHandler<T> fun(value: T)
            "#,
        );
        // fun(Base) 不可赋给 fun(Base?)
        let source = ws.ty("NullableHandler<NullableVarianceBase>");
        let target = ws.ty("NullableHandler<NullableVarianceBase?>");
        assert!(!ws.check_type(&source, &target));

        // 协变位置上可空联合仍应正常接受.
        ws.def("---@alias NullableBox<T> { value: T }");
        let box_source = ws.ty("NullableBox<NullableVarianceBase>");
        let box_target = ws.ty("NullableBox<NullableVarianceBase?>");
        assert!(ws.check_type(&box_source, &box_target));
    }

    #[test]
    fn test_same_family_multi_param_alias_reports_actual_failure() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@alias MixedVariance<A, B> { set: fun(value: A), value: B }");

        let source = ws.ty("MixedVariance<string | number, string>");
        let target = ws.ty("MixedVariance<string, number>");
        let chain = match check_assignable(ws.get_db_mut(), &source, &target) {
            AssignabilityResult::NotAssignable(chain) => chain.expect("chain must exist"),
            other => panic!("expected not assignable, got {:?}", other),
        };

        let messages = chain_messages(&chain);
        // 完整流程证据: 字段定位 + 最深层失败对, 快捷实参尝试的方差误报不入链.
        assert_eq!(
            messages,
            vec![
                ChainMessage::Field {
                    name: "value".to_string()
                },
                ChainMessage::NotAssignable {
                    source: "string".to_string(),
                    target: "number".to_string(),
                },
            ]
        );
    }

    #[test]
    fn test_same_family_generic_class_contravariant_position() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class HandlerVarianceAnimal
            ---@field name string

            ---@class HandlerVarianceDog: HandlerVarianceAnimal
            ---@field trick string

            ---@class HandlerVarianceBox<T>
            ---@field callback fun(value: T): boolean
            "#,
        );
        let animal = ws.ty("HandlerVarianceBox<HandlerVarianceAnimal>");
        let dog = ws.ty("HandlerVarianceBox<HandlerVarianceDog>");

        // 逆变: fun(Animal) 可赋给 fun(Dog), 因此 Box<Animal> -> Box<Dog> 应通过.
        assert!(ws.check_type(&animal, &dog));
        assert!(!ws.check_type(&dog, &animal));
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
        ws.def(
            r#"
            ---@class DefaultGeneric<T = string>
            ---@field value T
            "#,
        );

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
    fn test_structured_member_keeps_source_direction() {
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
    fn test_structured_index_member_keeps_source_direction() {
        let mut ws = VirtualWorkspace::new();
        let named_field_source = ws.ty("{ value: integer }");
        let string_index_target = ws.ty("{ [string]: number }");

        assert!(ws.check_type(&named_field_source, &string_index_target));
        assert!(!ws.check_type(&string_index_target, &named_field_source));
    }

    #[test]
    fn test_sequence_requires_target_index_signature() {
        let mut ws = VirtualWorkspace::new();
        let array_source = ws.ty("number[]");
        let tuple_source = ws.ty("[number]");
        let string_index_target = ws.ty("{ [string]: boolean }");

        assert!(!ws.check_type(&array_source, &string_index_target));
        assert!(!ws.check_type(&tuple_source, &string_index_target));
    }

    #[test]
    fn test_sequence_required_index_keeps_nullable_element_mismatch() {
        let mut ws = VirtualWorkspace::new();
        let nullable_array = ws.ty("(string?)[]");
        let required_index_target = ws.ty("{ [1]: string }");

        assert!(!ws.check_type(&nullable_array, &required_index_target));
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

        // 对齐 TS 结构化语义: Holder 无字段时, Holder<string> 与 NumberHolder 结构等价, 不再按名义实参报错.
        assert!(ws.has_no_diagnostic(
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

    #[test]
    fn test_nullable_target_relates_non_nil_and_nil_branches() {
        let db = DbIndex::new();
        let target = LuaUnionType::Nullable(LuaType::String).into();

        assert_eq!(
            probe_assignable(&db, &LuaType::String, &target),
            RelationOutcome::Related
        );
        assert_eq!(
            probe_assignable(&db, &LuaType::Nil, &target),
            RelationOutcome::Related
        );
        assert_eq!(
            probe_assignable(&db, &LuaType::Number, &target),
            RelationOutcome::Unrelated
        );
    }
}
