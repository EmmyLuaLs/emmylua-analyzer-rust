#[cfg(test)]
mod test {
    use hashbrown::HashMap;
    use std::sync::Arc;

    use super::super::instantiate_type::{regularize_tpl_candidate_type, widen_tpl_candidate_type};
    use crate::{
        AsyncState, DbIndex, DiagnosticCode, GenericTpl, GenericTplId, LuaArrayType,
        LuaFunctionType, LuaIntersectionType, LuaMemberKey, LuaObjectType, LuaTupleStatus,
        LuaTupleType, LuaType, LuaUnionType, TypeMapper, TypeMapperValue, VariadicType,
        VirtualWorkspace,
    };
    use smol_str::SmolStr;

    #[test]
    fn test_variadic_func() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@generic T, R
        ---@param call async fun(...: T...): R...
        ---@return async fun(...: T...): R...
        function async_create(call)

        end


        ---@param a number
        ---@param b string
        ---@param c boolean
        ---@return number
        function locaf(a, b, c)

        end
        "#,
        );

        let ty = ws.expr_ty("async_create(locaf)");
        let expected = ws.ty("async fun(a: number, b: string, c:boolean): number...");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_select_type() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
        ---@param ... string
        function ffff(...)
            a, b, c = select(2, ...)
        end
        "#,
        );

        let a_ty = ws.expr_ty("a");
        let b_ty = ws.expr_ty("b");
        let c_ty = ws.expr_ty("c");
        let expected = ws.ty("string");
        assert_eq!(a_ty, expected);
        assert_eq!(b_ty, expected);
        assert_eq!(c_ty, expected);

        ws.def(
            r#"
        e, f = select(2, "a", "b", "c")
        "#,
        );

        let e = ws.expr_ty("e");
        let expected = LuaType::String;
        let f = ws.expr_ty("f");
        let expected_f = LuaType::String;
        assert_eq!(e, expected);
        assert_eq!(f, expected_f);

        ws.def(
            r#"
        h = select('#', "a", "b")
        "#,
        );

        let h = ws.expr_ty("h");
        let expected = LuaType::IntegerConst(2);
        assert_eq!(h, expected);

        // select(n, func()) where func() returns multiple values
        ws.def(
            r#"
        ---@return integer, string
        function multi_ret() end
        g = select(2, multi_ret())
        "#,
        );

        let g = ws.expr_ty("g");
        let expected_g = ws.ty("string");
        assert_eq!(g, expected_g);
    }

    #[test]
    fn test_unpack() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        local h ---@type number[]
        a, b, c = table.unpack(h)
        "#,
        );

        let a = ws.expr_ty("a");
        let expected = ws.ty("number?");
        let b = ws.expr_ty("b");
        let expected_b = ws.ty("number?");
        let c = ws.expr_ty("c");
        let expected_c = ws.ty("number?");
        assert_eq!(a, expected);
        assert_eq!(b, expected_b);
        assert_eq!(c, expected_c);
    }

    #[test]
    fn test_return() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@class ab
                ---@field a number
                local A

                ---@generic T
                ---@param a T
                ---@return T
                local function name(a)
                    return a
                end

                local a = name(A)
                a.b = 1
                R = A.b
        "#,
        );

        let a = ws.expr_ty("R");
        let expected = ws.ty("nil");
        assert_eq!(a, expected);
    }

    #[test]
    fn test_issue_797() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
---@class Holder<T>

---@class C_StringHolder : Holder<string>

---@class C_StringHolderExt : C_StringHolder

---@class C_StringHolderWith<T> : Holder<string>

---@class C_StringHolderWithExt<T> : C_StringHolderWith<T>

---@alias A_StringHolder Holder<string>

---@alias A_StringHolderExt A_StringHolder

---@alias A_StringHolderWith<T> Holder<string>

---@alias A_StringHolderWithExt<T> A_StringHolderWith<T>

---@generic T
---@param v Holder<T>
---@return T
local function extract_holder(v) return v end

local direct ---@type Holder<string>

local class_a ---@type C_StringHolder
local class_b ---@type C_StringHolderExt
local class_c ---@type C_StringHolderWith<table>
local class_d ---@type C_StringHolderWithExt<table>

local alias_a ---@type A_StringHolder
local alias_b ---@type A_StringHolderExt
local alias_c ---@type A_StringHolderWith<table>
local alias_d ---@type A_StringHolderWithExt<table>

