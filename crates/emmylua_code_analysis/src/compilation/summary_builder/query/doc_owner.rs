use rowan::TextSize;

use super::super::{SalsaLookupBucket, build_lookup_buckets, find_bucket_indices};

use crate::{
    SalsaBindingTargetSummary, SalsaDeclId, SalsaDocOwnerBindingIndexSummary,
    SalsaDocOwnerBindingSummary, SalsaDocOwnerKindSummary, SalsaMemberTargetId,
    SalsaMemberTargetSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocOwnerResolutionSummary {
    Unresolved,
    Decl(SalsaDeclId),
    Member(SalsaMemberTargetId),
    Signature(TextSize),
    Ambiguous(Vec<SalsaBindingTargetSummary>),
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocOwnerResolveSummary {
    pub owner_kind: SalsaDocOwnerKindSummary,
    pub owner_offset: TextSize,
    pub resolution: SalsaDocOwnerResolutionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocOwnerResolveIndex {
    pub bindings: Vec<SalsaDocOwnerResolveSummary>,
    by_owner_offset: Vec<SalsaLookupBucket<TextSize>>,
    by_decl: Vec<SalsaLookupBucket<SalsaDeclId>>,
    by_member: Vec<SalsaLookupBucket<SalsaMemberTargetId>>,
    by_signature: Vec<SalsaLookupBucket<TextSize>>,
}

pub fn build_doc_owner_resolve_index(
    owner_bindings: &SalsaDocOwnerBindingIndexSummary,
) -> SalsaDocOwnerResolveIndex {
    let bindings = owner_bindings
        .bindings
        .iter()
        .map(resolve_doc_owner_binding)
        .collect::<Vec<_>>();
    let mut owner_offset_entries = Vec::with_capacity(bindings.len());
    let mut decl_entries = Vec::new();
    let mut member_entries = Vec::new();
    let mut signature_entries = Vec::new();

    for (index, binding) in bindings.iter().enumerate() {
        owner_offset_entries.push((binding.owner_offset, index));

        match &binding.resolution {
            SalsaDocOwnerResolutionSummary::Decl(decl_id) => {
                decl_entries.push((*decl_id, index));
            }
            SalsaDocOwnerResolutionSummary::Member(target) => {
                member_entries.push((target.clone(), index));
            }
            SalsaDocOwnerResolutionSummary::Signature(signature_offset) => {
                signature_entries.push((*signature_offset, index));
            }
            SalsaDocOwnerResolutionSummary::Unresolved
            | SalsaDocOwnerResolutionSummary::Ambiguous(_) => {}
        }
    }

    SalsaDocOwnerResolveIndex {
        bindings,
        by_owner_offset: build_lookup_buckets(owner_offset_entries),
        by_decl: build_lookup_buckets(decl_entries),
        by_member: build_lookup_buckets(member_entries),
        by_signature: build_lookup_buckets(signature_entries),
    }
}

pub fn find_doc_owner_resolve_at(
    resolve_index: &SalsaDocOwnerResolveIndex,
    owner_offset: TextSize,
) -> Option<SalsaDocOwnerResolveSummary> {
    find_bucket_indices(&resolve_index.by_owner_offset, &owner_offset)
        .and_then(|indices| indices.first().copied())
        .map(|index| resolve_index.bindings[index].clone())
}

pub fn collect_doc_owner_resolves_for_decl(
    resolve_index: &SalsaDocOwnerResolveIndex,
    decl_id: SalsaDeclId,
) -> Vec<SalsaDocOwnerResolveSummary> {
    collect_resolves(
        resolve_index,
        find_bucket_indices(&resolve_index.by_decl, &decl_id),
    )
}

pub fn collect_doc_owner_resolves_for_member(
    resolve_index: &SalsaDocOwnerResolveIndex,
    member_target: &SalsaMemberTargetSummary,
) -> Vec<SalsaDocOwnerResolveSummary> {
    let member_target = SalsaMemberTargetId::from(member_target);
    collect_resolves(
        resolve_index,
        find_bucket_indices(&resolve_index.by_member, &member_target),
    )
}

pub fn collect_doc_owner_resolves_for_signature(
    resolve_index: &SalsaDocOwnerResolveIndex,
    signature_offset: TextSize,
) -> Vec<SalsaDocOwnerResolveSummary> {
    collect_resolves(
        resolve_index,
        find_bucket_indices(&resolve_index.by_signature, &signature_offset),
    )
}

fn collect_resolves(
    resolve_index: &SalsaDocOwnerResolveIndex,
    indices: Option<&[usize]>,
) -> Vec<SalsaDocOwnerResolveSummary> {
    indices
        .into_iter()
        .flatten()
        .map(|index| resolve_index.bindings[*index].clone())
        .collect()
}

fn resolve_doc_owner_binding(binding: &SalsaDocOwnerBindingSummary) -> SalsaDocOwnerResolveSummary {
    let resolution = match binding.targets.as_slice() {
        [] => SalsaDocOwnerResolutionSummary::Unresolved,
        [SalsaBindingTargetSummary::Decl(decl_id)] => {
            SalsaDocOwnerResolutionSummary::Decl(*decl_id)
        }
        [SalsaBindingTargetSummary::Member(target)] => {
            SalsaDocOwnerResolutionSummary::Member(target.clone())
        }
        [SalsaBindingTargetSummary::Signature(signature_offset)] => {
            SalsaDocOwnerResolutionSummary::Signature(*signature_offset)
        }
        _ => SalsaDocOwnerResolutionSummary::Ambiguous(binding.targets.clone()),
    };

    SalsaDocOwnerResolveSummary {
        owner_kind: binding.owner_kind.clone(),
        owner_offset: binding.owner_offset,
        resolution,
    }
}
