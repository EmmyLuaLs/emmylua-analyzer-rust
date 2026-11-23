#[cfg(test)]
mod test {
    use emmylua_parser::{LuaAstNode, LuaIndexExpr};
    use smol_str::SmolStr;

    use crate::{LuaIndexAccessKey, LuaObjectType, LuaType, LuaUnionType, VirtualWorkspace};

    // 可选对象字段在短路条件前后都应保留具名字段类型.
    #[test]
    fn test_optional_object_field_preserves_shape() {
        let mut ws = VirtualWorkspace::new();
        let post = LuaType::Object(
            LuaObjectType::new(vec![(
                LuaIndexAccessKey::String("a".into()),
                LuaType::String,
            )])
            .into(),
        );
        let optional_post = LuaType::Union(LuaUnionType::Nullable(post.clone()).into());
        let headers = ws.ty("string?");
        let file_id = ws.def(
            r#"
        ---@class Fix01
        ---@field post? { a: string }

        ---@param options? Fix01
        local function request_url(options)
            if not options then
                return
            end
            local headers = options.post and options.post.a
        end
        "#,
        );
        let model = ws.analysis.semantic_model(file_id);
        let post_types: Vec<_> = model
            .chunk()
            .expect("chunk")
            .descendants::<LuaIndexExpr>()
            .filter(|expr| expr.get_text() == "options.post")
            .map(|expr| model.type_of_expr_at(expr.get_syntax_id(), expr.get_range().start()))
            .collect();

        assert_eq!(post_types, vec![optional_post, post]);
        let facts = model.file_facts().expect("file facts");
        let decl = facts.decl_named("headers").expect("headers declaration");
        assert_eq!(model.type_of_decl(&decl.id), Some(headers));
    }

    // 对象注解的完整投影应在可空和联合类型中保留字段.
    #[test]
    fn test_optional_object_type_projection() {
        let object = LuaType::Object(
            LuaObjectType::new(vec![(
                LuaIndexAccessKey::String("a".into()),
                LuaType::String,
            )])
            .into(),
        );
        let optional = LuaType::Union(LuaUnionType::Nullable(object.clone()).into());
        for (type_repr, expected) in [
            ("{ a: string }?", optional.clone()),
            ("{ a: string }|nil", optional),
            ("{ a: string }", object),
        ] {
            let mut ws = VirtualWorkspace::new();
            let file_id = ws.def(&format!(
                r#"
                ---@type {type_repr}
                local post
                "#,
            ));
            let model = ws.analysis.semantic_model(file_id);
            let facts = model.file_facts().expect("file facts");
            let decl = facts.decl_named("post").expect("post declaration");
            let syntax = decl.doc_type_syntax.expect("post annotation");
            assert_eq!(
                model.project_doc_type(model.file_id(), syntax, &[]),
                expected,
                "{type_repr}"
            );
            assert_eq!(model.type_of_decl(&decl.id), Some(expected), "{type_repr}");
        }
    }

    // 可选对象字段保留泛型实参, 类的泛型参数优先于命名空间中的同名类型.
    #[test]
    fn test_optional_object_generic_field_projection() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@namespace OptionalObject
            ---@class T

            ---@class Request<T>
            ---@field post? { a: T, nested?: { value: T }, [integer]: { value: T } }
            ---@field transform fun(value: T): T

