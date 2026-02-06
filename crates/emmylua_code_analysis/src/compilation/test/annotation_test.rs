#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_issue_223() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.check_code_for(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
        --- @return integer
        function foo()
            local a
            return a --[[@as integer]]
        end
        "#,
        );
    }

    // workaround for table
    #[test]
    fn test_issue_234() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        GG = {} --- @type table

        GG.f = {}

        function GG.fun() end

        function GG.f.fun() end
        "#,
        );

        let ty = ws.expr_ty("GG.fun");
        assert_eq!(
            format!("{:?}", ty),
            "Signature(LuaSignatureId { file_id: FileId { id: 13 }, position: 76 })"
        );
    }

    #[test]
    fn test_issue_493() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        local async = {}
        --- @async
        --- @generic T, R
        --- @param argc integer
        --- @param func fun(...:T..., cb: fun(...:R...))
        --- @param ... T...
        --- @return R...
        function async.await(argc, func, ...)
            error('not implemented')
        end

        --- @param func async fun()
        function async.run(func)
            error('not implemented')
        end

        --- @alias FsStat {path: string, type:string, size:integer}

        --- @param path string
        --- @param callback fun(stat: FsStat)
        local function fs_stat(path, callback)
            error('not implemented')
        end

        async.run(function ()
            stat = async.await(2, fs_stat, 'a.lua')
        end)

        "#,
        );

        let ty = ws.expr_ty("stat");
        let expected = ws.ty("FsStat");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_issue_497() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        --- @generic T, R
        --- @param argc integer
        --- @param func fun(...:T..., cb: fun(...:R...))
        --- @return async fun(...:T...):R...
        local function wrap(argc, func) end

        --- @param a string
        --- @param b string
        --- @param callback fun(out: string)
        local function system(a, b, callback) end

        local wrapped = wrap(3, system)
        -- type is 'async fun(a: string, b: string): unknown'

        d = wrapped("a", "b")
        "#,
        );

        let ty = ws.expr_ty("d");
        let expected = ws.ty("string");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_generic_type_inference() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::TypeNotFound,
            r#"
            ---@class AnonymousObserver<T>: Observer<T>
        "#,
        ));
    }

    #[test]
    fn test_generic_type_extends() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@generic T
            ---@[constructor("__init")]
            ---@param name `T`
            ---@return T
            function meta(name)
            end
        "#,
        );
        ws.def(
            r#"
            ---@class State
            ---@field a string

            ---@class StateMachine<T: State>
            ---@field aaa T
            ---@field new fun(self: self): self
            StateMachine = meta("StateMachine")

            ---@return self
            function StateMachine:abc()
            end


            ---@return self
            function StateMachine:__init()
            end
            "#,
        );
        {
            ws.def(
                r#"
            A = StateMachine:new()
            "#,
            );
            let ty = ws.expr_ty("A");
            let expected = ws.ty("StateMachine<State>");
            assert_eq!(ty, expected);
        }
        {
            ws.def(
                r#"
            B = StateMachine:abc()
            "#,
            );
            let ty = ws.expr_ty("B");
            let expected = ws.ty("StateMachine<State>");
            assert_eq!(ty, expected);
        }
        {
            ws.def(
                r#"
            C = StateMachine:abc()
            "#,
            );
            let ty = ws.expr_ty("C");
            let expected = ws.ty("StateMachine<State>");
            assert_eq!(ty, expected);
        }
    }

    #[test]
    fn test_merge_right_mapped_type() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@generic T: table, U: table
        ---@param a T
        ---@param b U
        ---@return Merge<T, U>
        local function extend(a, b) end

        ---@type { x: number, y: number }
        local a = { x = 1, y = 2 }

        ---@type { y: string, z: boolean }
        local b = { y = "hello", z = true }

        c = extend(a, b)
        "#,
        );

        let ty = ws.expr_ty("c.y");
        let expected = ws.ty("string");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_extend_with_policy_overwrite() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@overload fun<T: table, U: table>(a: T, b: U, policy: "never"): T & U
        ---@overload fun<T: table, U: table>(a: T, b: U, policy: "right"): Merge<T, U>
        ---@overload fun<T: table, U: table>(a: T, b: U, policy: "left"): Merge<U, T>
        ---@generic T: table, U: table
        ---@param a T
        ---@param b U
        ---@param policy "never" | "left" | "right"
        ---@return (T & U) | Merge<T, U> | Merge<U, T>
        local function extend(a, b, policy) end

        ---@type { x: number, y: number }
        local a = { x = 1, y = 2 }

        ---@type { y: string, z: boolean }
        local b = { y = "hello", z = true }

        c_never = extend(a, b, "never")
        c_right = extend(a, b, "right")
        c_left = extend(a, b, "left")
        "#,
        );

        let never_ty = ws.expr_ty("c_never.y");
        let never_expected = ws.ty("never");
        assert_eq!(never_ty, never_expected);

        let right_ty = ws.expr_ty("c_right.y");
        let right_expected = ws.ty("string");
        assert_eq!(right_ty, right_expected);

        let left_ty = ws.expr_ty("c_left.y");
        let left_expected = ws.ty("number");
        assert_eq!(left_ty, left_expected);
    }

    #[test]
    fn test_intersection_conflict_yields_never() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@return { y: number } & { y: string }
        local function foo() end

        c = foo()
        "#,
        );

        let ty = ws.expr_ty("c.y");
        let expected = ws.ty("never");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_type_return_usage() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::AnnotationUsageError,
            r#"
            ---@type string
            return ""
        "#,
        ));
    }
}