result = {
    direct = extract_holder(direct),

    class_a = extract_holder(class_a),
    class_b = extract_holder(class_b),
    class_c = extract_holder(class_c),
    class_d = extract_holder(class_d),

    alias_a = extract_holder(alias_a),
    alias_b = extract_holder(alias_b),
    alias_c = extract_holder(alias_c),
    alias_d = extract_holder(alias_d),
}
        "#,
        );

        let a = ws.expr_ty("result");
        let a_desc = ws.humanize_type_detailed(a);
        let expected = r#"{
    direct: string,
    class_a: string,
    class_b: string,
    class_c: string,
    class_d: string,
    alias_a: string,
    alias_b: string,
    alias_c: string,
    alias_d: string,
}"#;
        assert_eq!(a_desc, expected);
    }

    #[test]
    fn test_call_generic() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Warp<T> T

            ---@generic T
            ---@param ... Warp<T>
            function test(...)
            end
        "#,
        );

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Warp<number>, Warp<string>
            local a, b
            test(a, b)
        "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Warp<number>, Warp<string>
            local a, b
            test--[[@<number | string>]](a, b)
        "#,
        ));
    }

    #[test]
    fn test_generic_alias_instantiation() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Arrayable<T> T | T[]

            ---@class Suite

            ---@generic T
            ---@param value Arrayable<T>
            ---@return T[]
            function toArray(value)
            end
        "#,
        );

        ws.def(
            r#"
            ---@type Arrayable<Suite>
            local suite

            arraySuites = toArray(suite)
        "#,
        );

        let a = ws.expr_ty("arraySuites");
        let expected = ws.ty("Suite[]");
        assert_eq!(a, expected);
    }

    #[test]
    fn test_generic_alias_instantiation2() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Arrayable<T> T | T[]

            ---@class Suite

            ---@param value Arrayable<Suite>
            function toArray(value)

            end
        "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"

            ---@type Suite
            local suite

            local arraySuites = toArray(suite)
            "#
        ));
    }

    #[test]
    fn test_inference_mapper_fallback_and_explicit_precedence() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T = string
            ---@return T
            local function defaulted()
            end

            ---@generic T: integer
            ---@return T
            local function constrained()
            end

            ---@generic T
            ---@param value T
            ---@return T
            local function explicit(value)
            end

            default_result = defaulted()
            constraint_result = constrained()
            explicit_result = explicit--[[@<string>]](1)

            "#,
        );

        assert_eq!(ws.expr_ty("default_result"), ws.ty("string"));
        assert_eq!(ws.expr_ty("constraint_result"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("explicit_result"), ws.ty("string"));
    }

    #[test]
    fn test_mapper_reducer_reuses_alias_mapped_and_function_shapes() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias MapperBox<T> { value: T, list: T[] }
            ---@alias Copy<T> { [K in keyof T]: T[K]; }

            ---@generic T
            ---@param value T
            ---@return MapperBox<T>
            local function box(value)
            end

            ---@generic T
            ---@param value T
            ---@return Copy<T>
            local function copy(value)
            end

            ---@generic T
            ---@param value T
            ---@return fun(next: T): T
            local function make_id(value)
            end

            box_result = box("name")
            box_value = box_result.value
            box_list_item = box_result.list[1]

            copied = copy({ name = "a", count = 1 })
            copied_name = copied.name
            copied_count = copied.count

            made = make_id(1)
            made_ret = made(2)
            "#,
        );

        assert_eq!(ws.expr_ty("box_value"), ws.ty("string"));
        assert_eq!(ws.expr_ty("box_list_item"), ws.ty("string?"));
        assert_eq!(ws.expr_ty("copied_name"), ws.ty("string"));
        assert_eq!(ws.expr_ty("copied_count"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("made_ret"), ws.ty("integer"));
    }

    #[test]
    fn test_structural_instantiate_fast_path_preserves_plain_shapes() {
        let db = DbIndex::new();
        let empty_mapper = TypeMapper::empty();

        let plain_array = LuaType::Array(LuaArrayType::from_base_type(LuaType::Number).into());
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &plain_array,
                &empty_mapper
            ),
            plain_array
        );

        let plain_tuple = LuaType::Tuple(
            LuaTupleType::new(
                vec![LuaType::Number, LuaType::String],
                LuaTupleStatus::DocResolve,
            )
            .into(),
        );
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &plain_tuple,
                &empty_mapper
            ),
            plain_tuple
        );

        let plain_object = LuaType::Object(
            LuaObjectType::new_with_fields(
                HashMap::from([
                    (LuaMemberKey::Name(SmolStr::new("name")), LuaType::String),
                    (LuaMemberKey::Name(SmolStr::new("count")), LuaType::Number),
                ]),
                Vec::new(),
            )
            .into(),
        );
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &plain_object,
                &empty_mapper
            ),
            plain_object
        );

        let plain_union =
            LuaType::Union(LuaUnionType::from_vec(vec![LuaType::Number, LuaType::String]).into());
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &plain_union,
                &empty_mapper
            ),
            plain_union
        );

        let plain_intersection = LuaType::Intersection(
            LuaIntersectionType::new(vec![LuaType::Number, LuaType::String]).into(),
        );
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &plain_intersection,
                &empty_mapper
            ),
            plain_intersection
        );

        let plain_table_generic =
            LuaType::TableGeneric(Arc::new(vec![LuaType::Number, LuaType::String]));
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &plain_table_generic,
                &empty_mapper
            ),
            plain_table_generic
        );

        let plain_variadic = LuaType::Variadic(VariadicType::Base(LuaType::Number).into());
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &plain_variadic,
                &empty_mapper
            ),
            plain_variadic
        );

        let plain_doc_function = LuaType::DocFunction(
            LuaFunctionType::new(
                AsyncState::None,
                false,
                false,
                vec![("value".to_string(), Some(LuaType::Number))],
                LuaType::String,
            )
            .into(),
        );
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &plain_doc_function,
                &empty_mapper
            ),
            plain_doc_function
        );

        let plain_type_guard = LuaType::TypeGuard(Arc::new(LuaType::Number));
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &plain_type_guard,
                &empty_mapper
            ),
            plain_type_guard
        );
    }

    #[test]
    fn test_structural_instantiate_fast_path_instantiates_template_children() {
        let db = DbIndex::new();
        let mapper = TypeMapper::from_values(
            vec![GenericTplId::Func(0)],
            vec![TypeMapperValue::type_value(LuaType::String)],
        );
        let tpl = LuaType::TplRef(Arc::new(GenericTpl::new(
            GenericTplId::Func(0),
            SmolStr::new("T0").into(),
            None,
            None,
        )));

        let templated_array = LuaType::Array(LuaArrayType::from_base_type(tpl.clone()).into());
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &templated_array,
                &mapper
            ),
            LuaType::Array(LuaArrayType::from_base_type(LuaType::String).into())
        );

        let templated_tuple = LuaType::Tuple(
            LuaTupleType::new(
                vec![LuaType::Number, tpl.clone()],
                LuaTupleStatus::DocResolve,
            )
            .into(),
        );
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &templated_tuple,
                &mapper
            ),
            LuaType::Tuple(
                LuaTupleType::new(
                    vec![LuaType::Number, LuaType::String],
                    LuaTupleStatus::DocResolve,
                )
                .into()
            )
        );

        let templated_object = LuaType::Object(
            LuaObjectType::new_with_fields(
                HashMap::from([
                    (LuaMemberKey::Name(SmolStr::new("name")), tpl.clone()),
                    (LuaMemberKey::Name(SmolStr::new("count")), LuaType::Number),
                ]),
                Vec::new(),
            )
            .into(),
        );
        let expected_object = LuaType::Object(
            LuaObjectType::new_with_fields(
                HashMap::from([
                    (LuaMemberKey::Name(SmolStr::new("name")), LuaType::String),
                    (LuaMemberKey::Name(SmolStr::new("count")), LuaType::Number),
                ]),
                Vec::new(),
            )
            .into(),
        );
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &templated_object,
                &mapper
            ),
            expected_object
        );

        let templated_union =
            LuaType::Union(LuaUnionType::from_vec(vec![LuaType::Number, tpl.clone()]).into());
        let expected_union =
            LuaType::Union(LuaUnionType::from_vec(vec![LuaType::Number, LuaType::String]).into());
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &templated_union,
                &mapper
            ),
            expected_union
        );

        let templated_intersection = LuaType::Intersection(
            LuaIntersectionType::new(vec![LuaType::Number, tpl.clone()]).into(),
        );
        let expected_intersection = LuaType::Intersection(
            LuaIntersectionType::new(vec![LuaType::Number, LuaType::String]).into(),
        );
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &templated_intersection,
                &mapper
            ),
            expected_intersection
        );

        let templated_table_generic = LuaType::TableGeneric(Arc::new(vec![tpl.clone()]));
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &templated_table_generic,
                &mapper
            ),
            LuaType::TableGeneric(Arc::new(vec![LuaType::String]))
        );

        let templated_variadic = LuaType::Variadic(VariadicType::Base(tpl.clone()).into());
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &templated_variadic,
                &mapper
            ),
            LuaType::Variadic(VariadicType::Base(LuaType::String).into())
        );

        let templated_doc_function = LuaType::DocFunction(
            LuaFunctionType::new(
                AsyncState::None,
                false,
                false,
                vec![("value".to_string(), Some(tpl.clone()))],
                tpl.clone(),
            )
            .into(),
        );
        let expected_doc_function = LuaType::DocFunction(
            LuaFunctionType::new(
                AsyncState::None,
                false,
                false,
                vec![("value".to_string(), Some(LuaType::String))],
                LuaType::String,
            )
            .into(),
        );
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &templated_doc_function,
                &mapper
            ),
            expected_doc_function
        );

        let templated_type_guard = LuaType::TypeGuard(Arc::new(tpl.clone()));
        assert_eq!(
            super::super::instantiate_type::instantiate_type_generic(
                &db,
                &templated_type_guard,
                &mapper
            ),
            LuaType::TypeGuard(Arc::new(LuaType::String))
        );
    }

    #[test]
    fn test_123() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T
            ---@param x T
            ---@return T
            function f(x)
                return x
            end

            A = f("hello")
            B = f({value = "hello"})
            C = B.value
            "#,
        );

        let a_ty = ws.expr_ty("A");
        assert_eq!(ws.humanize_type(a_ty), "\"hello\"");

        let b_ty = ws.expr_ty("B");
        let b_desc = ws.humanize_type_detailed(b_ty);
        assert!(
            b_desc.contains("value: string"),
            "unexpected type: {}",
            b_desc
        );

        let c_ty = ws.expr_ty("C");
        assert_eq!(ws.humanize_type(c_ty), "string");
    }

    #[test]
    fn test_regularize_tpl_candidate_type_preserves_root_primitive_and_widens_nested_literals() {
        let mut ws = VirtualWorkspace::new();

        let root_literal = ws.ty(r#""mode""#);
        let regularized_root = {
            let db = ws.analysis.compilation.get_db();
            regularize_tpl_candidate_type(db, root_literal.clone())
        };
        assert_eq!(regularized_root, root_literal);

        let table = ws.expr_ty(r#"{ kind = "mode", count = 1 }"#);
        let regularized_table = {
            let db = ws.analysis.compilation.get_db();
            regularize_tpl_candidate_type(db, table)
        };
        assert_eq!(regularized_table, ws.ty("{ kind: string, count: integer }"));
    }

    #[test]
    fn test_widen_tpl_candidate_type_widens_root_primitive_and_structural_literals() {
        let mut ws = VirtualWorkspace::new();

        let root_literal = ws.ty(r#""mode""#);
        let widened_root = {
            let db = ws.analysis.compilation.get_db();
            widen_tpl_candidate_type(db, root_literal)
        };
        assert_eq!(widened_root, LuaType::String);

        let root_union = ws.ty(r#""left" | "right""#);
        let widened_root_union = {
            let db = ws.analysis.compilation.get_db();
            widen_tpl_candidate_type(db, root_union.clone())
        };
        assert_eq!(widened_root_union, root_union);

        let tuple = ws.expr_ty(r#"{ "mode", 1 }"#);
        let widened_tuple = {
            let db = ws.analysis.compilation.get_db();
            widen_tpl_candidate_type(db, tuple)
        };
        assert_eq!(ws.humanize_type(widened_tuple), "(string,integer)");

        let table = ws.expr_ty(r#"{ kind = "mode", count = 1 }"#);
        let widened_table = {
            let db = ws.analysis.compilation.get_db();
            widen_tpl_candidate_type(db, table)
        };
        assert_eq!(widened_table, ws.ty("{ kind: string, count: integer }"));
    }
}
