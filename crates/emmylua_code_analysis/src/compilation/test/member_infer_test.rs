#[cfg(test)]
mod test {
    use smol_str::SmolStr;

    use crate::{LuaType, LuaUnionType, VirtualWorkspace};

    #[test]
    fn test_issue_318() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        local map = {
            a = 'a',
            b = 'b',
            c = 'c',
        }
        local key      --- @type string
        c = map[key]   -- type should be ('a'|'b'|'c'|nil)

        "#,
        );

        let c_ty = ws.expr_ty("c");

        let union_type = LuaType::Union(
            LuaUnionType::from_vec(vec![
                LuaType::StringConst(SmolStr::new("a").into()),
                LuaType::StringConst(SmolStr::new("b").into()),
                LuaType::StringConst(SmolStr::new("c").into()),
                LuaType::Nil,
            ])
            .into(),
        );

        assert_eq!(c_ty, union_type);
    }

    #[test]
    fn test_issue_314_generic_inheritance() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@class foo<T>: T
        local foo_mt = {}

        ---@type foo<{a: string}>
        local bar = { a = 'test' }

        c = bar.a -- should be string

        ---@class buz<T>: foo<T>
        local buz_mt = {}

        ---@type buz<{a: integer}>
        local qux = { a = 5 }

        d = qux.a -- should be integer
        "#,
        );

        let c_ty = ws.expr_ty("c");
        let d_ty = ws.expr_ty("d");

        assert_eq!(c_ty, LuaType::String);
        assert_eq!(d_ty, LuaType::Integer);
    }

    #[test]
    fn test_issue_397() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        --- @class A
        --- @field field? integer

        --- @class B : A
        --- @field field integer

        --- @type B
        local b = { field = 1 }

        local key1 --- @type 'field'
        local key2 = 'field'

        a = b.field -- type is integer - correct
        d = b['field'] -- type is integer - correct
        e = b[key1] -- type is integer? - wrong
        f = b[key2] -- type is integer? - wrong
        "#,
        );

        let a_ty = ws.expr_ty("a");
        let d_ty = ws.expr_ty("d");
        let e_ty = ws.expr_ty("e");
        let f_ty = ws.expr_ty("f");

        assert_eq!(a_ty, LuaType::Integer);
        assert_eq!(d_ty, LuaType::Integer);
        assert_eq!(e_ty, LuaType::Integer);
        assert_eq!(f_ty, LuaType::Integer);
    }

    #[test]
    fn test_keyof() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@class SuiteHooks
        ---@field beforeAll string
        ---@field afterAll number

        ---@type SuiteHooks
        local hooks = {}

        ---@type keyof SuiteHooks
        local name = "beforeAll"

        A = hooks[name]
        "#,
        );

        let ty = ws.expr_ty("A");
        let expected =
            LuaType::Union(LuaUnionType::from_vec(vec![LuaType::String, LuaType::Number]).into());
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_local_shadow_global_member_owner() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        local table = {}
        table.unpack = 1
        A = table.unpack
        "#,
        );

        assert_eq!(ws.expr_ty("A"), LuaType::IntegerConst(1));
    }

    #[test]
    fn test_assign_table_literal_preserves_class_fields() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@class A
        ---@field a string
        ---@field b? number

        ---@type A
        local a
        a = { a = "hello" }

        c = a.a
        "#,
        );

        assert_eq!(
            ws.expr_ty("c"),
            LuaType::StringConst(SmolStr::new("hello").into())
        );
    }

    #[test]
    fn test_assign_object_return_preserves_class_fields() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@class A
        ---@field a string|number
        ---@field b number

        ---@return {a: string}
        local function make()
            return { a = "hello" }
        end

        ---@type A
        local a
        a = make()

        c = a.a
        d = a.b
        "#,
        );

        assert_eq!(ws.expr_ty("c"), LuaType::String);
        assert_eq!(ws.expr_ty("d"), LuaType::Number);
    }

    #[test]
    fn test_assign_table_literal_preserves_class_fields_from_antecedent() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@class A
        ---@field a string
        ---@field b? number

        ---@type A
        local global_a

        ---@return A
        local function make()
            return global_a
        end

        local a = make()
        a = { a = "hello" }

        c = a.a
        "#,
        );

        assert_eq!(
            ws.expr_ty("c"),
            LuaType::StringConst(SmolStr::new("hello").into())
        );
    }

    #[test]
    fn test_assign_from_nil_uses_expr_type() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        local a
        a = "hello"
        b = a
        "#,
        );

        assert_eq!(
            ws.expr_ty("b"),
            LuaType::StringConst(SmolStr::new("hello").into())
        );
    }

    #[test]
    fn test_global_member_owner_prefers_declared_type() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@class Foo
        ---@field existing string

        ---@type Foo
        Foo = {}
        Foo.extra = 1

        ---@type Foo
        local other

        A = other.extra
        "#,
        );

        assert_eq!(ws.expr_ty("A"), LuaType::Nil);
    }

    #[test]
    fn test_non_name_prefix_uses_inferred_type() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        local t = {}
        (t).bar = "hi"
        A = t.bar
        "#,
        );

        assert_eq!(
            ws.expr_ty("A"),
            LuaType::StringConst(SmolStr::new("hi").into())
        );
    }

    #[test]
    fn test_table_expr_key_string() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        local key = tostring(1)
        local t = { [key] = 1 }
        value = t[key]
        "#,
        );

        let value_ty = ws.expr_ty("value");
        assert!(
            matches!(value_ty, LuaType::Integer | LuaType::IntegerConst(_)),
            "expected integer type, got {:?}",
            value_ty
        );
    }

    #[test]
    fn test_table_expr_key_doc_const() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@type 'field'
        local key = "field"
        local t = { [key] = 1 }
        value = t[key]
        "#,
        );

        let value_ty = ws.expr_ty("value");
        assert!(
            matches!(value_ty, LuaType::Integer | LuaType::IntegerConst(_)),
            "expected integer type, got {:?}",
            value_ty
        );
    }

    #[test]
    fn test_union_member_access_preserves_never() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@class A
        ---@field y never

        ---@class B
        ---@field y never

        ---@return A|B
        local function make() end

        local value = make()

        result = value.y
        "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("never"));
    }

    #[test]
    fn test_table_expr_index_preserves_never() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@return { y: number } & { y: string }
        local function impossible() end

        local t = {
            a = impossible().y,
        }

        result = t["a"]
        "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("never"));
    }

    #[test]
    fn test_rawget_guard_narrows_matching_index_expr() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@class T
        ---@field x? integer

        ---@type T
        local t = {}

        if rawget(t, "x") then
            result = t.x
        end
        "#,
        );

        assert_eq!(ws.expr_ty("result"), LuaType::Integer);
    }

    #[test]
    fn test_rawget_doc_function_guard_narrows_matching_index_expr() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@class T
        ---@field x? integer

        ---@type T
        local t = {}

        ---@class Utils
        ---@field get fun(tbl: T, key: "x"): std.RawGet<T, "x">

        ---@type Utils
        local utils = { get = rawget }

        if utils.get(t, "x") then
            result = t.x
        end
        "#,
        );

        assert_eq!(ws.expr_ty("result"), LuaType::Integer);
    }

    #[test]
    fn test_rawget_alias_guard_narrows_matching_index_expr() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@class T
        ---@field x? integer

        ---@type T
        local t = {}
        local get = rawget

        if get(t, "x") then
            result = t.x
        end
        "#,
        );

        assert_eq!(ws.expr_ty("result"), LuaType::Integer);
    }

    #[test]
    fn test_type_guard_call_narrows_matching_index_expr() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@generic T
        ---@param inst any
        ---@param type `T`
        ---@return TypeGuard<T>
        local function instance_of(inst, type)
            return true
        end

        ---@class T
        ---@field x? string|integer

        ---@type T
        local t = {}

        if instance_of(t.x, "string") then
            result = t.x
        end
        "#,
        );

        assert_eq!(ws.expr_ty("result"), LuaType::String);
    }

    #[test]
    fn test_alias_predicate_guard_narrows_matching_index_expr() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@class T
        ---@field x? integer

        ---@type T
        local t = {}

        local ok = t.x ~= nil
        if ok then
            result = t.x
        end
        "#,
        );

        assert_eq!(ws.expr_ty("result"), LuaType::Integer);
    }

    #[test]
    fn test_alias_chain_predicate_guard_keeps_matching_index_expr_wide() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@class T
        ---@field x? integer

        ---@type T
        local t = {}

        local has_x = t.x ~= nil
        local ok = has_x
        if ok then
            result = t.x
        end
        "#,
        );

        assert_eq!(ws.expr_ty("result"), ws.ty("integer?"));
    }
}
