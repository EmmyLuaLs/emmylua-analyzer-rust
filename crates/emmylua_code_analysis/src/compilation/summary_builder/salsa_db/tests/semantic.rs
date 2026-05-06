use super::*;
use crate::*;

#[test]
fn test_summary_builder_owner_binding_and_use_site_structures() {
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

local fn = function(y)
  return y
end

Box.value = fn(Box.value)
local result = Box:run(Box.value)
print(result)"#;
    set_test_file(&mut compilation, 10, "C:/ws/binding_use_site.lua", source);

    let file = compilation.file();
    let doc = compilation.doc();
    let lexical = compilation.lexical();

    let summary = file.summary(FileId::new(10)).expect("file summary");
    let decl_tree = file.decl_tree(FileId::new(10)).expect("decl tree summary");
    let signatures = doc.signatures(FileId::new(10)).expect("signature summary");
    let owner_bindings = doc
        .owner_bindings(FileId::new(10))
        .expect("doc owner binding summary");
    let owner_resolve_index = doc
        .owner_resolve_index(FileId::new(10))
        .expect("doc owner resolve index");
    let use_sites = lexical
        .use_sites(FileId::new(10))
        .expect("use site summary");

    assert_eq!(owner_bindings.as_ref(), &summary.doc_owner_bindings);
    assert_eq!(use_sites.as_ref(), &summary.use_sites);

    let box_decl_id = decl_tree
        .decls
        .iter()
        .find(|decl| decl.name == "Box" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
        .map(|decl| decl.id)
        .expect("Box decl id");
    let fn_decl_id = decl_tree
        .decls
        .iter()
        .find(|decl| decl.name == "fn" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
        .map(|decl| decl.id)
        .expect("fn decl id");
    let result_decl_id = decl_tree
        .decls
        .iter()
        .find(|decl| {
            decl.name == "result" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .map(|decl| decl.id)
        .expect("result decl id");
    let run_signature_offset = signatures
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("Box.run"))
        .map(|signature| signature.syntax_offset)
        .expect("Box.run signature");

    assert!(owner_bindings.bindings.iter().any(|binding| matches!(
        binding,
        SalsaDocOwnerBindingSummary {
            owner_kind: SalsaDocOwnerKindSummary::LocalStat,
            targets,
            ..
        } if targets == &vec![SalsaBindingTargetSummary::Decl(box_decl_id)]
    )));
    assert!(owner_bindings.bindings.iter().any(|binding| matches!(
        binding,
        SalsaDocOwnerBindingSummary {
            owner_kind: SalsaDocOwnerKindSummary::TableField,
            targets,
            ..
        } if targets.iter().any(|target| matches!(
            target,
            SalsaBindingTargetSummary::Member(member_target)
                if matches!(&member_target.root, SalsaMemberRootSummary::LocalDecl { name, decl_id, .. } if name == "Box" && *decl_id == box_decl_id)
                    && member_target.member_name == "value"
        ))
    )));
    assert!(owner_bindings.bindings.iter().any(|binding| matches!(
        binding,
        SalsaDocOwnerBindingSummary {
            owner_kind: SalsaDocOwnerKindSummary::FuncStat,
            targets,
            ..
        } if targets == &vec![SalsaBindingTargetSummary::Signature(run_signature_offset)]
    )));
    assert!(owner_resolve_index.bindings.iter().any(|binding| matches!(
        binding,
        SalsaDocOwnerResolveSummary {
            owner_kind: SalsaDocOwnerKindSummary::LocalStat,
            resolution: SalsaDocOwnerResolutionSummary::Decl(decl_id),
            ..
        } if *decl_id == box_decl_id
    )));
    assert!(owner_resolve_index.bindings.iter().any(|binding| matches!(
        binding,
        SalsaDocOwnerResolveSummary {
            owner_kind: SalsaDocOwnerKindSummary::FuncStat,
            resolution: SalsaDocOwnerResolutionSummary::Signature(signature_offset),
            ..
        } if *signature_offset == run_signature_offset
    )));

    let box_value_member_target = owner_bindings
        .bindings
        .iter()
        .find(|binding| binding.owner_kind == SalsaDocOwnerKindSummary::TableField)
        .and_then(|binding| {
            binding.targets.iter().find_map(|target| match target {
                SalsaBindingTargetSummary::Member(member_target) => Some(member_target.clone()),
                _ => None,
            })
        })
        .expect("Box.value member target");

    assert_eq!(
        doc.owner_resolves_for_decl(FileId::new(10), box_decl_id),
        Some(vec![SalsaDocOwnerResolveSummary {
            owner_kind: SalsaDocOwnerKindSummary::LocalStat,
            owner_offset: owner_bindings
                .bindings
                .iter()
                .find(|binding| binding.owner_kind == SalsaDocOwnerKindSummary::LocalStat)
                .map(|binding| binding.owner_offset)
                .expect("Box local owner offset"),
            resolution: SalsaDocOwnerResolutionSummary::Decl(box_decl_id),
        }])
    );
    assert_eq!(
        doc.owner_resolves_for_member(FileId::new(10), box_value_member_target.clone()),
        Some(vec![SalsaDocOwnerResolveSummary {
            owner_kind: SalsaDocOwnerKindSummary::TableField,
            owner_offset: owner_bindings
                .bindings
                .iter()
                .find(|binding| binding.owner_kind == SalsaDocOwnerKindSummary::TableField)
                .map(|binding| binding.owner_offset)
                .expect("Box.value owner offset"),
            resolution: SalsaDocOwnerResolutionSummary::Member(box_value_member_target),
        }])
    );
    assert_eq!(
        doc.owner_resolves_for_signature(FileId::new(10), run_signature_offset),
        Some(vec![SalsaDocOwnerResolveSummary {
            owner_kind: SalsaDocOwnerKindSummary::FuncStat,
            owner_offset: owner_bindings
                .bindings
                .iter()
                .find(|binding| binding.owner_kind == SalsaDocOwnerKindSummary::FuncStat)
                .map(|binding| binding.owner_offset)
                .expect("run owner offset"),
            resolution: SalsaDocOwnerResolutionSummary::Signature(run_signature_offset),
        }])
    );

    let local_owner_offset = owner_bindings
        .bindings
        .iter()
        .find(|binding| binding.owner_kind == SalsaDocOwnerKindSummary::LocalStat)
        .map(|binding| binding.owner_offset)
        .expect("local owner offset");
    assert!(matches!(
        doc.owner_resolve(FileId::new(10), local_owner_offset),
        Some(SalsaDocOwnerResolveSummary {
            resolution: SalsaDocOwnerResolutionSummary::Decl(decl_id),
            ..
        }) if decl_id == box_decl_id
    ));

    assert!(use_sites.names.iter().any(|name_use| matches!(
        name_use,
        SalsaNameUseSummary {
            name,
            role: SalsaUseSiteRoleSummary::CallCallee,
            resolution: SalsaNameUseResolutionSummary::LocalDecl(decl_id),
            ..
        } if name == "fn" && *decl_id == fn_decl_id
    )));
    assert!(use_sites.names.iter().any(|name_use| matches!(
        name_use,
        SalsaNameUseSummary {
            name,
            role: SalsaUseSiteRoleSummary::CallCallee,
            resolution: SalsaNameUseResolutionSummary::Global,
            ..
        } if name == "print"
    )));
    assert!(use_sites.names.iter().any(|name_use| matches!(
        name_use,
        SalsaNameUseSummary {
            name,
            role: SalsaUseSiteRoleSummary::Read,
            resolution: SalsaNameUseResolutionSummary::LocalDecl(decl_id),
            ..
        } if name == "result" && *decl_id == result_decl_id
    )));

    assert!(use_sites.members.iter().any(|member_use| matches!(
        member_use,
        SalsaMemberUseSummary {
            role: SalsaUseSiteRoleSummary::Write,
            target,
            ..
        } if matches!(&target.root, SalsaMemberRootSummary::LocalDecl { name, decl_id, .. } if name == "Box" && *decl_id == box_decl_id)
            && target.member_name == "value"
    )));
    assert!(use_sites.members.iter().any(|member_use| matches!(
        member_use,
        SalsaMemberUseSummary {
            role: SalsaUseSiteRoleSummary::CallCallee,
            target,
            ..
        } if matches!(&target.root, SalsaMemberRootSummary::LocalDecl { name, decl_id, .. } if name == "Box" && *decl_id == box_decl_id)
            && target.member_name == "run"
    )));

    assert!(use_sites.calls.iter().any(|call| matches!(
        call,
        SalsaCallUseSummary {
            callee_name: Some(name),
            arg_count,
            require_path: None,
            ..
        } if name == "fn" && *arg_count == 1
    )));
    assert!(use_sites.calls.iter().any(|call| matches!(
        call,
        SalsaCallUseSummary {
            is_colon_call: true,
            callee_member: Some(target),
            arg_count,
            ..
        } if matches!(&target.root, SalsaMemberRootSummary::LocalDecl { name, decl_id, .. } if name == "Box" && *decl_id == box_decl_id)
            && target.member_name == "run" && *arg_count == 1
    )));

    let fn_call = use_sites
        .calls
        .iter()
        .find(|call| matches!(call, SalsaCallUseSummary { callee_name: Some(name), arg_count, .. } if name == "fn" && *arg_count == 1))
        .cloned()
        .expect("fn call use");
    let box_run_call = use_sites
        .calls
        .iter()
        .find(|call| {
            matches!(
                call,
                SalsaCallUseSummary {
                    is_colon_call: true,
                    callee_member: Some(target),
                    arg_count,
                    require_path: None,
                    ..
                } if matches!(&target.root, SalsaMemberRootSummary::LocalDecl { name, decl_id, .. } if name == "Box" && *decl_id == box_decl_id)
                    && target.member_name == "run" && *arg_count == 1
            )
        })
        .cloned()
        .expect("Box.run call use");

    let fn_call_use = use_sites
        .names
        .iter()
        .find(|name_use| {
            matches!(
                name_use,
                SalsaNameUseSummary {
                    name,
                    role: SalsaUseSiteRoleSummary::CallCallee,
                    resolution: SalsaNameUseResolutionSummary::LocalDecl(decl_id),
                    ..
                } if name == "fn" && *decl_id == fn_decl_id
            )
        })
        .cloned()
        .expect("fn lexical name use");
    let fn_call_use_offset = fn_call_use.syntax_offset;
    assert_eq!(
        lexical.name_resolution(FileId::new(10), fn_call_use_offset),
        Some(fn_call_use)
    );
    assert!(matches!(
        lexical.use_at(FileId::new(10), fn_call_use_offset),
        Some(SalsaLexicalUseSummary::Name {
            name,
            role: SalsaUseSiteRoleSummary::CallCallee,
            resolution: SalsaNameUseResolutionSummary::LocalDecl(decl_id),
            ..
        }) if name == "fn" && decl_id == fn_decl_id
    ));

    let box_value_write = use_sites
        .members
        .iter()
        .find(|member_use| {
            matches!(
                member_use,
                SalsaMemberUseSummary {
                    role: SalsaUseSiteRoleSummary::Write,
                    target,
                    ..
                } if matches!(&target.root, SalsaMemberRootSummary::LocalDecl { name, decl_id, .. } if name == "Box" && *decl_id == box_decl_id)
                    && target.member_name == "value"
            )
        })
        .cloned()
        .expect("Box.value lexical member use");
    let box_value_write_offset = box_value_write.syntax_offset;
    assert_eq!(
        lexical.member_resolution(FileId::new(10), box_value_write_offset),
        Some(box_value_write.clone())
    );
    assert!(matches!(
        lexical.use_at(FileId::new(10), box_value_write_offset),
        Some(SalsaLexicalUseSummary::Member {
            role: SalsaUseSiteRoleSummary::Write,
            target,
            ..
        }) if matches!(&target.root, SalsaMemberRootSummary::LocalDecl { name, decl_id, .. } if name == "Box" && *decl_id == box_decl_id)
            && target.member_name == "value"
    ));

    let lexical_index = lexical
        .use_index(FileId::new(10))
        .expect("lexical use index");
    assert_eq!(
        lexical_index.uses.len(),
        use_sites.names.len() + use_sites.members.len() + use_sites.calls.len()
    );
    assert!(lexical_index.uses.iter().any(|use_summary| matches!(
        use_summary,
        SalsaLexicalUseSummary::Call {
            callee_name: Some(name),
            arg_count,
            require_path: None,
            ..
        } if name == "fn" && *arg_count == 1
    )));

    assert_eq!(
        lexical.call_at(FileId::new(10), fn_call.syntax_offset),
        Some(fn_call.clone())
    );
    assert_eq!(
        lexical.call_references_for_name(FileId::new(10), "fn".into()),
        Some(vec![fn_call])
    );
    assert_eq!(
        lexical.global_name_references(FileId::new(10), "print".into()),
        Some(
            use_sites
                .names
                .iter()
                .filter(|name_use| {
                    name_use.name == "print"
                        && matches!(name_use.resolution, SalsaNameUseResolutionSummary::Global)
                })
                .cloned()
                .collect()
        )
    );
    assert_eq!(
        lexical.name_references_by_role(FileId::new(10), SalsaUseSiteRoleSummary::CallCallee),
        Some(
            use_sites
                .names
                .iter()
                .filter(|name_use| name_use.role == SalsaUseSiteRoleSummary::CallCallee)
                .cloned()
                .collect()
        )
    );
    assert_eq!(
        lexical
            .member_references(FileId::new(10), box_value_write.target.clone())
            .map(|mut refs| {
                refs.sort_by_key(|member_use| member_use.syntax_offset);
                refs
            }),
        Some({
            let mut refs = use_sites
                .members
                .iter()
                .filter(|member_use| member_use.target == box_value_write.target)
                .cloned()
                .collect::<Vec<_>>();
            refs.sort_by_key(|member_use| member_use.syntax_offset);
            refs
        })
    );
    assert_eq!(
        lexical.member_references_by_role(FileId::new(10), SalsaUseSiteRoleSummary::Write),
        Some(
            use_sites
                .members
                .iter()
                .filter(|member_use| member_use.role == SalsaUseSiteRoleSummary::Write)
                .cloned()
                .collect()
        )
    );
    assert_eq!(
        lexical.call_references_for_member(
            FileId::new(10),
            box_run_call
                .callee_member
                .clone()
                .expect("Box.run call member target")
        ),
        Some(vec![box_run_call])
    );

    let result_refs = lexical
        .decl_references(FileId::new(10), result_decl_id)
        .expect("result lexical references");
    assert_eq!(result_refs.len(), 1);
    assert!(matches!(
        &result_refs[0],
        SalsaNameUseSummary {
            name,
            role: SalsaUseSiteRoleSummary::Read,
            resolution: SalsaNameUseResolutionSummary::LocalDecl(decl_id),
            ..
        } if name == "result" && *decl_id == result_decl_id
    ));
}

#[test]
fn test_summary_builder_signature_and_call_explain_query() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Box
local Box = {}

---@async
---@generic T: integer
---@param value T?
---@return T result
function Box:run(value)
  return value
end

local out = Box:run(1)"#;
    set_test_file(&mut compilation, 20, "C:/ws/signature_explain.lua", source);

    let doc = compilation.doc();
    let signature_summary = doc.signatures(FileId::new(20)).expect("signature summary");
    let signature_offset = signature_summary
        .signatures
        .iter()
        .find(|signature| signature.name.as_deref() == Some("Box.run"))
        .map(|signature| signature.syntax_offset)
        .expect("Box.run signature offset");
    let call_offset = signature_summary
        .calls
        .iter()
        .map(|call| call.syntax_offset)
        .max()
        .expect("call offset");

    let semantic = compilation.semantic();
    let semantic_file = semantic.file();
    let semantic_target = semantic.target();
    let semantic_signature_target = semantic_target
        .signature(FileId::new(20), signature_offset)
        .expect("semantic signature target");
    let semantic_signature_summary = semantic_file
        .signature_summary(FileId::new(20), signature_offset)
        .expect("semantic signature summary");

    let call_explain = doc
        .call_explain(FileId::new(20), call_offset)
        .expect("call explain");

    assert_eq!(
        semantic_signature_summary.explain.signature.syntax_offset,
        signature_offset
    );
    assert_eq!(semantic_signature_summary.explain.generics.len(), 1);
    assert_eq!(
        semantic_signature_summary.explain.generics[0].params.len(),
        1
    );
    assert!(matches!(
        semantic_signature_summary.explain.generics[0].params[0].bound_type,
        Some(SalsaSignatureTypeExplainSummary {
            lowered: Some(SalsaDocTypeLoweredNode {
                kind: SalsaDocTypeLoweredKind::Name { ref name },
                ..
            }),
            ..
        }) if name == "integer"
    ));
    assert_eq!(semantic_signature_summary.explain.params.len(), 1);
    assert!(
        semantic_signature_summary.explain.params[0]
            .doc_param_offset
            .is_some()
    );
    assert!(semantic_signature_summary.explain.params[0].is_nullable);
    assert!(matches!(
        semantic_signature_summary.explain.params[0].doc_type,
        Some(SalsaSignatureTypeExplainSummary {
            lowered: Some(SalsaDocTypeLoweredNode {
                kind: SalsaDocTypeLoweredKind::Nullable { .. },
                ..
            }),
            ..
        })
    ));
    assert_eq!(semantic_signature_summary.explain.returns.len(), 1);
    assert_eq!(semantic_signature_summary.explain.returns[0].items.len(), 1);
    assert!(matches!(
        semantic_signature_summary.explain.returns[0].items[0].doc_type.lowered,
        Some(SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Name { ref name },
            ..
        }) if name == "T"
    ));
    assert_eq!(call_explain.call.syntax_offset, call_offset);
    assert!(matches!(
        call_explain.lexical_call,
        Some(SalsaCallUseSummary {
            callee_member: Some(ref target),
            ..
        }) if matches!(&target.root, SalsaMemberRootSummary::LocalDecl { name, .. } if name == "Box")
            && target.member_name == "run"
    ));
    assert_eq!(
        call_explain.candidate_signature_offsets,
        vec![signature_offset]
    );
    assert_eq!(
        call_explain.resolved_signature_offset,
        Some(signature_offset)
    );

    let explain_index = doc
        .signature_explain_index(FileId::new(20))
        .expect("signature explain index");
    assert!(
        explain_index
            .signatures
            .iter()
            .any(|signature| signature.signature.syntax_offset == signature_offset)
    );
    assert!(
        explain_index
            .calls
            .iter()
            .any(|call| call.call.syntax_offset == call_offset)
    );
    let semantic_call_explain = semantic_file
        .call_explain(FileId::new(20), call_offset)
        .expect("semantic call explain");

    assert_eq!(
        semantic_signature_summary.signature.syntax_offset,
        signature_offset
    );
    assert_eq!(
        semantic_signature_summary.doc_owners,
        semantic_signature_target.doc_owners
    );
    assert_eq!(
        semantic_signature_summary.tag_properties,
        semantic_signature_target.tag_properties
    );
    assert_eq!(
        semantic_signature_summary.properties,
        semantic_signature_target.properties
    );
    assert!(
        semantic_signature_summary
            .tag_properties
            .iter()
            .any(|property| property.is_async())
    );
    let semantic_return_summary = semantic_signature_summary
        .return_summary
        .expect("semantic signature return summary");
    assert_eq!(semantic_return_summary.signature_offset, signature_offset);
    assert_eq!(
        semantic_return_summary.doc_returns,
        semantic_signature_summary.explain.returns
    );
    assert_eq!(semantic_return_summary.values.len(), 1);
    assert_eq!(semantic_call_explain, call_explain);
    assert!(semantic_call_explain.overload_returns.is_empty());
    assert_eq!(semantic_call_explain.args.len(), 1);
    assert!(matches!(
        semantic_call_explain.args[0].expected_doc_type,
        Some(SalsaSignatureTypeExplainSummary {
            lowered: Some(SalsaDocTypeLoweredNode {
                kind: SalsaDocTypeLoweredKind::Nullable { .. },
                ..
            }),
            ..
        })
    ));
    assert!(matches!(
        semantic_call_explain.args[0].expected_param,
        Some(SalsaSignatureParamExplainSummary { ref name, .. }) if name == "value"
    ));
    assert_eq!(
        semantic_call_explain.returns,
        semantic_signature_summary.explain.returns
    );
}

#[test]
fn test_summary_builder_call_explain_prefers_unique_exact_arity_candidate() {
    let mut compilation = setup_compilation();
    let source = r#"---@return integer
function make(a, b)
  return 1
end

---@return string
local function make(a)
  return "x"
end

local value = make(1)"#;
    set_test_file(
        &mut compilation,
        21,
        "C:/ws/call_explain_best_candidate.lua",
        source,
    );

    let doc = compilation.doc();
    let signature_summary = doc.signatures(FileId::new(21)).expect("signature summary");
    let call_offset = signature_summary
        .calls
        .iter()
        .map(|call| call.syntax_offset)
        .max()
        .expect("call offset");

    let call_explain = doc
        .call_explain(FileId::new(21), call_offset)
        .expect("call explain");

    assert_eq!(call_explain.candidate_signature_offsets.len(), 1);
    assert_eq!(
        call_explain.resolved_signature_offset,
        call_explain.candidate_signature_offsets.first().copied()
    );
    assert_eq!(call_explain.returns.len(), 1);
    assert_eq!(call_explain.returns[0].items.len(), 1);
    assert!(matches!(
        call_explain.returns[0].items[0].doc_type.lowered,
        Some(SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Name { ref name },
            ..
        }) if name == "string"
    ));
}

