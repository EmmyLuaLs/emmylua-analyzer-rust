// #[allow(unused)]
mod analyzer;
mod decl;
mod index;
mod member;
mod module;
mod return_flow;
mod summary_builder;
mod test;
mod types;

use emmylua_parser::{LuaChunk, LuaParseError};
use hashbrown::HashMap;
use hashbrown::HashSet;
use lsp_types::Uri;
use std::sync::Arc;

pub(crate) use self::summary_builder::analyze_module_return_points;
use crate::{
    DbIndex, Emmyrc, FileId, InFiled, LuaDocument, LuaIndex, LuaInferCache, LuaMember,
    LuaMemberId, LuaMemberOwner, LuaSemanticDeclId, LuaSignature, LuaSignatureId,
    Workspace as LuaWorkspace, WorkspaceId as LuaWorkspaceId,
    module_query::{
        export::semantic_id_from_compilation_module,
        identity::find_compilation_module,
    },
    semantic::SemanticModel,
};
pub use decl::{CompilationDeclIndex, CompilationDeclTree};
use index::{CompilationIndexContext, FileBackedIndex};
pub use member::{
    CompilationMemberFeature, CompilationMemberIndex, CompilationMemberInfo, CompilationMemberKind,
    CompilationMemberSource,
};
pub use module::{
    CompilationModuleIndex, CompilationModuleInfo, CompilationModuleNode, CompilationModuleNodeId,
    CompilationModuleVisibility,
};
pub use return_flow::{
    LuaReturnPoint, analyze_func_body_missing_return_flags_with, analyze_func_body_returns_with,
    does_func_body_always_return_or_exit,
};
pub use summary_builder::*;
pub(crate) use types::union_type_shallow;
pub use types::*;

#[derive(Debug)]
pub struct LuaCompilation {
    emmyrc: Arc<Emmyrc>,
    legacy: DbIndex,
    summary: SalsaSummaryHost,
    decls: CompilationDeclIndex,
    modules: CompilationModuleIndex,
    types: CompilationTypeIndex,
    members: CompilationMemberIndex,
}

impl LuaCompilation {
    pub fn new(emmyrc: Arc<Emmyrc>) -> Self {
        let mut compilation = Self {
            emmyrc: emmyrc.clone(),
            legacy: DbIndex::new(),
            summary: SalsaSummaryHost::new(emmyrc.clone()),
            decls: CompilationDeclIndex::new(),
            modules: CompilationModuleIndex::new(),
            types: CompilationTypeIndex::new(),
            members: CompilationMemberIndex::new(),
        };

    compilation.legacy.update_config(emmyrc.clone());
        compilation.modules.update_config(emmyrc);
        compilation.sync_summary_workspaces();
        compilation
    }

    fn sync_summary_workspaces(&mut self) {
        let workspaces = self.modules.get_workspaces().to_vec();
        self.summary.set_workspaces(workspaces.clone());
        self.modules.set_workspaces(workspaces);
    }

    // Rebuild summary salsa inputs from the summary host Vfs after index-wide resets.
    fn sync_summary_files(&mut self, file_ids: &[FileId]) {
        for file_id in file_ids {
            if !self.summary.sync_file(*file_id) {
                self.summary.remove_file(*file_id);
                FileBackedIndex::remove_file(&mut self.decls, *file_id);
                FileBackedIndex::remove_file(&mut self.modules, *file_id);
                FileBackedIndex::remove_file(&mut self.types, *file_id);
                FileBackedIndex::remove_file(&mut self.members, *file_id);
                self.legacy.get_file_dependencies_index_mut().remove(*file_id);
                continue;
            }

            let base_ctx = CompilationIndexContext::new(&self.summary, self.summary.vfs());
            FileBackedIndex::sync_file(&mut self.decls, &base_ctx, *file_id);
            FileBackedIndex::sync_file(&mut self.modules, &base_ctx, *file_id);

            let type_ctx = base_ctx.with_modules(&self.modules);
            FileBackedIndex::sync_file(&mut self.types, &type_ctx, *file_id);

            let member_ctx = type_ctx.with_types(&self.types);
            FileBackedIndex::sync_file(&mut self.members, &member_ctx, *file_id);

            self.sync_summary_require_dependencies(*file_id);
        }
    }

    fn sync_summary_require_dependencies(&mut self, file_id: FileId) {
        self.legacy.get_file_dependencies_index_mut().remove(file_id);

        let Some(required_modules) = self.summary.semantic().file().required_modules(file_id)
        else {
            return;
        };

        for module_path in required_modules {
            let Some(module_info) = self.modules.find_module(module_path.as_str()) else {
                continue;
            };

            self.legacy
                .get_file_dependencies_index_mut()
                .add_required_file(file_id, module_info.file_id);
        }
    }

