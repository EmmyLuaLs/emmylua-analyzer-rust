use super::*;
use crate::*;

#[test]
fn test_summary_builder_property_structure() {
    let mut compilation = setup_compilation();
    let source = r#"local M = { name = "demo", nested = { enabled = true }, run = function() end }
GG = { value = 1 }
---@class User
---@field id integer
---@field name string"#;
    set_test_file(&mut compilation, 7, "C:/ws/properties.lua", source);

    let file = compilation.file();
    let properties = file.properties(FileId::new(7)).expect("property summary");
    let decl_tree = file.decl_tree(FileId::new(7)).expect("decl tree");

    let m_decl_id = decl_tree
        .decls
        .iter()
        .find(|decl| decl.name == "M" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
        .map(|decl| decl.id)
        .expect("M decl id");

    assert!(properties.properties.iter().any(|property| matches!(
        (&property.owner, &property.key, &property.source, &property.kind),
        (
            SalsaPropertyOwnerSummary::Decl { name, is_global: false, .. },
            SalsaPropertyKeySummary::Name(key),
            SalsaPropertySourceSummary::TableField,
            SalsaPropertyKindSummary::Value,
        ) if name == "M" && key == "name"
    )));
    assert!(properties.properties.iter().any(|property| matches!(
        (&property.owner, &property.key, &property.source, &property.kind),
        (
            SalsaPropertyOwnerSummary::Member(target),
            SalsaPropertyKeySummary::Name(key),
            SalsaPropertySourceSummary::TableField,
            SalsaPropertyKindSummary::Value,
        ) if matches!(&target.root, SalsaMemberRootSummary::LocalDecl { name, .. } if name == "M")
            && target.member_name == "nested" && key == "enabled"
    )));
    assert!(properties.properties.iter().any(|property| matches!(
        (&property.owner, &property.key, &property.source, &property.kind),
        (
            SalsaPropertyOwnerSummary::Decl { name, is_global: false, .. },
            SalsaPropertyKeySummary::Name(key),
            SalsaPropertySourceSummary::TableField,
            SalsaPropertyKindSummary::Function,
        ) if name == "M" && key == "run"
    )));
    assert!(properties.properties.iter().any(|property| matches!(
        (&property.owner, &property.key, &property.source),
        (
            SalsaPropertyOwnerSummary::Decl { name, is_global: true, .. },
            SalsaPropertyKeySummary::Name(key),
            SalsaPropertySourceSummary::TableField,
        ) if name == "GG" && key == "value"
    )));
    assert!(properties.properties.iter().any(|property| matches!(
        (&property.owner, &property.key, &property.source),
        (
            SalsaPropertyOwnerSummary::Type(name),
            SalsaPropertyKeySummary::Name(key),
            SalsaPropertySourceSummary::DocField,
        ) if name == "User" && key == "id"
    )));
    assert!(properties.properties.iter().any(|property| matches!(
        (&property.owner, &property.key, &property.source),
        (
            SalsaPropertyOwnerSummary::Type(name),
            SalsaPropertyKeySummary::Name(key),
            SalsaPropertySourceSummary::DocField,
        ) if name == "User" && key == "name"
    )));

    let nested_enabled = properties
        .properties
        .iter()
        .find(|property| {
            matches!(
                (&property.owner, &property.key),
                (
                    SalsaPropertyOwnerSummary::Member(target),
                    SalsaPropertyKeySummary::Name(key),
                ) if matches!(&target.root, SalsaMemberRootSummary::LocalDecl { name, .. } if name == "M")
                    && target.member_name == "nested" && key == "enabled"
            )
        })
        .cloned()
        .expect("M.nested.enabled property");
    let nested_enabled_owner = match &nested_enabled.owner {
        SalsaPropertyOwnerSummary::Member(target) => target.clone(),
        _ => unreachable!("nested_enabled owner must be member"),
    };

    assert_eq!(
        file.property_at(FileId::new(7), nested_enabled.syntax_offset),
        Some(nested_enabled.clone())
    );
    assert_eq!(
        file.properties_for_decl(FileId::new(7), m_decl_id),
        Some(
            properties
                .properties
                .iter()
                .filter(|property| matches!(
                    property.owner,
                    SalsaPropertyOwnerSummary::Decl { decl_id, .. } if decl_id == m_decl_id
                ))
                .cloned()
                .collect()
        )
    );
    assert_eq!(
        file.properties_for_member(FileId::new(7), nested_enabled_owner.clone()),
        Some(vec![nested_enabled.clone()])
    );
    assert_eq!(
        file.properties_for_type(FileId::new(7), "User".into()),
        Some(
            properties
                .properties
                .iter()
                .filter(|property| matches!(
                    &property.owner,
                    SalsaPropertyOwnerSummary::Type(name) if name == "User"
                ))
                .cloned()
                .collect()
        )
    );
    assert_eq!(
        file.properties_for_source(FileId::new(7), SalsaPropertySourceSummary::DocField),
        Some(
            properties
                .properties
                .iter()
                .filter(|property| property.source == SalsaPropertySourceSummary::DocField)
                .cloned()
                .collect()
        )
    );
    assert_eq!(
        file.properties_for_key(FileId::new(7), SalsaPropertyKeySummary::Name("name".into())),
        Some(
            properties
                .properties
                .iter()
                .filter(|property| { property.key == SalsaPropertyKeySummary::Name("name".into()) })
                .cloned()
                .collect()
        )
    );
    assert_eq!(
        file.properties_for_decl_and_key(
            FileId::new(7),
            m_decl_id,
            SalsaPropertyKeySummary::Name("name".into())
        ),
        Some(
            properties
                .properties
                .iter()
                .filter(|property| {
                    matches!(
                        property.owner,
                        SalsaPropertyOwnerSummary::Decl { decl_id, .. } if decl_id == m_decl_id
                    ) && property.key == SalsaPropertyKeySummary::Name("name".into())
                })
                .cloned()
                .collect()
        )
    );
    assert_eq!(
        file.properties_for_member_and_key(
            FileId::new(7),
            nested_enabled_owner,
            SalsaPropertyKeySummary::Name("enabled".into())
        ),
        Some(vec![nested_enabled.clone()])
    );
    assert_eq!(
        file.properties_for_type_and_key(
            FileId::new(7),
            "User".into(),
            SalsaPropertyKeySummary::Name("name".into())
        ),
        Some(
            properties
                .properties
                .iter()
                .filter(|property| {
                    matches!(
                        &property.owner,
                        SalsaPropertyOwnerSummary::Type(name) if name == "User"
                    ) && property.key == SalsaPropertyKeySummary::Name("name".into())
                })
                .cloned()
                .collect()
        )
    );
}

#[test]
fn test_summary_builder_property_expr_key_uses_syntax_id() {
    let code = r#"local tag = { id = 1 }
local M = { [tag] = 1, [tag.id] = 2 }"#;
    let mut compilation = setup_compilation();
    set_test_file(&mut compilation, 20, "C:/ws/property_expr_key.lua", code);

    let tree = LuaParser::parse(code, ParserConfig::default());
    let chunk = tree.get_chunk_node();
    let mut expr_syntax_ids = chunk
        .descendants::<LuaTableField>()
        .filter_map(|field| match field.get_field_key()? {
            LuaIndexKey::Expr(expr) => Some(expr.get_syntax_id().into()),
            _ => None,
        });

    let tag_key_syntax_id = expr_syntax_ids.next().expect("[tag] syntax id");
    let tag_member_key_syntax_id = expr_syntax_ids.next().expect("[tag.id] syntax id");
    assert_ne!(tag_key_syntax_id, tag_member_key_syntax_id);

    let file = compilation.file();
    let tag_properties = file.properties_for_key(
        FileId::new(20),
        SalsaPropertyKeySummary::Expr(tag_key_syntax_id),
    );
    let tag_member_properties = file.properties_for_key(
        FileId::new(20),
        SalsaPropertyKeySummary::Expr(tag_member_key_syntax_id),
    );

    assert_eq!(
        tag_properties.as_ref().map(|properties| properties.len()),
        Some(1)
    );
    assert_eq!(
        tag_member_properties
            .as_ref()
            .map(|properties| properties.len()),
        Some(1)
    );
    assert_ne!(tag_properties, tag_member_properties);
}

#[test]
fn test_summary_builder_property_mirrors_class_and_enum_table_fields_to_type_owner() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Box
local Box = { value = 1, nested = { enabled = true } }

---@enum Mode: integer
local Mode = { Fast = 1, Slow = 2 }"#;
    set_test_file(
        &mut compilation,
        21,
        "C:/ws/property_type_owner.lua",
        source,
    );

    let file = compilation.file();
    let box_type_properties = file
        .properties_for_type(FileId::new(21), "Box".into())
        .expect("Box type properties");
    let mode_type_properties = file
        .properties_for_type(FileId::new(21), "Mode".into())
        .expect("Mode type properties");

    assert!(box_type_properties.iter().any(|property| matches!(
        (&property.owner, &property.key, &property.source),
        (
            SalsaPropertyOwnerSummary::Type(name),
            SalsaPropertyKeySummary::Name(key),
            SalsaPropertySourceSummary::TableField,
        ) if name == "Box" && key == "value"
    )));
    assert!(box_type_properties.iter().any(|property| matches!(
        (&property.owner, &property.key, &property.source, &property.value_expr_offset),
        (
            SalsaPropertyOwnerSummary::Type(name),
            SalsaPropertyKeySummary::Name(key),
            SalsaPropertySourceSummary::TableField,
            Some(_),
        ) if name == "Box" && key == "nested"
    )));
    assert!(mode_type_properties.iter().any(|property| matches!(
        (&property.owner, &property.key, &property.source),
        (
            SalsaPropertyOwnerSummary::Type(name),
            SalsaPropertyKeySummary::Name(key),
            SalsaPropertySourceSummary::TableField,
        ) if name == "Mode" && key == "Fast"
    )));
}