#[test]
fn test_summary_builder_call_explain_prefers_nullable_tail_over_variadic_fallback() {
    let mut compilation = setup_compilation();
    let source = r#"---@return integer
local function pick(...)
  return 1
end

---@param value integer?
---@return string
local function pick(value)
  return \"x\"
end

local value = pick()"#;
    set_test_file(
        &mut compilation,
        22,
        "C:/ws/call_explain_nullable_tail_candidate.lua",
        source,
    );

    let doc = compilation.doc();
    let signature_summary = doc.signatures(FileId::new(22)).expect("signature summary");
    let call_offset = signature_summary
        .calls
        .iter()
        .map(|call| call.syntax_offset)
        .max()
        .expect("call offset");

    let call_explain = doc
        .call_explain(FileId::new(22), call_offset)
        .expect("call explain");

    assert_eq!(call_explain.candidate_signature_offsets.len(), 1);
    assert_eq!(
        call_explain.resolved_signature_offset,
        call_explain.candidate_signature_offsets.first().copied()
    );
    assert_eq!(call_explain.returns.len(), 1);
    assert_eq!(call_explain.returns[0].items.len(), 1);
    assert!(matches!(
        call_explain.returns[0].items[0].doc_type.lowered,
        Some(SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Name { ref name },
            ..
        }) if name == "string"
    ));
}

