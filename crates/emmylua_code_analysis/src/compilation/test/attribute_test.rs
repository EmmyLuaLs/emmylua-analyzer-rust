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
            ---@attribute constructor(name: string, root_class: string?, strip_self: boolean?, return_mode: "self"|"doc"|"default"?)

            ---@generic T
            ---@[constructor("__init")]
            ---@param name `T`
            ---@return T
            function meta(name)
            end
                "#,
            ),
        ]);
    }

    #[test]
    fn test_def_attribute() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.check_code_for(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        ---@[lsp_optimization("check_table_field")]
        local config = {}
        "#,
        );
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
                ---@attribute constructor(name: string, root_class: string?, strip_self: boolean?, return_mode: "self"|"doc"|"default"?)

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
            ---@attribute constructor(name: string, root_class: string?, strip_self: boolean?, return_mode: "self"|"doc"|"default"?)

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
        assert_eq!(ty_desc, "ClassB<T>");
    }

    #[test]
    fn test_issue_1008_new_mode() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "init.lua",
            r#"
            ---@attribute constructor(name: string, root_class: string?, strip_self: boolean?, return_mode: "self"|"doc"|"default"?)

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
                ---@attribute constructor(name: string, root_class: string?, strip_self: boolean?, return_mode: "self"|"doc"|"default"?)

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
}