#[test]
fn test_summary_builder_property_does_not_mirror_type_annotated_table_fields_to_type_owner() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Completion2.A
---@field event fun(aaa)

---@type Completion2.A
local a = {
    event = function(aaa)
        return aaa
    end,
}"#;
    set_test_file(
        &mut compilation,
        23,
        "C:/ws/property_type_annotation_owner.lua",
        source,
    );

    let properties = compilation
        .file()
        .properties_for_type(FileId::new(23), "Completion2.A".into())
        .expect("Completion2.A type properties");

    assert!(!properties.iter().any(|property| matches!(
        (&property.owner, &property.key, &property.source, &property.kind, &property.value_expr_offset),
        (
            SalsaPropertyOwnerSummary::Type(name),
            SalsaPropertyKeySummary::Name(key),
            SalsaPropertySourceSummary::TableField,
            SalsaPropertyKindSummary::Function,
            Some(_),
        ) if name == "Completion2.A" && key == "event"
    )));
}

#[test]
fn test_summary_builder_property_type_owner_expands_tail_call_sequence_slot() {
    let mut compilation = setup_compilation();
    let source = r#"---@return integer
---@return string
local function pair()
  return 1, "two"
end

---@class Pair
local Pair = { pair() }"#;
    set_test_file(
        &mut compilation,
        22,
        "C:/ws/property_type_owner_tail_call_slot.lua",
        source,
    );

    let properties = compilation
        .file()
        .properties_for_type_and_key(
            FileId::new(22),
            "Pair".into(),
            SalsaPropertyKeySummary::Sequence(2),
        )
        .expect("Pair sequence slot properties");

    assert_eq!(properties.len(), 1);
    assert_eq!(properties[0].key, SalsaPropertyKeySummary::Sequence(2));
    assert_eq!(properties[0].value_result_index, 1);
    assert!(properties[0].source_call_syntax_id.is_some());
    assert!(properties[0].expands_multi_result_tail);
}

