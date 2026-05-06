#[cfg(test)]
mod test {

    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_1() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
                ---@class Component
                ---@class G.A
                ---@class G.B: Component

                ---@generic T: Component
                ---@param name `T`
                ---@return T
                local function new(name)
                    return name
                end

                new("G.A")
        "#
        ));
    }

    #[test]
    fn test_2() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
                ---@class Component
                ---@class G.A
                ---@class G.B: Component

                ---@generic T: Component
                ---@param name T
                ---@return T
                local function new(name)
                    return name
                end

                new("G.A")
        "#
        ));
    }

    #[test]
    fn test_3() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            local nargs = select('#')
        "#
        ));
    }

    #[test]
    fn test_4() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
                ---@class Component
                ---@class G.A
                ---@class G.B: Component
                ---@class G.C: G.B

                ---@generic T: Component
                ---@param name `T`
                ---@return T
                local function new(name)
                    return name
                end

                new("G.C")
        "#
        ));
    }

    #[test]
    fn test_class_1() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
                ---@class Component
                ---@class G.A
                ---@class G.B: Component

                ---@class GenericTest<T: Component>
                local M = {}

                ---@param a T
                function M.new(a)
                end

                ---@type G.A
                local a

                M.new(a)
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"

                ---@type G.B
                local b

                ---@type GenericTest
                local gt

                gt.new(b)
        "#
        ));
    }

    #[test]
    fn test_class_2() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@class Component
            ---@class G.A
            ---@class G.B: Component

            ---@class GenericTest<T: Component>
            local M = {}

            ---@param a T
            function M.new(a)
            end

            ---@type GenericTest<G.A>
            local a
        "#
        ));
    }

    #[test]
    fn test_class_generic_default_constraint_mismatch() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@class Base<T extends number = string>
            "#
        ));
    }

    #[test]
    fn test_bare_type_use_does_not_repeat_generic_default_constraint_mismatch() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "base.lua",
            r#"
            ---@class Base<T extends number = string>
            "#,
        );

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@type Base
            Base_A = {}
            "#
        ));
    }

    #[test]
    fn test_explicit_type_arg_still_reports_constraint_mismatch() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "base.lua",
            r#"
            ---@class Base<T extends number = number>
            "#,
        );

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@type Base<string>
            Base_A = {}
            "#
        ));
    }

    #[test]
    fn test_class_generic_default_constraint_match() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@class Base<T extends number = number>
            "#
        ));
    }

    #[test]
    fn test_dependent_generic_default_must_satisfy_rigid_type_param_constraint() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@class Box<T extends string, U extends T = "x">
            "#
        ));
    }

    #[test]
    fn test_dependent_generic_default_can_reference_same_type_param() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@class Box<T extends string, U extends T = T>
            "#
        ));
    }

    #[test]
    fn test_dependent_generic_default_uses_type_param_constraint_as_upper_bound() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@class Box<T extends string, U extends string = T>
            "#
        ));
    }

    #[test]
    fn test_alias_generic_default_constraint_mismatch() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@alias Base<T extends number = string> T
            "#
        ));
    }

    #[test]
    fn test_function_generic_default_constraint_mismatch() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@generic T extends number = string
            local function f()
            end
            "#
        ));
    }

    #[test]
    fn test_call_constraint_uses_uninferred_generic_default() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@generic T = number, U extends T
            ---@param value U
            local function f(value)
            end

            f("not a number")
            "#
        ));
    }

    #[test]
    fn test_extend_string() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
                ---@class ABC1

                ---@generic T: string
                ---@param t `T`
                ---@return T
                local function test(t)
                end

                test("ABC1")
        "#
        ));
    }

    #[test]
    fn test_str_tpl_ref_param() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
                ---@generic T
                ---@param a `T`
                local function bar(a)
                end

                ---@generic T
                ---@param a `T`
                local function foo(a)
                    bar(a)
                end
        "#
        ));
    }

    #[test]
    fn test_issue_516() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
                ---@generic T: table
                ---@param t T
                ---@return T
                local function wrap(t)
                    return t
                end

                local a --- @type string[]?
                wrap(assert(a))
        "#
        ));
    }

    #[test]
    fn test_union() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class ab

            ---@generic T
            ---@param a `T`|T
            ---@return T
            function name(a)
                return a
            end
        "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@type ab
            local a

            name(a)
        "#
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            name("ab")
        "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            name("a")
        "#
        ));
    }

    #[test]
    fn test_union_2() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T: table
            ---@param obj T
            function add(obj)
            end

            ---@class GCNode
        "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@generic T: table
            ---@param obj T | string
            ---@return T?
            function bindGC(obj)
                if type(obj) == "string" then
                    ---@type GCNode
                    obj = {}
                end

                return add(obj)
            end
        "#
        ));
    }

    #[test]
    fn test_union_3() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T: table
            ---@param obj T
            function add(obj)
            end


        "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"

            ---@class GCNode<T: table>
            GCNode = {}

            ---@param obj T
            ---@return T?
            function GCNode:bindGC(obj)
                return add(obj)
            end
        "#
        ));
    }

    #[test]
    fn test_object_constraint_with_class_duck_typing() {
        let mut ws = VirtualWorkspace::new();
        // A @class whose inferred Def type (e.g. from `return self`) should satisfy
        // an object constraint via structural duck typing, same as Ref types do.
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
                ---@class MyPos
                ---@field x number
                ---@field y number
                ---@field z number
                ---@overload fun(x: number, y: number, z: number): MyPos
                MyPos = {}
                MyPos.__index = MyPos

                setmetatable(MyPos, {
                    __call = function(x, y, z)
                        return setmetatable({ x = x, y = y, z = z }, MyPos)
                    end
                })

                function MyPos:next()
                    self.x = self.x + 1
                    self.y = self.y + 1
                    return self
                end

                ---@generic T: { x: number, y: number, z: number }
                ---@param pos T
                local function getTile(pos) end

                local p = MyPos(0, 0, 0):next()
                getTile(p)

                local p2 = MyPos(1, 1, 1)
                getTile(p2:next())
            "#
        ));
    }

    #[test]
    fn test_generic_keyof_param_scope() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@generic T, K extends keyof T
                ---@param object T
                ---@param key K
                ---@return std.RawGet<T, K>
                function pick(object, key)
                end

                ---@class Person
                ---@field name string
            "#,
        );
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@type Person
            local person

            pick(person, "abc")
        "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@type Person
            local person

            pick(person, "name")
        "#
        ));
    }

    #[test]
    fn test_overload_generic_constraint_merged_params() {
        // When a generic signature has overloads, the merged main signature
        // unions parameter types from all overloads. Constraint checks must
        // run against the resolved overload, not the merged signature.
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "mylib.lua",
            r#"
            ---@meta mylib
            local M = {}

            ---@generic T: table
            ---@overload fun(a: string, b: T?): T
            ---@overload fun(a: string, b: integer, c: T?): T
            function M.decode(a, b_or_c, c) end

            return M
        "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            local m = require("mylib")
            local ret = m.decode("x", 42)
        "#
        ));
    }
}
