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

        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
                local a = items[index]
                local b = a.a
        "#,
        ));
    }
}
