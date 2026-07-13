use rowan::TextSize;

use super::super::{SalsaLookupBucket, build_lookup_buckets, find_bucket_indices};
use crate::{SalsaSyntaxIdSummary, SalsaTableShapeIndexSummary, SalsaTableShapeSummary};

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaTableShapeQueryIndex {
    pub tables: Vec<SalsaTableShapeSummary>,
    by_syntax_offset: Vec<SalsaLookupBucket<TextSize>>,
    by_syntax_id: Vec<SalsaLookupBucket<SalsaSyntaxIdSummary>>,
}

pub fn build_table_shape_query_index(
    summary: &SalsaTableShapeIndexSummary,
) -> SalsaTableShapeQueryIndex {
    let tables = summary.tables.clone();
    let by_syntax_offset = build_lookup_buckets(
        tables
            .iter()
            .enumerate()
            .map(|(index, table)| (table.syntax_offset, index))
            .collect(),
    );
    let by_syntax_id = build_lookup_buckets(
        tables
            .iter()
            .enumerate()
            .map(|(index, table)| (table.syntax_id, index))
            .collect(),
    );

    SalsaTableShapeQueryIndex {
        tables,
        by_syntax_offset,
        by_syntax_id,
    }
}

pub fn find_table_shape_at_in_index(
    index: &SalsaTableShapeQueryIndex,
    syntax_offset: TextSize,
) -> Option<SalsaTableShapeSummary> {
    find_bucket_indices(&index.by_syntax_offset, &syntax_offset)
        .and_then(|indices| indices.first().copied())
        .map(|table_index| index.tables[table_index].clone())
}

pub fn find_table_shape_by_syntax_id_in_index(
    index: &SalsaTableShapeQueryIndex,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaTableShapeSummary> {
    find_bucket_indices(&index.by_syntax_id, &syntax_id)
        .and_then(|indices| indices.first().copied())
        .map(|table_index| index.tables[table_index].clone())
}
