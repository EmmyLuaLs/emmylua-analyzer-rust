use hashbrown::{HashMap, HashSet};
use smol_str::SmolStr;

use crate::{
    FileId, SalsaDocSummary, SalsaDocTypeLoweredKind, SalsaDocTypeNodeKey, SalsaDocTypeRef,
    SalsaSummaryHost, WorkspaceId,
};

use super::{CompilationTypeDecl, CompilationTypeDeclId, CompilationTypeDeclScope};
use crate::compilation::{CompilationIndexContext, CompilationModuleIndex, FileBackedIndex};

pub type CompilationTypeDeclTree = HashMap<SmolStr, Option<CompilationTypeDeclId>>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompilationSuperTypeOrigin {
    file_id: FileId,
    workspace_id: WorkspaceId,
    name: SmolStr,
}

#[derive(Debug, Default)]
pub struct CompilationTypeIndex {
    file_namespace: HashMap<FileId, SmolStr>,
    file_using_namespace: HashMap<FileId, Vec<SmolStr>>,
    file_types: HashMap<FileId, Vec<CompilationTypeDeclId>>,
    file_super_types: HashMap<FileId, Vec<(CompilationTypeDeclId, CompilationSuperTypeOrigin)>>,
    decls: HashMap<CompilationTypeDeclId, CompilationTypeDecl>,
    super_types: HashMap<CompilationTypeDeclId, Vec<CompilationSuperTypeOrigin>>,
    global_name_type_map: HashMap<SmolStr, CompilationTypeDeclId>,
    internal_name_type_map: HashMap<WorkspaceId, HashMap<SmolStr, CompilationTypeDeclId>>,
    local_name_type_map: HashMap<FileId, HashMap<SmolStr, CompilationTypeDeclId>>,
}

impl CompilationTypeIndex {
    pub fn new() -> Self {
        Self::default()
    }

    fn rebuild_file(
        &mut self,
        summary: &SalsaSummaryHost,
        modules: &CompilationModuleIndex,
        file_id: FileId,
    ) {
        self.remove(file_id);

        let Some(doc) = summary.doc().summary(file_id) else {
            return;
        };

        let properties = summary
            .semantic()
            .file()
            .tag_properties(file_id)
            .unwrap_or_default();
        if let Some(namespace) = properties
            .iter()
            .find_map(|property| property.namespace().cloned())
        {
            self.file_namespace.insert(file_id, namespace);
        }

        let using_names = properties
            .iter()
            .flat_map(|property| property.using_names().cloned())
            .collect::<Vec<_>>();
        if !using_names.is_empty() {
            self.file_using_namespace.insert(file_id, using_names);
        }

        let workspace_id = modules
            .get_module(file_id)
            .map(|module| module.workspace_id)
            .unwrap_or(WorkspaceId::MAIN);
        let namespace = self.file_namespace.get(&file_id).cloned();
        let type_decls = build_type_decls(summary, file_id, workspace_id, namespace.as_ref(), &doc);
        for decl in type_decls {
            self.index_decl(file_id, decl);
        }
    }