#[test]
fn test_summary_builder_property_decl_owner_expands_tail_call_sequence_slot() {
    let mut compilation = setup_compilation();
    let source = r#"---@return integer
---@return string
local function pair()
  return 1, "two"
end

local holder = { pair() }"#;
    set_test_file(
        &mut compilation,
        24,
        "C:/ws/property_decl_owner_tail_call_slot.lua",
        source,
    );

    let holder_decl_id = compilation
        .file()
        .decl_tree(FileId::new(24))
        .expect("decl tree")
        .decls
        .clone()
        .into_iter()
        .find(|decl| {
            decl.name == "holder" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .map(|decl| decl.id)
        .expect("holder decl id");

    let properties = compilation
        .file()
        .properties_for_decl_and_key(
            FileId::new(24),
            holder_decl_id,
            SalsaPropertyKeySummary::Sequence(2),
        )
        .expect("holder sequence slot properties");

    assert_eq!(properties.len(), 1);
    assert_eq!(properties[0].key, SalsaPropertyKeySummary::Sequence(2));
    assert_eq!(properties[0].value_result_index, 1);
    assert!(properties[0].source_call_syntax_id.is_some());
    assert!(properties[0].expands_multi_result_tail);
}

#[test]
fn test_summary_builder_property_member_owner_expands_tail_call_integer_slot() {
    let mut compilation = setup_compilation();
    let source = r#"---@return integer
---@return string
local function pair()
  return 1, "two"
end

local holder = {
  nested = { pair() },
}"#;
    set_test_file(
        &mut compilation,
        25,
        "C:/ws/property_member_owner_tail_call_slot.lua",
        source,
    );

    let nested_owner = compilation
        .file()
        .properties(FileId::new(25))
        .expect("property summary")
        .properties
        .clone()
        .into_iter()
        .find_map(|property| match (&property.owner, &property.key) {
            (
                SalsaPropertyOwnerSummary::Member(target),
                SalsaPropertyKeySummary::Sequence(1),
            ) if matches!(&target.root, SalsaMemberRootSummary::LocalDecl { name, .. } if name == "holder")
                && target.member_name == "nested" =>
            {
                Some(target.clone())
            }
            _ => None,
        })
        .expect("nested owner target");

    let properties = compilation
        .file()
        .properties_for_member_and_key(
            FileId::new(25),
            nested_owner,
            SalsaPropertyKeySummary::Integer(2),
        )
        .expect("nested integer slot properties");

    assert_eq!(properties.len(), 1);
    assert_eq!(properties[0].key, SalsaPropertyKeySummary::Integer(2));
    assert_eq!(properties[0].value_result_index, 1);
    assert!(properties[0].source_call_syntax_id.is_some());
    assert!(properties[0].expands_multi_result_tail);
}
