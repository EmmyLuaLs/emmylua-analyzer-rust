use std::collections::BTreeSet;

use smol_str::SmolStr;

use super::data::{SalsaTypeCandidateSummary, SalsaTypeNarrowSummary};
use crate::compilation::summary_builder::query::{
    SalsaDocTypeLoweredIndex, SalsaDocTypeLoweredKind, SalsaDocTypeRef, SalsaPropertyQueryIndex,
    collect_properties_for_type_and_key_in_index, find_resolved_doc_type_by_key_from_parts,
};
use crate::{
    SalsaDocTypeBinaryOperatorSummary, SalsaDocTypeIndexSummary, SalsaDocTypeKindSummary,
    SalsaDocTypeNodeKey,
};

pub(crate) fn apply_narrows_to_candidate(
    mut candidate: SalsaTypeCandidateSummary,
    property_query_index: &SalsaPropertyQueryIndex,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    active_narrows: &[SalsaTypeNarrowSummary],
) -> SalsaTypeCandidateSummary {
    let original_offsets = candidate.explicit_type_offsets.clone();
    let mut offsets = narrow_type_offsets(
        original_offsets.clone(),
        property_query_index,
        doc_types,
        lowered_types,
        active_narrows,
    );
    if offsets.is_empty() && !original_offsets.is_empty() {
        offsets = original_offsets;
    }
    candidate.explicit_type_offsets = offsets;
    candidate
}

pub(crate) fn narrow_type_offsets(
    mut offsets: Vec<SalsaDocTypeNodeKey>,
    property_query_index: &SalsaPropertyQueryIndex,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    active_narrows: &[SalsaTypeNarrowSummary],
) -> Vec<SalsaDocTypeNodeKey> {
    for narrow in active_narrows {
        offsets = match narrow {
            SalsaTypeNarrowSummary::Truthy => offsets
                .into_iter()
                .flat_map(|offset| {
                    narrow_truthy_type_offset(
                        offset,
                        doc_types,
                        lowered_types,
                        &mut BTreeSet::new(),
                    )
                })
                .collect(),
            SalsaTypeNarrowSummary::Falsey => offsets
                .into_iter()
                .flat_map(|offset| {
                    narrow_falsey_type_offset(
                        offset,
                        doc_types,
                        lowered_types,
                        &mut BTreeSet::new(),
                    )
                })
                .collect(),
            SalsaTypeNarrowSummary::Literal {
                literal_text,
                positive,
            } => offsets
                .into_iter()
                .flat_map(|offset| {
                    narrow_literal_offset(
                        offset,
                        literal_text,
                        *positive,
                        doc_types,
                        lowered_types,
                        &mut BTreeSet::new(),
                    )
                })
                .collect(),
            SalsaTypeNarrowSummary::TypeGuard { type_name } => offsets
                .into_iter()
                .flat_map(|offset| {
                    narrow_type_guard_offset(
                        offset,
                        type_name,
                        doc_types,
                        lowered_types,
                        &mut BTreeSet::new(),
                    )
                })
                .collect(),
            SalsaTypeNarrowSummary::ExcludeTypeGuard { type_name } => offsets
                .into_iter()
                .flat_map(|offset| {
                    exclude_type_guard_offset(
                        offset,
                        type_name,
                        doc_types,
                        lowered_types,
                        &mut BTreeSet::new(),
                    )
                })
                .collect(),
            SalsaTypeNarrowSummary::FieldLiteral {
                member_name,
                literal_text,
                positive,
            } => offsets
                .into_iter()
                .flat_map(|offset| {
                    narrow_field_literal_offset(
                        offset,
                        member_name,
                        literal_text,
                        *positive,
                        property_query_index,
                        doc_types,
                        lowered_types,
                        &mut BTreeSet::new(),
                    )
                })
                .collect(),
        };
    }
    offsets
}