#[test]
fn test_summary_builder_call_explain_prefers_colon_shape_adjusted_candidate() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Box
local Box = {}

---@return integer
function Box.run(self, value)
  return 1
end

---@param extra integer?
---@return string
function Box:run(value, extra)
  return \"x\"
end

local value = Box:run(1)"#;
    set_test_file(
        &mut compilation,
        23,
        "C:/ws/call_explain_colon_shape_candidate.lua",
        source,
    );

    let doc = compilation.doc();
    let signature_summary = doc.signatures(FileId::new(23)).expect("signature summary");
    let call_offset = signature_summary
        .calls
        .iter()
        .map(|call| call.syntax_offset)
        .max()
        .expect("call offset");

    let call_explain = doc
        .call_explain(FileId::new(23), call_offset)
        .expect("call explain");

    assert_eq!(call_explain.candidate_signature_offsets.len(), 1);
    assert_eq!(
        call_explain.resolved_signature_offset,
        call_explain.candidate_signature_offsets.first().copied()
    );
    assert_eq!(call_explain.args.len(), 1);
    assert!(matches!(
        call_explain.args[0].expected_param,
        Some(SalsaSignatureParamExplainSummary { ref name, .. }) if name == "value"
    ));
    assert_eq!(call_explain.returns.len(), 1);
    assert_eq!(call_explain.returns[0].items.len(), 1);
    assert!(matches!(
        call_explain.returns[0].items[0].doc_type.lowered,
        Some(SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Name { ref name },
            ..
        }) if name == "integer"
    ));
}

