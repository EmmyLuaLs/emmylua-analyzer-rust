#[cfg(test)]
mod test {
    use emmylua_parser::{LuaAstNode, LuaLocalName};

    use crate::{DiagnosticCode, LuaDeclId, LuaSemanticDeclId, VirtualWorkspace};

    fn assert_type_decl_deprecated(content: &str, name: &str) {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(content);
        let db = ws.analysis.compilation.get_db();
        let type_decl = db
            .get_type_index()
            .find_type_decl(file_id, name, db.resolve_workspace_id(file_id))
            .expect("type declaration must exist");
        let property = db
            .get_property_index()
            .get_property(&LuaSemanticDeclId::TypeDecl(type_decl.get_id()))
            .expect("type declaration property must exist");

        assert!(property.deprecated().is_some());
    }

    fn assert_lua_decl_deprecated(content: &str, name: &str) {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(content);
        let db = ws.analysis.compilation.get_db();
        let local_name = ws.get_node::<LuaLocalName>(file_id);
        assert_eq!(local_name.get_text(), name);
        let decl = db
            .get_decl_index()
            .get_decl(&LuaDeclId::new(file_id, local_name.get_position()))
            .expect("declaration must exist");
        let property = db
            .get_property_index()
            .get_property(&LuaSemanticDeclId::LuaDecl(decl.get_id()))
            .expect("declaration property must exist");

        assert!(property.deprecated().is_some());
    }

    #[test]
    fn test_deprecated_alias_use() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::Deprecated,
            r#"
            ---@deprecated test
            ---@alias std.ConstTpl<T> unknown
        "#
        ));
    }

    #[test]
    fn test_deprecated_alias_no_usage_error() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AnnotationUsageError,
            r#"
            ---@deprecated test
            ---@alias std.ConstTpl<T> unknown
        "#
        ));
    }

    #[test]
    fn test_deprecated_alias_attaches_to_type_decl() {
        assert_type_decl_deprecated(
            r#"
            ---@deprecated test
            ---@alias ConstTpl unknown
        "#,
            "ConstTpl",
        );
    }

    #[test]
    fn test_deprecated_alias_after_alias_attaches_to_type_decl() {
        assert_type_decl_deprecated(
            r#"
            ---@alias ConstTpl unknown
            ---@deprecated test
        "#,
            "ConstTpl",
        );
    }

    #[test]
    fn test_deprecated_class_no_usage_error() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AnnotationUsageError,
            r#"
            ---@deprecated test
            ---@class Foo
        "#
        ));
    }

    #[test]
    fn test_deprecated_class_attaches_to_type_decl() {
        assert_type_decl_deprecated(
            r#"
            ---@deprecated test
            ---@class Foo
            local Foo = {}
        "#,
            "Foo",
        );
    }

    #[test]
    fn test_deprecated_class_usage_diagnostic() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::Deprecated,
            r#"
            ---@deprecated test
            ---@class Foo
            local Foo = {}

            local x = Foo
        "#
        ));
    }

    #[test]
    fn test_deprecated_class_type_annotation_diagnostic() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::Deprecated,
            r#"
            ---@deprecated
            ---@class A

            ---@type A
            local a
        "#
        ));
    }

    #[test]
    fn test_deprecated_class_param_annotation_diagnostic() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::Deprecated,
            r#"
            ---@deprecated
            ---@class A

            ---@param a A
            local function f(a)
            end
        "#
        ));
    }

    #[test]
    fn test_deprecated_class_after_class_attaches_to_decl() {
        assert_lua_decl_deprecated(
            r#"
            ---@class Foo
            ---@deprecated test
            local Foo = {}
        "#,
            "Foo",
        );
    }
}
