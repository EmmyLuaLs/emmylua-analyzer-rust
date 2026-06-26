use super::super::{SalsaLookupBucket, build_lookup_buckets, find_bucket_indices};
use crate::{
    SalsaDeclSummary, SalsaDeclTreeSummary, SalsaMemberIndexSummary, SalsaMemberSummary,
    SalsaSyntaxIdSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDeclQueryIndex {
    pub decls: Vec<SalsaDeclSummary>,
    by_syntax_id: Vec<SalsaLookupBucket<SalsaSyntaxIdSummary>>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaMemberQueryIndex {
    pub members: Vec<SalsaMemberSummary>,
    by_syntax_id: Vec<SalsaLookupBucket<SalsaSyntaxIdSummary>>,
}

pub fn build_decl_query_index(summary: &SalsaDeclTreeSummary) -> SalsaDeclQueryIndex {
    let decls = summary.decls.clone();
    let by_syntax_id = build_lookup_buckets(
        decls
            .iter()
            .enumerate()
            .filter_map(|(index, decl)| decl.syntax_id.map(|syntax_id| (syntax_id, index)))
            .collect(),
    );

    SalsaDeclQueryIndex {
        decls,
        by_syntax_id,
    }
}

pub fn build_member_query_index(summary: &SalsaMemberIndexSummary) -> SalsaMemberQueryIndex {
    let members = summary.members.clone();
    let by_syntax_id = build_lookup_buckets(
        members
            .iter()
            .enumerate()
            .map(|(index, member)| (member.syntax_id, index))
            .collect(),
    );

    SalsaMemberQueryIndex {
        members,
        by_syntax_id,
    }
}

pub fn find_decl_by_syntax_id_in_index(
    index: &SalsaDeclQueryIndex,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaDeclSummary> {
    find_bucket_indices(&index.by_syntax_id, &syntax_id)
        .and_then(|indices| indices.first().copied())
        .map(|decl_index| index.decls[decl_index].clone())
}

pub fn find_member_by_syntax_id_in_index(
    index: &SalsaMemberQueryIndex,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaMemberSummary> {
    find_bucket_indices(&index.by_syntax_id, &syntax_id)
        .and_then(|indices| indices.first().copied())
        .map(|member_index| index.members[member_index].clone())
}