fn narrow_literal_offset(
    offset: SalsaDocTypeNodeKey,
    literal_text: &SmolStr,
    positive: bool,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited: &mut BTreeSet<SalsaDocTypeNodeKey>,
) -> Vec<SalsaDocTypeNodeKey> {
    if !visited.insert(offset) {
        return vec![offset];
    }
    let Some(doc_type) = doc_types
        .types
        .iter()
        .find(|doc_type| doc_type.node_key() == offset)
    else {
        let Some(resolved) =
            find_resolved_doc_type_by_key_from_parts(doc_types, lowered_types, offset)
        else {
            return vec![offset];
        };
        return match resolved.lowered.kind {
            SalsaDocTypeLoweredKind::Literal { text } => {
                if (positive && text == *literal_text) || (!positive && text != *literal_text) {
                    vec![offset]
                } else {
                    Vec::new()
                }
            }
            SalsaDocTypeLoweredKind::Union { item_types }
            | SalsaDocTypeLoweredKind::MultiLineUnion { item_types } => item_types
                .into_iter()
                .flat_map(|type_ref| match type_ref {
                    SalsaDocTypeRef::Node(offset) => narrow_literal_offset(
                        offset,
                        literal_text,
                        positive,
                        doc_types,
                        lowered_types,
                        visited,
                    ),
                    SalsaDocTypeRef::Incomplete => vec![offset],
                })
                .collect(),
            _ => vec![offset],
        };
    };

    match &doc_type.kind {
        SalsaDocTypeKindSummary::Literal { text } => {
            if (positive && *text == *literal_text) || (!positive && *text != *literal_text) {
                vec![offset]
            } else {
                Vec::new()
            }
        }
        SalsaDocTypeKindSummary::Binary {
            op: SalsaDocTypeBinaryOperatorSummary::Union,
            left_type_offset,
            right_type_offset,
        } => {
            let mut narrowed = Vec::new();
            if let Some(left) = left_type_offset {
                narrowed.extend(narrow_literal_offset(
                    *left,
                    literal_text,
                    positive,
                    doc_types,
                    lowered_types,
                    visited,
                ));
            }
            if let Some(right) = right_type_offset {
                narrowed.extend(narrow_literal_offset(
                    *right,
                    literal_text,
                    positive,
                    doc_types,
                    lowered_types,
                    visited,
                ));
            }
            dedupe_offsets(narrowed)
        }
        SalsaDocTypeKindSummary::MultiLineUnion { item_type_offsets }
        | SalsaDocTypeKindSummary::Tuple { item_type_offsets } => item_type_offsets
            .iter()
            .flat_map(|item| {
                narrow_literal_offset(
                    *item,
                    literal_text,
                    positive,
                    doc_types,
                    lowered_types,
                    visited,
                )
            })
            .collect(),
        SalsaDocTypeKindSummary::Nullable { inner_type_offset }
        | SalsaDocTypeKindSummary::Unary {
            inner_type_offset, ..
        } => inner_type_offset
            .map(|inner| {
                narrow_literal_offset(
                    inner,
                    literal_text,
                    positive,
                    doc_types,
                    lowered_types,
                    visited,
                )
            })
            .unwrap_or_else(|| vec![offset]),
        _ => vec![offset],
    }
}

fn narrow_field_literal_offset(
    offset: SalsaDocTypeNodeKey,
    member_name: &SmolStr,
    literal_text: &SmolStr,
    positive: bool,
    property_query_index: &SalsaPropertyQueryIndex,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited: &mut BTreeSet<SalsaDocTypeNodeKey>,
) -> Vec<SalsaDocTypeNodeKey> {
    if !visited.insert(offset) {
        return narrow_duplicate_doc_type_entries_at_offset(
            offset,
            member_name,
            literal_text,
            positive,
            property_query_index,
            doc_types,
        );
    }
    let Some(doc_type) = doc_types
        .types
        .iter()
        .find(|doc_type| doc_type.node_key() == offset)
    else {
        return narrow_field_literal_lowered_fallback(
            offset,
            member_name,
            literal_text,
            positive,
            property_query_index,
            doc_types,
            lowered_types,
            visited,
        );
    };

    match &doc_type.kind {
        SalsaDocTypeKindSummary::Name { name } => name
            .as_ref()
            .map(|type_name| {
                narrow_named_type_by_discriminant(
                    offset,
                    type_name,
                    member_name,
                    literal_text,
                    positive,
                    property_query_index,
                    doc_types,
                )
            })
            .unwrap_or_else(|| vec![offset]),
        SalsaDocTypeKindSummary::Binary {
            op: SalsaDocTypeBinaryOperatorSummary::Union,
            left_type_offset,
            right_type_offset,
        } => {
            let mut narrowed = Vec::new();
            if let Some(left) = left_type_offset {
                narrowed.extend(narrow_field_literal_offset(
                    *left,
                    member_name,
                    literal_text,
                    positive,
                    property_query_index,
                    doc_types,
                    lowered_types,
                    visited,
                ));
            }
            if let Some(right) = right_type_offset {
                narrowed.extend(narrow_field_literal_offset(
                    *right,
                    member_name,
                    literal_text,
                    positive,
                    property_query_index,
                    doc_types,
                    lowered_types,
                    visited,
                ));
            }
            dedupe_offsets(narrowed)
        }
        SalsaDocTypeKindSummary::MultiLineUnion { item_type_offsets }
        | SalsaDocTypeKindSummary::Tuple { item_type_offsets } => item_type_offsets
            .iter()
            .flat_map(|item| {
                narrow_field_literal_offset(
                    *item,
                    member_name,
                    literal_text,
                    positive,
                    property_query_index,
                    doc_types,
                    lowered_types,
                    visited,
                )
            })
            .collect(),
        SalsaDocTypeKindSummary::Nullable { inner_type_offset }
        | SalsaDocTypeKindSummary::Unary {
            inner_type_offset, ..
        } => inner_type_offset
            .map(|inner| {
                narrow_field_literal_offset(
                    inner,
                    member_name,
                    literal_text,
                    positive,
                    property_query_index,
                    doc_types,
                    lowered_types,
                    visited,
                )
            })
            .unwrap_or_else(|| vec![offset]),
        _ => vec![offset],
    }
}

