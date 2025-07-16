#[cfg(test)]
mod tests {
    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_enum_value_mismatch_string() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::EnumValueMismatch,
            r#"
                ---@enum Status
                local Status = {
                    PENDING = "pending",
                    COMPLETED = "completed",
                    FAILED = "failed"
                }

                ---@type Status
                local status

                if status == "invalid" then
                end
                "#,
        ));
    }

    #[test]
    fn test_enum_value_mismatch_number() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::EnumValueMismatch,
            r#"
                ---@enum ErrorCode
                local ErrorCode = {
                    SUCCESS = 0,
                    NOT_FOUND = 404,
                    SERVER_ERROR = 500
                }

                ---@type ErrorCode
                local code

                if code == 999 then
                end
                "#,
        ));
    }

    #[test]
    fn test_enum_value_mismatch_elseif() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::EnumValueMismatch,
            r#"
            ---@enum State
            local State = {
                ACTIVE = "active",
                INACTIVE = "inactive"
            }

            ---@type State
            local state

            if state == "active" then
            elseif state == "unknown" then
            end
            "#,
        ));
    }

    #[test]
    fn test_enum_value_mismatch_reverse_order() {
        let mut ws = VirtualWorkspace::new();

        assert!(!ws.check_code_for(
            DiagnosticCode::EnumValueMismatch,
            r#"
            ---@enum Color
            local Color = {
                RED = "red",
                GREEN = "green",
                BLUE = "blue"
            }

            ---@type Color
            local color

            if "purple" == color then
            end
            "#,
        ));
    }

    #[test]
    fn test_enum_value_valid_cases() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::EnumValueMismatch,
            r#"
                ---@enum Status
                local Status = {
                    PENDING = "pending",
                    COMPLETED = "completed"
                }

                ---@type Status
                local status

                if status == "pending" then
                elseif status == "completed" then
                end
                "#,
        ));
    }

    #[test]
    fn test_enum_value_valid_exact_matches() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::EnumValueMismatch,
            r#"
                ---@enum Numbers
                local Numbers = {
                    ONE = 1,
                    TWO = 2,
                    THREE = 3
                }

                ---@type Numbers
                local num = Numbers.ONE

                if num == 1 then
                end

                if num == 2 then
                end
            "#,
        ));
    }
}
