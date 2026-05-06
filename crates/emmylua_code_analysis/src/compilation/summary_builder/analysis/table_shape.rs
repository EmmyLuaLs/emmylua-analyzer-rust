use std::collections::BTreeSet;

use emmylua_parser::{
    LuaAstNode, LuaExpr, LuaIndexKey, LuaLiteralToken, LuaTableExpr, NumberResult,
};
use rowan::TextSize;

use super::super::{
    SalsaSequenceShapeKindSummary, SalsaTableShapeIndexSummary, SalsaTableShapeKindSummary,
    SalsaTableShapeSummary,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SequenceValueKind {
    NumberLiteral,
    StringLiteral,
    BoolLiteral,
    NilLiteral,
    OtherLiteral,
    Closure,
    Table,
    Name,
    Member,
    Call,
    Other,
}

pub fn analyze_table_shape_summary(chunk: emmylua_parser::LuaChunk) -> SalsaTableShapeIndexSummary {
    SalsaTableShapeIndexSummary {
        tables: chunk
            .descendants::<LuaTableExpr>()
            .map(analyze_table_expr_shape)
            .collect(),
    }
}

pub(crate) fn analyze_table_expr_shape(table_expr: LuaTableExpr) -> SalsaTableShapeSummary {
    let mut named_field_count = 0usize;
    let mut integer_field_count = 0usize;
    let mut sequence_field_count = 0usize;
    let mut expr_field_count = 0usize;
    let mut sequence_value_kinds = Vec::new();

    for (field, key) in table_expr.get_fields_with_keys() {
        match key {
            LuaIndexKey::Name(_) | LuaIndexKey::String(_) => {
                named_field_count += 1;
            }
            LuaIndexKey::Integer(number_token) => {
                if matches!(number_token.get_number_value(), NumberResult::Int(_)) {
                    integer_field_count += 1;
                } else {
                    expr_field_count += 1;
                }
            }
            LuaIndexKey::Idx(_) => {
                sequence_field_count += 1;
                sequence_value_kinds.push(sequence_value_kind(field.get_value_expr()));
            }
            LuaIndexKey::Expr(_) => {
                expr_field_count += 1;
            }
        }
    }

    let (kind, sequence_kind, sequence_len) = if named_field_count == 0
        && integer_field_count == 0
        && sequence_field_count == 0
        && expr_field_count == 0
    {
        (
            SalsaTableShapeKindSummary::Empty,
            SalsaSequenceShapeKindSummary::None,
            0,
        )
    } else if sequence_field_count > 0
        && named_field_count == 0
        && integer_field_count == 0
        && expr_field_count == 0
    {
        (
            SalsaTableShapeKindSummary::SequenceLike,
            classify_sequence_shape_kind(&sequence_value_kinds),
            sequence_field_count,
        )
    } else if expr_field_count > 0 || sequence_field_count > 0 {
        (
            SalsaTableShapeKindSummary::Mixed,
            SalsaSequenceShapeKindSummary::None,
            sequence_field_count,
        )
    } else {
        (
            SalsaTableShapeKindSummary::ObjectLike,
            SalsaSequenceShapeKindSummary::None,
            0,
        )
    };

    SalsaTableShapeSummary {
        syntax_id: table_expr.get_syntax_id().into(),
        syntax_offset: TextSize::from(u32::from(table_expr.get_position())),
        kind,
        sequence_kind,
        sequence_len,
    }
}

fn classify_sequence_shape_kind(
    sequence_value_kinds: &[SequenceValueKind],
) -> SalsaSequenceShapeKindSummary {
    if sequence_value_kinds.len() <= 1 {
        return SalsaSequenceShapeKindSummary::ArrayLike;
    }

    let distinct = sequence_value_kinds
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if distinct.len() > 1 {
        SalsaSequenceShapeKindSummary::TupleLike
    } else {
        SalsaSequenceShapeKindSummary::ArrayLike
    }
}

fn sequence_value_kind(value_expr: Option<LuaExpr>) -> SequenceValueKind {
    match value_expr {
        Some(LuaExpr::LiteralExpr(literal_expr)) => match literal_expr.get_literal() {
            Some(LuaLiteralToken::Number(_)) => SequenceValueKind::NumberLiteral,
            Some(LuaLiteralToken::String(_)) => SequenceValueKind::StringLiteral,
            Some(LuaLiteralToken::Bool(_)) => SequenceValueKind::BoolLiteral,
            Some(LuaLiteralToken::Nil(_)) => SequenceValueKind::NilLiteral,
            Some(LuaLiteralToken::Dots(_) | LuaLiteralToken::Question(_)) => {
                SequenceValueKind::OtherLiteral
            }
            None => SequenceValueKind::OtherLiteral,
        },
        Some(LuaExpr::ClosureExpr(_)) => SequenceValueKind::Closure,
        Some(LuaExpr::TableExpr(_)) => SequenceValueKind::Table,
        Some(LuaExpr::NameExpr(_)) => SequenceValueKind::Name,
        Some(LuaExpr::IndexExpr(_)) => SequenceValueKind::Member,
        Some(LuaExpr::CallExpr(_)) => SequenceValueKind::Call,
        Some(_) | None => SequenceValueKind::Other,
    }
}