    fn write_local_file(&mut self, uri: &Uri, text: Option<String>) -> FileId {
        let summary_file_id = self.summary.update_file_by_uri(uri, text.clone());
        let legacy_file_id = self.legacy.get_vfs_mut().set_file_content(uri, text);
        debug_assert_eq!(summary_file_id, legacy_file_id);
        summary_file_id
    }

    fn write_remote_file(&mut self, uri: &Uri, text: Option<String>) -> FileId {
        let summary_file_id = self.summary.update_remote_file_by_uri(uri, text.clone());
        let legacy_file_id = self.legacy.get_vfs_mut().set_remote_file_content(uri, text);
        debug_assert_eq!(summary_file_id, legacy_file_id);
        summary_file_id
    }

    pub fn get_semantic_model(&'_ self, file_id: FileId) -> Option<SemanticModel<'_>> {
        let cache = LuaInferCache::new(file_id, Default::default());
        let tree = self.summary.vfs().get_syntax_tree(&file_id)?;
        Some(SemanticModel::new(
            file_id,
            self,
            &self.summary,
            cache,
            self.emmyrc.clone(),
            tree.get_chunk_node(),
        ))
    }

    pub fn update_index(&mut self, file_ids: Vec<FileId>) {
        self.sync_summary_workspaces();
        self.sync_summary_files(&file_ids);

        let need_analyzed_files = file_ids
            .iter()
            .filter_map(|file_id| {
                self.legacy
                    .get_vfs()
                    .get_syntax_tree(file_id)
                    .map(|tree| InFiled::new(*file_id, tree.get_chunk_node()))
            })
            .collect::<Vec<_>>();

        analyzer::analyze(&mut self.legacy, need_analyzed_files, self.emmyrc.clone());

        // Keep the legacy analyzer pipeline disconnected until compilation owns the
        // remaining semantic/diagnostic consumers.
        self.sync_summary_doc_diagnostics(&file_ids);
    }

    fn sync_summary_doc_diagnostics(&mut self, file_ids: &[FileId]) {
        for file_id in file_ids {
            let Some(properties) = self.summary.doc().tag_properties(*file_id) else {
                continue;
            };

            for property in properties {
                let owner = property.owner.clone();
                let Some(diagnostics) = self
                    .summary
                    .doc()
                    .resolved_tag_diagnostics(*file_id, owner.clone())
                else {
                    continue;
                };

                for diagnostic in diagnostics {
                    self.apply_summary_doc_diagnostic(*file_id, &owner, diagnostic);
                }
            }
        }
    }

    fn apply_summary_doc_diagnostic(
        &mut self,
        _file_id: FileId,
        owner: &SalsaDocOwnerSummary,
        _diagnostic: SalsaResolvedDocDiagnosticActionSummary,
    ) {
        let _ = owner.syntax_offset.is_none();
    }

    pub fn remove_index(&mut self, file_ids: Vec<FileId>) {
        for file_id in &file_ids {
            self.summary.remove_file(*file_id);
            FileBackedIndex::remove_file(&mut self.decls, *file_id);
            FileBackedIndex::remove_file(&mut self.modules, *file_id);
            FileBackedIndex::remove_file(&mut self.types, *file_id);
            FileBackedIndex::remove_file(&mut self.members, *file_id);
        }
        self.legacy.remove_index(file_ids);
    }

    pub fn clear_index(&mut self) {
        self.legacy.clear();
        self.summary.clear();
        FileBackedIndex::clear(&mut self.decls);
        FileBackedIndex::clear(&mut self.modules);
        FileBackedIndex::clear(&mut self.types);
        FileBackedIndex::clear(&mut self.members);
        self.sync_summary_workspaces();
    }

    pub fn add_workspace(&mut self, workspace: LuaWorkspace) {
        self.legacy.get_module_index_mut().add_workspace_root_with_import(
            workspace.root.clone(),
            workspace.import.clone(),
            workspace.id,
        );
        self.modules
            .set_workspaces(self.legacy.get_module_index().get_workspaces().to_vec());
        self.sync_summary_workspaces();
    }

    pub fn clear_non_std_workspaces(&mut self) {
        self.legacy.get_module_index_mut().clear_non_std_workspaces();
        self.modules
            .set_workspaces(self.legacy.get_module_index().get_workspaces().to_vec());
        self.sync_summary_workspaces();
    }

    pub fn update_file_by_uri(&mut self, uri: &Uri, text: Option<String>) -> Option<FileId> {
        let is_removed = text.is_none();
        let file_id = self.write_local_file(uri, text);

        self.remove_index(vec![file_id]);
        if !is_removed {
            self.update_index(vec![file_id]);
        }

        Some(file_id)
    }

    pub fn update_remote_file_by_uri(&mut self, uri: &Uri, text: Option<String>) -> FileId {
        let is_removed = text.is_none();
        let file_id = self.write_remote_file(uri, text);

        self.remove_index(vec![file_id]);
        if !is_removed {
            self.update_index(vec![file_id]);
        }

        file_id
    }

    pub fn update_files_by_uri(&mut self, files: Vec<(Uri, Option<String>)>) -> Vec<FileId> {
        let mut removed_files = HashSet::new();
        let mut updated_files = HashSet::new();

        for (uri, text) in files {
            let is_new_text = text.is_some();
            let file_id = self.write_local_file(&uri, text);
            removed_files.insert(file_id);
            if is_new_text {
                updated_files.insert(file_id);
            }
        }

        self.remove_index(removed_files.into_iter().collect());
        let updated_files: Vec<FileId> = updated_files.into_iter().collect();
        self.update_index(updated_files.clone());
        updated_files
    }

    pub(crate) fn update_files_by_uri_sorted(
        &mut self,
        files: Vec<(Uri, Option<String>)>,
    ) -> Vec<FileId> {
        let mut removed_files = HashSet::new();
        let mut updated_files = HashSet::new();

        for (uri, text) in files {
            let is_new_text = text.is_some();
            let file_id = self.write_local_file(&uri, text);
            removed_files.insert(file_id);
            if is_new_text {
                updated_files.insert(file_id);
            }
        }

        self.remove_index(removed_files.into_iter().collect());
        let mut updated_files: Vec<FileId> = updated_files.into_iter().collect();
        updated_files.sort();
        self.update_index(updated_files.clone());
        updated_files
    }

    pub fn remove_file_by_uri(&mut self, uri: &Uri) -> Option<FileId> {
        let summary_file_id = self.summary.remove_file_by_uri(uri);
        let legacy_file_id = self.legacy.get_vfs_mut().remove_file(uri);
        debug_assert_eq!(summary_file_id, legacy_file_id);
        let file_id = summary_file_id?;
        self.remove_index(vec![file_id]);
        Some(file_id)
    }

    pub fn summary(&self) -> &SalsaSummaryHost {
        &self.summary
    }

    pub fn get_document(&self, file_id: FileId) -> Option<LuaDocument<'_>> {
        self.summary.vfs().get_document(&file_id)
    }

