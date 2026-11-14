#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_constructor() {
        let mut ws = VirtualWorkspace::new();

        ws.def_files(vec![
            (
                "init.lua",
                r#"
                A = meta("A")
                "#,
            ),
            (
                "meta.lua",
                r#"
            ---@class Attribute
            ---@class constructor: Attribute
            ---@overload fun(name: string, root_class?: string, strip_self?: boolean, return_mode?: "self"|"doc"|"default")

            ---@generic T
            ---@[constructor("__init")]
            ---@param name `T`
            ---@return T
            function meta(name)
            end
                "#,
            ),
        ]);

        let ty = ws.expr_ty("A");
        assert_eq!(ws.humanize_type(ty), "A");
    }

    #[test]
    fn test_attribute_overload_uses_arg_type_for_diagnostic() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AttributeParamTypeMismatch,
            r#"
        ---@class Attribute
        ---@class custom_attribute: Attribute
        ---@overload fun(value: string)
        ---@overload fun(value: integer)

        ---@[custom_attribute(1)]
        local value
        "#,
        ));
    }

    #[test]
    fn test_delayed_definition() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@[lsp_optimization("delayed_definition")]
        local config

        function func()
            A = config
        end

        config = 1
        "#,
        );

        let ty = ws.expr_ty("A");
        let ty_desc = ws.humanize_type(ty);
        assert_eq!(ty_desc, "integer");
    }

    #[test]
    fn test_constructor_attribute() {
        let mut ws = VirtualWorkspace::new();

        ws.def_files(vec![
            (
                "1_main.lua",
                r#"
                local MyClass = require("2_myclass")

                instance = MyClass("Test")
                "#,
            ),
            (
                "2_myclass.lua",
                r#"
                ---@class MyClass
                local MyClass = meta("MyClass")

                ---@param name string
                function MyClass:init(name)
                end

                return MyClass
                "#,
            ),
            (
                "3_meta.lua",
                r#"
                ---@class Attribute
                ---@class constructor: Attribute
                ---@overload fun(name: string, root_class?: string, strip_self?: boolean, return_mode?: "self"|"doc"|"default")

                ---@class class
                ---@field is_class true

                ---@generic T
                ---@[constructor("init", "class")]
                ---@param class `T`
                ---@return T
                function meta(class) return {} end
                "#,
            ),
        ]);

        let ty = ws.expr_ty("instance");
        let ty_desc = ws.humanize_type(ty);
        assert_eq!(ty_desc, "MyClass");
    }

    #[test]
    fn test_issue_1008() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "init.lua",
            r#"
            ---@class Attribute
            ---@class constructor: Attribute
            ---@overload fun(name: string, root_class?: string, strip_self?: boolean, return_mode?: "self"|"doc"|"default")

            ---@generic T
            ---@[constructor("init")]
            ---@param class `T`
            ---@return T
            function class(class) return {} end

            ---@class ClassB<T>
            ClassB = class("ClassB")

            ---@param value T
            function ClassB:init(value) end
            "#,
        );

        ws.def(
            r#"
            A = ClassB("I'm ClassB")
            "#,
        );

        let ty = ws.expr_ty("A");
        let ty_desc = ws.humanize_type(ty);
        assert_eq!(ty_desc, "ClassB<string>");
    }

    #[test]
    fn test_issue_1008_new_mode() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "init.lua",
            r#"
            ---@class Attribute
            ---@class constructor: Attribute
            ---@overload fun(name: string, root_class?: string, strip_self?: boolean, return_mode?: "self"|"doc"|"default")

            ---@generic T
            ---@[constructor("init")]
            ---@param class `T`
            ---@return T
            function class(class) return {} end

            ---@class ClassB<T>
            ClassB = class("ClassB")

            ---@generic T
            ---@param value T
            ---@return ClassB<T>
            function ClassB:init(value) end
            "#,
        );

        ws.def(
            r#"
            A = ClassB("I'm ClassB")
            "#,
        );

        let ty = ws.expr_ty("A");
        let ty_desc = ws.humanize_type(ty);
        assert_eq!(ty_desc, "ClassB<string>");
    }

    #[test]
    fn test_attribute_constructor_return_mode() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "init.lua",
            r#"
                ---@class Attribute
                ---@class constructor: Attribute
                ---@overload fun(name: string, root_class?: string, strip_self?: boolean, return_mode?: "self"|"doc"|"default")

                ---@generic T
                ---@[constructor("__init")]
                ---@param name `T`
                ---@return T
                function class(name)
                    return {}
                end
            "#,
        );

        ws.def(
            r#"
            ---@class ClassA
            ---@field a number
            local classA = class("ClassA")

            function classA:__init()
                self.a = 1
            end
            A = classA()
            "#,
        );

        let ty = ws.expr_ty("A");
        let ty_desc = ws.humanize_type(ty);
        assert_eq!(ty_desc, "ClassA");
    }

    /// Defines the `constructor` attribute class and a `meta`-style factory whose
    /// first parameter is annotated with the given attribute line.
    fn define_meta(ws: &mut VirtualWorkspace, attribute_line: &str) {
        ws.def_file(
            "init.lua",
            &format!(
                r#"
                ---@class Attribute
                ---@class constructor: Attribute
                ---@overload fun(name: string, root_class?: string, strip_self?: boolean, return_mode?: "self"|"doc"|"default")

                ---@generic T
                {attribute_line}
                ---@param name `T`
                ---@return T
                function class(name)
                end
                "#
            ),
        );
    }

    fn define_constructor_class(ws: &mut VirtualWorkspace, init: &str) {
        ws.def(&format!(
            r#"
            ---@class ClassA
            ClassA = class("ClassA")

            {init}
            "#
        ));
    }

    #[test]
    fn test_constructor_strip_self_default_omits_self() {
        let mut ws = VirtualWorkspace::new();
        define_meta(&mut ws, r#"---@[constructor("__init")]"#);
        define_constructor_class(
            &mut ws,
            r#"
            ---@param value number
            function ClassA:__init(value)
                self.value = value
            end
            "#,
        );

        assert!(ws.has_no_diagnostic(DiagnosticCode::MissingParameter, "ClassA(1)"));
    }

    #[test]
    fn test_constructor_strip_self_false_requires_self_argument() {
        let mut ws = VirtualWorkspace::new();
        define_meta(&mut ws, r#"---@[constructor("__init", nil, false)]"#);
        define_constructor_class(
            &mut ws,
            r#"
            ---@param value number
            function ClassA:__init(value)
                self.value = value
            end
            "#,
        );

        // `self` is not stripped, so the explicit receiver becomes the first argument.
        assert!(!ws.has_no_diagnostic(DiagnosticCode::MissingParameter, "ClassA(1)"));
        assert!(ws.has_no_diagnostic(DiagnosticCode::MissingParameter, "ClassA({}, 1)"));
    }

    #[test]
    fn test_constructor_return_mode_default_uses_documented_return() {
        let mut ws = VirtualWorkspace::new();
        define_meta(&mut ws, r#"---@[constructor("__init")]"#);
        define_constructor_class(
            &mut ws,
            r#"
            ---@return number
            function ClassA:__init()
                return 1
            end
            "#,
        );

        let ty = ws.expr_ty("ClassA()");
        assert_eq!(ws.humanize_type(ty), "number");
    }

    #[test]
    fn test_constructor_return_mode_default_without_documented_return_uses_instance() {
        let mut ws = VirtualWorkspace::new();
        define_meta(&mut ws, r#"---@[constructor("__init")]"#);
        define_constructor_class(
            &mut ws,
            r#"
            function ClassA:__init()
            end
            "#,
        );

        let ty = ws.expr_ty("ClassA()");
        assert_eq!(ws.humanize_type(ty), "ClassA");
    }

    #[test]
    fn test_constructor_return_mode_self_ignores_documented_return() {
        let mut ws = VirtualWorkspace::new();
        define_meta(&mut ws, r#"---@[constructor("__init", nil, true, "self")]"#);
        define_constructor_class(
            &mut ws,
            r#"
            ---@return number
            function ClassA:__init()
                return 1
            end
            "#,
        );

        let ty = ws.expr_ty("ClassA()");
        assert_eq!(ws.humanize_type(ty), "ClassA");
    }
}
