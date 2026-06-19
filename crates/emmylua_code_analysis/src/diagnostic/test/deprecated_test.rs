#[cfg(test)]
mod test {
    use emmylua_parser::{LuaAstNode, LuaLocalName};

    use crate::{
        DiagnosticCode, LuaDeclId, LuaDeprecated, LuaMemberKey, LuaMemberOwner, LuaSemanticDeclId,
        VirtualWorkspace,
    };

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

    fn assert_type_decl_deprecated_message(content: &str, name: &str, expected: &str) {
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

        match property.deprecated() {
            Some(LuaDeprecated::DeprecatedWithMessage(message)) => assert_eq!(message, expected),
            Some(LuaDeprecated::Deprecated) => panic!("deprecated message must exist"),
            None => panic!("deprecated property must exist"),
        }
    }

    fn assert_type_decl_description(content: &str, name: &str, expected: &str) {
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

        assert_eq!(property.description().map(|it| it.as_str()), Some(expected));
    }

    fn assert_field_deprecated(content: &str, type_name: &str, field_name: &str) {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(content);
        let db = ws.analysis.compilation.get_db();
        let type_decl = db
            .get_type_index()
            .find_type_decl(file_id, type_name, db.resolve_workspace_id(file_id))
            .expect("type declaration must exist");
        let member_item = db
            .get_member_index()
            .get_member_item(
                &LuaMemberOwner::Type(type_decl.get_id()),
                &LuaMemberKey::Name(field_name.into()),
            )
            .expect("field member must exist");
        let member_id = member_item
            .get_member_ids()
            .into_iter()
            .next()
            .expect("field member id must exist");
        let property = db
            .get_property_index()
            .get_property(&LuaSemanticDeclId::Member(member_id))
            .expect("field property must exist");

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
    fn test_deprecated_alias_keeps_attached_description() {
        assert_type_decl_description(
            r#"
            ---this A
            ---@deprecated message
            ---@alias A<T> unknown
        "#,
            "A",
            "this A",
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

    #[test]
    fn test_deprecated_class_keeps_attached_description() {
        assert_type_decl_description(
            r#"
            ---Old user class
            ---@deprecated use ModernUser instead
            ---@class OldUser
            local OldUser = {}
        "#,
            "OldUser",
            "Old user class",
        );
    }

    #[test]
    fn test_deprecated_message_uses_inline_text_only() {
        assert_type_decl_deprecated_message(
            r#"
            ---@deprecated use ModernUser instead
            ---Old user class
            ---@class OldUser
            local OldUser = {}
        "#,
            "OldUser",
            "use ModernUser instead",
        );
    }

    #[test]
    fn test_deprecated_field_attaches_to_field() {
        assert_field_deprecated(
            r#"
            ---@class APIResponse
            ---@field success boolean
            ---@deprecated use errorMessage instead
            ---@field error string
        "#,
            "APIResponse",
            "error",
        );
    }
}
