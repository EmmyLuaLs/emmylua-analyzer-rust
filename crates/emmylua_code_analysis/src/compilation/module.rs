use std::{path::Path, sync::Arc};

use hashbrown::{HashMap, HashSet};
use regex::Regex;
use smol_str::SmolStr;

use emmylua_parser::{
    LuaAstNode, LuaCommentOwner, LuaDocTag, LuaReturnStat, VisibilityKind,
};

use crate::{
    Emmyrc, FileId, SalsaDocOwnerKindSummary, SalsaDocVersionConditionSummary,
    SalsaModuleExportQuerySummary, SalsaModuleExportSummary, SalsaSummaryHost, Vfs, Workspace,
    WorkspaceId,
};

use super::summary_builder::SalsaSemanticTargetSummary;
use super::{
    analyze_module_return_points,
    index::{CompilationIndexContext, FileBackedIndex},
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CompilationModuleNode {
    pub parent: Option<CompilationModuleNodeId>,
    pub children: HashMap<SmolStr, CompilationModuleNodeId>,
    pub file_ids: Vec<FileId>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub struct CompilationModuleNodeId {
    pub id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompilationModuleVisibility {
    #[default]
    Default,
    Public,
    Internal,
    Hide,
}

impl CompilationModuleVisibility {
    pub fn merge(self, visibility: Self) -> Self {
        match (self, visibility) {
            (Self::Hide, _) | (_, Self::Hide) => Self::Hide,
            (Self::Default, next) => next,
            (current, Self::Default) => current,
            (_, next) => next,
        }
    }

    pub fn resolve(self) -> Self {
        match self {
            Self::Default => Self::Public,
            visibility => visibility,
        }
    }

    pub fn is_hidden(self) -> bool {
        self == Self::Hide
    }

    pub fn is_requireable_from(
        self,
        owner_workspace: WorkspaceId,
        workspace_id: WorkspaceId,
    ) -> bool {
        match self.resolve() {
            Self::Public | Self::Default => true,
            Self::Internal => {
                (!owner_workspace.is_library() && !workspace_id.is_library())
                    || owner_workspace == workspace_id
            }
            Self::Hide => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilationModuleInfo {
    pub file_id: FileId,
    pub full_module_name: SmolStr,
    pub name: SmolStr,
    pub workspace_id: WorkspaceId,
    pub visible: CompilationModuleVisibility,
    pub export: Option<SalsaModuleExportSummary>,
    pub semantic_target: Option<SalsaSemanticTargetSummary>,
    pub version_conds: Vec<SalsaDocVersionConditionSummary>,
    pub is_meta: bool,
    pub is_remote: bool,
}

impl CompilationModuleInfo {
    pub fn has_export_type(&self) -> bool {
        self.export.is_some()
    }

    pub fn is_requireable_from(&self, workspace_id: WorkspaceId) -> bool {
        self.visible
            .is_requireable_from(self.workspace_id, workspace_id)
    }
}

#[derive(Debug)]
pub struct CompilationModuleIndex {
    module_patterns: Vec<Regex>,
    module_root_id: CompilationModuleNodeId,
    module_nodes: HashMap<CompilationModuleNodeId, CompilationModuleNode>,
    file_module_map: HashMap<FileId, CompilationModuleInfo>,
    file_module_node_map: HashMap<FileId, CompilationModuleNodeId>,
    full_module_name_to_file_ids: HashMap<SmolStr, Vec<FileId>>,
    module_name_to_file_ids: HashMap<SmolStr, Vec<FileId>>,
    workspaces: Vec<Workspace>,
    id_counter: u32,
    fuzzy_search: bool,
    module_replace_vec: Vec<(Regex, String)>,
}

impl CompilationModuleIndex {
    pub fn new() -> Self {
        let module_root_id = CompilationModuleNodeId { id: 0 };
        let mut module_nodes = HashMap::new();
        module_nodes.insert(module_root_id, CompilationModuleNode::default());

        Self {
            module_patterns: Vec::new(),
            module_root_id,
            module_nodes,
            file_module_map: HashMap::new(),
            file_module_node_map: HashMap::new(),
            full_module_name_to_file_ids: HashMap::new(),
            module_name_to_file_ids: HashMap::new(),
            workspaces: Vec::new(),
            id_counter: 1,
            fuzzy_search: false,
            module_replace_vec: Vec::new(),
        }
    }

    pub fn update_config(&mut self, config: Arc<Emmyrc>) {
        let mut extension_names = Vec::new();

        for extension in &config.runtime.extensions {
            if let Some(stripped) = extension
                .strip_prefix('.')
                .or_else(|| extension.strip_prefix("*."))
            {
                extension_names.push(stripped.to_string());
            } else {
                extension_names.push(extension.clone());
            }
        }

        if !extension_names.contains(&"lua".to_string()) {
            extension_names.push("lua".to_string());
        }

        let mut patterns = Vec::new();
        for extension in &extension_names {
            patterns.push(format!("?.{}", extension));
        }

        if config.runtime.require_pattern.is_empty() {
            for extension in &extension_names {
                patterns.push(format!("?/init.{}", extension));
            }
        } else {
            patterns.extend(config.runtime.require_pattern.clone());
        }

        self.set_module_extract_patterns(patterns);
        self.set_module_replace_patterns(
            config
                .workspace
                .module_map
                .iter()
                .map(|mapping| (mapping.pattern.clone(), mapping.replace.clone()))
                .collect(),
        );
        self.fuzzy_search = !config.strict.require_path;
    }

    pub fn set_workspaces(&mut self, workspaces: Vec<Workspace>) {
        self.workspaces = workspaces;
    }

    fn rebuild_file(&mut self, vfs: &Vfs, summary: &SalsaSummaryHost, file_id: FileId) {
        self.remove(file_id);

        let Some(path) = vfs.get_file_path(&file_id) else {
            return;
        };
        let Some((mut module_path, workspace_id)) = self.extract_module_path(path) else {
            return;
        };
        if !self.module_replace_vec.is_empty() {
            module_path = self.replace_module_path(&module_path);
        }

        let export_query = summary.semantic().file().module_export_query(file_id);

        let mut visible = CompilationModuleVisibility::Default;
        let mut version_conds = Vec::new();
        let mut is_meta = false;

        let use_return_visibility =
            module_uses_return_visibility(vfs, file_id, export_query.as_ref());

        for property in export_query
            .as_ref()
            .map(|query| query.tag_properties.as_slice())
            .unwrap_or(&[])
        {
            if !module_property_applies_to_export(use_return_visibility, &property.owner.kind) {
                continue;
            }

            if let Some(meta_name) = property.meta().cloned() {
                is_meta = true;
                if meta_name == "no-require" || meta_name == "_" {
                    visible = visible.merge(CompilationModuleVisibility::Hide);
                } else {
                    module_path = meta_name.to_string();
                }
            }

            if let Some(visibility) = property.visibility() {
                let mapped = match visibility {
                    crate::SalsaDocVisibilityKindSummary::Public => {
                        CompilationModuleVisibility::Public
                    }
                    crate::SalsaDocVisibilityKindSummary::Internal => {
                        CompilationModuleVisibility::Internal
                    }
                    _ => CompilationModuleVisibility::Default,
                };
                visible = visible.merge(mapped);
            }

            if !property.version_conds().is_empty() {
                version_conds = property.version_conds().to_vec();
            }
        }

        if use_return_visibility {
            if let Some(visibility) = first_return_visibility_from_comments(vfs, file_id)
            {
                visible = visible.merge(visibility);
            }
        }

        let full_module_name = SmolStr::new(module_path.replace(['\\', '/'], "."));
        let name = full_module_name
            .rsplit('.')
            .next()
            .map(SmolStr::new)
            .unwrap_or_else(|| full_module_name.clone());

        if self
            .index_module_path(file_id, full_module_name.as_str())
            .is_none()
        {
            return;
        }

        let info = CompilationModuleInfo {
            file_id,
            full_module_name: full_module_name.clone(),
            name: name.clone(),
            workspace_id,
            visible,
            export: export_query.as_ref().and_then(|query| query.export.clone()),
            semantic_target: export_query
                .as_ref()
                .and_then(|query| query.semantic_target.clone()),
            version_conds,
            is_meta,
            is_remote: vfs.is_remote_file(&file_id),
        };

        self.file_module_map.insert(file_id, info);
        self.full_module_name_to_file_ids
            .entry(full_module_name)
            .or_default()
            .push(file_id);
        if self.fuzzy_search {
            self.module_name_to_file_ids
                .entry(name)
                .or_default()
                .push(file_id);
        }
    }

    pub fn remove(&mut self, file_id: FileId) {
        let Some(info) = self.file_module_map.remove(&file_id) else {
            return;
        };

        let should_remove_full = if let Some(file_ids) = self
            .full_module_name_to_file_ids
            .get_mut(&info.full_module_name)
        {
            file_ids.retain(|id| *id != file_id);
            file_ids.is_empty()
        } else {
            false
        };
        if should_remove_full {
            self.full_module_name_to_file_ids
                .remove(&info.full_module_name);
        }

        let should_remove_name =
            if let Some(file_ids) = self.module_name_to_file_ids.get_mut(&info.name) {
                file_ids.retain(|id| *id != file_id);
                file_ids.is_empty()
            } else {
                false
            };
        if should_remove_name {
            self.module_name_to_file_ids.remove(&info.name);
        }

        let (mut parent_id, mut child_id) =
            if let Some(module_id) = self.file_module_node_map.remove(&file_id) {
                let Some(node) = self.module_nodes.get_mut(&module_id) else {
                    return;
                };
                node.file_ids.retain(|id| *id != file_id);
                if node.file_ids.is_empty() && node.children.is_empty() {
                    (node.parent, Some(module_id))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };

        while let Some(parent_node_id) = parent_id {
            let Some(child_module_id) = child_id else {
                break;
            };
            let Some(parent_node) = self.module_nodes.get_mut(&parent_node_id) else {
                break;
            };
            parent_node
                .children
                .retain(|_, node_child_id| *node_child_id != child_module_id);

            if parent_node_id == self.module_root_id {
                return;
            }

            if parent_node.file_ids.is_empty() && parent_node.children.is_empty() {
                child_id = Some(parent_node_id);
                parent_id = parent_node.parent;
                self.module_nodes.remove(&parent_node_id);
            } else {
                break;
            }
        }
    }

    pub fn clear(&mut self) {
        self.module_nodes.clear();
        self.module_nodes
            .insert(self.module_root_id, CompilationModuleNode::default());
        self.file_module_map.clear();
        self.file_module_node_map.clear();
        self.full_module_name_to_file_ids.clear();
        self.module_name_to_file_ids.clear();
        self.id_counter = 1;
    }

    pub fn get_module(&self, file_id: FileId) -> Option<&CompilationModuleInfo> {
        self.file_module_map.get(&file_id)
    }

    pub fn get_module_infos(&self) -> Vec<&CompilationModuleInfo> {
        self.file_module_map.values().collect()
    }

    pub fn find_module(&self, module_path: &str) -> Option<&CompilationModuleInfo> {
        let normalized = module_path.replace(['\\', '/'], ".");
        if let Some(file_id) = self
            .full_module_name_to_file_ids
            .get(normalized.as_str())
            .and_then(|file_ids| file_ids.first())
        {
            return self.file_module_map.get(file_id);
        }

        if !self.fuzzy_search {
            return None;
        }

        let last_name = normalized.rsplit('.').next()?;
        let suffix_with_boundary = format!(".{}", normalized);
        self.module_name_to_file_ids
            .get(last_name)
            .into_iter()
            .flatten()
            .filter_map(|file_id| {
                let info = self.file_module_map.get(file_id)?;
                let full_name = info.full_module_name.as_str();
                let leading_segment_count = if full_name == normalized {
                    Some(0)
                } else {
                    full_name.strip_suffix(&suffix_with_boundary).map(|prefix| {
                        prefix
                            .split('.')
                            .filter(|segment| !segment.is_empty())
                            .count()
                    })
                }?;
                Some((leading_segment_count, info))
            })
            .min_by(|(left_count, left_info), (right_count, right_info)| {
                left_count
                    .cmp(right_count)
                    .then_with(|| left_info.full_module_name.cmp(&right_info.full_module_name))
            })
            .map(|(_, info)| info)
    }

    pub fn find_module_node(&self, module_path: &str) -> Option<&CompilationModuleNode> {
        if module_path.is_empty() {
            return self.module_nodes.get(&self.module_root_id);
        }

        let normalized = module_path.replace(['\\', '/'], ".");
        let mut parent_node_id = self.module_root_id;
        for part in normalized.split('.') {
            let parent_node = self.module_nodes.get(&parent_node_id)?;
            parent_node_id = *parent_node.children.get(part)?;
        }

        self.module_nodes.get(&parent_node_id)
    }

    pub fn get_module_node(
        &self,
        module_id: &CompilationModuleNodeId,
    ) -> Option<&CompilationModuleNode> {
        self.module_nodes.get(module_id)
    }

    pub fn get_std_file_ids(&self) -> Vec<FileId> {
        self.file_module_map
            .values()
            .filter(|module_info| module_info.workspace_id.is_std())
            .map(|module_info| module_info.file_id)
            .collect()
    }

    pub fn get_main_workspace_file_ids(&self) -> Vec<FileId> {
        self.file_module_map
            .values()
            .filter(|module_info| module_info.workspace_id.is_main())
            .map(|module_info| module_info.file_id)
            .collect()
    }

    pub fn get_lib_file_ids(&self) -> Vec<FileId> {
        self.file_module_map
            .values()
            .filter(|module_info| module_info.workspace_id.is_library())
            .map(|module_info| module_info.file_id)
            .collect()
    }

    pub fn is_main(&self, file_id: &FileId) -> bool {
        self.file_module_map
            .get(file_id)
            .is_some_and(|module_info| module_info.workspace_id.is_main())
    }

    pub fn is_std(&self, file_id: &FileId) -> bool {
        self.file_module_map
            .get(file_id)
            .is_some_and(|module_info| module_info.workspace_id.is_std())
    }

    pub fn is_library(&self, file_id: &FileId) -> bool {
        self.file_module_map
            .get(file_id)
            .is_some_and(|module_info| module_info.workspace_id.is_library())
    }

    pub fn is_meta_file(&self, file_id: &FileId) -> bool {
        self.file_module_map
            .get(file_id)
            .is_some_and(|module_info| module_info.is_meta)
    }

    pub fn get_workspace_id(&self, file_id: FileId) -> Option<WorkspaceId> {
        self.file_module_map
            .get(&file_id)
            .map(|module_info| module_info.workspace_id)
    }

    pub fn get_workspaces(&self) -> &[Workspace] {
        self.workspaces.as_slice()
    }

    pub fn next_library_workspace_id(&self) -> u32 {
        let used: HashSet<u32> = self
            .workspaces
            .iter()
            .map(|workspace| workspace.id.id)
            .collect();
        let mut candidate = WorkspaceId::LIBRARY_START.id;
        while used.contains(&candidate) {
            candidate += 1;
        }
        candidate
    }

    fn set_module_extract_patterns(&mut self, patterns: Vec<String>) {
        let mut patterns = patterns;
        patterns.sort_by_key(|pattern| std::cmp::Reverse(pattern.len()));
        patterns.dedup();
        self.module_patterns.clear();
        for pattern in patterns {
            let regex_str = format!(
                "^{}$",
                regex::escape(&pattern.replace('\\', "/")).replace("\\?", "(.*)")
            );
            if let Ok(regex) = Regex::new(&regex_str) {
                self.module_patterns.push(regex);
            }
        }
    }

    fn set_module_replace_patterns(&mut self, patterns: HashMap<String, String>) {
        self.module_replace_vec.clear();
        for (key, value) in patterns {
            if let Ok(regex) = Regex::new(&key) {
                self.module_replace_vec.push((regex, value));
            }
        }
    }

    fn extract_module_path(&self, path: &Path) -> Option<(String, WorkspaceId)> {
        let mut matched_module_path: Option<(String, WorkspaceId)> = None;
        for workspace in &self.workspaces {
            if let Ok(relative_path) = path.strip_prefix(&workspace.root) {
                if !workspace.import.includes_path(relative_path) {
                    continue;
                }

                let relative_path_str = relative_path.to_str().unwrap_or_default();
                if relative_path_str.is_empty() {
                    if let Some(file_name) = workspace.root.file_prefix() {
                        return Some((file_name.to_string_lossy().to_string(), workspace.id));
                    }
                }

                let Some(module_path) = self.match_pattern(relative_path_str) else {
                    continue;
                };

                if let Some((matched, matched_workspace_id)) = matched_module_path.as_ref() {
                    if module_path.len() < matched.len() {
                        let workspace_id = if workspace.id.is_main() {
                            *matched_workspace_id
                        } else {
                            workspace.id
                        };
                        matched_module_path = Some((module_path, workspace_id));
                    }
                } else {
                    matched_module_path = Some((module_path, workspace.id));
                }
            }
        }
        matched_module_path
    }

    fn match_pattern(&self, path: &str) -> Option<String> {
        for pattern in &self.module_patterns {
            if let Some(captures) = pattern.captures(path)
                && let Some(matched) = captures.get(1)
            {
                return Some(matched.as_str().to_string());
            }
        }
        None
    }

    fn replace_module_path(&self, module_path: &str) -> String {
        let mut module_path = module_path.to_owned();
        for (pattern, replace) in &self.module_replace_vec {
            if let std::borrow::Cow::Owned(updated) = pattern.replace_all(&module_path, replace) {
                module_path = updated;
            }
        }
        module_path
    }

    fn index_module_path(
        &mut self,
        file_id: FileId,
        module_path: &str,
    ) -> Option<CompilationModuleNodeId> {
        let module_parts: Vec<&str> = module_path
            .split('.')
            .filter(|part| !part.is_empty())
            .collect();
        if module_parts.is_empty() {
            return None;
        }

        let mut parent_node_id = self.module_root_id;
        for part in &module_parts {
            let child_id = {
                let parent_node = self.module_nodes.get_mut(&parent_node_id)?;
                match parent_node.children.get(*part) {
                    Some(id) => *id,
                    None => {
                        let new_id = CompilationModuleNodeId {
                            id: self.id_counter,
                        };
                        parent_node.children.insert(SmolStr::new(*part), new_id);
                        new_id
                    }
                }
            };

            if let hashbrown::hash_map::Entry::Vacant(entry) = self.module_nodes.entry(child_id) {
                entry.insert(CompilationModuleNode {
                    children: HashMap::new(),
                    file_ids: Vec::new(),
                    parent: Some(parent_node_id),
                });
                self.id_counter += 1;
            }

            parent_node_id = child_id;
        }

        let node = self.module_nodes.get_mut(&parent_node_id)?;
        node.file_ids.push(file_id);
        self.file_module_node_map.insert(file_id, parent_node_id);
        Some(parent_node_id)
    }
}

fn module_property_applies_to_export(
    use_return_visibility: bool,
    owner_kind: &SalsaDocOwnerKindSummary,
) -> bool {
    match owner_kind {
        SalsaDocOwnerKindSummary::None => true,
        SalsaDocOwnerKindSummary::ReturnStat => use_return_visibility,
        _ => !use_return_visibility,
    }
}

fn module_uses_return_visibility(
    vfs: &Vfs,
    file_id: FileId,
    export_query: Option<&SalsaModuleExportQuerySummary>,
) -> bool {
    if let Some(query) = export_query {
        return matches!(
            query.export,
            Some(SalsaModuleExportSummary::Closure { .. })
                | Some(SalsaModuleExportSummary::Table { .. })
        );
    }

    let Some(tree) = vfs.get_syntax_tree(&file_id) else {
        return false;
    };
    let Some(block) = tree.get_chunk_node().get_block() else {
        return false;
    };

    let Some(first_return_expr) = analyze_module_return_points(block).into_iter().next() else {
        return false;
    };

    matches!(
        first_return_expr,
        emmylua_parser::LuaExpr::TableExpr(_) | emmylua_parser::LuaExpr::ClosureExpr(_)
    )
}

fn first_return_visibility_from_comments(
    vfs: &Vfs,
    file_id: FileId,
) -> Option<CompilationModuleVisibility> {
    let tree = vfs.get_syntax_tree(&file_id)?;
    let block = tree.get_chunk_node().get_block()?;
    let first_return_expr = analyze_module_return_points(block).into_iter().next()?;
    let return_stat = first_return_expr
        .syntax()
        .ancestors()
        .find_map(LuaReturnStat::cast)?;

    let mut visible = CompilationModuleVisibility::Default;
    let mut seen = false;
    for comment in return_stat.get_comments() {
        for tag in comment.get_doc_tags() {
            let LuaDocTag::Visibility(tag) = tag else {
                continue;
            };
            let Some(visibility) = tag
                .get_visibility_token()
                .and_then(|token| token.get_visibility())
            else {
                continue;
            };
            let mapped = match visibility {
                VisibilityKind::Public => CompilationModuleVisibility::Public,
                VisibilityKind::Internal => CompilationModuleVisibility::Internal,
                _ => CompilationModuleVisibility::Default,
            };
            visible = visible.merge(mapped);
            seen = true;
        }
    }

    seen.then_some(visible)
}

impl FileBackedIndex for CompilationModuleIndex {
    fn remove_file(&mut self, file_id: FileId) {
        CompilationModuleIndex::remove(self, file_id);
    }

    fn rebuild_file(&mut self, ctx: &CompilationIndexContext<'_>, file_id: FileId) {
        CompilationModuleIndex::rebuild_file(self, ctx.vfs, ctx.summary, file_id);
    }

    fn clear(&mut self) {
        CompilationModuleIndex::clear(self);
    }
}
