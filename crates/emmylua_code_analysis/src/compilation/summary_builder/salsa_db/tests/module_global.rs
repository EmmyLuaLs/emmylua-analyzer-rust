use crate::SalsaDeclId;

use super::*;

#[test]
fn test_summary_builder_decl_tree_and_globals() {
    let mut compilation = setup_compilation();
    let source = r#"local t = {}
function foo(a, ...)
  local y = a
end
function t:bar(x)
  return x
end
_G.baz = 1"#;

    set_test_file(&mut compilation, 2, "C:/ws/decls.lua", source);

    let file = compilation.file();
    let decl_tree = file.decl_tree(FileId::new(2)).expect("decl tree summary");
    let globals = file.globals(FileId::new(2)).expect("global summary");
    let members = file.members(FileId::new(2)).expect("member summary");

    assert!(decl_tree.decls.iter().any(|decl| decl.name == "t"));
    assert!(
        decl_tree
            .decls
            .iter()
            .any(|decl| decl.name == "foo" && matches!(decl.kind, SalsaDeclKindSummary::Global))
    );
    assert!(decl_tree.decls.iter().any(|decl| decl.name == "a" && matches!(decl.kind, SalsaDeclKindSummary::Param { .. })));
    assert!(
        decl_tree
            .decls
            .iter()
            .any(|decl| decl.name == "..."
                && matches!(decl.kind, SalsaDeclKindSummary::Param { .. }))
    );
    assert!(
        decl_tree
            .decls
            .iter()
            .any(|decl| decl.name == "self"
                && matches!(decl.kind, SalsaDeclKindSummary::ImplicitSelf))
    );
    assert!(decl_tree.decls.iter().any(|decl| decl.name == "x" && matches!(decl.kind, SalsaDeclKindSummary::Param { .. })));
    assert!(
        globals
            .entries
            .iter()
            .any(|entry| entry.name == "foo" && !entry.decl_ids.is_empty())
    );
    assert!(
        globals
            .entries
            .iter()
            .any(|entry| entry.name == "baz" && !entry.decl_ids.is_empty())
    );
    assert!(!globals.entries.iter().any(|entry| entry.name == "bar"));
    assert!(
        globals
            .functions
            .iter()
            .any(|func| func.name == "foo" && func.decl_id.is_some())
    );
    assert!(
        globals
            .variables
            .iter()
            .any(|var| var.name == "baz" && var.decl_id != SalsaDeclId::new(TextSize::from(0)))
    );
    assert!(
        !globals
            .members
            .iter()
            .any(|member| build_member_full_name(&member.target) == "t.bar")
    );
    assert!(members.members.iter().any(|member| {
        build_member_full_name(&member.target) == "t.bar"
            && member.is_method
            && matches!(
                member.target.root,
                crate::SalsaMemberRootSummary::LocalDecl { .. }
            )
    }));
}

#[test]
fn test_summary_builder_global_member_structures() {
    let mut compilation = setup_compilation();
    let source = r#"foo = function() end
function GG.fun() end
function GG.f.fun() end
GG.value = 1
_ENV.top = function() end"#;
    set_test_file(&mut compilation, 3, "C:/ws/global_members.lua", source);

    let file = compilation.file();
    let globals = file.globals(FileId::new(3)).expect("global summary");

    assert!(
        globals
            .functions
            .iter()
            .any(|func| func.name == "foo" && func.decl_id.is_some())
    );
    assert!(globals.members.iter().any(|member| {
        build_member_full_name(&member.target) == "GG.fun" && member.signature_offset.is_some()
    }));
    assert!(globals.members.iter().any(|member| {
        build_member_full_name(&member.target) == "GG.f.fun"
            && member.target.owner_segments.len() == 1
            && member.target.owner_segments[0] == "f"
    }));
    assert!(
        globals.members.iter().any(
            |member| build_member_full_name(&member.target) == "GG.value"
                && matches!(member.kind, crate::SalsaMemberKindSummary::Variable)
        )
    );
    assert!(
        globals.members.iter().any(
            |member| build_member_full_name(&member.target) == "_ENV.top"
                && matches!(
                    member.target.root,
                    crate::SalsaMemberRootSummary::Global(crate::SalsaGlobalRootSummary::Env)
                )
        )
    );
}

