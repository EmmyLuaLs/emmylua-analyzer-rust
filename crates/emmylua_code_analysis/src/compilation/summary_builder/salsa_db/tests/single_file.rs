use super::*;
use crate::*;

#[test]
fn test_summary_builder_single_file_confidence_structures() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Box
local Box = {
  ---@type integer
  value = 1,
}

---@param x integer
function Box:run(x)
  local result = self.value + x
  if result > 0 then
    return result
  end

  return require("pkg.util")
end

return Box"#;
    set_test_file(
        &mut compilation,
        11,
        "C:/ws/single_file_confidence.lua",
        source,
    );

    let file = compilation.file();
    let doc = compilation.doc();
    let lexical = compilation.lexical();
    let module = compilation.module();
    let flow = compilation.flow();
    let semantic = compilation.semantic();
    let semantic_file = semantic.file();

    let summary = file.summary(FileId::new(11)).expect("file summary");
    let decl_tree = file.decl_tree(FileId::new(11)).expect("decl tree");
    let properties = file.properties(FileId::new(11)).expect("properties");
    let signatures = doc.signatures(FileId::new(11)).expect("signatures");
    let owner_bindings = doc.owner_bindings(FileId::new(11)).expect("owner bindings");
    let use_sites = lexical.use_sites(FileId::new(11)).expect("use sites");
    let module_summary = module.summary(FileId::new(11)).expect("module summary");
    let flow_summary = flow.summary(FileId::new(11)).expect("flow summary");
    let semantic_summary = semantic_file
        .summary(FileId::new(11))
        .expect("semantic summary");

    let box_decl_id = decl_tree
        .decls
        .iter()
        .find(|decl| decl.name == "Box" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
        .map(|decl| decl.id)
        .expect("Box decl id");
    let run_signature = signatures
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("Box.run"))
        .cloned()
        .expect("Box.run signature");

    assert_eq!(&summary.properties, properties.as_ref());
    assert_eq!(&summary.signatures, signatures.as_ref());
    assert_eq!(&summary.doc_owner_bindings, owner_bindings.as_ref());
    assert_eq!(&summary.use_sites, use_sites.as_ref());
    assert_eq!(summary.module.as_ref(), Some(module_summary.as_ref()));
    assert_eq!(&summary.flow, flow_summary.as_ref());

    assert!(properties.properties.iter().any(|property| matches!(
        property,
        SalsaPropertySummary {
            owner: SalsaPropertyOwnerSummary::Decl { name, decl_id, .. },
            key: SalsaPropertyKeySummary::Name(key),
            source: SalsaPropertySourceSummary::TableField,
            ..
        } if name == "Box" && *decl_id == box_decl_id && key == "value"
    )));
    assert!(owner_bindings.bindings.iter().any(|binding| matches!(
        binding,
        SalsaDocOwnerBindingSummary {
            owner_kind: SalsaDocOwnerKindSummary::FuncStat,
            targets,
            ..
        } if targets == &vec![SalsaBindingTargetSummary::Signature(run_signature.syntax_offset)]
    )));
    assert!(use_sites.calls.iter().any(|call| matches!(
        call,
        SalsaCallUseSummary {
            kind: SalsaCallKindSummary::Require,
            require_path: Some(path),
            ..
        } if path == "pkg.util"
    )));
    assert!(matches!(
        &module_summary.export_target,
        Some(SalsaExportTargetSummary::LocalName(name)) if name == "Box"
    ));
    assert!(matches!(
        &module_summary.export,
        Some(SalsaModuleExportSummary::LocalDecl { name, decl_id })
            if name == "Box" && *decl_id == box_decl_id
    ));
    assert_eq!(semantic_summary.required_modules, vec!["pkg.util"]);
    assert_eq!(summary.flow.branch_count, 1);
    assert_eq!(summary.flow.return_count, 3);
}

#[test]
fn test_summary_builder_single_file_update_refreshes_all_views() {
    let mut compilation = setup_compilation();

    set_test_file(
        &mut compilation,
        12,
        "C:/ws/live.lua",
        r#"---@class Box
local Box = {}

return Box"#,
    );

    let before_summary = compilation
        .file()
        .summary(FileId::new(12))
        .expect("before summary");
    let before_use_sites = compilation
        .lexical()
        .use_sites(FileId::new(12))
        .expect("before use sites");

    set_test_file(
        &mut compilation,
        12,
        "C:/ws/live.lua",
        r#"---@class Box
local Box = {
  ---@type integer
  value = 1,
}

local current = Box.value
return require("live.box")"#,
    );

    let file = compilation.file();
    let doc = compilation.doc();
    let lexical = compilation.lexical();
    let module = compilation.module();
    let semantic = compilation.semantic();
    let semantic_file = semantic.file();

    let after_summary = file.summary(FileId::new(12)).expect("after summary");
    let after_properties = file.properties(FileId::new(12)).expect("after properties");
    let after_use_sites = lexical.use_sites(FileId::new(12)).expect("after use sites");
    let after_module = module.summary(FileId::new(12)).expect("after module");
    let after_owner_bindings = doc.owner_bindings(FileId::new(12)).expect("after bindings");
    let after_semantic = semantic_file
        .summary(FileId::new(12))
        .expect("after semantic");

    assert_eq!(before_summary.doc.type_defs.len(), 1);
    assert!(before_summary.properties.properties.is_empty());
    assert_eq!(before_use_sites.calls.len(), 0);

    assert_eq!(&after_summary.properties, after_properties.as_ref());
    assert_eq!(&after_summary.use_sites, after_use_sites.as_ref());
    assert_eq!(
        &after_summary.doc_owner_bindings,
        after_owner_bindings.as_ref()
    );
    assert_eq!(after_summary.module.as_ref(), Some(after_module.as_ref()));
    assert!(after_properties.properties.iter().any(|property| matches!(
        property.key,
        SalsaPropertyKeySummary::Name(ref key) if key == "value"
    )));
    assert!(after_use_sites.calls.iter().any(|call| matches!(
        call,
        SalsaCallUseSummary {
            kind: SalsaCallKindSummary::Require,
            require_path: Some(path),
            ..
        } if path == "live.box"
    )));
    assert!(after_use_sites.members.iter().any(|member_use| matches!(
        &member_use.target,
        target if matches!(&target.root, SalsaMemberRootSummary::LocalDecl { name, .. } if name == "Box")
            && target.member_name == "value"
    )));
    assert!(after_module.export_target.is_none());
    assert!(after_module.export.is_none());
    assert_eq!(after_semantic.required_modules, vec!["live.box"]);
}