    pub fn get_document_by_uri(&self, uri: &Uri) -> Option<LuaDocument<'_>> {
        let file_id = self.file_id_by_uri(uri)?;
        self.get_document(file_id)
    }

    pub fn file_id_by_uri(&self, uri: &Uri) -> Option<FileId> {
        self.summary.vfs().get_file_id(uri)
    }

    pub fn get_uri(&self, file_id: FileId) -> Option<Uri> {
        self.summary.vfs().get_uri(&file_id)
    }

    pub fn all_file_ids(&self) -> Vec<FileId> {
        self.summary.vfs().get_all_file_ids()
    }

    pub fn all_local_file_ids(&self) -> Vec<FileId> {
        self.summary.vfs().get_all_local_file_ids()
    }

    pub fn get_root(&self, file_id: FileId) -> Option<LuaChunk> {
        Some(self.summary.vfs().get_syntax_tree(&file_id)?.get_chunk_node())
    }

    pub fn get_file_parse_error(&self, file_id: FileId) -> Option<Vec<LuaParseError>> {
        self.summary.vfs().get_file_parse_error(&file_id)
    }

    pub fn module_index(&self) -> &CompilationModuleIndex {
        &self.modules
    }

    pub fn find_module_by_file_id(&self, file_id: FileId) -> Option<&CompilationModuleInfo> {
        self.modules.get_module(file_id)
    }

    pub fn module_infos(&self) -> Vec<&CompilationModuleInfo> {
        self.modules.get_module_infos()
    }

    pub fn find_module_node(&self, module_path: &str) -> Option<&CompilationModuleNode> {
        self.modules.find_module_node(module_path)
    }

    pub fn get_module_node(
        &self,
        module_id: &CompilationModuleNodeId,
    ) -> Option<&CompilationModuleNode> {
        self.modules.get_module_node(module_id)
    }

    pub fn module_workspace_id(&self, file_id: FileId) -> Option<LuaWorkspaceId> {
        self.modules.get_workspace_id(file_id)
    }

    pub fn resolved_workspace_id(&self, file_id: FileId) -> LuaWorkspaceId {
        self.module_workspace_id(file_id)
            .unwrap_or(LuaWorkspaceId::MAIN)
    }

    pub fn module_is_meta_file(&self, file_id: FileId) -> bool {
        self.modules.is_meta_file(&file_id)
    }

    pub fn module_is_std(&self, file_id: FileId) -> bool {
        self.modules.is_std(&file_id)
    }

    pub fn module_is_main(&self, file_id: FileId) -> bool {
        self.modules.is_main(&file_id)
    }

    pub fn module_is_library(&self, file_id: FileId) -> bool {
        self.modules.is_library(&file_id)
    }

    pub fn std_file_ids(&self) -> Vec<FileId> {
        self.modules.get_std_file_ids()
    }

    pub fn main_workspace_file_ids(&self) -> Vec<FileId> {
        self.modules.get_main_workspace_file_ids()
    }

    pub fn library_file_ids(&self) -> Vec<FileId> {
        self.modules.get_lib_file_ids()
    }

    pub fn next_library_workspace_id(&self) -> u32 {
        self.modules.next_library_workspace_id()
    }

    pub fn decl_index(&self) -> &CompilationDeclIndex {
        &self.decls
    }

    pub fn find_module_by_require_path(&self, module_path: &str) -> Option<&CompilationModuleInfo> {
        find_compilation_module(&self.modules, module_path)
    }

    pub fn find_required_module_export_type(&self, module_path: &str) -> Option<LuaType> {
        let module = self.find_module_by_require_path(module_path)?;
        crate::module_query::export::infer_module_export_type(self.legacy_db(), module.file_id)
    }

    pub fn find_required_module_semantic_id(&self, module_path: &str) -> Option<LuaSemanticDeclId> {
        let module = self.find_module_by_require_path(module_path)?;
        semantic_id_from_compilation_module(module)
    }

    pub fn type_index(&self) -> &CompilationTypeIndex {
        &self.types
    }

    fn workspace_id_for_type_lookup(&self, file_id: FileId) -> Option<LuaWorkspaceId> {
        Some(self.resolved_workspace_id(file_id))
    }

    fn compat_type_decl_by_compilation_id(
        &self,
        decl_id: &CompilationTypeDeclId,
    ) -> Option<&LuaTypeDecl> {
        let legacy_decl_id = LuaTypeDeclId::from(decl_id);
        self.legacy.get_type_index().get_type_decl(&legacy_decl_id)
    }

    pub fn find_type_decl(&self, file_id: FileId, name: &str) -> Option<&LuaTypeDecl> {
        self.types
            .find_type_decl(file_id, name, self.workspace_id_for_type_lookup(file_id))
            .and_then(|decl| self.compat_type_decl_by_compilation_id(&decl.id))
            .or_else(|| {
                self.legacy.get_type_index().find_type_decl(
                    file_id,
                    name,
                    self.workspace_id_for_type_lookup(file_id),
                )
            })
    }

    pub fn get_type_decl(&self, decl_id: &LuaTypeDeclId) -> Option<&LuaTypeDecl> {
        let compilation_decl_id = CompilationTypeDeclId::from(decl_id);
        if self.types.get_type_decl(&compilation_decl_id).is_some() {
            return self.compat_type_decl_by_compilation_id(&compilation_decl_id);
        }

        self.legacy.get_type_index().get_type_decl(decl_id)
    }

    pub fn get_type_cache(&self, owner: &LuaTypeOwner) -> Option<&LuaTypeCache> {
        self.legacy.get_type_index().get_type_cache(owner)
    }

    pub fn get_signature(&self, signature_id: &LuaSignatureId) -> Option<&LuaSignature> {
        self.legacy.get_signature_index().get(signature_id)
    }

    pub fn get_members(&self, owner: &LuaMemberOwner) -> Option<Vec<&LuaMember>> {
        self.legacy.get_member_index().get_members(owner)
    }

    pub fn get_generic_params(&self, decl_id: &LuaTypeDeclId) -> Option<&Vec<crate::GenericParam>> {
        self.legacy.get_type_index().get_generic_params(decl_id)
    }

    pub fn get_decl_type(&self, decl_id: &crate::LuaDeclId) -> Option<&LuaType> {
        self.get_type_cache(&(*decl_id).into())
            .map(LuaTypeCache::as_type)
    }

    pub fn get_member_type(&self, member_id: &LuaMemberId) -> Option<&LuaType> {
        self.get_type_cache(&(*member_id).into())
            .map(LuaTypeCache::as_type)
    }

    pub fn get_super_types(&self, decl_id: &LuaTypeDeclId) -> Option<Vec<LuaType>> {
        let compilation_decl_id = CompilationTypeDeclId::from(decl_id);
        let super_type_ids = self.types.get_super_type_ids(&compilation_decl_id);
        if !super_type_ids.is_empty() {
            return Some(
                super_type_ids
                    .iter()
                    .map(|super_id| LuaType::Ref(LuaTypeDeclId::from(super_id)))
                    .collect(),
            );
        }

        None
    }

    pub fn collect_super_types(&self, decl_id: &LuaTypeDeclId, collected_types: &mut Vec<LuaType>) {
        let mut queue = vec![decl_id.clone()];

        while let Some(current_id) = queue.pop() {
            let Some(super_types) = self.get_super_types(&current_id) else {
                continue;
            };

            for super_type in super_types {
                match &super_type {
                    LuaType::Ref(super_type_id) => {
                        if !collected_types.contains(&super_type) {
                            collected_types.push(super_type.clone());
                            queue.push(super_type_id.clone());
                        }
                    }
                    _ => {
                        if !collected_types.contains(&super_type) {
                            collected_types.push(super_type.clone());
                        }
                    }
                }
            }
        }
    }

    pub fn collect_super_types_with_self(
        &self,
        decl_id: &LuaTypeDeclId,
        typ: LuaType,
    ) -> Vec<LuaType> {
        let mut collected_types = vec![typ];
        self.collect_super_types(decl_id, &mut collected_types);
        collected_types
    }

    pub fn get_file_type_decls(&self, file_id: FileId) -> Vec<&LuaTypeDecl> {
        self.types
            .get_file_type_decls(file_id)
            .into_iter()
            .filter_map(|decl| self.compat_type_decl_by_compilation_id(&decl.id))
            .collect::<Vec<_>>()

        // self.db.get_type_index().get_file_type_decls(file_id)
    }

    pub fn get_visible_type_decls_by_full_name(
        &self,
        file_id: FileId,
        full_name: &str,
    ) -> Vec<&LuaTypeDecl> {
        self.types
            .get_visible_type_decls_by_full_name(
                file_id,
                full_name,
                self.workspace_id_for_type_lookup(file_id),
            )
            .into_iter()
            .filter_map(|decl| self.compat_type_decl_by_compilation_id(&decl.id))
            .collect::<Vec<_>>()
        // self.db
        //     .get_type_index()
        //     .get_visible_type_decls_by_full_name(
        //         file_id,
        //         full_name,
        //         self.workspace_id_for_type_lookup(file_id),
        //     )
    }

    pub fn member_index(&self) -> &CompilationMemberIndex {
        &self.members
    }

    pub fn get_merged_owner_members(
        &self,
        owner: &CompilationTypeDeclId,
    ) -> HashMap<smol_str::SmolStr, CompilationMemberInfo> {
        self.members.get_merged_owner_members(
            &self.types,
            owner,
            self.emmyrc.strict.meta_override_file_define,
        )
    }

    pub fn get_merged_member(
        &self,
        owner: &CompilationTypeDeclId,
        name: &str,
    ) -> Option<CompilationMemberInfo> {
        self.members.get_merged_member(
            &self.types,
            owner,
            name,
            self.emmyrc.strict.meta_override_file_define,
        )
    }

    pub fn find_type_merged_member(
        &self,
        file_id: FileId,
        type_name: &str,
        workspace_id: Option<LuaWorkspaceId>,
        member_name: &str,
    ) -> Option<CompilationMemberInfo> {
        let type_decl = self
            .types
            .find_type_decl(file_id, type_name, workspace_id)?;
        self.get_merged_member(&type_decl.id, member_name)
    }

    pub fn update_config(&mut self, config: Arc<Emmyrc>) {
        self.emmyrc = config.clone();
        self.legacy.update_config(config.clone());
        self.summary.update_config(config);
        self.modules.update_config(self.emmyrc.clone());
        self.sync_summary_workspaces();
    }

    pub(crate) fn legacy_db(&self) -> &DbIndex {
        &self.legacy
    }

    pub(crate) fn legacy_db_mut(&mut self) -> &mut DbIndex {
        &mut self.legacy
    }
}