#[test]
fn test_summary_builder_module_export_structure() {
    let mut compilation = setup_compilation();
    set_test_file(
        &mut compilation,
        4,
        "C:/ws/module_export.lua",
        r#"local M = {}
function M.run() end
return M.run"#,
    );
    set_test_file(
        &mut compilation,
        5,
        "C:/ws/module_local.lua",
        r#"local x = {}
return x"#,
    );
    set_test_file(
        &mut compilation,
        6,
        "C:/ws/module_global_member.lua",
        r#"function GG.run() end
return GG.run"#,
    );
    set_test_file(
        &mut compilation,
        16,
        "C:/ws/module_closure.lua",
        r#"return function(value) return value end"#,
    );
    set_test_file(
        &mut compilation,
        17,
        "C:/ws/module_table.lua",
        r#"return { value = 1 }"#,
    );
    set_test_file(
        &mut compilation,
        18,
        "C:/ws/module_global_function.lua",
        r#"function foo() end
return foo"#,
    );
    set_test_file(
        &mut compilation,
        19,
        "C:/ws/module_global_variable.lua",
        r#"bar = 1
return bar"#,
    );

    let module_member = compilation
        .module()
        .summary(FileId::new(4))
        .expect("module member summary");
    let module_local = compilation
        .module()
        .summary(FileId::new(5))
        .expect("module local summary");
    let module_closure = compilation
        .module()
        .summary(FileId::new(16))
        .expect("module closure summary");
    let module_table = compilation
        .module()
        .summary(FileId::new(17))
        .expect("module table summary");
    let module_queries = compilation.module();
    let module_global_function = compilation
        .module()
        .summary(FileId::new(18))
        .expect("module global function summary");
    let module_global_variable = compilation
        .module()
        .summary(FileId::new(19))
        .expect("module global variable summary");

    assert!(matches!(
        module_member.export_target,
        Some(crate::SalsaExportTargetSummary::Member(_))
    ));
    assert!(matches!(
        &module_member.export,
        Some(crate::SalsaModuleExportSummary::Member(crate::SalsaMemberSummary {
            target,
            ..
        })) if matches!(&target.root, crate::SalsaMemberRootSummary::LocalDecl { name, .. } if name == "M")
            && target.member_name == "run"
    ));
    assert!(matches!(
        &module_local.export,
        Some(crate::SalsaModuleExportSummary::LocalDecl { name, .. }) if name == "x"
    ));
    assert_eq!(
        module_queries.export_target(FileId::new(4)),
        module_member.export_target.clone()
    );
    assert_eq!(
        module_queries.export(FileId::new(4)),
        module_member.export.clone()
    );
    assert_eq!(
        module_queries.export(FileId::new(5)),
        module_local.export.clone()
    );
    let local_decl_id = match &module_local.export {
        Some(crate::SalsaModuleExportSummary::LocalDecl { decl_id, .. }) => *decl_id,
        _ => unreachable!("module_local should export a local decl"),
    };
    assert_eq!(
        module_queries.exports_decl(FileId::new(5), local_decl_id),
        Some(true)
    );
    assert_eq!(
        module_queries.exports_decl(
            FileId::new(5),
            crate::SalsaDeclId::new(local_decl_id.as_position() + TextSize::from(1)),
        ),
        Some(false)
    );

    let exported_member_target = match &module_member.export {
        Some(crate::SalsaModuleExportSummary::Member(member)) => member.target.clone(),
        _ => unreachable!("module_member should export a member"),
    };
    assert_eq!(
        module_queries.exports_member(FileId::new(4), exported_member_target.clone()),
        Some(true)
    );
    assert_eq!(
        module_queries.exports_member(
            FileId::new(4),
            crate::SalsaMemberTargetSummary {
                root: crate::SalsaMemberRootSummary::LocalDecl {
                    decl_id: 0.into(),
                    name: "M".into(),
                },
                owner_segments: vec![].into(),
                member_name: "missing".into(),
            }
        ),
        Some(false)
    );

    let closure_signature_offset = match &module_closure.export {
        Some(crate::SalsaModuleExportSummary::Closure { signature_offset }) => *signature_offset,
        _ => unreachable!("module_closure should export a closure"),
    };
    assert_eq!(
        module_queries.exports_closure(FileId::new(16), closure_signature_offset),
        Some(true)
    );
    assert_eq!(
        module_queries.exports_closure(
            FileId::new(16),
            closure_signature_offset + TextSize::from(1)
        ),
        Some(false)
    );

    let table_offset = match &module_table.export {
        Some(crate::SalsaModuleExportSummary::Table { table_offset }) => *table_offset,
        _ => unreachable!("module_table should export a table"),
    };
    assert_eq!(
        module_queries.exports_table(FileId::new(17), table_offset),
        Some(true)
    );
    assert_eq!(
        module_queries.exports_table(FileId::new(17), table_offset + TextSize::from(1)),
        Some(false)
    );
    assert!(matches!(
        &module_global_function.export,
        Some(crate::SalsaModuleExportSummary::GlobalFunction(crate::SalsaGlobalFunctionSummary {
            name,
            ..
        })) if name == "foo"
    ));
    assert!(matches!(
        &module_global_variable.export,
        Some(crate::SalsaModuleExportSummary::GlobalVariable(crate::SalsaGlobalVariableSummary {
            name,
            ..
        })) if name == "bar"
    ));
    assert_eq!(
        module_queries.exported_global_function(FileId::new(18)),
        match &module_global_function.export {
            Some(crate::SalsaModuleExportSummary::GlobalFunction(function)) =>
                Some(function.clone()),
            _ => None,
        }
    );
    assert_eq!(
        module_queries.exported_global_variable(FileId::new(19)),
        match &module_global_variable.export {
            Some(crate::SalsaModuleExportSummary::GlobalVariable(variable)) =>
                Some(variable.clone()),
            _ => None,
        }
    );
    assert_eq!(
        module_queries.exports_global_function(FileId::new(18), "foo".into()),
        Some(true)
    );
    assert_eq!(
        module_queries.exports_global_function(FileId::new(18), "bar".into()),
        Some(false)
    );
    assert_eq!(
        module_queries.exports_global_variable(FileId::new(19), "bar".into()),
        Some(true)
    );
    assert_eq!(
        module_queries.exports_global_variable(FileId::new(19), "foo".into()),
        Some(false)
    );
    let module_global_member = compilation
        .module()
        .summary(FileId::new(6))
        .expect("module global member summary");
    assert!(matches!(
        &module_global_member.export,
        Some(crate::SalsaModuleExportSummary::Member(
            crate::SalsaMemberSummary {
                target,
                ..
            }
        )) if matches!(&target.root, crate::SalsaMemberRootSummary::Global(
                crate::SalsaGlobalRootSummary::Name(_)
            ))
    ));

    let member_summary = compilation
        .file()
        .members(FileId::new(4))
        .expect("member summary");
    assert!(member_summary.members.iter().any(|member| matches!(
        &member.target.root,
        crate::SalsaMemberRootSummary::LocalDecl { name, .. } if name == "M"
    ) && member.target.member_name == "run"));
}