fn narrow_duplicate_doc_type_entries_at_offset(
    offset: SalsaDocTypeNodeKey,
    member_name: &SmolStr,
    literal_text: &SmolStr,
    positive: bool,
    property_query_index: &SalsaPropertyQueryIndex,
    doc_types: &SalsaDocTypeIndexSummary,
) -> Vec<SalsaDocTypeNodeKey> {
    let mut saw_named_entry = false;
    let narrowed = doc_types
        .types
        .iter()
        .filter(|doc_type| doc_type.node_key() == offset)
        .flat_map(|doc_type| match &doc_type.kind {
            SalsaDocTypeKindSummary::Name { name } => {
                saw_named_entry = true;
                name.as_ref()
                    .map(|type_name| {
                        narrow_named_type_by_discriminant(
                            offset,
                            type_name,
                            member_name,
                            literal_text,
                            positive,
                            property_query_index,
                            doc_types,
                        )
                    })
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        })
        .collect::<Vec<_>>();
    if narrowed.is_empty() && !saw_named_entry {
        vec![offset]
    } else {
        dedupe_offsets(narrowed)
    }
}

fn narrow_named_type_by_discriminant(
    offset: SalsaDocTypeNodeKey,
    type_name: &SmolStr,
    member_name: &SmolStr,
    literal_text: &SmolStr,
    positive: bool,
    property_query_index: &SalsaPropertyQueryIndex,
    doc_types: &SalsaDocTypeIndexSummary,
) -> Vec<SalsaDocTypeNodeKey> {
    let properties = collect_properties_for_type_and_key_in_index(
        property_query_index,
        type_name,
        &crate::SalsaPropertyKeySummary::Name(member_name.clone()),
    );
    if properties.is_empty() {
        return vec![offset];
    }

    let has_match = properties.iter().any(|property| {
        property
            .doc_type_offset
            .and_then(|doc_type_offset| discriminant_literal_text(doc_type_offset, doc_types))
            .is_some_and(|property_literal_text| property_literal_text == *literal_text)
    });

    if positive {
        if has_match { vec![offset] } else { Vec::new() }
    } else if has_match {
        Vec::new()
    } else {
        vec![offset]
    }
}

fn discriminant_literal_text(
    doc_type_offset: SalsaDocTypeNodeKey,
    doc_types: &SalsaDocTypeIndexSummary,
) -> Option<SmolStr> {
    let doc_type = doc_types
        .types
        .iter()
        .find(|doc_type| doc_type.node_key() == doc_type_offset)?;
    match &doc_type.kind {
        SalsaDocTypeKindSummary::Literal { text } => Some(text.clone()),
        _ => None,
    }
}

fn narrow_field_literal_lowered_fallback(
    offset: SalsaDocTypeNodeKey,
    member_name: &SmolStr,
    literal_text: &SmolStr,
    positive: bool,
    property_query_index: &SalsaPropertyQueryIndex,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited: &mut BTreeSet<SalsaDocTypeNodeKey>,
) -> Vec<SalsaDocTypeNodeKey> {
    if !visited.insert(offset) {
        return vec![offset];
    }
    let Some(resolved) = find_resolved_doc_type_by_key_from_parts(doc_types, lowered_types, offset)
    else {
        return vec![offset];
    };
    match resolved.lowered.kind {
        SalsaDocTypeLoweredKind::Name { name } => narrow_named_type_by_discriminant(
            offset,
            &name,
            member_name,
            literal_text,
            positive,
            property_query_index,
            doc_types,
        ),
        SalsaDocTypeLoweredKind::Union { item_types }
        | SalsaDocTypeLoweredKind::MultiLineUnion { item_types } => item_types
            .into_iter()
            .flat_map(|type_ref| match type_ref {
                SalsaDocTypeRef::Node(offset) => narrow_field_literal_lowered_fallback(
                    offset,
                    member_name,
                    literal_text,
                    positive,
                    property_query_index,
                    doc_types,
                    lowered_types,
                    visited,
                ),
                SalsaDocTypeRef::Incomplete => vec![offset],
            })
            .collect(),
        _ => vec![offset],
    }
}

fn narrow_truthy_type_offset(
    offset: SalsaDocTypeNodeKey,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited: &mut BTreeSet<SalsaDocTypeNodeKey>,
) -> Vec<SalsaDocTypeNodeKey> {
    if !visited.insert(offset) {
        return Vec::new();
    }
    let Some(resolved) = find_resolved_doc_type_by_key_from_parts(doc_types, lowered_types, offset)
    else {
        return vec![offset];
    };
    match resolved.lowered.kind {
        SalsaDocTypeLoweredKind::Name { name } if name == "nil" => Vec::new(),
        SalsaDocTypeLoweredKind::Literal { text } if text == "false" || text == "nil" => Vec::new(),
        SalsaDocTypeLoweredKind::Nullable { inner_type } => {
            narrowed_refs_to_offsets(vec![inner_type])
        }
        SalsaDocTypeLoweredKind::Union { item_types }
        | SalsaDocTypeLoweredKind::MultiLineUnion { item_types } => item_types
            .into_iter()
            .flat_map(|type_ref| {
                narrow_truthy_type_ref(type_ref, doc_types, lowered_types, visited)
            })
            .collect(),
        _ => vec![offset],
    }
}

fn narrow_truthy_type_ref(
    type_ref: SalsaDocTypeRef,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited: &mut BTreeSet<SalsaDocTypeNodeKey>,
) -> Vec<SalsaDocTypeNodeKey> {
    match type_ref {
        SalsaDocTypeRef::Node(offset) => {
            narrow_truthy_type_offset(offset, doc_types, lowered_types, visited)
        }
        SalsaDocTypeRef::Incomplete => Vec::new(),
    }
}

fn narrow_falsey_type_offset(
    offset: SalsaDocTypeNodeKey,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited: &mut BTreeSet<SalsaDocTypeNodeKey>,
) -> Vec<SalsaDocTypeNodeKey> {
    if !visited.insert(offset) {
        return Vec::new();
    }
    let Some(resolved) = find_resolved_doc_type_by_key_from_parts(doc_types, lowered_types, offset)
    else {
        return Vec::new();
    };
    match resolved.lowered.kind {
        SalsaDocTypeLoweredKind::Name { name } if name == "nil" => vec![offset],
        SalsaDocTypeLoweredKind::Literal { text } if text == "false" || text == "nil" => {
            vec![offset]
        }
        SalsaDocTypeLoweredKind::Nullable { .. } => vec![offset],
        SalsaDocTypeLoweredKind::Union { item_types }
        | SalsaDocTypeLoweredKind::MultiLineUnion { item_types } => item_types
            .into_iter()
            .flat_map(|type_ref| {
                narrow_falsey_type_ref(type_ref, doc_types, lowered_types, visited)
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn narrow_falsey_type_ref(
    type_ref: SalsaDocTypeRef,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited: &mut BTreeSet<SalsaDocTypeNodeKey>,
) -> Vec<SalsaDocTypeNodeKey> {
    match type_ref {
        SalsaDocTypeRef::Node(offset) => {
            narrow_falsey_type_offset(offset, doc_types, lowered_types, visited)
        }
        SalsaDocTypeRef::Incomplete => Vec::new(),
    }
}

fn narrow_type_guard_offset(
    offset: SalsaDocTypeNodeKey,
    type_name: &SmolStr,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited: &mut BTreeSet<SalsaDocTypeNodeKey>,
) -> Vec<SalsaDocTypeNodeKey> {
    if !visited.insert(offset) {
        return Vec::new();
    }
    let Some(resolved) = find_resolved_doc_type_by_key_from_parts(doc_types, lowered_types, offset)
    else {
        return vec![offset];
    };
    match resolved.lowered.kind {
        SalsaDocTypeLoweredKind::Name { name } if &name == type_name => vec![offset],
        SalsaDocTypeLoweredKind::Nullable { inner_type } => narrowed_refs_by_type_guard(
            vec![inner_type],
            type_name,
            doc_types,
            lowered_types,
            visited,
        ),
        SalsaDocTypeLoweredKind::Union { item_types }
        | SalsaDocTypeLoweredKind::MultiLineUnion { item_types } => {
            narrowed_refs_by_type_guard(item_types, type_name, doc_types, lowered_types, visited)
        }
        _ => Vec::new(),
    }
}

fn narrowed_refs_by_type_guard(
    item_types: Vec<SalsaDocTypeRef>,
    type_name: &SmolStr,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited: &mut BTreeSet<SalsaDocTypeNodeKey>,
) -> Vec<SalsaDocTypeNodeKey> {
    item_types
        .into_iter()
        .flat_map(|type_ref| match type_ref {
            SalsaDocTypeRef::Node(offset) => {
                narrow_type_guard_offset(offset, type_name, doc_types, lowered_types, visited)
            }
            SalsaDocTypeRef::Incomplete => Vec::new(),
        })
        .collect()
}

fn exclude_type_guard_offset(
    offset: SalsaDocTypeNodeKey,
    type_name: &SmolStr,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited: &mut BTreeSet<SalsaDocTypeNodeKey>,
) -> Vec<SalsaDocTypeNodeKey> {
    if !visited.insert(offset) {
        return Vec::new();
    }
    let Some(resolved) = find_resolved_doc_type_by_key_from_parts(doc_types, lowered_types, offset)
    else {
        return vec![offset];
    };
    match resolved.lowered.kind {
        SalsaDocTypeLoweredKind::Name { name } if &name == type_name => Vec::new(),
        SalsaDocTypeLoweredKind::Nullable { inner_type } => {
            let mut narrowed = vec![offset];
            narrowed.extend(excluded_refs_by_type_guard(
                vec![inner_type],
                type_name,
                doc_types,
                lowered_types,
                visited,
            ));
            dedupe_offsets(narrowed)
        }
        SalsaDocTypeLoweredKind::Union { item_types }
        | SalsaDocTypeLoweredKind::MultiLineUnion { item_types } => {
            excluded_refs_by_type_guard(item_types, type_name, doc_types, lowered_types, visited)
        }
        _ => vec![offset],
    }
}

fn excluded_refs_by_type_guard(
    item_types: Vec<SalsaDocTypeRef>,
    type_name: &SmolStr,
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    visited: &mut BTreeSet<SalsaDocTypeNodeKey>,
) -> Vec<SalsaDocTypeNodeKey> {
    item_types
        .into_iter()
        .flat_map(|type_ref| match type_ref {
            SalsaDocTypeRef::Node(offset) => {
                exclude_type_guard_offset(offset, type_name, doc_types, lowered_types, visited)
            }
            SalsaDocTypeRef::Incomplete => Vec::new(),
        })
        .collect()
}

fn narrowed_refs_to_offsets(item_types: Vec<SalsaDocTypeRef>) -> Vec<SalsaDocTypeNodeKey> {
    item_types
        .into_iter()
        .filter_map(|type_ref| match type_ref {
            SalsaDocTypeRef::Node(offset) => Some(offset),
            SalsaDocTypeRef::Incomplete => None,
        })
        .collect()
}

fn dedupe_offsets(offsets: Vec<SalsaDocTypeNodeKey>) -> Vec<SalsaDocTypeNodeKey> {
    let mut seen = BTreeSet::new();
    offsets
        .into_iter()
        .filter(|offset| seen.insert(*offset))
        .collect()
}