#[test]
fn test_summary_builder_single_file_semantic_query() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Box
local Box = {
  ---@type integer
  value = 1,
}

---@param x integer
function Box:run(x)
  return self.value + x
end

return Box"#;
    set_test_file(&mut compilation, 13, "C:/ws/semantic_single.lua", source);

    let file = compilation.file();
    let semantic = compilation.semantic();
    let semantic_file = semantic.file();
    let semantic_target = semantic.target();
    let doc = compilation.doc();

    let decl_tree = file.decl_tree(FileId::new(13)).expect("decl tree");
    let module_summary = compilation
        .module()
        .summary(FileId::new(13))
        .expect("module summary");
    let semantic_summary = semantic_file
        .summary(FileId::new(13))
        .expect("semantic summary");
    let tag_properties = doc
        .tag_properties(FileId::new(13))
        .expect("doc tag properties");

    let box_decl_id = decl_tree
        .decls
        .iter()
        .find(|decl| decl.name == "Box" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
        .map(|decl| decl.id)
        .expect("Box decl id");
    let run_signature_offset = compilation
        .doc()
        .signatures(FileId::new(13))
        .expect("signature summary")
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("Box.run"))
        .map(|signature| signature.syntax_offset)
        .expect("run signature");
    let box_value_member = compilation
        .doc()
        .owner_bindings(FileId::new(13))
        .expect("owner bindings")
        .bindings
        .iter()
        .find(|binding| binding.owner_kind == SalsaDocOwnerKindSummary::TableField)
        .and_then(|binding| {
            binding.targets.iter().find_map(|target| match target {
                SalsaBindingTargetSummary::Member(target) => Some(target.clone()),
                _ => None,
            })
        })
        .expect("Box.value member");

    let decl_semantic = semantic_target
        .decl(FileId::new(13), box_decl_id)
        .expect("decl semantic");
    assert!(decl_semantic.doc_owners.iter().any(|resolve| matches!(
        resolve.resolution,
        SalsaDocOwnerResolutionSummary::Decl(decl_id) if decl_id == box_decl_id
    )));
    assert!(decl_semantic.properties.iter().any(|property| matches!(
        property.key,
        SalsaPropertyKeySummary::Name(ref key) if key == "value"
    )));

    let member_semantic = semantic_target
        .member(FileId::new(13), box_value_member.clone())
        .expect("member semantic");
    assert!(member_semantic.doc_owners.iter().any(|resolve| matches!(
        &resolve.resolution,
        SalsaDocOwnerResolutionSummary::Member(target) if target == &box_value_member
    )));

    let signature_semantic = semantic_target
        .signature(FileId::new(13), run_signature_offset)
        .expect("signature semantic");
    assert!(signature_semantic.doc_owners.iter().any(|resolve| matches!(
        resolve.resolution,
        SalsaDocOwnerResolutionSummary::Signature(offset) if offset == run_signature_offset
    )));
    assert!(signature_semantic.tag_properties.is_empty());

    let export_semantic = semantic_summary
        .module_export
        .clone()
        .expect("module export semantic");
    let export_query = semantic_file
        .module_export_query(FileId::new(13))
        .expect("module export query");
    assert_eq!(
        export_semantic.export_target,
        module_summary
            .export_target
            .as_ref()
            .expect("export target")
            .clone()
    );
    assert_eq!(
        export_semantic.export,
        module_summary.export.as_ref().expect("export").clone()
    );
    assert!(matches!(
        export_semantic.semantic_target,
        Some(SalsaSemanticTargetSummary::Decl(decl_id)) if decl_id == box_decl_id
    ));
    assert_eq!(export_query.export_target, export_semantic.export_target);
    assert_eq!(export_query.export, Some(export_semantic.export.clone()));
    assert_eq!(
        export_query.semantic_target,
        export_semantic.semantic_target
    );
    assert_eq!(export_query.doc_owners, export_semantic.doc_owners);
    assert_eq!(export_query.tag_properties, export_semantic.tag_properties);
    assert_eq!(export_query.properties, decl_semantic.properties);
    assert_eq!(semantic_summary.file_tag_properties, tag_properties);
    assert_eq!(semantic_summary.module_export, Some(export_semantic));
}