    pub fn remove(&mut self, file_id: FileId) {
        self.file_namespace.remove(&file_id);
        self.file_using_namespace.remove(&file_id);

        if let Some(super_origins) = self.file_super_types.remove(&file_id) {
            for (decl_id, origin) in super_origins {
                let should_remove_decl = if let Some(origins) = self.super_types.get_mut(&decl_id) {
                    origins.retain(|existing| existing != &origin);
                    origins.is_empty()
                } else {
                    false
                };

                if should_remove_decl {
                    self.super_types.remove(&decl_id);
                }
            }
        }

        let Some(decl_ids) = self.file_types.remove(&file_id) else {
            self.local_name_type_map.remove(&file_id);
            return;
        };

        for decl_id in decl_ids {
            self.decls.remove(&decl_id);
            match decl_id.scope {
                CompilationTypeDeclScope::Global => {
                    self.global_name_type_map.remove(&decl_id.full_name);
                }
                CompilationTypeDeclScope::Internal(workspace_id) => {
                    let should_remove_workspace = if let Some(type_names) =
                        self.internal_name_type_map.get_mut(&workspace_id)
                    {
                        type_names.remove(&decl_id.full_name);
                        type_names.is_empty()
                    } else {
                        false
                    };
                    if should_remove_workspace {
                        self.internal_name_type_map.remove(&workspace_id);
                    }
                }
                CompilationTypeDeclScope::Local(owner_file_id) => {
                    let should_remove_file = if let Some(type_names) =
                        self.local_name_type_map.get_mut(&owner_file_id)
                    {
                        type_names.remove(&decl_id.full_name);
                        type_names.is_empty()
                    } else {
                        false
                    };
                    if should_remove_file {
                        self.local_name_type_map.remove(&owner_file_id);
                    }
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.file_namespace.clear();
        self.file_using_namespace.clear();
        self.file_types.clear();
        self.file_super_types.clear();
        self.decls.clear();
        self.super_types.clear();
        self.global_name_type_map.clear();
        self.internal_name_type_map.clear();
        self.local_name_type_map.clear();
    }

    pub fn get_file_namespace(&self, file_id: &FileId) -> Option<&SmolStr> {
        self.file_namespace.get(file_id)
    }

    pub fn get_file_using_namespace(&self, file_id: &FileId) -> Option<&Vec<SmolStr>> {
        self.file_using_namespace.get(file_id)
    }

    pub fn find_type_decl(
        &self,
        file_id: FileId,
        name: &str,
        workspace_id: Option<WorkspaceId>,
    ) -> Option<&CompilationTypeDecl> {
        let mut qualified_name = String::new();
        if let Some(namespace) = self.get_file_namespace(&file_id) {
            build_qualified_name(&mut qualified_name, namespace, name);
            if let Some(decl) =
                self.find_scoped_type_decl_by_name(file_id, workspace_id, &qualified_name, false)
            {
                return Some(decl);
            }
        }

        if let Some(namespaces) = self.get_file_using_namespace(&file_id) {
            for namespace in namespaces {
                build_qualified_name(&mut qualified_name, namespace, name);
                if let Some(decl) = self.find_scoped_type_decl_by_name(
                    file_id,
                    workspace_id,
                    &qualified_name,
                    false,
                ) {
                    return Some(decl);
                }
            }
        }

        self.find_scoped_type_decl_by_name(file_id, workspace_id, name, true)
    }

    pub fn find_type_decls(
        &self,
        file_id: FileId,
        prefix: &str,
        workspace_id: Option<WorkspaceId>,
    ) -> CompilationTypeDeclTree {
        let prefixes = self.collect_visible_prefixes(file_id, prefix);
        let mut prefix_results = (0..prefixes.len())
            .map(|_| CompilationTypeDeclTree::default())
            .collect::<Vec<_>>();

        for decl_id in self.decls.keys() {
            let decl_name = match decl_id.scope {
                CompilationTypeDeclScope::Global => decl_id.name(),
                CompilationTypeDeclScope::Internal(owner_workspace_id) => {
                    if workspace_id == Some(owner_workspace_id) {
                        decl_id.name()
                    } else {
                        continue;
                    }
                }
                CompilationTypeDeclScope::Local(owner_file_id) => {
                    if owner_file_id == file_id {
                        decl_id.name()
                    } else {
                        continue;
                    }
                }
            };

            for (index, visible_prefix) in prefixes.iter().enumerate() {
                if let Some(rest_name) = decl_name.strip_prefix(visible_prefix.as_str()) {
                    if let Some(separator_index) = rest_name.find('.') {
                        let name = SmolStr::new(&rest_name[..separator_index]);
                        prefix_results[index].entry(name).or_insert(None);
                    } else if !rest_name.is_empty() {
                        prefix_results[index]
                            .insert(SmolStr::new(rest_name), Some(decl_id.clone()));
                    }
                }
            }
        }

        let mut result = CompilationTypeDeclTree::default();
        for prefix_result in prefix_results {
            for (name, decl_id) in prefix_result {
                if let Some(decl_id) = decl_id {
                    result.insert(name, Some(decl_id));
                } else {
                    result.entry(name).or_insert(None);
                }
            }
        }

        result
    }

    pub fn get_visible_type_decls_by_full_name(
        &self,
        file_id: FileId,
        full_name: &str,
        workspace_id: Option<WorkspaceId>,
    ) -> Vec<&CompilationTypeDecl> {
        let mut decls = Vec::with_capacity(3);

        if let Some(decl) = self
            .local_name_type_map
            .get(&file_id)
            .and_then(|type_names| type_names.get(full_name))
            .and_then(|decl_id| self.decls.get(decl_id))
        {
            decls.push(decl);
        }

        if let Some(workspace_id) = workspace_id {
            if let Some(decl) = self
                .internal_name_type_map
                .get(&workspace_id)
                .and_then(|type_names| type_names.get(full_name))
                .and_then(|decl_id| self.decls.get(decl_id))
            {
                decls.push(decl);
            }
        }

        if let Some(decl) = self
            .global_name_type_map
            .get(full_name)
            .and_then(|decl_id| self.decls.get(decl_id))
        {
            decls.push(decl);
        }

        decls
    }

    pub fn get_type_decl(&self, decl_id: &CompilationTypeDeclId) -> Option<&CompilationTypeDecl> {
        self.decls.get(decl_id)
    }

    pub fn get_super_type_ids(
        &self,
        decl_id: &CompilationTypeDeclId,
    ) -> Vec<CompilationTypeDeclId> {
        let mut super_type_ids = Vec::new();
        let mut seen = HashSet::new();

        let Some(origins) = self.super_types.get(decl_id) else {
            return super_type_ids;
        };

        for origin in origins {
            let Some(super_decl) = self.find_type_decl(
                origin.file_id,
                origin.name.as_str(),
                Some(origin.workspace_id),
            ) else {
                continue;
            };

            if seen.insert(super_decl.id.clone()) {
                super_type_ids.push(super_decl.id.clone());
            }
        }

        super_type_ids
    }

    pub fn get_file_type_decls(&self, file_id: FileId) -> Vec<&CompilationTypeDecl> {
        self.file_types
            .get(&file_id)
            .into_iter()
            .flatten()
            .filter_map(|decl_id| self.decls.get(decl_id))
            .collect()
    }

    pub fn get_all_type_decls(&self) -> Vec<&CompilationTypeDecl> {
        self.decls.values().collect()
    }

    fn index_decl(&mut self, file_id: FileId, decl: CompilationTypeDecl) {
        let decl_id = decl.id.clone();
        let super_origins = decl
            .super_type_names
            .iter()
            .cloned()
            .map(|name| {
                (
                    decl_id.clone(),
                    CompilationSuperTypeOrigin {
                        file_id,
                        workspace_id: match decl_id.scope {
                            CompilationTypeDeclScope::Internal(workspace_id) => workspace_id,
                            _ => WorkspaceId::MAIN,
                        },
                        name,
                    },
                )
            })
            .collect::<Vec<_>>();

        self.file_types
            .entry(file_id)
            .or_default()
            .push(decl_id.clone());
        if !super_origins.is_empty() {
            for (owner_id, origin) in &super_origins {
                self.super_types
                    .entry(owner_id.clone())
                    .or_default()
                    .push(origin.clone());
            }
            self.file_super_types.insert(file_id, super_origins);
        }

        match decl_id.scope {
            CompilationTypeDeclScope::Global => {
                self.global_name_type_map
                    .insert(decl_id.full_name.clone(), decl_id.clone());
            }
            CompilationTypeDeclScope::Internal(workspace_id) => {
                self.internal_name_type_map
                    .entry(workspace_id)
                    .or_default()
                    .insert(decl_id.full_name.clone(), decl_id.clone());
            }
            CompilationTypeDeclScope::Local(owner_file_id) => {
                self.local_name_type_map
                    .entry(owner_file_id)
                    .or_default()
                    .insert(decl_id.full_name.clone(), decl_id.clone());
            }
        }
        self.decls.insert(decl_id, decl);
    }

    fn collect_visible_prefixes(&self, file_id: FileId, prefix: &str) -> Vec<SmolStr> {
        let mut prefixes: Vec<SmolStr> = Vec::new();
        let mut qualified_prefix = String::new();

        let mut push_unique_prefix = |candidate: &str| {
            if prefixes.iter().any(|prefix| prefix.as_str() == candidate) {
                return;
            }
            prefixes.push(SmolStr::new(candidate));
        };

        if let Some(namespace) = self.get_file_namespace(&file_id) {
            build_qualified_name(&mut qualified_prefix, namespace, prefix);
            push_unique_prefix(&qualified_prefix);
        }

        if let Some(namespaces) = self.get_file_using_namespace(&file_id) {
            for namespace in namespaces {
                build_qualified_name(&mut qualified_prefix, namespace, prefix);
                push_unique_prefix(&qualified_prefix);
            }
        }

        push_unique_prefix(prefix);
        prefixes
    }

    fn find_scoped_type_decl_by_name(
        &self,
        file_id: FileId,
        workspace_id: Option<WorkspaceId>,
        name: &str,
        allow_local: bool,
    ) -> Option<&CompilationTypeDecl> {
        if allow_local {
            if let Some(decl) = self
                .local_name_type_map
                .get(&file_id)
                .and_then(|type_names| type_names.get(name))
                .and_then(|decl_id| self.decls.get(decl_id))
            {
                return Some(decl);
            }
        }

        if let Some(workspace_id) = workspace_id {
            if let Some(decl) = self
                .internal_name_type_map
                .get(&workspace_id)
                .and_then(|type_names| type_names.get(name))
                .and_then(|decl_id| self.decls.get(decl_id))
            {
                return Some(decl);
            }
        }

        self.global_name_type_map
            .get(name)
            .and_then(|decl_id| self.decls.get(decl_id))
    }
}

fn build_type_decls(
    summary: &SalsaSummaryHost,
    file_id: FileId,
    workspace_id: WorkspaceId,
    namespace: Option<&SmolStr>,
    doc: &SalsaDocSummary,
) -> Vec<CompilationTypeDecl> {
    doc.type_defs
        .iter()
        .map(|type_def| {
            let full_name = namespace
                .map(|namespace| SmolStr::new(format!("{}.{}", namespace, type_def.name)))
                .unwrap_or_else(|| type_def.name.clone());
            let id = match type_def.visibility {
                crate::SalsaDocVisibilityKindSummary::Private => {
                    CompilationTypeDeclId::local(file_id, full_name.clone())
                }
                crate::SalsaDocVisibilityKindSummary::Internal => {
                    CompilationTypeDeclId::internal(workspace_id, full_name.clone())
                }
                _ if workspace_id.is_main() => {
                    CompilationTypeDeclId::internal(workspace_id, full_name.clone())
                }
                _ => CompilationTypeDeclId::global(full_name.clone()),
            };
            CompilationTypeDecl {
                file_id,
                id,
                simple_name: type_def.name.clone(),
                kind: type_def.kind.clone(),
                syntax_offset: type_def.syntax_offset.into(),
                generic_params: type_def.generic_params.clone(),
                super_type_offsets: type_def.super_type_offsets.clone(),
                super_type_names: build_super_type_names(
                    summary,
                    file_id,
                    &type_def.super_type_offsets,
                ),
                value_type_offset: type_def.value_type_offset,
            }
        })
        .collect()
}

fn build_super_type_names(
    summary: &SalsaSummaryHost,
    file_id: FileId,
    super_type_offsets: &[SalsaDocTypeNodeKey],
) -> Vec<SmolStr> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    let mut visited_offsets = HashSet::new();

    for type_offset in super_type_offsets {
        collect_super_type_names_from_offset(
            summary,
            file_id,
            *type_offset,
            &mut names,
            &mut seen,
            &mut visited_offsets,
        );
    }

    names
}

fn collect_super_type_names_from_offset(
    summary: &SalsaSummaryHost,
    file_id: FileId,
    type_offset: SalsaDocTypeNodeKey,
    out: &mut Vec<SmolStr>,
    seen: &mut HashSet<SmolStr>,
    visited_offsets: &mut HashSet<SalsaDocTypeNodeKey>,
) {
    if !visited_offsets.insert(type_offset) {
        return;
    }

    let Some(resolved) = summary.doc().resolved_type_at(file_id, type_offset.into()) else {
        return;
    };

    collect_super_type_names_from_kind(
        summary,
        file_id,
        &resolved.lowered.kind,
        out,
        seen,
        visited_offsets,
    );
}

fn collect_super_type_names_from_ref(
    summary: &SalsaSummaryHost,
    file_id: FileId,
    type_ref: &SalsaDocTypeRef,
    out: &mut Vec<SmolStr>,
    seen: &mut HashSet<SmolStr>,
    visited_offsets: &mut HashSet<SalsaDocTypeNodeKey>,
) {
    let SalsaDocTypeRef::Node(type_offset) = type_ref else {
        return;
    };

    collect_super_type_names_from_offset(
        summary,
        file_id,
        *type_offset,
        out,
        seen,
        visited_offsets,
    );
}

fn collect_super_type_names_from_kind(
    summary: &SalsaSummaryHost,
    file_id: FileId,
    kind: &SalsaDocTypeLoweredKind,
    out: &mut Vec<SmolStr>,
    seen: &mut HashSet<SmolStr>,
    visited_offsets: &mut HashSet<SalsaDocTypeNodeKey>,
) {
    match kind {
        SalsaDocTypeLoweredKind::Name { name } => {
            if seen.insert(name.clone()) {
                out.push(name.clone());
            }
        }
        SalsaDocTypeLoweredKind::Generic { base_type, .. }
        | SalsaDocTypeLoweredKind::Array {
            item_type: base_type,
        }
        | SalsaDocTypeLoweredKind::Variadic {
            item_type: base_type,
        }
        | SalsaDocTypeLoweredKind::Nullable {
            inner_type: base_type,
        } => {
            collect_super_type_names_from_ref(
                summary,
                file_id,
                base_type,
                out,
                seen,
                visited_offsets,
            );
        }
        SalsaDocTypeLoweredKind::Union { item_types }
        | SalsaDocTypeLoweredKind::Intersection { item_types }
        | SalsaDocTypeLoweredKind::Tuple { item_types }
        | SalsaDocTypeLoweredKind::MultiLineUnion { item_types } => {
            for item_type in item_types {
                collect_super_type_names_from_ref(
                    summary,
                    file_id,
                    item_type,
                    out,
                    seen,
                    visited_offsets,
                );
            }
        }
        SalsaDocTypeLoweredKind::Binary {
            left_type,
            right_type,
            ..
        } => {
            collect_super_type_names_from_ref(
                summary,
                file_id,
                left_type,
                out,
                seen,
                visited_offsets,
            );
            collect_super_type_names_from_ref(
                summary,
                file_id,
                right_type,
                out,
                seen,
                visited_offsets,
            );
        }
        SalsaDocTypeLoweredKind::Conditional {
            condition_type,
            true_type,
            false_type,
            ..
        } => {
            collect_super_type_names_from_ref(
                summary,
                file_id,
                condition_type,
                out,
                seen,
                visited_offsets,
            );
            collect_super_type_names_from_ref(
                summary,
                file_id,
                true_type,
                out,
                seen,
                visited_offsets,
            );
            collect_super_type_names_from_ref(
                summary,
                file_id,
                false_type,
                out,
                seen,
                visited_offsets,
            );
        }
        SalsaDocTypeLoweredKind::IndexAccess {
            base_type,
            index_type,
        } => {
            collect_super_type_names_from_ref(
                summary,
                file_id,
                base_type,
                out,
                seen,
                visited_offsets,
            );
            collect_super_type_names_from_ref(
                summary,
                file_id,
                index_type,
                out,
                seen,
                visited_offsets,
            );
        }
        _ => {}
    }
}

fn build_qualified_name(qualified_name: &mut String, namespace: &str, name: &str) {
    qualified_name.clear();
    qualified_name.push_str(namespace);
    qualified_name.push('.');
    qualified_name.push_str(name);
}

impl FileBackedIndex for CompilationTypeIndex {
    fn remove_file(&mut self, file_id: FileId) {
        CompilationTypeIndex::remove(self, file_id);
    }

    fn rebuild_file(&mut self, ctx: &CompilationIndexContext<'_>, file_id: FileId) {
        let Some(modules) = ctx.modules else {
            CompilationTypeIndex::remove(self, file_id);
            return;
        };

        CompilationTypeIndex::rebuild_file(self, ctx.summary, modules, file_id);
    }

    fn clear(&mut self) {
        CompilationTypeIndex::clear(self);
    }
}
