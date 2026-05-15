#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_builtin_pick_preserves_selected_properties() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
            ---@class BuiltinPickUser
            ---@field name string
            ---@field age number
            ---@field email string
            ---@field nickname? string

            ---@type Pick<BuiltinPickUser, "name" | "age" | "nickname">
            local picked
            PickedName = picked.name
            PickedAge = picked.age
            PickedNickname = picked.nickname

            ---@type Pick<BuiltinPickUser, keyof BuiltinPickUser>
            local pickedAll
            PickedAllEmail = pickedAll.email

            ---@type Pick<{id: integer, enabled: boolean, label?: string}, "id" | "label">
            local pickedLiteral
            PickedLiteralId = pickedLiteral.id
            PickedLiteralLabel = pickedLiteral.label
            "#,
        );

        assert_eq!(ws.expr_ty("PickedName"), ws.ty("string"));
        assert_eq!(ws.expr_ty("PickedAge"), ws.ty("number"));
        assert_eq!(ws.expr_ty("PickedNickname"), ws.ty("string?"));
        assert_eq!(ws.expr_ty("PickedAllEmail"), ws.ty("string"));
        assert_eq!(ws.expr_ty("PickedLiteralId"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("PickedLiteralLabel"), ws.ty("string?"));
    }

    #[test]
    fn test_builtin_pick_matches_ts6_key_constraint() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
            ---@class BuiltinPickConstraintUser
            ---@field name string
            ---@field age number
            "#,
        );

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@type Pick<BuiltinPickConstraintUser, "missing">
            local picked
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@type Pick<BuiltinPickConstraintUser, "name" | "missing">
            local picked
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UndefinedField,
            r#"
            ---@type Pick<BuiltinPickConstraintUser, never>
            local picked
            local name = picked.name
            "#
        ));
    }

    #[test]
    fn test_builtin_pick_empty_keyof_domain_converges() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
            ---@class BuiltinEmptyPickClass

            ---@type Pick<{}, keyof {}>
            local pickedEmptyObject
            PickedEmptyObjectMissing = pickedEmptyObject.missing

            ---@type Pick<BuiltinEmptyPickClass, keyof BuiltinEmptyPickClass>
            local pickedEmptyClass
            PickedEmptyClassMissing = pickedEmptyClass.missing
            "#,
        );

        assert_eq!(ws.expr_ty("PickedEmptyObjectMissing"), ws.ty("nil"));
        assert_eq!(ws.expr_ty("PickedEmptyClassMissing"), ws.ty("nil"));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@type Pick<{}, keyof {}>
            local picked
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UndefinedField,
            r#"
            ---@type Pick<{}, keyof {}>
            local picked
            local missing = picked.missing
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UndefinedField,
            r#"
            ---@class BuiltinEmptyPickDiagnosticClass

            ---@type Pick<BuiltinEmptyPickDiagnosticClass, keyof BuiltinEmptyPickDiagnosticClass>
            local picked
            local missing = picked.missing
            "#
        ));
    }

    #[test]
    fn test_builtin_omit_removes_properties() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
            ---@class BuiltinOmitUser
            ---@field name string
            ---@field age number
            ---@field email string
            ---@field nickname? string

            ---@type Omit<BuiltinOmitUser, "email">
            local omitted
            OmittedName = omitted.name
            OmittedAge = omitted.age
            OmittedNickname = omitted.nickname
            OmittedEmail = omitted.email

            ---@type Pick<BuiltinOmitUser, Exclude<keyof BuiltinOmitUser, "email">>
            local pickedWithoutEmail
            PickedWithoutEmailEmail = pickedWithoutEmail.email

            ---@type Omit<{id: integer, enabled: boolean, label?: string}, "enabled">
            local omittedLiteral
            OmittedLiteralId = omittedLiteral.id
            OmittedLiteralLabel = omittedLiteral.label
            "#,
        );

        assert_eq!(ws.expr_ty("OmittedName"), ws.ty("string"));
        assert_eq!(ws.expr_ty("OmittedAge"), ws.ty("number"));
        assert_eq!(ws.expr_ty("OmittedNickname"), ws.ty("string?"));
        assert_eq!(ws.expr_ty("PickedWithoutEmailEmail"), ws.ty("nil"));
        assert_eq!(ws.expr_ty("OmittedEmail"), ws.ty("nil"));
        assert_eq!(ws.expr_ty("OmittedLiteralId"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("OmittedLiteralLabel"), ws.ty("string?"));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UndefinedField,
            r#"
            ---@type Omit<BuiltinOmitUser, "email">
            local omitted
            local email = omitted.email
            "#
        ));
    }

    #[test]
    fn test_builtin_omit_matches_ts6_keyof_any_behavior() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
            ---@class BuiltinOmitKeyUser
            ---@field name string
            ---@field age number
            ---@field email string

            ---@type Omit<BuiltinOmitKeyUser, "missing">
            local omitMissing
            OmitMissingName = omitMissing.name
            OmitMissingEmail = omitMissing.email

            ---@type Omit<BuiltinOmitKeyUser, never>
            local omitNever
            OmitNeverName = omitNever.name
            OmitNeverEmail = omitNever.email

            ---@type Omit<BuiltinOmitKeyUser, keyof BuiltinOmitKeyUser>
            local omitAll
            OmitAllName = omitAll.name
            "#,
        );

        assert_eq!(ws.expr_ty("OmitMissingName"), ws.ty("string"));
        assert_eq!(ws.expr_ty("OmitMissingEmail"), ws.ty("string"));
        assert_eq!(ws.expr_ty("OmitNeverName"), ws.ty("string"));
        assert_eq!(ws.expr_ty("OmitNeverEmail"), ws.ty("string"));
        assert_eq!(ws.expr_ty("OmitAllName"), ws.ty("nil"));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::GenericConstraintMismatch,
            r#"
            ---@type Omit<BuiltinOmitKeyUser, "missing">
            local omitted
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UndefinedField,
            r#"
            ---@type Omit<BuiltinOmitKeyUser, keyof BuiltinOmitKeyUser>
            local omitted
            local name = omitted.name
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::UndefinedField,
            r#"
            ---@type Omit<BuiltinOmitKeyUser, string>
            local omitted
            local name = omitted.name
            "#
        ));
    }
}
