#[cfg(test)]
mod test {
    use std::{ops::Deref, sync::Arc};

    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_array_index() {
        let mut ws = VirtualWorkspace::new();
        let mut emmyrc = ws.analysis.get_emmyrc().deref().clone();
        emmyrc.strict.array_index = false;
        ws.analysis.update_config(Arc::new(emmyrc));
        ws.def(
            r#"
            ---@class Test.Add
            ---@field a string

            ---@type int
            index = 1
            ---@type Test.Add[]
            items = {}
        "#,
        );

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
                local a = items[index]
                local b = a.a
        "#,
        ));
    }

    #[test]
    fn test_create_array() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T
            ---@param ... T
            ---@return T[]
            local function new_array(...)
            end

            t = new_array(1, 2, 3, 4, 5)
        "#,
        );

        let t = ws.expr_ty("t");
        let t_expected = ws.ty("integer[]");
        assert_eq!(t, t_expected)
    }

    #[test]
    fn test_array_for_flow() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
        --- @param _x string
        local function foo(_x) end

        local list = {} --- @type string[]

        for i = #list, 1, -1 do
            foo(list[i])
        end
        "#,
        ));
    }
}