#[test]
fn test_summary_builder_call_explain_uses_return_overload_rows_for_returns() {
    let mut compilation = setup_compilation();
    let source = r#"---@return_overload true, integer
---@return_overload false, string
local function parse()
  return true, 1
end

local ok, value = parse()"#;
    set_test_file(
        &mut compilation,
        24,
        "C:/ws/call_explain_return_overload_rows.lua",
        source,
    );

    let doc = compilation.doc();
    let signature_summary = doc.signatures(FileId::new(24)).expect("signature summary");
    let call_offset = signature_summary
        .calls
        .iter()
        .map(|call| call.syntax_offset)
        .max()
        .expect("call offset");

    let call_explain = doc
        .call_explain(FileId::new(24), call_offset)
        .expect("call explain");

    assert_eq!(call_explain.candidate_signature_offsets.len(), 1);
    assert_eq!(call_explain.overload_returns.len(), 2);
    assert_eq!(call_explain.returns, call_explain.overload_returns);
    assert_eq!(call_explain.returns[0].items.len(), 2);
    assert_eq!(call_explain.returns[1].items.len(), 2);
    assert!(matches!(
        call_explain.returns[0].items[0].doc_type.lowered,
        Some(SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Literal { ref text },
            ..
        }) if text == "true"
    ));
    assert!(matches!(
        call_explain.returns[1].items[1].doc_type.lowered,
        Some(SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Name { ref name },
            ..
        }) if name == "string"
    ));
}

