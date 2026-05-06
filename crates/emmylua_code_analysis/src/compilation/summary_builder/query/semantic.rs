use std::collections::BTreeSet;

use rowan::TextSize;
use smol_str::SmolStr;

use super::super::{SalsaLookupBucket, build_lookup_buckets, find_bucket_indices};

use super::doc_owner::{
    collect_doc_owner_resolves_for_decl, collect_doc_owner_resolves_for_member,
    collect_doc_owner_resolves_for_signature,
};
use super::doc_tag::{
    SalsaDocTagQueryIndex, build_doc_tag_query_index_from_properties,
    collect_doc_tag_properties_for_owners_in_index, collect_ownerless_doc_tag_properties_in_index,
};
use super::property::{
    SalsaPropertyQueryIndex, build_property_query_index, collect_properties_for_decl_in_index,
    collect_properties_for_member_in_index,
};

use crate::{
    SalsaDeclId, SalsaDocOwnerResolutionSummary, SalsaDocOwnerResolveIndex,
    SalsaDocOwnerResolveSummary, SalsaDocOwnerSummary, SalsaDocTagPropertySummary,
    SalsaExportTargetSummary, SalsaLexicalUseIndex, SalsaLexicalUseSummary, SalsaMemberTargetId,
    SalsaMemberTargetSummary, SalsaModuleExportSummary, SalsaModuleSummary,
    SalsaPropertyIndexSummary, SalsaPropertyOwnerSummary, SalsaPropertySummary,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, salsa::Update)]
