#[cfg(test)]
mod test {

    use smol_str::SmolStr;

    use crate::{DiagnosticCode, LuaType, VirtualWorkspace};

    #[test]
    fn test_issue_421() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        local a         --- @type string?
        local b = { a } --- @type string[] error

        b[2] = nil
        "#,
        ));
    }

    #[test]
    fn test_issue_645() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
        --- @alias Dir -1|1

        ---@param d Dir
        local function foo(d) end

        foo(1)
        "#,
        ));
    }

    #[test]
    fn test_issue_925() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::TypeNotFound,
            r#"
            ---@alias Pick<T, K extends keyof T> { [P in K]: T[P]; }
        "#,
        ));
    }

    #[test]
    fn test_enum_flag_bitop_assignment_keeps_later_assign_check() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@enum SubscriberFlags
            local SubscriberFlags = {
                Tracking = 1 << 0
            }

            ---@class Subscriber
            ---@field flags SubscriberFlags

            ---@type Subscriber
            local subscriber

            subscriber.flags = subscriber.flags & ~SubscriberFlags.Tracking
            subscriber.flags = 9
            "#,
        ));
    }

    #[test]
    fn test_mixed_table_literal_member_types() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            local t = { 10, x = "hello", 30 }
            v1 = t[1]
            vx = t.x
            v2 = t[2]
            "#,
        );

        assert_eq!(ws.expr_ty("v1"), LuaType::IntegerConst(10));
        assert_eq!(
            ws.expr_ty("vx"),
            LuaType::StringConst(SmolStr::new("hello").into())
        );
        assert_eq!(ws.expr_ty("v2"), LuaType::IntegerConst(30));
    }

    #[test]
    fn test_mixed_table_literal_assign_to_tuple_or_class() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class MixedTarget
            ---@field [1] integer
            ---@field x string
            ---@field [2] integer

            ---@type MixedTarget
            local t = { 10, x = "hello", 30 }
            "#,
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            ---@class MixedTargetMismatch
            ---@field [1] string
            ---@field x string
            ---@field [2] integer

            ---@type MixedTargetMismatch
            local t = { 10, x = "hello", 30 }
            "#,
        ));
    }
}