#[test]
fn test_summary_builder_call_explain_prefers_doc_typed_candidate_over_untyped() {
    let mut compilation = setup_compilation();
    let source = r#"---@return integer
local function choose(value)
  return 1
end

---@param value integer
---@return string
local function choose(value)
  return "x"
end

local out = choose(1)"#;
    set_test_file(
        &mut compilation,
        25,
        "C:/ws/call_explain_prefers_doc_typed_candidate.lua",
        source,
    );

    let doc = compilation.doc();
    let signature_summary = doc.signatures(FileId::new(25)).expect("signature summary");
    let call_offset = signature_summary
        .calls
        .iter()
        .map(|call| call.syntax_offset)
        .max()
        .expect("call offset");

    let call_explain = doc
        .call_explain(FileId::new(25), call_offset)
        .expect("call explain");

    assert_eq!(call_explain.candidate_signature_offsets.len(), 1);
    assert!(matches!(
        call_explain.args[0].expected_param,
        Some(SalsaSignatureParamExplainSummary { ref name, .. }) if name == "value"
    ));
    assert!(matches!(
        call_explain.returns[0].items[0].doc_type.lowered,
        Some(SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Name { ref name },
            ..
        }) if name == "string"
    ));
}

#[test]
fn test_summary_builder_call_explain_prefers_specific_param_over_any() {
    let mut compilation = setup_compilation();
    let source = r#"---@param value any
---@return integer
local function pick(value)
  return 1
end

---@param value string
---@return string
local function pick(value)
  return "x"
end

local out = pick("x")"#;
    set_test_file(
        &mut compilation,
        26,
        "C:/ws/call_explain_prefers_specific_over_any.lua",
        source,
    );

    let doc = compilation.doc();
    let signature_summary = doc.signatures(FileId::new(26)).expect("signature summary");
    let call_offset = signature_summary
        .calls
        .iter()
        .map(|call| call.syntax_offset)
        .max()
        .expect("call offset");

    let call_explain = doc
        .call_explain(FileId::new(26), call_offset)
        .expect("call explain");

    assert_eq!(call_explain.candidate_signature_offsets.len(), 1);
    assert!(matches!(
        call_explain.args[0].expected_doc_type,
        Some(SalsaSignatureTypeExplainSummary {
            lowered: Some(SalsaDocTypeLoweredNode {
                kind: SalsaDocTypeLoweredKind::Name { ref name },
                ..
            }),
            ..
        }) if name == "string"
    ));
    assert!(matches!(
        call_explain.returns[0].items[0].doc_type.lowered,
        Some(SalsaDocTypeLoweredNode {
            kind: SalsaDocTypeLoweredKind::Name { ref name },
            ..
        }) if name == "string"
    ));
}
