use super::*;

#[test]
fn test_summary_builder_can_sync_file_from_vfs() {
    let emmyrc = Arc::new(Emmyrc::default());
    let mut vfs = Vfs::new();
    vfs.update_config(emmyrc.clone());

    let generator = VirtualUrlGenerator::new();
    let uri = generator.new_uri("summary_sync.lua");
    let file_id = vfs.set_file_content(&uri, Some("local value = 1\nreturn value".to_string()));

    let mut compilation = SalsaSummaryHost::new(emmyrc);
    compilation.set_workspaces(vec![Workspace::new(
        PathBuf::from(TEST_WORKSPACE_ROOT),
        WorkspaceImport::All,
        WorkspaceId::MAIN,
    )]);

    assert!(compilation.set_file_from_vfs(&vfs, file_id));

    let summary = compilation
        .file()
        .summary(file_id)
        .expect("summary builder summary from vfs");
    assert_eq!(summary.flow.return_count, 1);
    assert_eq!(
        summary
            .decl_tree
            .decls
            .iter()
            .filter(|decl| matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
            .count(),
        1
    );
}

#[test]
fn test_summary_builder_file_summary_is_independent() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Foo
local x = {}
if x then return x end"#;
    set_test_file(&mut compilation, 1, "C:/ws/foo/bar.lua", source);

    let file = compilation.file();
    let doc = compilation.doc();
    let lexical = compilation.lexical();
    let module = compilation.module();
    let flow = compilation.flow();
    let flow_queries = flow;

    let summary = file
        .summary(FileId::new(1))
        .expect("summary builder summary");
    assert_eq!(
        summary
            .decl_tree
            .decls
            .iter()
            .filter(|decl| matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
            .count(),
        1
    );
    assert_eq!(summary.flow.branch_count, 1);
    assert_eq!(summary.flow.return_count, 1);
    assert_eq!(summary.flow.block_count, 2);
    assert!(matches!(
        summary.module.as_ref().and_then(|module| module.export_target.as_ref()),
        Some(SalsaExportTargetSummary::LocalName(name)) if name == "x"
    ));

    let doc_summary = doc
        .summary(FileId::new(1))
        .expect("summary builder doc summary");
    let doc_type_summary = doc
        .types(FileId::new(1))
        .expect("summary builder doc type summary");
    let flow_summary = flow_queries
        .summary(FileId::new(1))
        .expect("summary builder flow summary");
    let signature_summary = doc
        .signatures(FileId::new(1))
        .expect("summary builder signature summary");
    let doc_owner_binding_summary = doc
        .owner_bindings(FileId::new(1))
        .expect("summary builder doc owner binding summary");
    let use_site_summary = lexical
        .use_sites(FileId::new(1))
        .expect("summary builder use site summary");
    let module_summary = module
        .summary(FileId::new(1))
        .expect("summary builder module summary");

    assert_eq!(doc_summary.as_ref(), &summary.doc);
    assert_eq!(doc_type_summary.as_ref(), &summary.doc_types);
    assert_eq!(flow_summary.as_ref(), &summary.flow);
    assert_eq!(signature_summary.as_ref(), &summary.signatures);
    assert_eq!(doc_owner_binding_summary.as_ref(), &summary.doc_owner_bindings);
    assert_eq!(use_site_summary.as_ref(), &summary.use_sites);
    assert_eq!(module_summary.as_ref(), summary.module.as_ref().expect("file summary module"));
    let property_summary = file
        .properties(FileId::new(1))
        .expect("summary builder property summary");
    let table_shape_summary = file
        .table_shapes(FileId::new(1))
        .expect("summary builder table shape summary");

    let decl_tree = file
        .decl_tree(FileId::new(1))
        .expect("summary builder decl tree");
    let globals = file
        .globals(FileId::new(1))
        .expect("summary builder global summary");

    assert_eq!(decl_tree.as_ref(), &summary.decl_tree);
    assert_eq!(globals.as_ref(), &summary.globals);
    assert_eq!(property_summary.as_ref(), &summary.properties);
    assert_eq!(table_shape_summary.as_ref(), &summary.table_shapes);
    assert_eq!(summary.members.members.len(), 0);
    assert_eq!(summary.properties.properties.len(), 0);
    assert_eq!(summary.table_shapes.tables.len(), 1);
    assert_eq!(summary.doc_types.types.len(), 0);
    assert_eq!(summary.signatures.signatures.len(), 0);
    assert_eq!(summary.signatures.calls.len(), 0);
    assert_eq!(summary.doc_owner_bindings.bindings.len(), 1);
    assert_eq!(summary.use_sites.names.len(), 2);
    assert_eq!(summary.use_sites.members.len(), 0);
    assert_eq!(summary.use_sites.calls.len(), 0);
    assert!(decl_tree
        .decls
        .iter()
        .any(|decl| decl.name == "x" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })));
    assert_eq!(summary.flow.blocks.len(), 2);
    assert_eq!(summary.flow.branches.len(), 1);
    assert_eq!(summary.flow.returns.len(), 1);
    assert!(
        decl_tree
            .scopes
            .iter()
            .any(|scope| matches!(scope.kind, SalsaScopeKindSummary::LocalOrAssignStat))
    );
    assert!(globals.variables.is_empty());
    assert!(globals.functions.is_empty());
}
