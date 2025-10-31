#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

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
        assert!(ws.check_code_for(DiagnosticCode::ParamTypeNotMatch, r#"pred('hello', 1, {})"#));
        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
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
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
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
        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
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
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
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
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
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
        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            ---@type fun(name: string, age: number)
            local greet
            f(greet, "a", "b")
            "#,
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
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
        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
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
        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            ---@type fun(): number
            local greet
            local m = f(greet)
            accept(m)
            "#,
        ));

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
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
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
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
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            ---@type Partial<{name: string, age: number}>
            local m
            accept(m)
            "#,
        ));
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
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
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
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            ---@class A
            ---@overload fun(name: string, age: number)
            local A = {}

            f(A, "b", 1)
            f("A", "b", 1)

            "#,
        ));
        assert!(!ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
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
            assert!(ws.check_code_for(
                DiagnosticCode::ParamTypeNotMatch,
                r#"
              A, B, C =  f1(1, "2", true)
            "#,
            ));
            assert_eq!(ws.expr_ty("A"), ws.ty("integer"));
            assert_eq!(ws.expr_ty("B"), ws.ty("string"));
            assert_eq!(ws.expr_ty("C"), ws.ty("boolean"));
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
            assert!(ws.check_code_for(
                DiagnosticCode::ParamTypeNotMatch,
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
            assert!(!ws.check_code_for(
                DiagnosticCode::ParamTypeNotMatch,
                r#"
              G, H =  f3(1, "2")
            "#,
            ));
            assert_eq!(ws.expr_ty("G"), ws.ty("integer"));
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
            assert!(!ws.check_code_for(
                DiagnosticCode::ParamTypeNotMatch,
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
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
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
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            local a
            if is_type(a, "number") then
                f(a)
            end
            "#,
        ));
    }
}