#[test]
fn test_summary_builder_module_export_query_tracks_state_by_export_kind() {
    let mut compilation = setup_compilation();
    set_test_file(
        &mut compilation,
        20,
        "C:/ws/module_export_query_decl.lua",
        "---@type string\nlocal value\n\nreturn value\n",
    );
    set_test_file(
        &mut compilation,
        21,
        "C:/ws/module_export_query_closure.lua",
        "return function()\n  return 1\nend\n",
    );
    set_test_file(
        &mut compilation,
        22,
        "C:/ws/module_export_query_table.lua",
        "return {}\n",
    );

    let decl_query = compilation
        .semantic()
        .file()
        .module_export_query(FileId::new(20))
        .expect("decl module export query");
    let closure_query = compilation
        .semantic()
        .file()
        .module_export_query(FileId::new(21))
        .expect("closure module export query");
    let table_query = compilation
        .semantic()
        .file()
        .module_export_query(FileId::new(22))
        .expect("table module export query");

    assert_eq!(
        decl_query.state,
        crate::SalsaModuleExportResolveStateSummary::Resolved
    );
    assert!(matches!(
        decl_query.semantic_target,
        Some(crate::SalsaSemanticTargetSummary::Decl(_))
    ));

    assert_eq!(
        closure_query.state,
        crate::SalsaModuleExportResolveStateSummary::Resolved
    );
    assert!(matches!(
        closure_query.semantic_target,
        Some(crate::SalsaSemanticTargetSummary::Signature(_))
    ));

    assert_eq!(
        table_query.state,
        crate::SalsaModuleExportResolveStateSummary::Partial
    );
    assert!(table_query.semantic_target.is_none());
    assert!(matches!(
        table_query.export,
        Some(crate::SalsaModuleExportSummary::Table { .. })
    ));
}

#[test]
fn test_summary_builder_module_export_query_state_uses_solver_fixedpoint() {
    let mut compilation = setup_compilation();
    let source = r#"---@return string
local function make()
  return "x"
end

local exported = make()

return exported
"#;
    set_test_file(
        &mut compilation,
        23,
        "C:/ws/module_export_query_solver_state.lua",
        source,
    );

    let summary = compilation
        .semantic()
        .file()
        .module_export_query(FileId::new(23))
        .expect("module export query");

    assert_eq!(
        summary.state,
        crate::SalsaModuleExportResolveStateSummary::Resolved
    );
    assert!(matches!(
        summary.semantic_target,
        Some(crate::SalsaSemanticTargetSummary::Decl(_))
    ));
}
