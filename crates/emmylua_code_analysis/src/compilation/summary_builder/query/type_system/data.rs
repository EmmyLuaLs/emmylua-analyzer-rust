use std::collections::BTreeMap;

use rowan::TextSize;
use smol_str::SmolStr;

use crate::compilation::summary_builder::query::{
    SalsaDocOwnerResolveIndex, collect_doc_owner_resolves_for_decl,
    collect_doc_owner_resolves_for_member,
};
use crate::compilation::summary_builder::{
    SalsaLookupBucket, build_lookup_buckets, find_bucket_indices,
};
use crate::extend_property_owner_with_key;
use crate::{
    SalsaDeclId, SalsaDeclKindSummary, SalsaDeclSummary, SalsaDeclTreeSummary, SalsaDocSummary,
    SalsaDocTypeNodeKey, SalsaGlobalSummary, SalsaMemberIndexSummary, SalsaMemberTargetId,
    SalsaMemberTargetSummary, SalsaMemberUseSummary, SalsaNameUseResolutionSummary,
    SalsaNameUseSummary, SalsaPropertyIndexSummary, SalsaPropertySummary,
    SalsaSignatureIndexSummary, SalsaSyntaxIdSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub enum SalsaTypeCandidateOriginSummary {
    Decl(SalsaDeclId),
    GlobalFunction(TextSize),
    Member(SalsaMemberTargetId),
    Property(TextSize),
    Assignment(TextSize),
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaTypeCandidateSummary {
    pub origin: SalsaTypeCandidateOriginSummary,
    pub explicit_type_offsets: Vec<SalsaDocTypeNodeKey>,
    pub named_type_names: Vec<SmolStr>,
    pub initializer_offset: Option<TextSize>,
    pub value_expr_syntax_id: Option<SalsaSyntaxIdSummary>,
    pub value_result_index: usize,
    pub source_call_syntax_id: Option<SalsaSyntaxIdSummary>,
    pub signature_offset: Option<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDeclTypeInfoSummary {
    pub decl_id: SalsaDeclId,
    pub name: SmolStr,
    pub explicit_type_offsets: Vec<SalsaDocTypeNodeKey>,
    pub named_type_names: Vec<SmolStr>,
    pub initializer_offset: Option<TextSize>,
    pub value_expr_syntax_id: Option<SalsaSyntaxIdSummary>,
    pub value_result_index: usize,
    pub source_call_syntax_id: Option<SalsaSyntaxIdSummary>,
    pub signature_offset: Option<TextSize>,
    pub value_signature_offset: Option<TextSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaNameTypeInfoSummary {
    pub name_use: SalsaNameUseSummary,
    pub decl_type: Option<SalsaDeclTypeInfoSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaGlobalTypeInfoSummary {
    pub name: SmolStr,
    pub candidates: Vec<SalsaTypeCandidateSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaMemberTypeInfoSummary {
    pub target: SalsaMemberTargetId,
    pub candidates: Vec<SalsaTypeCandidateSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaProgramPointTypeInfoSummary {
    pub syntax_offset: TextSize,
    pub program_point_offset: TextSize,
    pub name_use: SalsaNameUseSummary,
    pub base_decl_type: Option<SalsaDeclTypeInfoSummary>,
    pub candidates: Vec<SalsaTypeCandidateSummary>,
    pub active_narrows: Vec<SalsaTypeNarrowSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaProgramPointMemberTypeInfoSummary {
    pub syntax_offset: TextSize,
    pub program_point_offset: TextSize,
    pub member_use: SalsaMemberUseSummary,
    pub base_member_type: Option<SalsaMemberTypeInfoSummary>,
    pub owner_decl_id: Option<SalsaDeclId>,
    pub owner_candidates: Vec<SalsaTypeCandidateSummary>,
    pub candidates: Vec<SalsaTypeCandidateSummary>,
    pub active_narrows: Vec<SalsaTypeNarrowSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaTypeNarrowSummary {
    Truthy,
    Falsey,
    Literal {
        literal_text: SmolStr,
        positive: bool,
    },
    TypeGuard {
        type_name: SmolStr,
    },
    ExcludeTypeGuard {
        type_name: SmolStr,
    },
    FieldLiteral {
        member_name: SmolStr,
        literal_text: SmolStr,
        positive: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDeclTypeQueryIndex {
    pub decls: Vec<SalsaDeclTypeInfoSummary>,
    pub(crate) by_decl_id: Vec<SalsaLookupBucket<SalsaDeclId>>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaGlobalTypeQueryIndex {
    pub globals: Vec<SalsaGlobalTypeInfoSummary>,
    pub(crate) by_name: Vec<SalsaLookupBucket<SmolStr>>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaMemberTypeQueryIndex {
    pub members: Vec<SalsaMemberTypeInfoSummary>,
    pub(crate) by_target: Vec<SalsaLookupBucket<SalsaMemberTargetId>>,
}

pub fn build_decl_type_query_index(
    decl_tree: &SalsaDeclTreeSummary,
    doc: &SalsaDocSummary,
    signatures: &SalsaSignatureIndexSummary,
    owner_resolve_index: &SalsaDocOwnerResolveIndex,
) -> SalsaDeclTypeQueryIndex {
    let decls = decl_tree
        .decls
        .iter()
        .map(|decl| build_decl_type_info(decl, doc, signatures, owner_resolve_index))
        .collect::<Vec<_>>();
    let by_decl_id = build_lookup_buckets(
        decls
            .iter()
            .enumerate()
            .map(|(index, decl)| (decl.decl_id, index))
            .collect(),
    );

    SalsaDeclTypeQueryIndex { decls, by_decl_id }
}

pub fn find_decl_type_info(
    index: &SalsaDeclTypeQueryIndex,
    decl_id: SalsaDeclId,
) -> Option<SalsaDeclTypeInfoSummary> {
    find_bucket_indices(&index.by_decl_id, &decl_id)
        .and_then(|indices| indices.first().copied())
        .and_then(|entry_index| index.decls.get(entry_index).cloned())
}

pub fn find_name_type_info(
    index: &SalsaDeclTypeQueryIndex,
    name_use: &SalsaNameUseSummary,
) -> SalsaNameTypeInfoSummary {
    let decl_type = match name_use.resolution {
        SalsaNameUseResolutionSummary::LocalDecl(decl_id) => find_decl_type_info(index, decl_id),
        SalsaNameUseResolutionSummary::Global => None,
    };

    SalsaNameTypeInfoSummary {
        name_use: name_use.clone(),
        decl_type,
    }
}

pub fn build_global_type_query_index(
    globals: &SalsaGlobalSummary,
    decl_types: &SalsaDeclTypeQueryIndex,
) -> SalsaGlobalTypeQueryIndex {
    let mut by_name = BTreeMap::<SmolStr, Vec<SalsaTypeCandidateSummary>>::new();

    for entry in &globals.entries {
        let candidates = by_name.entry(entry.name.clone()).or_default();
        for decl_id in &entry.decl_ids {
            if let Some(decl_type) = find_decl_type_info(decl_types, *decl_id) {
                candidates.push(decl_type_to_candidate(&decl_type));
            }
        }
    }

    for function in &globals.functions {
        by_name
            .entry(function.name.clone())
            .or_default()
            .push(SalsaTypeCandidateSummary {
                origin: SalsaTypeCandidateOriginSummary::GlobalFunction(function.signature_offset),
                explicit_type_offsets: Vec::new(),
                named_type_names: Vec::new(),
                initializer_offset: None,
                value_expr_syntax_id: None,
                value_result_index: 0,
                source_call_syntax_id: None,
                signature_offset: Some(function.signature_offset),
            });
    }

    let globals = by_name
        .into_iter()
        .map(|(name, candidates)| SalsaGlobalTypeInfoSummary { name, candidates })
        .collect::<Vec<_>>();
    let by_name = build_lookup_buckets(
        globals
            .iter()
            .enumerate()
            .map(|(index, global)| (global.name.clone(), index))
            .collect(),
    );

    SalsaGlobalTypeQueryIndex { globals, by_name }
}

pub fn find_global_type_info(
    index: &SalsaGlobalTypeQueryIndex,
    name: &SmolStr,
) -> Option<SalsaGlobalTypeInfoSummary> {
    find_bucket_indices(&index.by_name, name)
        .and_then(|indices| indices.first().copied())
        .and_then(|entry_index| index.globals.get(entry_index).cloned())
}

pub fn find_global_name_type_info(
    index: &SalsaGlobalTypeQueryIndex,
    name_use: &SalsaNameUseSummary,
) -> Option<SalsaGlobalTypeInfoSummary> {
    match name_use.resolution {
        SalsaNameUseResolutionSummary::Global => find_global_type_info(index, &name_use.name),
        SalsaNameUseResolutionSummary::LocalDecl(_) => None,
    }
}

pub fn build_member_type_query_index(
    members: &SalsaMemberIndexSummary,
    properties: &SalsaPropertyIndexSummary,
    signatures: &SalsaSignatureIndexSummary,
    doc: &SalsaDocSummary,
    owner_resolve_index: &SalsaDocOwnerResolveIndex,
) -> SalsaMemberTypeQueryIndex {
    let mut by_target = BTreeMap::<SalsaMemberTargetId, Vec<SalsaTypeCandidateSummary>>::new();

    for member in &members.members {
        by_target
            .entry(member.target.clone())
            .or_default()
            .push(SalsaTypeCandidateSummary {
                origin: SalsaTypeCandidateOriginSummary::Member(member.target.clone()),
                explicit_type_offsets: Vec::new(),
                named_type_names: Vec::new(),
                initializer_offset: member.value_expr_offset(),
                value_expr_syntax_id: member.value_expr_syntax_id,
                value_result_index: member.value_result_index,
                source_call_syntax_id: member.source_call_syntax_id,
                signature_offset: member.signature_offset.or_else(|| {
                    find_signature_initializer_offset(signatures, member.value_expr_offset())
                }),
            });
    }

    for property in &properties.properties {
        let Some(target) = extend_property_owner_with_key(&property.owner, &property.key) else {
            continue;
        };

        by_target
            .entry(target)
            .or_default()
            .push(property_to_candidate_with_signatures(
                property.clone(),
                signatures,
            ));
    }

    for member in &members.members {
        let candidates = by_target.entry(member.target.clone()).or_default();
        for resolve in collect_doc_owner_resolves_for_member(owner_resolve_index, &member.target) {
            let explicit_type_offsets = doc
                .type_tags
                .iter()
                .filter(|tag| tag.owner.syntax_offset == Some(resolve.owner_offset))
                .flat_map(|tag| tag.type_offsets.iter().copied())
                .collect::<Vec<_>>();
            if explicit_type_offsets.is_empty() {
                continue;
            }

            candidates.push(SalsaTypeCandidateSummary {
                origin: SalsaTypeCandidateOriginSummary::Property(resolve.owner_offset),
                explicit_type_offsets,
                named_type_names: collect_named_type_names_for_owner(resolve.owner_offset, doc),
                initializer_offset: None,
                value_expr_syntax_id: None,
                value_result_index: 0,
                source_call_syntax_id: None,
                signature_offset: None,
            });
        }
    }

    let members = by_target
        .into_iter()
        .map(|(target, candidates)| SalsaMemberTypeInfoSummary { target, candidates })
        .collect::<Vec<_>>();
    let by_target = build_lookup_buckets(
        members
            .iter()
            .enumerate()
            .map(|(index, member)| (member.target.clone(), index))
            .collect(),
    );

    SalsaMemberTypeQueryIndex { members, by_target }
}

pub fn find_member_type_info(
    index: &SalsaMemberTypeQueryIndex,
    target: &SalsaMemberTargetSummary,
) -> Option<SalsaMemberTypeInfoSummary> {
    let target = SalsaMemberTargetId::from(target);
    find_bucket_indices(&index.by_target, &target)
        .and_then(|indices| indices.first().copied())
        .and_then(|entry_index| index.members.get(entry_index).cloned())
}

pub fn find_member_use_type_info(
    index: &SalsaMemberTypeQueryIndex,
    member_use: &SalsaMemberUseSummary,
) -> Option<SalsaMemberTypeInfoSummary> {
    find_member_type_info(index, &member_use.target)
}

fn build_decl_type_info(
    decl: &SalsaDeclSummary,
    doc: &SalsaDocSummary,
    signatures: &SalsaSignatureIndexSummary,
    owner_resolve_index: &SalsaDocOwnerResolveIndex,
) -> SalsaDeclTypeInfoSummary {
    SalsaDeclTypeInfoSummary {
        decl_id: decl.id,
        name: decl.name.clone(),
        explicit_type_offsets: collect_decl_explicit_type_offsets(
            decl,
            doc,
            signatures,
            owner_resolve_index,
        ),
        named_type_names: collect_decl_named_type_names(decl, doc, owner_resolve_index),
        initializer_offset: decl.value_expr_offset(),
        value_expr_syntax_id: decl.value_expr_syntax_id,
        value_result_index: decl.value_result_index,
        source_call_syntax_id: decl.source_call_syntax_id,
        signature_offset: match decl.kind {
            SalsaDeclKindSummary::Param {
                signature_offset, ..
            } => Some(signature_offset),
            _ => None,
        },
        value_signature_offset: find_signature_initializer_offset(
            signatures,
            decl.value_expr_offset(),
        ),
    }
}

fn find_signature_initializer_offset(
    signatures: &SalsaSignatureIndexSummary,
    initializer_offset: Option<TextSize>,
) -> Option<TextSize> {
    initializer_offset.filter(|offset| {
        signatures
            .signatures
            .iter()
            .any(|signature| signature.syntax_offset == *offset)
    })
}

fn collect_decl_explicit_type_offsets(
    decl: &SalsaDeclSummary,
    doc: &SalsaDocSummary,
    signatures: &SalsaSignatureIndexSummary,
    owner_resolve_index: &SalsaDocOwnerResolveIndex,
) -> Vec<SalsaDocTypeNodeKey> {
    let mut collected = std::collections::BTreeSet::new();

    match &decl.kind {
        SalsaDeclKindSummary::Param {
            signature_offset,
            idx,
        } => {
            if let Some(signature) = signatures
                .signatures
                .iter()
                .find(|signature| signature.syntax_offset == *signature_offset)
                && let Some(param) = signature.params.get(*idx)
            {
                for doc_param in &doc.params {
                    if doc_param.owner.syntax_offset == Some(signature.owner_offset)
                        && doc_param.name == param.name
                        && let Some(type_offset) = doc_param.type_offset
                    {
                        collected.insert(type_offset);
                    }
                }
            }
        }
        SalsaDeclKindSummary::Local { .. } | SalsaDeclKindSummary::Global => {
            for resolve in collect_doc_owner_resolves_for_decl(owner_resolve_index, decl.id) {
                for tag in &doc.type_tags {
                    if tag.owner.syntax_offset == Some(resolve.owner_offset) {
                        for type_offset in &tag.type_offsets {
                            collected.insert(*type_offset);
                        }
                    }
                }
            }
        }
        SalsaDeclKindSummary::ImplicitSelf => {}
    }

    collected.into_iter().collect()
}

fn collect_decl_named_type_names(
    decl: &SalsaDeclSummary,
    doc: &SalsaDocSummary,
    owner_resolve_index: &SalsaDocOwnerResolveIndex,
) -> Vec<SmolStr> {
    let mut collected = std::collections::BTreeSet::new();

    if matches!(
        decl.kind,
        SalsaDeclKindSummary::Local { .. } | SalsaDeclKindSummary::Global
    ) {
        for resolve in collect_doc_owner_resolves_for_decl(owner_resolve_index, decl.id) {
            for type_name in collect_named_type_names_for_owner(resolve.owner_offset, doc) {
                collected.insert(type_name);
            }
        }
    }

    collected.into_iter().collect()
}

fn collect_named_type_names_for_owner(
    owner_offset: TextSize,
    doc: &SalsaDocSummary,
) -> Vec<SmolStr> {
    doc.type_defs
        .iter()
        .filter(|type_def| {
            type_def.owner.syntax_offset == Some(owner_offset)
                && matches!(
                    type_def.kind,
                    crate::SalsaDocTypeDefKindSummary::Class
                        | crate::SalsaDocTypeDefKindSummary::Enum
                )
        })
        .map(|type_def| type_def.name.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn decl_type_to_candidate(
    decl_type: &SalsaDeclTypeInfoSummary,
) -> SalsaTypeCandidateSummary {
    SalsaTypeCandidateSummary {
        origin: SalsaTypeCandidateOriginSummary::Decl(decl_type.decl_id),
        explicit_type_offsets: decl_type.explicit_type_offsets.clone(),
        named_type_names: decl_type.named_type_names.clone(),
        initializer_offset: decl_type.initializer_offset,
        value_expr_syntax_id: decl_type.value_expr_syntax_id,
        value_result_index: decl_type.value_result_index,
        source_call_syntax_id: decl_type.source_call_syntax_id,
        signature_offset: decl_type.value_signature_offset,
    }
}

pub(crate) fn property_to_candidate(property: SalsaPropertySummary) -> SalsaTypeCandidateSummary {
    SalsaTypeCandidateSummary {
        origin: SalsaTypeCandidateOriginSummary::Property(property.syntax_offset),
        explicit_type_offsets: property.doc_type_offset.into_iter().collect(),
        named_type_names: Vec::new(),
        initializer_offset: property.value_expr_offset,
        value_expr_syntax_id: property.value_expr_syntax_id,
        value_result_index: property.value_result_index,
        source_call_syntax_id: property.source_call_syntax_id,
        signature_offset: None,
    }
}

pub(crate) fn property_to_candidate_with_signatures(
    property: SalsaPropertySummary,
    signatures: &SalsaSignatureIndexSummary,
) -> SalsaTypeCandidateSummary {
    SalsaTypeCandidateSummary {
        origin: SalsaTypeCandidateOriginSummary::Property(property.syntax_offset),
        explicit_type_offsets: property.doc_type_offset.into_iter().collect(),
        named_type_names: Vec::new(),
        initializer_offset: property.value_expr_offset,
        value_expr_syntax_id: property.value_expr_syntax_id,
        value_result_index: property.value_result_index,
        source_call_syntax_id: property.source_call_syntax_id,
        signature_offset: find_signature_initializer_offset(signatures, property.value_expr_offset),
    }
}

pub(crate) fn dedupe_type_candidates(
    candidates: Vec<SalsaTypeCandidateSummary>,
) -> Vec<SalsaTypeCandidateSummary> {
    let mut seen = std::collections::BTreeSet::new();
    candidates
        .into_iter()
        .filter(|candidate| {
            seen.insert((
                candidate.origin.clone(),
                candidate.explicit_type_offsets.clone(),
                candidate.named_type_names.clone(),
                candidate.initializer_offset,
                candidate.value_expr_syntax_id,
                candidate.value_result_index,
                candidate.source_call_syntax_id,
                candidate.signature_offset,
            ))
        })
        .collect()
}
