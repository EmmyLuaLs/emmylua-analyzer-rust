use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualCodeAction, check};
use emmylua_code_analysis::{DiagnosticCode, Emmyrc};
use googletest::prelude::*;

#[gtest]
fn test_1() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new();
    ws.def(
        r#"
            ---@class Cast1
            ---@field get fun(self: self, a: number): Cast1?
        "#,
    );

    check!(ws.check_code_action(
        r#"
                ---@type Cast1
                local A

                local _a = A:get(1):get(2):get(3)
            "#,
        vec![
            VirtualCodeAction {
                title: "use cast to remove nil".to_string()
            },
            VirtualCodeAction {
                title: "Disable current line diagnostic (need-check-nil)".to_string()
            },
            VirtualCodeAction {
                title: "Disable all diagnostics in current file (need-check-nil)".to_string()
            },
            VirtualCodeAction {
                title: "Disable all diagnostics in current project (need-check-nil)".to_string()
            },
            VirtualCodeAction {
                title: "use cast to remove nil".to_string()
            },
            VirtualCodeAction {
                title: "Disable current line diagnostic (need-check-nil)".to_string()
            },
            VirtualCodeAction {
                title: "Disable all diagnostics in current file (need-check-nil)".to_string()
            },
            VirtualCodeAction {
                title: "Disable all diagnostics in current project (need-check-nil)".to_string()
            },
        ]
    ));

    Ok(())
}

#[gtest]
fn test_add_doc_tag() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new();
    let mut emmyrc = Emmyrc::default();
    emmyrc
        .diagnostics
        .enables
        .push(DiagnosticCode::UnknownDocTag);
    ws.analysis.update_config(emmyrc.into());
    check!(ws.check_code_action(
        r#"
                ---@class Cast1
                ---@foo bar
            "#,
        vec![
            VirtualCodeAction {
                title: "Add @foo to the list of known tags".to_string()
            },
            VirtualCodeAction {
                title: "Disable current line diagnostic (unknown-doc-tag)".to_string()
            },
            VirtualCodeAction {
                title: "Disable all diagnostics in current file (unknown-doc-tag)".to_string()
            },
            VirtualCodeAction {
                title: "Disable all diagnostics in current project (unknown-doc-tag)".to_string()
            },
        ]
    ));

    Ok(())
}
