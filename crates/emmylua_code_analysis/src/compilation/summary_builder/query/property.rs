use rowan::TextSize;

use super::super::{SalsaLookupBucket, build_lookup_buckets, find_bucket_indices};

use crate::{
    SalsaDeclId, SalsaMemberTargetId, SalsaMemberTargetSummary, SalsaPropertyIndexSummary,
    SalsaPropertyKeySummary, SalsaPropertySourceSummary, SalsaPropertySummary,
    SalsaSyntaxIdSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, salsa::Update)]
struct SalsaDeclPropertyKeyLookup {
    pub decl_id: SalsaDeclId,
    pub key: SalsaPropertyKeySummary,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, salsa::Update)]
struct SalsaMemberPropertyKeyLookup {
    pub member_target: SalsaMemberTargetId,
    pub key: SalsaPropertyKeySummary,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, salsa::Update)]
struct SalsaTypePropertyKeyLookup {
    pub type_name: smol_str::SmolStr,
    pub key: SalsaPropertyKeySummary,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaPropertyQueryIndex {
    pub properties: Vec<SalsaPropertySummary>,
    by_syntax_offset: Vec<SalsaLookupBucket<TextSize>>,
    by_value_expr_syntax_id: Vec<SalsaLookupBucket<SalsaSyntaxIdSummary>>,
    by_decl: Vec<SalsaLookupBucket<SalsaDeclId>>,
    by_member: Vec<SalsaLookupBucket<SalsaMemberTargetId>>,
    by_type: Vec<SalsaLookupBucket<smol_str::SmolStr>>,
    by_source: Vec<SalsaLookupBucket<SalsaPropertySourceSummary>>,
    by_key: Vec<SalsaLookupBucket<SalsaPropertyKeySummary>>,
    by_decl_and_key: Vec<SalsaLookupBucket<SalsaDeclPropertyKeyLookup>>,
    by_member_and_key: Vec<SalsaLookupBucket<SalsaMemberPropertyKeyLookup>>,
    by_type_and_key: Vec<SalsaLookupBucket<SalsaTypePropertyKeyLookup>>,
}

pub fn build_property_query_index(
    properties: &SalsaPropertyIndexSummary,
) -> SalsaPropertyQueryIndex {
    let properties = properties.properties.clone();
    let mut syntax_offset_entries = Vec::with_capacity(properties.len());
    let mut value_expr_syntax_id_entries = Vec::new();
    let mut decl_entries = Vec::new();
    let mut member_entries = Vec::new();
    let mut type_entries = Vec::new();
    let mut source_entries = Vec::with_capacity(properties.len());
    let mut key_entries = Vec::with_capacity(properties.len());
    let mut decl_and_key_entries = Vec::new();
    let mut member_and_key_entries = Vec::new();
    let mut type_and_key_entries = Vec::new();

    for (index, property) in properties.iter().enumerate() {
        syntax_offset_entries.push((property.syntax_offset, index));
        if let Some(value_expr_syntax_id) = property.value_expr_syntax_id {
            value_expr_syntax_id_entries.push((value_expr_syntax_id, index));
        }
        source_entries.push((property.source.clone(), index));
        key_entries.push((property.key.clone(), index));

        match &property.owner {
            crate::SalsaPropertyOwnerSummary::Decl { decl_id, .. } => {
                decl_entries.push((*decl_id, index));
                decl_and_key_entries.push((
                    SalsaDeclPropertyKeyLookup {
                        decl_id: *decl_id,
                        key: property.key.clone(),
                    },
                    index,
                ));
            }
            crate::SalsaPropertyOwnerSummary::Member(target) => {
                member_entries.push((target.clone(), index));
                member_and_key_entries.push((
                    SalsaMemberPropertyKeyLookup {
                        member_target: target.clone(),
                        key: property.key.clone(),
                    },
                    index,
                ));
            }
            crate::SalsaPropertyOwnerSummary::Type(type_name) => {
                type_entries.push((type_name.clone(), index));
                type_and_key_entries.push((
                    SalsaTypePropertyKeyLookup {
                        type_name: type_name.clone(),
                        key: property.key.clone(),
                    },
                    index,
                ));
            }
        }
    }

    SalsaPropertyQueryIndex {
        properties,
        by_syntax_offset: build_lookup_buckets(syntax_offset_entries),
        by_value_expr_syntax_id: build_lookup_buckets(value_expr_syntax_id_entries),
        by_decl: build_lookup_buckets(decl_entries),
        by_member: build_lookup_buckets(member_entries),
        by_type: build_lookup_buckets(type_entries),
        by_source: build_lookup_buckets(source_entries),
        by_key: build_lookup_buckets(key_entries),
        by_decl_and_key: build_lookup_buckets(decl_and_key_entries),
        by_member_and_key: build_lookup_buckets(member_and_key_entries),
        by_type_and_key: build_lookup_buckets(type_and_key_entries),
    }
}

pub fn find_property_at(
    properties: &SalsaPropertyIndexSummary,
    syntax_offset: TextSize,
) -> Option<SalsaPropertySummary> {
    let index = build_property_query_index(properties);
    find_property_at_in_index(&index, syntax_offset)
}

pub fn find_property_by_value_expr_syntax_id(
    properties: &SalsaPropertyIndexSummary,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaPropertySummary> {
    let index = build_property_query_index(properties);
    find_property_by_value_expr_syntax_id_in_index(&index, syntax_id)
}

pub fn collect_properties_for_decl(
    properties: &SalsaPropertyIndexSummary,
    decl_id: SalsaDeclId,
) -> Vec<SalsaPropertySummary> {
    let index = build_property_query_index(properties);
    collect_properties_for_decl_in_index(&index, decl_id)
}

pub fn collect_properties_for_member(
    properties: &SalsaPropertyIndexSummary,
    member_target: &SalsaMemberTargetSummary,
) -> Vec<SalsaPropertySummary> {
    let index = build_property_query_index(properties);
    collect_properties_for_member_in_index(&index, member_target)
}

pub fn collect_properties_for_type(
    properties: &SalsaPropertyIndexSummary,
    type_name: &str,
) -> Vec<SalsaPropertySummary> {
    let index = build_property_query_index(properties);
    collect_properties_for_type_in_index(&index, type_name)
}

pub fn collect_properties_for_source(
    properties: &SalsaPropertyIndexSummary,
    source: &SalsaPropertySourceSummary,
) -> Vec<SalsaPropertySummary> {
    let index = build_property_query_index(properties);
    collect_properties_for_source_in_index(&index, source)
}

pub fn collect_properties_for_key(
    properties: &SalsaPropertyIndexSummary,
    key: &SalsaPropertyKeySummary,
) -> Vec<SalsaPropertySummary> {
    let index = build_property_query_index(properties);
    collect_properties_for_key_in_index(&index, key)
}

pub fn collect_properties_for_decl_and_key(
    properties: &SalsaPropertyIndexSummary,
    decl_id: SalsaDeclId,
    key: &SalsaPropertyKeySummary,
) -> Vec<SalsaPropertySummary> {
    let index = build_property_query_index(properties);
    collect_properties_for_decl_and_key_in_index(&index, decl_id, key)
}

pub fn collect_properties_for_member_and_key(
    properties: &SalsaPropertyIndexSummary,
    member_target: &SalsaMemberTargetSummary,
    key: &SalsaPropertyKeySummary,
) -> Vec<SalsaPropertySummary> {
    let index = build_property_query_index(properties);
    collect_properties_for_member_and_key_in_index(&index, member_target, key)
}

pub fn collect_properties_for_type_and_key(
    properties: &SalsaPropertyIndexSummary,
    type_name: &str,
    key: &SalsaPropertyKeySummary,
) -> Vec<SalsaPropertySummary> {
    let index = build_property_query_index(properties);
    collect_properties_for_type_and_key_in_index(&index, type_name, key)
}

pub fn find_property_at_in_index(
    index: &SalsaPropertyQueryIndex,
    syntax_offset: TextSize,
) -> Option<SalsaPropertySummary> {
    find_bucket_indices(&index.by_syntax_offset, &syntax_offset)
        .and_then(|indices| indices.first().copied())
        .map(|property_index| index.properties[property_index].clone())
}

pub fn find_property_by_value_expr_syntax_id_in_index(
    index: &SalsaPropertyQueryIndex,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaPropertySummary> {
    find_bucket_indices(&index.by_value_expr_syntax_id, &syntax_id)
        .and_then(|indices| indices.first().copied())
        .map(|property_index| index.properties[property_index].clone())
}

pub fn collect_properties_for_decl_in_index(
    index: &SalsaPropertyQueryIndex,
    decl_id: SalsaDeclId,
) -> Vec<SalsaPropertySummary> {
    collect_properties(index, find_bucket_indices(&index.by_decl, &decl_id))
}

pub fn collect_properties_for_member_in_index(
    index: &SalsaPropertyQueryIndex,
    member_target: &SalsaMemberTargetSummary,
) -> Vec<SalsaPropertySummary> {
    let member_target = SalsaMemberTargetId::from(member_target);
    collect_properties(index, find_bucket_indices(&index.by_member, &member_target))
}

pub fn collect_properties_for_type_in_index(
    index: &SalsaPropertyQueryIndex,
    type_name: &str,
) -> Vec<SalsaPropertySummary> {
    collect_properties(
        index,
        find_bucket_indices(&index.by_type, &type_name.into()),
    )
}

pub fn collect_properties_for_source_in_index(
    index: &SalsaPropertyQueryIndex,
    source: &SalsaPropertySourceSummary,
) -> Vec<SalsaPropertySummary> {
    collect_properties(index, find_bucket_indices(&index.by_source, source))
}

pub fn collect_properties_for_key_in_index(
    index: &SalsaPropertyQueryIndex,
    key: &SalsaPropertyKeySummary,
) -> Vec<SalsaPropertySummary> {
    collect_properties(index, find_bucket_indices(&index.by_key, key))
}

pub fn collect_properties_for_decl_and_key_in_index(
    index: &SalsaPropertyQueryIndex,
    decl_id: SalsaDeclId,
    key: &SalsaPropertyKeySummary,
) -> Vec<SalsaPropertySummary> {
    let mut properties = collect_properties(
        index,
        find_bucket_indices(
            &index.by_decl_and_key,
            &SalsaDeclPropertyKeyLookup {
                decl_id,
                key: key.clone(),
            },
        ),
    );
    properties.extend(collect_expanded_multi_result_tail_properties(
        collect_properties_for_decl_in_index(index, decl_id),
        key,
    ));
    dedupe_properties(properties)
}

pub fn collect_properties_for_member_and_key_in_index(
    index: &SalsaPropertyQueryIndex,
    member_target: &SalsaMemberTargetSummary,
    key: &SalsaPropertyKeySummary,
) -> Vec<SalsaPropertySummary> {
    let member_target = SalsaMemberTargetId::from(member_target);
    let mut properties = collect_properties(
        index,
        find_bucket_indices(
            &index.by_member_and_key,
            &SalsaMemberPropertyKeyLookup {
                member_target: member_target.clone(),
                key: key.clone(),
            },
        ),
    );
    properties.extend(collect_expanded_multi_result_tail_properties(
        collect_properties(index, find_bucket_indices(&index.by_member, &member_target)),
        key,
    ));
    dedupe_properties(properties)
}

pub fn collect_properties_for_type_and_key_in_index(
    index: &SalsaPropertyQueryIndex,
    type_name: &str,
    key: &SalsaPropertyKeySummary,
) -> Vec<SalsaPropertySummary> {
    let mut properties = collect_properties(
        index,
        find_bucket_indices(
            &index.by_type_and_key,
            &SalsaTypePropertyKeyLookup {
                type_name: type_name.into(),
                key: key.clone(),
            },
        ),
    );
    properties.extend(collect_expanded_multi_result_tail_properties(
        collect_properties_for_type_in_index(index, type_name),
        key,
    ));
    dedupe_properties(properties)
}

fn collect_properties(
    index: &SalsaPropertyQueryIndex,
    property_indices: Option<&[usize]>,
) -> Vec<SalsaPropertySummary> {
    property_indices
        .into_iter()
        .flatten()
        .map(|property_index| index.properties[*property_index].clone())
        .collect()
}

fn collect_expanded_multi_result_tail_properties(
    properties: Vec<SalsaPropertySummary>,
    requested_key: &SalsaPropertyKeySummary,
) -> Vec<SalsaPropertySummary> {
    properties
        .into_iter()
        .filter_map(|property| expand_multi_result_tail_property(property, requested_key))
        .collect()
}

fn expand_multi_result_tail_property(
    mut property: SalsaPropertySummary,
    requested_key: &SalsaPropertyKeySummary,
) -> Option<SalsaPropertySummary> {
    let requested_index = numeric_key_index(requested_key)?;
    let SalsaPropertyKeySummary::Sequence(base_index) = property.key else {
        return None;
    };
    if !property.expands_multi_result_tail || requested_index < base_index {
        return None;
    }
    if requested_index == base_index
        && requested_key == &SalsaPropertyKeySummary::Sequence(base_index)
    {
        return None;
    }

    property.key = requested_key.clone();
    property.value_result_index += requested_index - base_index;
    Some(property)
}

fn numeric_key_index(key: &SalsaPropertyKeySummary) -> Option<usize> {
    match key {
        SalsaPropertyKeySummary::Integer(value) if *value > 0 => Some(*value as usize),
        SalsaPropertyKeySummary::Sequence(value) => Some(*value),
        _ => None,
    }
}

fn dedupe_properties(properties: Vec<SalsaPropertySummary>) -> Vec<SalsaPropertySummary> {
    let mut deduped = Vec::new();
    for property in properties {
        if !deduped.contains(&property) {
            deduped.push(property);
        }
    }
    deduped
}