pub enum SalsaSemanticTargetSummary {
    Decl(SalsaDeclId),
    Member(SalsaMemberTargetId),
    Signature(TextSize),
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticTargetInfoSummary {
    pub target: SalsaSemanticTargetSummary,
    pub doc_owners: Vec<SalsaDocOwnerResolveSummary>,
    pub tag_properties: Vec<SalsaDocTagPropertySummary>,
    pub properties: Vec<SalsaPropertySummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaModuleExportSemanticSummary {
    pub export_target: SalsaExportTargetSummary,
    pub export: SalsaModuleExportSummary,
    pub semantic_target: Option<SalsaSemanticTargetSummary>,
    pub doc_owners: Vec<SalsaDocOwnerResolveSummary>,
    pub tag_properties: Vec<SalsaDocTagPropertySummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSingleFileSemanticSummary {
    pub file_tag_properties: Vec<SalsaDocTagPropertySummary>,
    pub required_modules: Vec<SmolStr>,
    pub targets: Vec<SalsaSemanticTargetInfoSummary>,
    pub module_export: Option<SalsaModuleExportSemanticSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSemanticTargetQueryIndex {
    pub targets: Vec<SalsaSemanticTargetInfoSummary>,
    by_decl: Vec<SalsaLookupBucket<SalsaDeclId>>,
    by_member: Vec<SalsaLookupBucket<SalsaMemberTargetId>>,
    by_signature: Vec<SalsaLookupBucket<TextSize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
struct SalsaSemanticSupportIndex {
    property_query_index: SalsaPropertyQueryIndex,
    doc_tag_query_index: SalsaDocTagQueryIndex,
    semantic_targets: Vec<SalsaSemanticTargetSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
struct SalsaSemanticResolutionIndex {
    semantic_targets: Vec<SalsaSemanticTargetSummary>,
    doc_owners: Vec<SalsaDocOwnerResolveSummary>,
    doc_owner_summaries: Vec<SalsaDocOwnerSummary>,
}

pub fn build_single_file_semantic_summary(
    properties: &SalsaPropertyIndexSummary,
    owner_resolve_index: &SalsaDocOwnerResolveIndex,
    lexical_uses: &SalsaLexicalUseIndex,
    tag_properties: &[SalsaDocTagPropertySummary],
    module: Option<&SalsaModuleSummary>,
) -> SalsaSingleFileSemanticSummary {
    let support_index =
        build_semantic_support_index(properties, owner_resolve_index, tag_properties);

    let targets = support_index
        .semantic_targets
        .iter()
        .cloned()
        .map(|target| build_target_info(target, &support_index, owner_resolve_index))
        .collect();

    let module_export = module.and_then(|module| {
        build_module_export_semantic(
            module,
            owner_resolve_index,
            &support_index.doc_tag_query_index,
        )
    });

    SalsaSingleFileSemanticSummary {
        file_tag_properties: support_index.doc_tag_query_index.properties.clone(),
        required_modules: collect_required_modules(lexical_uses),
        targets,
        module_export,
    }
}

pub fn find_semantic_decl(
    summary: &SalsaSingleFileSemanticSummary,
    decl_id: SalsaDeclId,
) -> Option<SalsaSemanticTargetInfoSummary> {
    let index = build_semantic_target_query_index(summary);
    find_semantic_decl_in_index(&index, decl_id)
}

pub fn find_semantic_member(
    summary: &SalsaSingleFileSemanticSummary,
    member_target: &SalsaMemberTargetSummary,
) -> Option<SalsaSemanticTargetInfoSummary> {
    let index = build_semantic_target_query_index(summary);
    find_semantic_member_in_index(&index, member_target)
}

pub fn find_semantic_signature(
    summary: &SalsaSingleFileSemanticSummary,
    signature_offset: TextSize,
) -> Option<SalsaSemanticTargetInfoSummary> {
    let index = build_semantic_target_query_index(summary);
    find_semantic_signature_in_index(&index, signature_offset)
}

pub fn find_module_export_semantic(
    summary: &SalsaSingleFileSemanticSummary,
) -> Option<SalsaModuleExportSemanticSummary> {
    summary.module_export.clone()
}

pub fn build_semantic_target_query_index(
    summary: &SalsaSingleFileSemanticSummary,
) -> SalsaSemanticTargetQueryIndex {
    let targets = summary.targets.clone();
    let mut decl_entries = Vec::new();
    let mut member_entries = Vec::new();
    let mut signature_entries = Vec::new();

    for (index, target) in targets.iter().enumerate() {
        match &target.target {
            SalsaSemanticTargetSummary::Decl(decl_id) => decl_entries.push((*decl_id, index)),
            SalsaSemanticTargetSummary::Member(member_target) => {
                member_entries.push((member_target.clone(), index));
            }
            SalsaSemanticTargetSummary::Signature(signature_offset) => {
                signature_entries.push((*signature_offset, index));
            }
        }
    }

    SalsaSemanticTargetQueryIndex {
        targets,
        by_decl: build_lookup_buckets(decl_entries),
        by_member: build_lookup_buckets(member_entries),
        by_signature: build_lookup_buckets(signature_entries),
    }
}

pub fn find_semantic_decl_in_index(
    index: &SalsaSemanticTargetQueryIndex,
    decl_id: SalsaDeclId,
) -> Option<SalsaSemanticTargetInfoSummary> {
    find_semantic_target(index, find_bucket_indices(&index.by_decl, &decl_id))
}

pub fn find_semantic_member_in_index(
    index: &SalsaSemanticTargetQueryIndex,
    member_target: &SalsaMemberTargetSummary,
) -> Option<SalsaSemanticTargetInfoSummary> {
    let member_target = SalsaMemberTargetId::from(member_target);
    find_semantic_target(index, find_bucket_indices(&index.by_member, &member_target))
}

pub fn find_semantic_signature_in_index(
    index: &SalsaSemanticTargetQueryIndex,
    signature_offset: TextSize,
) -> Option<SalsaSemanticTargetInfoSummary> {
    find_semantic_target(
        index,
        find_bucket_indices(&index.by_signature, &signature_offset),
    )
}

fn build_target_info(
    target: SalsaSemanticTargetSummary,
    support_index: &SalsaSemanticSupportIndex,
    owner_resolve_index: &SalsaDocOwnerResolveIndex,
) -> SalsaSemanticTargetInfoSummary {
    let semantic_index = build_semantic_resolution_index([target.clone()], owner_resolve_index);
    let tag_properties =
        tag_properties_for_semantic_index(&semantic_index, &support_index.doc_tag_query_index);
    let properties = properties_for_target(&support_index.property_query_index, &target);

    SalsaSemanticTargetInfoSummary {
        target,
        doc_owners: semantic_index.doc_owners,
        tag_properties,
        properties,
    }
}

fn build_module_export_semantic(
    module: &SalsaModuleSummary,
    owner_resolve_index: &SalsaDocOwnerResolveIndex,
    tag_properties: &SalsaDocTagQueryIndex,
) -> Option<SalsaModuleExportSemanticSummary> {
    let export_target = module.export_target.clone()?;
    let export = module.export.clone()?;
    let semantic_index = build_semantic_resolution_index(
        semantic_targets_from_module_export(&export),
        owner_resolve_index,
    );
    let semantic_target = semantic_index.semantic_targets.first().cloned();
    let tag_properties = module_export_tag_properties(&semantic_index, tag_properties);

    Some(SalsaModuleExportSemanticSummary {
        export_target,
        export,
        semantic_target,
        doc_owners: semantic_index.doc_owners,
        tag_properties,
    })
}

fn build_semantic_support_index(
    properties: &SalsaPropertyIndexSummary,
    owner_resolve_index: &SalsaDocOwnerResolveIndex,
    tag_properties: &[SalsaDocTagPropertySummary],
) -> SalsaSemanticSupportIndex {
    let property_query_index = build_property_query_index(properties);
    let doc_tag_query_index = build_doc_tag_query_index_from_properties(tag_properties);
    let semantic_targets = collect_semantic_targets(properties, owner_resolve_index);

    SalsaSemanticSupportIndex {
        property_query_index,
        doc_tag_query_index,
        semantic_targets,
    }
}

fn collect_semantic_targets(
    properties: &SalsaPropertyIndexSummary,
    owner_resolve_index: &SalsaDocOwnerResolveIndex,
) -> Vec<SalsaSemanticTargetSummary> {
    let mut targets = owner_resolve_index
        .bindings
        .iter()
        .filter_map(|resolve| semantic_target_from_resolution(&resolve.resolution))
        .collect::<BTreeSet<_>>();

    targets.extend(
        properties
            .properties
            .iter()
            .filter_map(semantic_target_from_property),
    );

    targets.into_iter().collect()
}

fn build_semantic_resolution_index<I>(
    semantic_targets: I,
    owner_resolve_index: &SalsaDocOwnerResolveIndex,
) -> SalsaSemanticResolutionIndex
where
    I: IntoIterator<Item = SalsaSemanticTargetSummary>,
{
    let semantic_targets = semantic_targets
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut doc_owners = Vec::new();
    let mut doc_owner_summaries = Vec::new();
    let mut seen_owners = BTreeSet::new();

    for target in &semantic_targets {
        for resolve in owner_resolves_for_target(owner_resolve_index, target) {
            let owner_summary = resolve_owner_summary(&resolve);
            if seen_owners.insert(owner_summary.clone()) {
                doc_owner_summaries.push(owner_summary);
                doc_owners.push(resolve);
            }
        }
    }

    SalsaSemanticResolutionIndex {
        semantic_targets,
        doc_owners,
        doc_owner_summaries,
    }
}

fn semantic_target_from_resolution(
    resolution: &SalsaDocOwnerResolutionSummary,
) -> Option<SalsaSemanticTargetSummary> {
    match resolution {
        SalsaDocOwnerResolutionSummary::Decl(decl_id) => {
            Some(SalsaSemanticTargetSummary::Decl(*decl_id))
        }
        SalsaDocOwnerResolutionSummary::Member(target) => {
            Some(SalsaSemanticTargetSummary::Member(target.clone()))
        }
        SalsaDocOwnerResolutionSummary::Signature(signature_offset) => {
            Some(SalsaSemanticTargetSummary::Signature(*signature_offset))
        }
        SalsaDocOwnerResolutionSummary::Unresolved
        | SalsaDocOwnerResolutionSummary::Ambiguous(_) => None,
    }
}

fn semantic_target_from_property(
    property: &SalsaPropertySummary,
) -> Option<SalsaSemanticTargetSummary> {
    match &property.owner {
        SalsaPropertyOwnerSummary::Decl { decl_id, .. } => {
            Some(SalsaSemanticTargetSummary::Decl(*decl_id))
        }
        SalsaPropertyOwnerSummary::Member(target) => {
            Some(SalsaSemanticTargetSummary::Member(target.clone()))
        }
        SalsaPropertyOwnerSummary::Type(_) => None,
    }
}

fn semantic_targets_from_module_export(
    export: &SalsaModuleExportSummary,
) -> Vec<SalsaSemanticTargetSummary> {
    let mut targets = Vec::new();

    match export {
        SalsaModuleExportSummary::LocalDecl { decl_id, .. } => {
            targets.push(SalsaSemanticTargetSummary::Decl(*decl_id));
        }
        SalsaModuleExportSummary::Member(member) => {
            targets.push(SalsaSemanticTargetSummary::Member(member.target.clone()));
        }
        SalsaModuleExportSummary::GlobalVariable(variable) => {
            targets.push(SalsaSemanticTargetSummary::Decl(variable.decl_id));
        }
        SalsaModuleExportSummary::GlobalFunction(function) => {
            if let Some(decl_id) = function.decl_id {
                targets.push(SalsaSemanticTargetSummary::Decl(decl_id));
            }
            targets.push(SalsaSemanticTargetSummary::Signature(
                function.signature_offset,
            ));
        }
        SalsaModuleExportSummary::Closure { signature_offset } => {
            targets.push(SalsaSemanticTargetSummary::Signature(*signature_offset));
        }
        SalsaModuleExportSummary::Table { .. } => {}
    }

    targets.sort();
    targets.dedup();
    targets
}

fn owner_resolves_for_target(
    owner_resolve_index: &SalsaDocOwnerResolveIndex,
    target: &SalsaSemanticTargetSummary,
) -> Vec<SalsaDocOwnerResolveSummary> {
    match target {
        SalsaSemanticTargetSummary::Decl(decl_id) => {
            collect_doc_owner_resolves_for_decl(owner_resolve_index, *decl_id)
        }
        SalsaSemanticTargetSummary::Member(member_target) => {
            collect_doc_owner_resolves_for_member(owner_resolve_index, member_target)
        }
        SalsaSemanticTargetSummary::Signature(signature_offset) => {
            collect_doc_owner_resolves_for_signature(owner_resolve_index, *signature_offset)
        }
    }
}

fn tag_properties_for_semantic_index(
    semantic_index: &SalsaSemanticResolutionIndex,
    tag_properties: &SalsaDocTagQueryIndex,
) -> Vec<SalsaDocTagPropertySummary> {
    collect_doc_tag_properties_for_owners_in_index(
        tag_properties,
        &semantic_index.doc_owner_summaries,
    )
}

fn module_export_tag_properties(
    semantic_index: &SalsaSemanticResolutionIndex,
    tag_properties: &SalsaDocTagQueryIndex,
) -> Vec<SalsaDocTagPropertySummary> {
    let mut collected = collect_ownerless_doc_tag_properties_in_index(tag_properties);
    let mut seen_owners = collected
        .iter()
        .map(|property| property.owner.clone())
        .collect::<BTreeSet<_>>();
    let matched_owner_properties = collect_doc_tag_properties_for_owners_in_index(
        tag_properties,
        &semantic_index.doc_owner_summaries,
    );

    if matched_owner_properties.is_empty() {
        let owner_properties = tag_properties
            .properties
            .iter()
            .filter(|property| property.owner.syntax_offset.is_some())
            .collect::<Vec<_>>();
        if owner_properties.len() == 1 && seen_owners.insert(owner_properties[0].owner.clone()) {
            collected.push(owner_properties[0].clone());
        }
        return collected;
    }

    for property in matched_owner_properties {
        if seen_owners.insert(property.owner.clone()) {
            collected.push(property);
        }
    }

    collected
}

fn collect_required_modules(lexical_uses: &SalsaLexicalUseIndex) -> Vec<SmolStr> {
    let mut required_modules = Vec::new();
    let mut seen_modules = BTreeSet::new();

    for use_summary in &lexical_uses.uses {
        let SalsaLexicalUseSummary::Call {
            require_path: Some(require_path),
            ..
        } = use_summary
        else {
            continue;
        };

        if seen_modules.insert(require_path.clone()) {
            required_modules.push(require_path.clone());
        }
    }

    required_modules
}

fn properties_for_target(
    properties: &SalsaPropertyQueryIndex,
    target: &SalsaSemanticTargetSummary,
) -> Vec<SalsaPropertySummary> {
    match target {
        SalsaSemanticTargetSummary::Decl(decl_id) => {
            collect_properties_for_decl_in_index(properties, *decl_id)
        }
        SalsaSemanticTargetSummary::Member(member_target) => {
            collect_properties_for_member_in_index(properties, member_target)
        }
        SalsaSemanticTargetSummary::Signature(_) => Vec::new(),
    }
}

fn resolve_owner_summary(resolve: &SalsaDocOwnerResolveSummary) -> SalsaDocOwnerSummary {
    SalsaDocOwnerSummary {
        kind: resolve.owner_kind.clone(),
        syntax_offset: Some(resolve.owner_offset),
    }
}

fn find_semantic_target(
    index: &SalsaSemanticTargetQueryIndex,
    indices: Option<&[usize]>,
) -> Option<SalsaSemanticTargetInfoSummary> {
    indices
        .and_then(|indices| indices.first().copied())
        .map(|target_index| index.targets[target_index].clone())
}
