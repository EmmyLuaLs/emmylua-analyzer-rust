#[cfg(test)]
mod test {
    use emmylua_parser::LuaClosureExpr;

    use crate::{
        DiagnosticCode, GenericTplId, LuaSignatureId, LuaType, LuaTypeDeclId, TypeSubstitutor,
        VirtualWorkspace, complete_type_generic_args, instantiate_type_generic,
    };

    #[test]
    fn test_issue_586() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            --- @generic T
            --- @param cb fun(...: T...)
            --- @param ... T...
            function invoke1(cb, ...)
                cb(...)
            end

            invoke1(
                function(a, b, c)
                    _a = a
                    _b = b
                    _c = c
                end,
                1, "2", "3"
            )
            "#,
        );

        let a_ty = ws.expr_ty("_a");
        let b_ty = ws.expr_ty("_b");
        let c_ty = ws.expr_ty("_c");

        assert_eq!(a_ty, ws.ty("integer"));
        assert_eq!(b_ty, ws.ty("string"));
        assert_eq!(c_ty, ws.ty("string"));
    }

    #[test]
    fn test_issue_658() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            --- @generic T1, T2, R
            --- @param fn fun(_:T1..., _:T2...): R...
            --- @param ... T1...
            --- @return fun(_:T2...): R...
            local function curry(fn, ...)
            local nargs, args = select('#', ...), { ... }
            return function(...)
                local nargs2 = select('#', ...)
                for i = 1, nargs2 do
                args[nargs + i] = select(i, ...)
                end
                return fn(unpack(args, 1, nargs + nargs2))
            end
            end

            --- @param a string
            --- @param b string
            --- @param c table
            local function foo(a, b, c) end

            bar = curry(foo, 'a')
            "#,
        );

        let bar_ty = ws.expr_ty("bar");
        let expected = ws.ty("fun(b:string, c:table)");
        assert_eq!(bar_ty, expected);
    }

    #[test]
    fn test_generic_params() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Observable<T>
            ---@class Subject<T>: Observable<T>

            ---@generic T
            ---@param ... Observable<T>
            ---@return Observable<T>
            function concat(...)
            end
            "#,
        );

        ws.def(
            r#"
            ---@type Subject<number>
            local s1
            A = concat(s1)
            "#,
        );

        let a_ty = ws.expr_ty("A");
        let expected = ws.ty("Observable<number>");
        assert_eq!(a_ty, expected);
    }

    #[test]
    fn test_issue_646() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Base
            ---@field a string
            "#,
        );
        ws.def(
            r#"
            ---@generic T: Base
            ---@param file T
            function dirname(file)
                A = file.a
            end
            "#,
        );

        let a_ty = ws.expr_ty("A");
        let expected = ws.ty("string");
        assert_eq!(a_ty, expected);
    }

    #[test]
    fn test_local_generics_in_global_scope() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @generic T
                --- @param x T
                function foo(x)
                    a = x
                end
            "#,
        );
        let a_ty = ws.expr_ty("a");
        assert_eq!(a_ty, ws.ty("unknown"));
    }

    // Currently fails:
    /*
    #[test]
    fn test_local_generics_in_global_scope_member() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                t = {}

                --- @generic T
                --- @param x T
                function foo(x)
                    t.a = x
                end
                local b = t.a
            "#,
        );
        let a_ty = ws.expr_ty("t.a");
        assert_eq!(a_ty, LuaType::Unknown);
    }
    */

    #[test]
    fn test_issue_738() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Predicate<A> fun(...: A...): boolean
            ---@type Predicate<[string, integer, table]>
            pred = function() end
            "#,
        );
        assert!(ws.has_no_diagnostic(DiagnosticCode::ParamTypeMismatch, r#"pred('hello', 1, {})"#));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"pred('hello',"1", {})"#
        ));
    }

    #[test]
    fn test_infer_type() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias A01<T> T extends infer P and P or unknown

            ---@param v number
            function f(v)
            end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type A01<number>
            local a
            f(a)
            "#,
        ));
    }

    #[test]
    fn test_infer_type_params() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias A02<T> T extends (fun(v1: infer P)) and P or string

            ---@param v fun(v1: number)
            function f(v)
            end
            "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type A02<number>
            local a
            f(a)
            "#,
        ));
    }

    #[test]
    fn test_infer_type_params_extract() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias A02<T> T extends (fun(v0: number, v1: infer P)) and P or string

            ---@param v number
            function accept(v)
            end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type A02<fun(v0: number, v1: number)>
            local a
            accept(a)
            "#,
        ));
    }

    #[test]
    fn test_return_generic() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias A01<T> T

            ---@param v number
            function f(v)
            end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type A01<number>
            local a
            f(a)
            "#,
        ));
    }

    #[test]
    fn test_infer_parameters() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Parameters<T> T extends (fun(...: infer P): any) and P or unknown

            ---@generic T
            ---@param fn T
            ---@param ... Parameters<T>...
            function f(fn, ...)
            end
            "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type fun(name: string, age: number)
            local greet
            f(greet, "a", "b")
            "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type fun(name: string, age: number)
            local greet
            f(greet, "a", 1)
            "#,
        ));
    }

    #[test]
    fn test_infer_parameters_2() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias A01<T> T extends (fun(a: any, b: infer P): any) and P or number

            ---@alias A02 number

            ---@param v number
            function f(v)
            end
            "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type A01<fun(a: A02, b: string)>
            local a
            f(a)
            "#,
        ));
    }

    #[test]
    fn test_infer_return_parameters() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@alias ReturnType<T> T extends (fun(...: any): infer R) and R or unknown

            ---@generic T
            ---@param fn T
            ---@return ReturnType<T>
            function f(fn, ...)
            end

            ---@param v string
            function accept(v)
            end
            "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type fun(): number
            local greet
            local m = f(greet)
            accept(m)
            "#,
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type fun(): string
            local greet
            local m = f(greet)
            accept(m)
            "#,
        ));
    }

    #[test]
    fn test_type_mapped_pick() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@alias Pick<T, K extends keyof T> { [P in K]: T[P]; }

            ---@param v {name: string, age: number}
            function accept(v)
            end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Pick<{name: string, age: number, email: string}, "name" | "age">
            local m
            accept(m)
            "#,
        ));
    }

    #[test]
    fn test_type_partial() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@alias Partial<T> { [P in keyof T]?: T[P]; }

            ---@param v {name?: string, age?: number}
            function accept(v)
            end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Partial<{name: string, age: number}>
            local m
            accept(m)
            "#,
        ));
    }

    #[test]
    fn test_keyof_alias_residual_resolves_after_forwarding() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Keys<T> keyof T
            ---@alias ForwardKeys<T> Keys<T>

            ---@param key "a" | "b"
            function accept(key) end
            "#,
        );

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type ForwardKeys<{ a: string, b: number }>
            local key
            accept(key)
            "#
        ));
    }

    #[test]
    fn test_mapped_alias_residual_resolves_after_forwarding() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Copy<T> { [K in keyof T]: T[K]; }
            ---@alias ForwardCopy<T> Copy<T>

            ---@type ForwardCopy<{ a: string, b: number }>
            local copy

            A = copy.a
            B = copy.b
            "#,
        );

        assert_eq!(ws.expr_ty("A"), ws.ty("string"));
        assert_eq!(ws.expr_ty("B"), ws.ty("number"));
    }

    #[test]
    fn test_mapped_unresolved_key_domain_preserves_residual() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Copy<T> { [K in keyof T]: T[K]; }

            ---@generic T
            ---@param value Copy<T>
            ---@return Copy<T>
            function keep(value) end

            ---@type Copy<{ a: string }>
            local concrete

            A = keep(concrete).a
            "#,
        );

        assert_eq!(ws.expr_ty("A"), ws.ty("string"));
    }

    #[test]
    fn test_alias_argument_binding_ignores_shadowing_function_generic() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Box<T> fun<T>(x: T): T

            ---@type Box<string>
            local f

            Result = f(1)
            "#,
        );

        let result_ty = ws.expr_ty("Result");
        assert_eq!(ws.humanize_type(result_ty), "1");
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Box<string>
            local f

            f(1)
            "#
        ));
    }

    #[test]
    fn test_alias_argument_binding_ignores_shadowing_mapped_key() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Shadow<T> { [T in keyof { a: string }]: T; }

            ---@type Shadow<number>
            local value

            A = value.a
            "#,
        );

        assert_eq!(ws.expr_ty("A"), ws.ty(r#""a""#));
    }

    #[test]
    fn test_conditional_alias_residual_resolves_after_forwarding() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Extract<T, U> T extends U and T or never
            ---@alias KeepA<T> Extract<T, "a">
            ---@alias Forward<T> KeepA<T>
            "#,
        );

        let generic_ty = ws.ty(r#"Forward<"a" | "b">"#);
        let instantiated =
            instantiate_type_generic(ws.get_db_mut(), &generic_ty, &TypeSubstitutor::new());
        assert_eq!(instantiated, ws.ty(r#""a""#));
    }

    #[test]
    fn test_nested_mapped_conditional_alias_residual_resolves() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Wrapper<T>
            ---@alias UnwrapFields<T> { [K in keyof T]: T[K] extends Wrapper<infer U> and U or T[K]; }
            ---@alias Forward<T> UnwrapFields<T>

            ---@type Forward<{ a: Wrapper<string>, b: number }>
            local value

            A = value.a
            B = value.b
            "#,
        );

        assert_eq!(ws.expr_ty("A"), ws.ty("string"));
        assert_eq!(ws.expr_ty("B"), ws.ty("number"));
    }

    #[test]
    fn test_recursive_alias_instantiation_budget_falls_back_safely() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Loop<T> Loop<T>
            ---@alias Forward<T> Loop<T>

            ---@type Forward<string>
            local value

            Value = value
            "#,
        );

        let value_ty = ws.expr_ty("Value");
        assert_eq!(ws.humanize_type(value_ty), "Forward<string>");
    }

    #[test]
    fn test_issue_787() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Wrapper<T>

            ---@alias UnwrapUnion<T> { [K in keyof T]: T[K] extends Wrapper<infer U> and U or unknown; }

            ---@generic T
            ---@param ... T...
            ---@return UnwrapUnion<T>...
            function unwrap(...) end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type Wrapper<int>, Wrapper<int>, Wrapper<string>
            local a, b, c

            D, E, F = unwrap(a, b, c)
            "#,
        ));
        assert_eq!(ws.expr_ty("D"), ws.ty("int"));
        assert_eq!(ws.expr_ty("E"), ws.ty("int"));
        assert_eq!(ws.expr_ty("F"), ws.ty("string"));
    }

    #[test]
    fn test_infer_new_constructor() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias ConstructorParameters<T> T extends new (fun(...: infer P): any) and P or never

            ---@generic T
            ---@param name `T`|T
            ---@param ... ConstructorParameters<T>...
            function f(name, ...)
            end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@class A
            ---@overload fun(name: string, age: number)
            local A = {}

            f(A, "b", 1)
            f("A", "b", 1)

            "#,
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            f("A", "b", "1")
            "#,
        ));
    }

    #[test]
    fn test_variadic_base() {
        let mut ws = VirtualWorkspace::new();
        {
            ws.def(
                r#"
            ---@generic T
            ---@param ... T... # 所有传入参数合并为一个`可变序列`, 即(T1, T2, ...)
            ---@return T # 返回可变序列
            function f1(...) end
            "#,
            );
            assert!(ws.has_no_diagnostic(
                DiagnosticCode::ParamTypeMismatch,
                r#"
              A, B, C =  f1(1, "2", true)
            "#,
            ));
            let a_ty = ws.expr_ty("A");
            let b_ty = ws.expr_ty("B");
            let c_ty = ws.expr_ty("C");
            assert_eq!(ws.humanize_type(a_ty), "1");
            assert_eq!(ws.humanize_type(b_ty), "\"2\"");
            assert_eq!(ws.humanize_type(c_ty), "true");
        }
        {
            ws.def(
                r#"
                ---@generic T
                ---@param ... T...
                ---@return T... # `...`的作用是转换类型为序列, 此时 T 为序列, 那么 T... = T
                function f2(...) end
            "#,
            );
            assert!(ws.has_no_diagnostic(
                DiagnosticCode::ParamTypeMismatch,
                r#"
              D, E, F =  f2(1, "2", true)
            "#,
            ));
            assert_eq!(ws.expr_ty("D"), ws.ty("integer"));
            assert_eq!(ws.expr_ty("E"), ws.ty("string"));
            assert_eq!(ws.expr_ty("F"), ws.ty("boolean"));
        }

        {
            ws.def(
                r#"
            ---@generic T
            ---@param ... T # T为单类型, `@param ... T`在语义上等同于 TS 的 T[]
            ---@return T # 返回一个单类型
            function f3(...) end
            "#,
            );
            assert!(!ws.has_no_diagnostic(
                DiagnosticCode::ParamTypeMismatch,
                r#"
              G, H =  f3(1, "2")
            "#,
            ));
            let g_ty = ws.expr_ty("G");
            assert_eq!(ws.humanize_type(g_ty), "1");
            assert_eq!(ws.expr_ty("H"), ws.ty("any"));
        }

        {
            ws.def(
                r#"
            ---@generic T
            ---@param ... T # T为单类型
            ---@return T... # 将单类型转为可变序列返回, 即返回了(T, T, T, ...)
            function f4(...) end
            "#,
            );
            assert!(!ws.has_no_diagnostic(
                DiagnosticCode::ParamTypeMismatch,
                r#"
              I, J, K =  f4(1, "2")
            "#,
            ));
            assert_eq!(ws.expr_ty("I"), ws.ty("integer"));
            assert_eq!(ws.expr_ty("J"), ws.ty("integer"));
            assert_eq!(ws.expr_ty("K"), ws.ty("integer"));
        }
    }

    #[test]
    fn test_long_extends_1() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@alias IsTypeGuard<T>
            --- T extends "nil"
            ---     and nil
            ---     or T extends "number"
            ---         and number
            ---         or T

            ---@param v number
            function f(v)
            end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@type IsTypeGuard<"number">
            local a
            f(a)
            "#,
        ));
    }

    #[test]
    fn test_long_extends_2() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias std.type
            ---| "nil"
            ---| "number"
            ---| "string"
            ---| "boolean"
            ---| "table"
            ---| "function"
            ---| "thread"
            ---| "userdata"

            ---@alias TypeGuard<T> boolean
        "#,
        );

        ws.def(
            r#"
            ---@alias IsTypeGuard<T>
            --- T extends "nil"
            ---     and nil
            ---     or T extends "number"
            ---         and number
            ---         or T

            ---@param v number
            function f(v)
            end

            ---@generic TP: std.type
            ---@param obj any
            ---@param tp std.ConstTpl<TP>
            ---@return TypeGuard<IsTypeGuard<TP>>
            function is_type(obj, tp)
            end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            local a
            if is_type(a, "number") then
                f(a)
            end
            "#,
        ));
    }

    #[test]
    fn test_issue_846() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@alias Parameters<T extends function> T extends (fun(...: infer P): any) and P or never

            ---@param x number
            ---@param y number
            ---@return number
            function pow(x, y) end

            ---@generic F
            ---@param f F
            ---@return Parameters<F>
            function return_params(f) end
            "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            result = return_params(pow)
            "#,
        ));
        let result_ty = ws.expr_ty("result");
        assert_eq!(ws.humanize_type(result_ty), "(number,number)");
    }

    #[test]
    fn test_overload() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@class Expect
            ---@overload fun<T>(actual: T): T
            local expect = {}

            result = expect("")
            "#,
        ));
        let result_ty = ws.expr_ty("result");
        assert_eq!(ws.humanize_type(result_ty), "\"\"");
    }

    #[test]
    fn test_call_overload_self_generic() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Callable
            ---@overload fun<T>(self: self, value: T): T
            ---@type Callable
            local c

            result = c(c, 1)
            "#,
        );

        let result_ty = ws.expr_ty("result");
        assert_eq!(ws.humanize_type(result_ty), "1");
    }

    #[test]
    fn test_function_generic_constraint_is_fallback() {
        let mut ws = VirtualWorkspace::new();
        {
            ws.def(
                r#"
            ---@generic T: number
            ---@return T
            local function use()
            end

            result = use()
            "#,
            );

            let result_ty = ws.expr_ty("result");
            assert_eq!(ws.humanize_type(result_ty), "number");
        }
    }

    #[test]
    fn test_type_annotation_generic_constraint_is_not_default() {
        let mut ws = VirtualWorkspace::new();
        // 根据 ts 的行为, any调用函数的结果必然是any
        ws.def(
            r#"
            ---@class Box<T: number>
            local Box = {}

            ---@return T
            function Box:get()
            end

            ---@type Box
            local box

            Result = box:get()
            "#,
        );

        let result_ty = ws.expr_ty("Result");
        assert!(result_ty.is_any(), "{result_ty:?}");
    }

    #[test]
    fn test_generic_default_metadata_storage() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Box<T = string>

            ---@alias Optional<T = number> T | nil
            "#,
        );

        let db = ws.analysis.compilation.get_db();
        let box_params = db
            .get_type_index()
            .get_generic_params(&LuaTypeDeclId::global("Box"))
            .expect("Box generic params");
        assert_eq!(box_params.len(), 1);
        assert_eq!(box_params[0].name.as_str(), "T");
        assert_eq!(box_params[0].tpl_id, Some(GenericTplId::Type(0)));
        let box_default = box_params[0]
            .default_type
            .clone()
            .expect("Box default type");
        assert_eq!(ws.humanize_type(box_default), "string");

        let optional_params = ws
            .analysis
            .compilation
            .get_db()
            .get_type_index()
            .get_generic_params(&LuaTypeDeclId::global("Optional"))
            .expect("Optional generic params");
        assert_eq!(optional_params.len(), 1);
        assert_eq!(optional_params[0].name.as_str(), "T");
        assert_eq!(optional_params[0].tpl_id, Some(GenericTplId::Type(0)));
        let optional_default = optional_params[0]
            .default_type
            .clone()
            .expect("Optional default type");
        assert_eq!(ws.humanize_type(optional_default), "number");
    }

    #[test]
    fn test_function_generic_default_metadata_storage() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@generic T = string
            ---@return T
            local function id()
            end
            "#,
        );

        let closure = ws.get_node::<LuaClosureExpr>(file_id);
        let signature_id = LuaSignatureId::from_closure(file_id, &closure);
        let signature = ws
            .analysis
            .compilation
            .get_db()
            .get_signature_index()
            .get(&signature_id)
            .expect("signature");
        assert_eq!(signature.generic_params.len(), 1);
        assert_eq!(signature.generic_params[0].name, "T");
        let default_type = signature.generic_params[0]
            .default_type
            .clone()
            .expect("signature default type");
        assert_eq!(ws.humanize_type(default_type), "string");
    }

    #[test]
    fn test_bare_generic_type_uses_default() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Box<T = string>

            ---@type Box
            BoxDefault = {}
            "#,
        );

        let value_ty = ws.expr_ty("BoxDefault");
        assert_eq!(ws.humanize_type(value_ty), "Box<string>");
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingTypeArgument,
            r#"
            ---@type Box
            local value
            "#,
        ));
    }

    #[test]
    fn test_bare_generic_type_uses_default_with_extends() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Base<T extends number = string>
            ---@type Base
            Base_A = {}
            "#,
        );

        let value_ty = ws.expr_ty("Base_A");
        assert_eq!(ws.humanize_type(value_ty), "Base<string>");
    }

    #[test]
    fn test_partial_generic_type_fills_trailing_default() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Pair<T, U = string>

            ---@type Pair<number>
            PairValue = {}
            "#,
        );

        let value_ty = ws.expr_ty("PairValue");
        assert_eq!(ws.humanize_type(value_ty), "Pair<number,string>");
    }

    #[test]
    fn test_missing_non_defaulted_generic_param_still_reports() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Pair<T, U = string>
            "#,
        );

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingTypeArgument,
            r#"
            ---@type Pair
            local value
            "#,
        ));
    }

    #[test]
    fn test_missing_required_generic_param_does_not_complete_defaults() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Pair<T, U = string>
            "#,
        );

        let completion = complete_type_generic_args(
            ws.analysis.compilation.get_db(),
            &LuaTypeDeclId::global("Pair"),
            Vec::new(),
        );
        assert_eq!(completion.missing_required_count, 1);
        assert_eq!(completion.completed_args, None);

        ws.def(
            r#"
            ---@type Pair
            PairValue = {}
            "#,
        );
        assert!(matches!(ws.expr_ty("PairValue"), LuaType::Any));
    }

    #[test]
    fn test_generic_default_can_reference_earlier_param() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Pair<T = string, U = T[]>

            ---@type Pair<number>
            PairValue = {}
            "#,
        );

        let value_ty = ws.expr_ty("PairValue");
        assert_eq!(ws.humanize_type(value_ty), "Pair<number,number[]>");
    }

    #[test]
    fn test_generic_default_can_reference_defaulted_generic_type() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class A<T = string>

            ---@class B<U = A>

            ---@type B
            BValue = {}
            "#,
        );

        let value_ty = ws.expr_ty("BValue");
        assert_eq!(ws.humanize_type(value_ty), "B<A<string>>");

        let b_params = ws
            .analysis
            .compilation
            .get_db()
            .get_type_index()
            .get_generic_params(&LuaTypeDeclId::global("B"))
            .expect("B generic params");
        let default_type = b_params[0].default_type.clone().expect("B default type");
        assert_eq!(ws.humanize_type(default_type), "A<string>");
    }

    #[test]
    fn test_generic_default_can_reference_later_defaulted_generic_type() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class B<U = A>

            ---@class A<T = string>

            ---@type B
            BValue = {}
            "#,
        );

        let value_ty = ws.expr_ty("BValue");
        assert_eq!(ws.humanize_type(value_ty), "B<A<string>>");

        let b_params = ws
            .analysis
            .compilation
            .get_db()
            .get_type_index()
            .get_generic_params(&LuaTypeDeclId::global("B"))
            .expect("B generic params");
        let default_type = b_params[0].default_type.clone().expect("B default type");
        assert_eq!(ws.humanize_type(default_type), "A<string>");
    }

    #[test]
    fn test_generic_default_cycle_does_not_expand_forever() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class A<T = B>

            ---@class B<U = A>

            ---@type A
            AValue = {}
            "#,
        );

        let value_ty = ws.expr_ty("AValue");
        assert_eq!(ws.humanize_type(value_ty), "A<B>");
    }

    #[test]
    fn test_alias_body_reuses_pre_resolved_generic_metadata() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Optional<T = string> T | nil

            ---@type Optional
            OptionalValue = nil
            "#,
        );

        let value_ty = ws.expr_ty("OptionalValue");
        assert_eq!(
            ws.humanize_type_detailed(value_ty),
            "Optional<string> = string?"
        );
    }

    #[test]
    fn test_class_super_reuses_pre_resolved_generic_metadata() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Parent<T>
            ---@field value T

            ---@class Box<T = string>: Parent<T>

            ---@type Box
            local box
            BoxValue = box.value
            "#,
        );

        let value_ty = ws.expr_ty("BoxValue");
        assert_eq!(ws.humanize_type(value_ty), "string");
    }

    #[test]
    fn test_function_generic_defaults_at_call_sites() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T = string
            ---@return T
            function use_default()
            end

            ---@generic T = string
            ---@return T
            function use_explicit()
            end

            ---@generic T = string
            ---@param value T
            ---@return T
            function use_inferred(value)
            end

            DefaultResult = use_default()
            ExplicitResult = use_explicit--[[@<number>]]()
            InferredResult = use_inferred(1)
            "#,
        );

        let default_result = ws.expr_ty("DefaultResult");
        assert_eq!(ws.humanize_type(default_result), "string");
        let explicit_result = ws.expr_ty("ExplicitResult");
        assert_eq!(ws.humanize_type(explicit_result), "number");
        let inferred_result = ws.expr_ty("InferredResult");
        assert_eq!(ws.humanize_type(inferred_result), "1");
    }

    #[test]
    fn test_function_generic_default_can_reference_earlier_param_at_call_sites() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T = string, U = T[]
            ---@return U
            function use_nested_default()
            end

            DefaultResult = use_nested_default()
            ExplicitResult = use_nested_default--[[@<number>]]()
            "#,
        );

        let default_result = ws.expr_ty("DefaultResult");
        assert_eq!(ws.humanize_type(default_result), "string[]");
        let explicit_result = ws.expr_ty("ExplicitResult");
        assert_eq!(ws.humanize_type(explicit_result), "number[]");
    }

    #[test]
    fn test_function_variadic_generic_default_at_call_sites() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T = string
            ---@return T...
            function use_default_vararg_ret()
            end

            ---@generic T: string
            ---@return T...
            function use_constraint_vararg_ret()
            end

            DefaultA, DefaultB = use_default_vararg_ret()
            ConstraintResult = use_constraint_vararg_ret()
            "#,
        );

        let default_a = ws.expr_ty("DefaultA");
        assert_eq!(ws.humanize_type(default_a), "string");
        let default_b = ws.expr_ty("DefaultB");
        assert_eq!(ws.humanize_type(default_b), "string");
        let constraint_result = ws.expr_ty("ConstraintResult");
        assert_eq!(ws.humanize_type(constraint_result), "string");
    }

    #[test]
    fn test_generic_defaults_visible_before_cross_file_doc_analysis() {
        let mut ws = VirtualWorkspace::new();
        ws.def_files(vec![
            (
                "use.lua",
                r#"
                ---@type Box
                CrossFileBox = {}
                "#,
            ),
            (
                "decl.lua",
                r#"
                ---@class Box<T = string>
                "#,
            ),
        ]);

        let value_ty = ws.expr_ty("CrossFileBox");
        assert_eq!(ws.humanize_type(value_ty), "Box<string>");
    }

    #[test]
    fn test_generic_extends_function_params() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias ConstructorParameters<T> T extends new (fun(...: infer P): any) and P or never

            ---@alias Parameters<T extends function> T extends (fun(...: infer P): any) and P or never

            ---@alias ReturnType<T extends function> T extends (fun(...: any): infer R) and R or any

            ---@alias Procedure fun(...: any[]): any

            ---@alias MockParameters<T> T extends Procedure and Parameters<T> or never

            ---@alias MockReturnType<T> T extends Procedure and ReturnType<T> or never

            ---@class Mock<T>
            ---@field calls MockParameters<T>[]
            ---@overload fun(...: MockParameters<T>...): MockReturnType<T>
            "#,
        );
        {
            ws.def(
                r#"
                ---@generic T: Procedure
                ---@param a T
                ---@return Mock<T>
                local function fn(a)
                end

                local sum = fn(function(a, b)
                    return a + b
                end)
                A = sum
            "#,
            );

            let result_ty = ws.expr_ty("A");
            assert_eq!(
                ws.humanize_type_detailed(result_ty),
                "Mock<fun(a, b) -> any>"
            );
        }

        {
            ws.def(
                r#"
                ---@generic T: Procedure
                ---@param a T?
                ---@return Mock<T>
                local function fn(a)
                end
                fnresult = fn()

                result = fn().calls
            "#,
            );
            let fnresult_ty = ws.expr_ty("fnresult");
            assert_eq!(ws.humanize_type(fnresult_ty), "Mock<Procedure>");

            let result_ty = ws.expr_ty("result");
            assert_eq!(ws.humanize_type(result_ty), "any[][]");
        }
    }

    #[test]
    fn test_plain_tpl_literal_key_inference_widens_through_finalize() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias std.RawGet<T, K> unknown

            ---@alias std.ConstTpl<T> unknown

            ---@generic T, K extends keyof T
            ---@param object T
            ---@param key K
            ---@return std.RawGet<T, K>
            function pick(object, key)
            end

            ---@class Person
            ---@field age integer
        "#,
        );

        ws.def(
            r#"
            ---@type Person
            local person

            result = pick(person, "age")
        "#,
        );

        let result_ty = ws.expr_ty("result");
        assert_eq!(ws.humanize_type(result_ty), "integer");
    }

    #[test]
    fn test_const_tpl_candidate_preserves_literal_through_plain_return() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias std.ConstTpl<T> unknown

            ---@generic T
            ---@param value std.ConstTpl<T>
            ---@return T
            function keep_const(value)
            end

            result = keep_const("mode")
        "#,
        );

        let result_ty = ws.expr_ty("result");
        assert_eq!(ws.humanize_type(result_ty), "\"mode\"");
    }

    #[test]
    fn test_plain_tpl_top_level_return_preserves_primitive_literal() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T
            ---@param value T
            ---@return T
            function id(value)
            end

            result = id("mode")
        "#,
        );

        let result_ty = ws.expr_ty("result");
        assert_eq!(ws.humanize_type(result_ty), "\"mode\"");
    }

    #[test]
    fn test_transparent_alias_top_level_return_preserves_primitive_literal() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Id<T> T

            ---@generic T
            ---@param value T
            ---@return Id<T>
            function id(value)
            end

            result = id("mode")
        "#,
        );

        let result_ty = ws.expr_ty("result");
        assert_eq!(ws.humanize_type(result_ty), "\"mode\"");
    }

    #[test]
    fn test_plain_tpl_top_level_return_preserves_primitive_literal_union() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T
            ---@param value T
            ---@return T
            function id(value)
            end

            ---@alias Choice "left" | "right"

            ---@type Choice
            local choice

            result = id(choice)
        "#,
        );

        let result_ty = ws.expr_ty("result");
        assert_eq!(result_ty, ws.ty("\"left\" | \"right\""));
    }

    #[test]
    fn test_primitive_constraint_preserves_literal_candidate() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T: string
            ---@param value T
            ---@return T
            function constrained(value)
            end

            result = constrained("mode")
        "#,
        );

        let result_ty = ws.expr_ty("result");
        assert_eq!(ws.humanize_type(result_ty), "\"mode\"");
    }

    #[test]
    fn test_contextual_widening_keeps_bare_literal_but_widens_nested_literals() {
        use crate::{LuaMemberKey, LuaObjectType, WideningContext, widen_type_with_context};
        use smol_str::SmolStr;

        let mut ws = VirtualWorkspace::new();
        let bare = LuaType::StringConst(SmolStr::new("mode").into());
        assert_eq!(
            widen_type_with_context(bare.clone(), WideningContext::Root),
            bare
        );

        let object = LuaType::Object(
            LuaObjectType::new_with_fields(
                [(
                    LuaMemberKey::Name("kind".into()),
                    LuaType::StringConst(SmolStr::new("mode").into()),
                )]
                .into_iter()
                .collect(),
                Vec::new(),
            )
            .into(),
        );
        let widened = widen_type_with_context(object, WideningContext::Root);
        assert_eq!(widened, ws.ty("{ kind: string }"));
    }

    #[test]
    fn test_extends_true() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::TypeNotFound,
            r#"
            ---@alias TestA<T> T extends "test" and number or string
            ---@alias TestB<T> T extends true and number or string
            ---@alias TestC<T> T extends 111 and number or string
            "#,
        ));
    }

    #[test]
    fn test_issue_986() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Foo
            ---@field cost number

            ---@generic K extends keyof Foo
            ---@param key K
            ---@return Foo[K]
            function get(key)
            end

            A = get('cost')
        "#,
        );
        let result_ty = ws.expr_ty("A");
        assert_eq!(ws.humanize_type(result_ty), "number");
    }

    #[test]
    fn test_extends_conditional_generic() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Procedure fun(...: any...): any

            ---@alias MockContextCalls<T> T extends any... and any[] or T

            ---@alias ParametersNew<T extends function> T extends (fun(...: infer P): any) and P or never

            ---@alias MockParameters<T> T extends Procedure and ParametersNew<T> or never

            ---@class MockContext<T>
            ---@field calls (MockContextCalls<MockParameters<T>>)[]

            ---@class Mock<T>
            ---@field ctx MockContext<T>

            ---@type Mock<fun(...: any...): any>
            local mock

            Calls = mock.ctx.calls

        "#,
        );
        let result_ty = ws.expr_ty("Calls");
        assert_eq!(ws.humanize_type(result_ty), "any[][]");
    }

    #[test]
    fn test_extends_conditional_generic_preserves_partial_instantiation() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias IdIfHasFoo<T> T extends { foo: any } and T or never

            ---@class Inner<T>
            ---@field value IdIfHasFoo<T>

            ---@class Wrapper<U>
            ---@field inner Inner<{ foo: U }>

            ---@type Wrapper<string>
            local wrapper

            Value = wrapper.inner.value
            Foo = wrapper.inner.value.foo
        "#,
        );

        let value_ty = ws.expr_ty("Value");
        let value_desc = ws.humanize_type_detailed(value_ty);
        assert!(!value_desc.contains("foo: any"), "{value_desc}");

        let foo_ty = ws.expr_ty("Foo");
        let foo_desc = ws.humanize_type(foo_ty);
        assert_ne!(foo_desc, "any", "{foo_desc}");
    }

    #[test]
    fn test_generic_type_infer() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Base<T>
            ---@alias Holder<V> V
            "#,
        );
        {
            // 根据 TS 的做法, 如果 Base<T> 没有声明约束, 那么这里应该是 any 并报错
            ws.def(
                r#"
            ---@type Base
            Base_A = {}
            "#,
            );

            let v_ty = ws.expr_ty("Base_A");
            assert_eq!(ws.humanize_type_detailed(v_ty), "any");
            assert!(!ws.has_no_diagnostic(
                DiagnosticCode::MissingTypeArgument,
                r#"
            ---@type Base
            local a
            "#,
            ));
            assert!(!ws.has_no_diagnostic(
                DiagnosticCode::MissingTypeArgument,
                r#"
            ---@type Holder
            local h
            "#,
            ));
        }
    }

    #[test]
    fn test_overload_self_generic_class_instance() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            --- @class A<T>
            --- @overload fun(): self
            local ClassA

            --- @type A
            a1 = {}

            a2 = ClassA()

            ---@type A<string>
            a3 = ClassA()
            "#,
        );

        let a1_ty = ws.expr_ty("a1");
        assert!(a1_ty.is_any(), "{a1_ty:?}");

        let a2_ty = ws.expr_ty("a2");
        assert_eq!(ws.humanize_type(a2_ty), "A<unknown>");

        let a3_ty = ws.expr_ty("a3");
        assert_eq!(ws.humanize_type(a3_ty), "A<string>");
    }

    #[test]
    fn test_conditional_generic_missing_class_arg_uses_unknown_operand() {
        let mut ws = VirtualWorkspace::new();
        // `extends unknown` 几乎等效于返回`true`分支结果
        // 只有在分发`never`时才返回`never`, 但本质上 `never` 分发并没有对`unknown`做判断
        ws.def(
            r#"
            ---@alias IsString<T> T extends string and number or boolean
            ---@alias ExtendsUnknown<T> T extends unknown and number or boolean

            ---@class Box<T = unknown>
            ---@field value T

            ---@generic T
            ---@param box Box<T>
            ---@return IsString<T>
            function get(box) end

            ---@generic T
            ---@param value T
            ---@return ExtendsUnknown<T>
            function getExtendsUnknown(value) end

            ---@type Box
            local box

            Result = get(box)

            ---@type string
            local value

            ExtendsUnknownResult = getExtendsUnknown(value)
            "#,
        );

        let result_ty = ws.expr_ty("Result");
        assert_eq!(ws.humanize_type(result_ty), "boolean");

        let extends_unknown_ty = ws.expr_ty("ExtendsUnknownResult");
        assert_eq!(ws.humanize_type(extends_unknown_ty), "number");
    }

    #[test]
    fn test_conditional_generic_concrete_type_extends_any_selects_true_branch() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias ExtendsAny<T> T extends any and number or boolean

            ---@generic T
            ---@param value T
            ---@return ExtendsAny<T>
            function getExtendsAny(value) end

            ---@type string
            local value

            Result = getExtendsAny(value)
            "#,
        );

        let result_ty = ws.expr_ty("Result");
        assert_eq!(ws.humanize_type(result_ty), "number");
    }

    #[test]
    fn test_distributed_function_generic_conditional_return_filters_union_members() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@generic T
                ---@param value T
                ---@return T extends string and T or never
                function extractString(value) end

                ---@type string|integer
                local value

                A = extractString(value)
            "#,
        );

        assert_eq!(ws.expr_ty("A"), ws.ty("string"));
    }

    #[test]
    fn test_union_never() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias Exclude<T, U> T extends U and never or T

                ---@param value string
                function test_union_never(value) end
            "#,
        );

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"

            ---@type Exclude<string|nil, nil>
            local a

            test_union_never(a)
            "#,
        ));
    }

    #[test]
    fn test_distributed_extract_keeps_matching_union_members() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias Extract<T, U> T extends U and T or never

                ---@generic T
                ---@param value T
                ---@return Extract<T, string|nil>
                function extract(value) end

                ---@type string|integer|nil
                local value

                A = extract(value)
            "#,
        );

        assert_eq!(ws.expr_ty("A"), ws.ty("string|nil"));
    }

    #[test]
    fn test_distributed_extract_rejects_removed_union_member() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias Extract<T, U> T extends U and T or never

                ---@generic T
                ---@param value T
                ---@return Extract<T, string|nil>
                function extract(value) end

                ---@param value string|nil
                function accept(value) end
            "#,
        );

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
                ---@type integer
                local value

                accept(extract(value))
            "#,
        ));
    }

    #[test]
    fn test_distributed_literal_union_keeps_literal_member() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias Extract<T, U> T extends U and T or never

                ---@generic T
                ---@param value T
                ---@return Extract<T, "a">
                function extract(value) end

                ---@type "a"|"b"
                local value

                A = extract(value)
            "#,
        );

        let a_ty = ws.expr_ty("A");
        assert_eq!(ws.humanize_type(a_ty), "\"a\"");
    }

    #[test]
    fn test_distributed_non_nullable_removes_nil() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias Exclude<T, U> T extends U and never or T

                ---@generic T
                ---@param value T
                ---@return Exclude<T, nil>
                function excludeNil(value) end

                ---@type string|nil
                local value

                A = excludeNil(value)
            "#,
        );

        assert_eq!(ws.expr_ty("A"), ws.ty("string"));
    }

    #[test]
    fn test_distributed_never_remains_never() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias ToArray<T> T extends any and T[] or never

                ---@generic T
                ---@param value T
                ---@return ToArray<T>
                function toArray(value) end

                ---@type never
                local value

                A = toArray(value)
            "#,
        );

        assert_eq!(ws.expr_ty("A"), ws.ty("never"));
    }

    #[test]
    fn test_wrapped_checked_type_does_not_distribute() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias Wrapped<T, U> { value: T } extends { value: U } and T or never

                ---@generic T
                ---@param value T
                ---@return Wrapped<T, nil>
                function wrapped(value) end

                ---@type string|nil
                local value

                A = wrapped(value)
            "#,
        );

        assert_eq!(ws.expr_ty("A"), ws.ty("string|nil"));
    }

    #[test]
    fn test_tuple_wrapped_checked_type_does_not_distribute() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias WrappedTuple<T, U> [T] extends [U] and T or never

                ---@generic T
                ---@param value T
                ---@return WrappedTuple<T, nil>
                function wrappedTuple(value) end

                ---@type string|nil
                local value

                A = wrappedTuple(value)
            "#,
        );

        assert_eq!(ws.expr_ty("A"), ws.ty("string|nil"));
    }

    #[test]
    fn test_distributed_conditional_infer_object_union() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias ValueOf<T> T extends { value: infer P } and P or never

                ---@generic T
                ---@param value T
                ---@return ValueOf<T>
                function valueOf(value) end

                ---@type { value: string } | { value: integer }
                local value

                A = valueOf(value)
            "#,
        );

        assert_eq!(ws.expr_ty("A"), ws.ty("string|integer"));
    }
}
