use rowan::TextSize;
use smol_str::SmolStr;

use super::super::{SalsaLookupBucket, build_lookup_buckets, find_bucket_indices};

use crate::{
    SalsaDeclId, SalsaDeclKindSummary, SalsaDeclSummary, SalsaDeclTreeSummary,
    SalsaExportTargetSummary, SalsaGlobalFunctionSummary, SalsaGlobalRootSummary,
    SalsaGlobalSummary, SalsaGlobalVariableSummary, SalsaMemberIndexSummary,
    SalsaMemberPathRootSummary, SalsaMemberPathSummary, SalsaMemberRootSummary, SalsaMemberSummary,
    SalsaMemberTargetSummary, SalsaModuleExportSummary, SalsaModuleSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub struct SalsaModuleNameKey(pub SmolStr);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub struct SalsaModuleMemberPathKey(pub SalsaMemberPathSummary);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub struct SalsaModuleDeclHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub struct SalsaModuleGlobalFunctionHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub struct SalsaModuleGlobalVariableHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub struct SalsaModuleMemberHandle(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaModuleResolveIndex {
    pub decls: Vec<SalsaDeclSummary>,
    pub global_functions: Vec<SalsaGlobalFunctionSummary>,
    pub global_variables: Vec<SalsaGlobalVariableSummary>,
    pub members: Vec<SalsaMemberSummary>,
    by_decl_name: Vec<SalsaLookupBucket<SalsaModuleNameKey>>,
    by_global_function_name: Vec<SalsaLookupBucket<SalsaModuleNameKey>>,
    by_global_variable_name: Vec<SalsaLookupBucket<SalsaModuleNameKey>>,
    by_member_path: Vec<SalsaLookupBucket<SalsaModuleMemberPathKey>>,
}

pub fn build_module_resolve_index(
    decl_tree: &SalsaDeclTreeSummary,
    globals: &SalsaGlobalSummary,
    members: &SalsaMemberIndexSummary,
) -> SalsaModuleResolveIndex {
    let decls = decl_tree.decls.clone();
    let global_functions = globals.functions.clone();
    let global_variables = globals.variables.clone();
    let members = members.members.clone();

    SalsaModuleResolveIndex {
        by_decl_name: build_lookup_buckets(
            decls
                .iter()
                .enumerate()
                .map(|(index, decl)| (module_name_key(decl.name.clone()), index))
                .collect(),
        ),
        by_global_function_name: build_lookup_buckets(
            global_functions
                .iter()
                .enumerate()
                .map(|(index, function)| (module_name_key(function.name.clone()), index))
                .collect(),
        ),
        by_global_variable_name: build_lookup_buckets(
            global_variables
                .iter()
                .enumerate()
                .map(|(index, variable)| (module_name_key(variable.name.clone()), index))
                .collect(),
        ),
        by_member_path: build_lookup_buckets(
            members
                .iter()
                .enumerate()
                .map(|(index, member)| (module_member_path_key_from_target(&member.target), index))
                .collect(),
        ),
        decls,
        global_functions,
        global_variables,
        members,
    }
}

pub fn find_module_export_target(module: &SalsaModuleSummary) -> Option<SalsaExportTargetSummary> {
    module.export_target.clone()
}

pub fn find_module_export(module: &SalsaModuleSummary) -> Option<SalsaModuleExportSummary> {
    module.export.clone()
}

pub fn find_module_exported_global_function(
    module: &SalsaModuleSummary,
) -> Option<SalsaGlobalFunctionSummary> {
    match &module.export {
        Some(SalsaModuleExportSummary::GlobalFunction(function)) => Some(function.clone()),
        _ => None,
    }
}

pub fn find_module_exported_global_variable(
    module: &SalsaModuleSummary,
) -> Option<SalsaGlobalVariableSummary> {
    match &module.export {
        Some(SalsaModuleExportSummary::GlobalVariable(variable)) => Some(variable.clone()),
        _ => None,
    }
}

pub fn module_exports_decl(module: &SalsaModuleSummary, decl_id: SalsaDeclId) -> bool {
    matches!(
        &module.export,
        Some(SalsaModuleExportSummary::LocalDecl { decl_id: exported_decl_id, .. })
            if *exported_decl_id == decl_id
    )
}

pub fn module_exports_member(
    module: &SalsaModuleSummary,
    member_target: &SalsaMemberTargetSummary,
) -> bool {
    matches!(
        &module.export,
        Some(SalsaModuleExportSummary::Member(member)) if member.target == *member_target
    )
}

pub fn module_exports_closure(module: &SalsaModuleSummary, signature_offset: TextSize) -> bool {
    matches!(
        &module.export,
        Some(SalsaModuleExportSummary::Closure { signature_offset: exported_signature_offset })
            if *exported_signature_offset == signature_offset
    )
}

pub fn module_exports_table(module: &SalsaModuleSummary, table_offset: TextSize) -> bool {
    matches!(
        &module.export,
        Some(SalsaModuleExportSummary::Table { table_offset: exported_table_offset })
            if *exported_table_offset == table_offset
    )
}

pub fn module_exports_global_function(module: &SalsaModuleSummary, name: &str) -> bool {
    matches!(
        &module.export,
        Some(SalsaModuleExportSummary::GlobalFunction(function)) if function.name == name
    )
}

pub fn module_exports_global_variable(module: &SalsaModuleSummary, name: &str) -> bool {
    matches!(
        &module.export,
        Some(SalsaModuleExportSummary::GlobalVariable(variable)) if variable.name == name
    )
}

pub fn resolve_module_export(
    export_target: &SalsaExportTargetSummary,
    decl_tree: &SalsaDeclTreeSummary,
    globals: &SalsaGlobalSummary,
    members: &SalsaMemberIndexSummary,
) -> Option<SalsaModuleExportSummary> {
    let index = build_module_resolve_index(decl_tree, globals, members);
    resolve_module_export_in_index(export_target, &index)
}

pub fn resolve_module_export_in_index(
    export_target: &SalsaExportTargetSummary,
    index: &SalsaModuleResolveIndex,
) -> Option<SalsaModuleExportSummary> {
    match export_target {
        SalsaExportTargetSummary::LocalName(name) => resolve_named_export(name, index),
        SalsaExportTargetSummary::Member(target) => resolve_member_export(target, index),
        SalsaExportTargetSummary::Closure(signature_offset) => {
            Some(SalsaModuleExportSummary::Closure {
                signature_offset: *signature_offset,
            })
        }
        SalsaExportTargetSummary::Table(table_offset) => Some(SalsaModuleExportSummary::Table {
            table_offset: *table_offset,
        }),
    }
}

fn resolve_member_export(
    target: &SalsaMemberPathSummary,
    index: &SalsaModuleResolveIndex,
) -> Option<SalsaModuleExportSummary> {
    let member_handle = find_member_handle_by_path(index, target)?;
    Some(SalsaModuleExportSummary::Member(
        member_at_handle(index, member_handle)?.clone(),
    ))
}

fn resolve_named_export(
    name: &str,
    index: &SalsaModuleResolveIndex,
) -> Option<SalsaModuleExportSummary> {
    if let Some(decl_handle) = find_decl_handle_by_name(index, name) {
        let decl = decl_at_handle(index, decl_handle)?;
        match decl.kind {
            SalsaDeclKindSummary::Global => {
                if let Some(function_handle) = find_global_function_handle_by_name(index, name) {
                    let function = global_function_at_handle(index, function_handle)?;
                    return Some(SalsaModuleExportSummary::GlobalFunction(function.clone()));
                }
                if let Some(variable_handle) = find_global_variable_handle_by_name(index, name) {
                    let variable = global_variable_at_handle(index, variable_handle)?;
                    return Some(SalsaModuleExportSummary::GlobalVariable(variable.clone()));
                }
                Some(SalsaModuleExportSummary::LocalDecl {
                    name: name.into(),
                    decl_id: decl.id,
                })
            }
            _ => Some(SalsaModuleExportSummary::LocalDecl {
                name: name.into(),
                decl_id: decl.id,
            }),
        }
    } else if let Some(function_handle) = find_global_function_handle_by_name(index, name) {
        let function = global_function_at_handle(index, function_handle)?;
        Some(SalsaModuleExportSummary::GlobalFunction(function.clone()))
    } else {
        let variable_handle = find_global_variable_handle_by_name(index, name)?;
        let variable = global_variable_at_handle(index, variable_handle)?;
        Some(SalsaModuleExportSummary::GlobalVariable(variable.clone()))
    }
}

pub fn find_decl_handle_by_name(
    index: &SalsaModuleResolveIndex,
    name: &str,
) -> Option<SalsaModuleDeclHandle> {
    let name = module_name_key(SmolStr::new(name));
    find_bucket_indices(&index.by_decl_name, &name)
        .and_then(|indices| indices.last().copied())
        .map(index_to_decl_handle)
}

pub fn find_global_function_handle_by_name(
    index: &SalsaModuleResolveIndex,
    name: &str,
) -> Option<SalsaModuleGlobalFunctionHandle> {
    let name = module_name_key(SmolStr::new(name));
    find_bucket_indices(&index.by_global_function_name, &name)
        .and_then(|indices| indices.first().copied())
        .map(index_to_global_function_handle)
}

pub fn find_global_variable_handle_by_name(
    index: &SalsaModuleResolveIndex,
    name: &str,
) -> Option<SalsaModuleGlobalVariableHandle> {
    let name = module_name_key(SmolStr::new(name));
    find_bucket_indices(&index.by_global_variable_name, &name)
        .and_then(|indices| indices.first().copied())
        .map(index_to_global_variable_handle)
}

pub fn find_member_handle_by_path(
    index: &SalsaModuleResolveIndex,
    path: &SalsaMemberPathSummary,
) -> Option<SalsaModuleMemberHandle> {
    find_bucket_indices(&index.by_member_path, &module_member_path_key(path.clone()))
        .and_then(|indices| indices.first().copied())
        .map(index_to_member_handle)
}

pub fn decl_at_handle(
    index: &SalsaModuleResolveIndex,
    handle: SalsaModuleDeclHandle,
) -> Option<&SalsaDeclSummary> {
    index.decls.get(handle.0 as usize)
}

pub fn global_function_at_handle(
    index: &SalsaModuleResolveIndex,
    handle: SalsaModuleGlobalFunctionHandle,
) -> Option<&SalsaGlobalFunctionSummary> {
    index.global_functions.get(handle.0 as usize)
}

pub fn global_variable_at_handle(
    index: &SalsaModuleResolveIndex,
    handle: SalsaModuleGlobalVariableHandle,
) -> Option<&SalsaGlobalVariableSummary> {
    index.global_variables.get(handle.0 as usize)
}

pub fn member_at_handle(
    index: &SalsaModuleResolveIndex,
    handle: SalsaModuleMemberHandle,
) -> Option<&SalsaMemberSummary> {
    index.members.get(handle.0 as usize)
}

fn module_name_key(name: SmolStr) -> SalsaModuleNameKey {
    SalsaModuleNameKey(name)
}

fn module_member_path_key(path: SalsaMemberPathSummary) -> SalsaModuleMemberPathKey {
    SalsaModuleMemberPathKey(path)
}

fn module_member_path_key_from_target(
    target: &SalsaMemberTargetSummary,
) -> SalsaModuleMemberPathKey {
    module_member_path_key(member_target_path(target))
}

fn index_to_decl_handle(index: usize) -> SalsaModuleDeclHandle {
    SalsaModuleDeclHandle(index as u32)
}

fn index_to_global_function_handle(index: usize) -> SalsaModuleGlobalFunctionHandle {
    SalsaModuleGlobalFunctionHandle(index as u32)
}

fn index_to_global_variable_handle(index: usize) -> SalsaModuleGlobalVariableHandle {
    SalsaModuleGlobalVariableHandle(index as u32)
}

fn index_to_member_handle(index: usize) -> SalsaModuleMemberHandle {
    SalsaModuleMemberHandle(index as u32)
}

fn member_target_path(target: &SalsaMemberTargetSummary) -> SalsaMemberPathSummary {
    let root = match &target.root {
        SalsaMemberRootSummary::Global(SalsaGlobalRootSummary::Env) => {
            SalsaMemberPathRootSummary::Env
        }
        SalsaMemberRootSummary::Global(SalsaGlobalRootSummary::Name(name)) => {
            SalsaMemberPathRootSummary::Name(name.clone())
        }
        SalsaMemberRootSummary::LocalDecl { name, .. } => {
            SalsaMemberPathRootSummary::Name(name.clone())
        }
    };

    SalsaMemberPathSummary {
        root,
        owner_segments: target.owner_segments.clone(),
        member_name: target.member_name.clone(),
    }
}
