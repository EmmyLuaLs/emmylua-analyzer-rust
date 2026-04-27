use super::*;

#[test]
fn test_summary_builder_table_shape_classifies_empty_object_sequence_and_mixed_tables() {
    let mut compilation = setup_compilation();
    let source = r#"local empty = {}
local object = { value = 1, name = "x" }
local array = { 1, 2, 3 }
local tuple = { 1, "x" }
local mixed = { value = 1, 2 }"#;
    set_test_file(&mut compilation, 155, "C:/ws/table_shape.lua", source);

    let table_shapes = compilation
        .file()
        .table_shapes(FileId::new(155))
        .expect("table shape summary");

    assert_eq!(table_shapes.tables.len(), 5);

    assert_eq!(
        table_shapes.tables[0].kind,
        crate::SalsaTableShapeKindSummary::Empty
    );
    assert_eq!(
        table_shapes.tables[1].kind,
        crate::SalsaTableShapeKindSummary::ObjectLike
    );
    assert_eq!(
        table_shapes.tables[2].kind,
        crate::SalsaTableShapeKindSummary::SequenceLike
    );
    assert_eq!(
        table_shapes.tables[2].sequence_kind,
        crate::SalsaSequenceShapeKindSummary::ArrayLike
    );
    assert_eq!(table_shapes.tables[2].sequence_len, 3);
    assert_eq!(
        table_shapes.tables[3].kind,
        crate::SalsaTableShapeKindSummary::SequenceLike
    );
    assert_eq!(
        table_shapes.tables[3].sequence_kind,
        crate::SalsaSequenceShapeKindSummary::TupleLike
    );
    assert_eq!(table_shapes.tables[3].sequence_len, 2);
    assert_eq!(
        table_shapes.tables[4].kind,
        crate::SalsaTableShapeKindSummary::Mixed
    );
}
