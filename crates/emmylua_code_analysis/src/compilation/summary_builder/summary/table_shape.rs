use rowan::TextSize;

use super::SalsaSyntaxIdSummary;

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaTableShapeKindSummary {
    Empty,
    ObjectLike,
    SequenceLike,
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaSequenceShapeKindSummary {
    None,
    ArrayLike,
    TupleLike,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaTableShapeSummary {
    pub syntax_id: SalsaSyntaxIdSummary,
    pub syntax_offset: TextSize,
    pub kind: SalsaTableShapeKindSummary,
    pub sequence_kind: SalsaSequenceShapeKindSummary,
    pub sequence_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaTableShapeIndexSummary {
    pub tables: Vec<SalsaTableShapeSummary>,
}
