use hashbrown::HashMap;
use rowan::TextSize;
use std::collections::VecDeque;

use hashbrown::HashSet;
use smol_str::SmolStr;

use crate::{
    FileId, SalsaGlobalRootSummary, SalsaMemberIndexSummary, SalsaMemberKindSummary,
    SalsaMemberRootSummary, SalsaMemberTargetSummary, SalsaPropertyIndexSummary,
    SalsaPropertyKeySummary, SalsaPropertyKindSummary, SalsaPropertyOwnerSummary,
    SalsaPropertySourceSummary, SalsaPropertySummary, WorkspaceId, extend_property_owner_with_key,
};

use super::{
    CompilationIndexContext, CompilationTypeDeclId, CompilationTypeIndex, FileBackedIndex,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CompilationMemberSource {
    RuntimeMember,
    Property(SalsaPropertySourceSummary),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CompilationMemberFeature {
    FileFieldDecl,
    FileDefine,
    FileMethodDecl,
    MetaFieldDecl,
    MetaDefine,
    MetaMethodDecl,
}

impl CompilationMemberFeature {
    pub fn is_file_decl(self) -> bool {
        matches!(self, Self::FileFieldDecl | Self::FileMethodDecl)
    }

    pub fn is_meta_decl(self) -> bool {
        matches!(
            self,
            Self::MetaFieldDecl | Self::MetaMethodDecl | Self::MetaDefine
        )
    }

    pub fn is_file_define(self) -> bool {
        matches!(self, Self::FileDefine)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompilationMemberKind {
    Field,
    Function,
    Method,
    Table,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompilationMemberInfo {
    pub file_id: FileId,
    pub owner: CompilationTypeDeclId,
    pub name: SmolStr,
    pub kind: CompilationMemberKind,
    pub feature: CompilationMemberFeature,
    pub source: CompilationMemberSource,
    pub syntax_offset: Option<TextSize>,
}

#[derive(Debug, Default)]
pub struct CompilationMemberIndex {
    file_members: HashMap<FileId, Vec<CompilationMemberInfo>>,
    owner_members: HashMap<CompilationTypeDeclId, HashMap<SmolStr, Vec<CompilationMemberInfo>>>,
}

impl CompilationMemberIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_owner_members(
        &self,
        owner: &CompilationTypeDeclId,
    ) -> Option<&HashMap<SmolStr, Vec<CompilationMemberInfo>>> {
        self.owner_members.get(owner)
    }

    pub fn get_member(
        &self,
        owner: &CompilationTypeDeclId,
        name: &str,
    ) -> Option<&Vec<CompilationMemberInfo>> {
        self.owner_members.get(owner)?.get(name)
    }

    pub fn get_definition_member(
        &self,
        owner: &CompilationTypeDeclId,
        name: &str,
        meta_override_file_define: bool,
    ) -> Option<&CompilationMemberInfo> {
        let entries = self.get_member(owner, name)?;
        resolve_definition_member(entries, meta_override_file_define)
    }

    pub fn get_merged_owner_members(
        &self,
        types: &CompilationTypeIndex,
        owner: &CompilationTypeDeclId,
        meta_override_file_define: bool,
    ) -> HashMap<SmolStr, CompilationMemberInfo> {
        let mut merged = HashMap::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::from([owner.clone()]);

        while let Some(current_owner) = queue.pop_front() {
            if !visited.insert(current_owner.clone()) {
                continue;
            }

            if let Some(owner_members) = self.get_owner_members(&current_owner) {
                for (name, entries) in owner_members {
                    if merged.contains_key(name) {
                        continue;
                    }

                    if let Some(definition) =
                        resolve_definition_member(entries, meta_override_file_define)
                    {
                        merged.insert(name.clone(), definition.clone());
                    }
                }
            }

            for super_type_id in types.get_super_type_ids(&current_owner) {
                if !visited.contains(&super_type_id) {
                    queue.push_back(super_type_id);
                }
            }
        }

        merged
    }

    pub fn get_merged_member(
        &self,
        types: &CompilationTypeIndex,
        owner: &CompilationTypeDeclId,
        name: &str,
        meta_override_file_define: bool,
    ) -> Option<CompilationMemberInfo> {
        if let Some(member) = self.get_definition_member(owner, name, meta_override_file_define) {
            return Some(member.clone());
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::from(types.get_super_type_ids(owner));

        while let Some(current_owner) = queue.pop_front() {
            if !visited.insert(current_owner.clone()) {
                continue;
            }

            if let Some(member) =
                self.get_definition_member(&current_owner, name, meta_override_file_define)
            {
                return Some(member.clone());
            }

            for super_type_id in types.get_super_type_ids(&current_owner) {
                if !visited.contains(&super_type_id) {
                    queue.push_back(super_type_id);
                }
            }
        }

        None
    }

    pub fn remove(&mut self, file_id: FileId) {
        let Some(members) = self.file_members.remove(&file_id) else {
            return;
        };

        for member in members {
            let mut remove_owner = false;
            if let Some(owner_members) = self.owner_members.get_mut(&member.owner) {
                let mut remove_name = false;
                if let Some(entries) = owner_members.get_mut(&member.name) {
                    entries.retain(|existing| existing != &member);
                    remove_name = entries.is_empty();
                }
                if remove_name {
                    owner_members.remove(&member.name);
                }
                remove_owner = owner_members.is_empty();
            }

            if remove_owner {
                self.owner_members.remove(&member.owner);
            }
        }
    }

    pub fn clear(&mut self) {
        self.file_members.clear();
        self.owner_members.clear();
    }

    fn rebuild_file(
        &mut self,
        summary: &crate::SalsaSummaryHost,
        types: &CompilationTypeIndex,
        workspace_id: WorkspaceId,
        file_id: FileId,
    ) {
        let mut collected = Vec::new();
        let is_meta_file = summary
            .doc()
            .tags(file_id)
            .map(|tags| {
                tags.iter()
                    .any(|tag| tag.kind == crate::SalsaDocTagKindSummary::Meta)
            })
            .unwrap_or(false);

        if let Some(members) = summary.file().members(file_id) {
            collected.extend(collect_runtime_member_contributions(
                &members,
                types,
                file_id,
                workspace_id,
                is_meta_file,
            ));
        }

        if let Some(properties) = summary.file().properties(file_id) {
            collected.extend(collect_property_member_contributions(
                &properties,
                types,
                file_id,
                workspace_id,
                is_meta_file,
            ));
        }

        if collected.is_empty() {
            return;
        }

        for member in &collected {
            self.owner_members
                .entry(member.owner.clone())
                .or_default()
                .entry(member.name.clone())
                .or_default()
                .push(member.clone());
        }

        self.file_members.insert(file_id, collected);
    }
}

impl FileBackedIndex for CompilationMemberIndex {
    fn remove_file(&mut self, file_id: FileId) {
        CompilationMemberIndex::remove(self, file_id);
    }

    fn rebuild_file(&mut self, ctx: &CompilationIndexContext<'_>, file_id: FileId) {
        let Some(types) = ctx.types else {
            CompilationMemberIndex::remove(self, file_id);
            return;
        };

        let workspace_id = ctx
            .modules
            .and_then(|modules| {
                modules
                    .get_module(file_id)
                    .map(|module| module.workspace_id)
            })
            .unwrap_or(WorkspaceId::MAIN);

        CompilationMemberIndex::rebuild_file(self, ctx.summary, types, workspace_id, file_id);
    }

    fn clear(&mut self) {
        CompilationMemberIndex::clear(self);
    }
}

fn collect_runtime_member_contributions(
    members: &SalsaMemberIndexSummary,
    types: &CompilationTypeIndex,
    file_id: FileId,
    workspace_id: WorkspaceId,
    is_meta_file: bool,
) -> Vec<CompilationMemberInfo> {
    members
        .members
        .iter()
        .filter_map(|member| {
            let owner_name = member_owner_name(&member.target)?;
            let owner = types
                .find_type_decl(file_id, owner_name.as_str(), Some(workspace_id))?
                .id
                .clone();

            Some(CompilationMemberInfo {
                file_id,
                owner,
                name: member.target.member_name.clone(),
                kind: match member.kind {
                    SalsaMemberKindSummary::Variable => CompilationMemberKind::Field,
                    SalsaMemberKindSummary::Function => CompilationMemberKind::Function,
                    SalsaMemberKindSummary::Method => CompilationMemberKind::Method,
                },
                feature: match member.kind {
                    SalsaMemberKindSummary::Variable => {
                        if is_meta_file {
                            CompilationMemberFeature::MetaDefine
                        } else {
                            CompilationMemberFeature::FileDefine
                        }
                    }
                    SalsaMemberKindSummary::Function | SalsaMemberKindSummary::Method => {
                        if is_meta_file {
                            CompilationMemberFeature::MetaMethodDecl
                        } else {
                            CompilationMemberFeature::FileMethodDecl
                        }
                    }
                },
                source: CompilationMemberSource::RuntimeMember,
                syntax_offset: member.signature_offset.or(member.value_expr_offset()),
            })
        })
        .collect()
}

fn collect_property_member_contributions(
    properties: &SalsaPropertyIndexSummary,
    types: &CompilationTypeIndex,
    file_id: FileId,
    workspace_id: WorkspaceId,
    is_meta_file: bool,
) -> Vec<CompilationMemberInfo> {
    properties
        .properties
        .iter()
        .filter_map(|property| {
            let (owner, name) =
                resolve_property_owner_and_name(property, types, file_id, workspace_id)?;

            Some(CompilationMemberInfo {
                file_id,
                owner,
                name,
                kind: match property.kind {
                    SalsaPropertyKindSummary::Value => CompilationMemberKind::Field,
                    SalsaPropertyKindSummary::Function => CompilationMemberKind::Function,
                    SalsaPropertyKindSummary::Table => CompilationMemberKind::Table,
                },
                feature: match property.source {
                    SalsaPropertySourceSummary::DocField => {
                        if is_meta_file {
                            CompilationMemberFeature::MetaFieldDecl
                        } else {
                            CompilationMemberFeature::FileFieldDecl
                        }
                    }
                    SalsaPropertySourceSummary::TableField => {
                        if is_meta_file {
                            CompilationMemberFeature::MetaDefine
                        } else {
                            CompilationMemberFeature::FileDefine
                        }
                    }
                },
                source: CompilationMemberSource::Property(property.source.clone()),
                syntax_offset: Some(property.syntax_offset),
            })
        })
        .collect()
}

fn resolve_property_owner_and_name(
    property: &SalsaPropertySummary,
    types: &CompilationTypeIndex,
    file_id: FileId,
    workspace_id: WorkspaceId,
) -> Option<(CompilationTypeDeclId, SmolStr)> {
    match &property.owner {
        SalsaPropertyOwnerSummary::Type(type_name) => {
            let SalsaPropertyKeySummary::Name(name) = &property.key else {
                return None;
            };

            let owner = types
                .find_type_decl(file_id, type_name.as_str(), Some(workspace_id))?
                .id
                .clone();
            Some((owner, name.clone()))
        }
        owner => {
            let target = extend_property_owner_with_key(owner, &property.key)?;
            let owner_name = member_owner_name(&target)?;
            let owner = types
                .find_type_decl(file_id, owner_name.as_str(), Some(workspace_id))?
                .id
                .clone();
            Some((owner, target.member_name.clone()))
        }
    }
}

fn member_owner_name(target: &SalsaMemberTargetSummary) -> Option<SmolStr> {
    let mut segments = Vec::new();

    match &target.root {
        SalsaMemberRootSummary::Global(SalsaGlobalRootSummary::Name(name)) => {
            segments.push(name.as_str());
        }
        SalsaMemberRootSummary::LocalDecl { name, .. } => {
            segments.push(name.as_str());
        }
        SalsaMemberRootSummary::Global(SalsaGlobalRootSummary::Env) => return None,
    }

    for segment in target.owner_segments.iter() {
        segments.push(segment.as_str());
    }

    Some(SmolStr::new(segments.join(".")))
}

fn resolve_definition_member(
    entries: &[CompilationMemberInfo],
    meta_override_file_define: bool,
) -> Option<&CompilationMemberInfo> {
    if entries.is_empty() {
        return None;
    }

    if entries.iter().any(|entry| entry.feature.is_file_decl()) {
        return entries
            .iter()
            .filter(|entry| entry.feature.is_file_decl())
            .min_by_key(|entry| definition_sort_key(entry));
    }

    if meta_override_file_define && entries.iter().any(|entry| entry.feature.is_meta_decl()) {
        return entries
            .iter()
            .filter(|entry| entry.feature.is_meta_decl())
            .min_by_key(|entry| definition_sort_key(entry));
    }

    if entries.iter().any(|entry| entry.feature.is_file_define()) {
        return entries
            .iter()
            .filter(|entry| entry.feature.is_file_define())
            .min_by_key(|entry| definition_sort_key(entry));
    }

    entries
        .iter()
        .min_by_key(|entry| definition_sort_key(entry))
}

fn definition_sort_key(entry: &CompilationMemberInfo) -> (Option<TextSize>, u32) {
    (entry.syntax_offset, entry.file_id.id)
}