            ---@type Request<string>
            local options
            Post = options.post
            A = options.post and options.post.a
            Nested = options.post and options.post.nested and options.post.nested.value
            Transform = options.transform
            "#,
        );

        let nested = LuaType::Object(
            LuaObjectType::new(vec![(
                LuaIndexAccessKey::String("value".into()),
                LuaType::String,
            )])
            .into(),
        );
        let post = LuaType::Object(
            LuaObjectType::new(vec![
                (LuaIndexAccessKey::String("a".into()), LuaType::String),
                (
                    LuaIndexAccessKey::String("nested".into()),
                    LuaType::Union(LuaUnionType::Nullable(nested.clone()).into()),
                ),
                (LuaIndexAccessKey::Type(LuaType::Integer), nested),
            ])
            .into(),
        );
        assert_eq!(
            ws.expr_ty("Post"),
            LuaType::Union(LuaUnionType::Nullable(post).into())
        );
        let optional_string = ws.ty("string?");
        for name in ["A", "Nested"] {
            assert_eq!(ws.expr_ty(name), optional_string, "{name}");
        }
        let LuaType::DocFunction(transform) = ws.expr_ty("Transform") else {
            panic!("transform must be a function");
        };
        assert_eq!(transform.get_params()[0].1, Some(LuaType::String));
        assert_eq!(transform.get_ret(), &LuaType::String);
    }

    // 可选对象的泛型别名应保留字段类型和定义所在的作用域.
    #[test]
    fn test_optional_object_alias_projection() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@namespace OptionalObjectAlias
            ---@class T
            ---@alias Post<T> { a: T }?
            ---@class Request<T>
            ---@field post Post<T>

            ---@type Request<string>
            local options
            Headers = options.post and options.post.a
            "#,
        );

        assert_eq!(ws.expr_ty("Headers"), ws.ty("string?"));
    }

    // 字段使用完整注解投影时仍保留内建泛型语义和数值常量.
    #[test]
    fn test_doc_field_projection_preserves_leaf_types() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Fields
            ---@field guard TypeGuard<string>
            ---@field value 42
            ---@type Fields
            local fields
            Guard = fields.guard
            Value = fields.value
            "#,
        );

        assert_eq!(
            [ws.expr_ty("Guard"), ws.expr_ty("Value")],
            [
                LuaType::TypeGuard(LuaType::String.into()),
                LuaType::IntegerConst(42)
            ]
        );
    }

    // 容器内函数的泛型独立于类泛型, 未声明泛型的函数仍使用类的实参.
    #[test]
    fn test_doc_projection_array_function_generic_scope() {
        for (field_type, callback) in [
            ("(fun<U>(value: U): U)[]", "instance.callbacks[1]"),
            ("(fun<T>(value: T): T)[]", "instance.callbacks[1]"),
            (
                "fun(): ((fun<U>(value: U): U)[])",
                "instance.callbacks()[1]",
            ),
            (
                "fun(): ((fun<T>(value: T): T)[])",
                "instance.callbacks()[1]",
            ),
        ] {
            let mut ws = VirtualWorkspace::new();
            ws.def(&format!(
                r#"
                ---@class Callbacks<T>
                ---@field callbacks {field_type}
                ---@field inherited (fun(value: T): T)[]
                ---@type Callbacks<string>
                local instance
                NumberResult = {callback}(123)
                BooleanResult = {callback}(true)
                InheritedResult = instance.inherited[1]("value")
                "#,
            ));

            assert_eq!(
                [
                    ws.expr_ty("NumberResult"),
                    ws.expr_ty("BooleanResult"),
                    ws.expr_ty("InheritedResult"),
                ],
                [
                    LuaType::Integer,
                    LuaType::BooleanConst(true),
                    LuaType::String
                ],
                "{field_type}",
            );
        }
    }

    // 函数放入数组或元组后, 其类型应与独立声明时一致.
    #[test]
    fn test_doc_projection_container_function_type() {
        for function_type in ["fun<U>(value: U): U", "fun<U>(value: [U]): [U]"] {
            for field_type in [format!("({function_type})[]"), format!("[{function_type}]")] {
                let mut ws = VirtualWorkspace::new();
                ws.def(&format!(
                    r#"
                    ---@class Container<T>
                    ---@field callbacks {field_type}
                    ---@type Container<string>
                    local instance
                    Callbacks = instance.callbacks
                    "#,
                ));
                let callbacks = ws.expr_ty("Callbacks");
                let callback = match callbacks {
                    LuaType::Array(array) => array.get_base().clone(),
                    LuaType::Tuple(tuple) => tuple.get_types()[0].clone(),
                    other => panic!("expected a container, got {other:?}"),
                };
                assert_eq!(callback, ws.ty(function_type), "{field_type}");
            }
        }
    }

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
    fn test_issue_1075_large_table_dynamic_string_key() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let names = (0..256)
            .map(|i| format!("            ITEM_{i} = \"Item {i}\","))
            .collect::<Vec<_>>()
            .join("\n");
        let strings = format!(
            r#"
        STRINGS = {{
            NAMES = {{
{names}
            }}
        }}
        "#
        );

        ws.def_files(vec![
            ("strings.lua", &strings),
            (
                "skinsutils.lua",
                r#"
        function get_skin_name(name)
            return STRINGS.NAMES[string.upper(name)]
        end

        Result = get_skin_name("item_1")
        "#,
            ),
        ]);

        let result_ty = ws.expr_ty("Result");
        let expected_ty = ws.ty("string?");
        assert!(ws.check_type(&expected_ty, &result_ty));
    }

    #[test]
    fn test_exact_missing_table_key_does_not_scan_broad_members() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        local t = {
            a = 1,
            b = "b",
        }

        value = t["missing"]
        "#,
        );

        assert_eq!(ws.expr_ty("value"), LuaType::Nil);
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
    fn test_nested_unresolved_prefix_keeps_member_owner_retry() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        M.child.leaf = 1
        M = {
            child = {},
        }
        A = M.child.leaf
        "#,
        );

        assert_eq!(ws.expr_ty("A"), LuaType::IntegerConst(1));
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

    #[test]
    fn test_self_member_with_unresolvable_receiver() {
        let mut ws = VirtualWorkspace::new();
        // `getTable()` 接收者无法静态解析时成员回退挂在隐式 self 声明上
        ws.def(
            r#"
            function getTable():m()
                self.x = 1
                B = self.x
            end
            "#,
        );
        let ty = ws.expr_ty("B");
        assert_eq!(ws.humanize_type(ty), "1");
    }
}
