use crate::VirtualWorkspace;

#[test]
fn p9_deprecated_index_is_incrementally_maintained() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file(
        "old.lua",
        r#"
        ---@deprecated
        function old_api() end
        "#,
    );
    let consumer = ws.def_file("consumer.lua", "local x = old_api");
    let model = ws.analysis.semantic_model(consumer);
    assert!(model.is_global_deprecated("old_api"));

    // Remove the deprecated declaration via a normal incremental write.
    ws.def_file(
        "old.lua",
        r#"
        function old_api() end
        "#,
    );
    let model = ws.analysis.semantic_model(consumer);
    assert!(
        !model.is_global_deprecated("old_api"),
        "deprecated index must be updated on file write"
    );

    // Re-add it; the reverse transition must also work.
    ws.def_file(
        "old.lua",
        r#"
        ---@deprecated
        function old_api() end
        "#,
    );
    let model = ws.analysis.semantic_model(consumer);
    assert!(model.is_global_deprecated("old_api"));

    ws.analysis.db.rebuild_metrics.reset();
    ws.def_file(
        "old.lua",
        r#"
        function old_api() end
        "#,
    );
    assert_eq!(
        ws.analysis.db.rebuild_metrics.full_rebuilds(),
        0,
        "deprecated index update must stay incremental"
    );
    assert_eq!(
        ws.analysis.db.rebuild_metrics.workspace_index_rebuilds(),
        0,
        "deprecated index update must stay workspace-local"
    );
}
