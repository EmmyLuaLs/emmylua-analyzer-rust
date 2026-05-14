use super::*;

#[test]
fn test_summary_builder_type_query_resolves_local_decl_and_name_use() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local value = init()
print(value)"#;
    set_test_file(&mut compilation, 30, "C:/ws/type_local.lua", source);

    let decl_tree = compilation
        .file()
        .decl_tree(FileId::new(30))
        .expect("decl tree summary");
    let decl = decl_tree
        .decls
        .iter()
        .find(|decl| {
            decl.name == "value" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. })
        })
        .expect("local value decl");

    let decl_type = compilation
        .types()
        .decl(FileId::new(30), decl.id)
        .expect("decl type summary");
    assert_eq!(decl_type.name, "value");
    assert_eq!(decl_type.explicit_type_offsets.len(), 1);
    assert!(decl_type.initializer_offset.is_some());

    let doc_summary = compilation
        .doc()
        .summary(FileId::new(30))
        .expect("doc summary");
    let doc_tag = doc_summary.type_tags.first().expect("type doc tag");
    assert_eq!(
        doc_tag.type_offsets.as_slice(),
        decl_type.explicit_type_offsets.as_slice()
    );

    let name_offset = source.rfind("value").expect("name use offset") as u32;
    let name_type = compilation
        .types()
        .name(FileId::new(30), TextSize::from(name_offset))
        .expect("name type summary");
    assert_eq!(name_type.name_use.name, "value");
    assert_eq!(name_type.decl_type, Some(decl_type));
}

#[test]
fn test_summary_builder_type_query_resolves_param_doc_type() {
    let mut compilation = setup_compilation();
    let source = r#"---@param value integer
local function test(value)
  return value
end"#;
    set_test_file(&mut compilation, 31, "C:/ws/type_param.lua", source);

    let decl_tree = compilation
        .file()
        .decl_tree(FileId::new(31))
        .expect("decl tree summary");
    let param_decl = decl_tree
        .decls
        .iter()
        .find(|decl| {
            decl.name == "value" && matches!(decl.kind, SalsaDeclKindSummary::Param { .. })
        })
        .expect("param value decl");

    let decl_type = compilation
        .types()
        .decl(FileId::new(31), param_decl.id)
        .expect("param type summary");
    assert_eq!(decl_type.name, "value");
    assert_eq!(decl_type.explicit_type_offsets.len(), 1);
    assert!(decl_type.signature_offset.is_some());
}

#[test]
fn test_summary_builder_type_query_resolves_global_candidates() {
    let mut compilation = setup_compilation();
    let source = r#"---@type integer
foo = 1
print(foo)"#;
    set_test_file(&mut compilation, 32, "C:/ws/type_global.lua", source);

    let decl_tree = compilation
        .file()
        .decl_tree(FileId::new(32))
        .expect("decl tree summary");
    let global_decl = decl_tree
        .decls
        .iter()
        .find(|decl| decl.name == "foo" && matches!(decl.kind, SalsaDeclKindSummary::Global))
        .expect("foo global decl");
    let decl_type = compilation
        .types()
        .decl(FileId::new(32), global_decl.id)
        .expect("foo global decl type");

    let name_offset = source.rfind("foo").expect("global name use offset") as u32;
    let global_type = compilation
        .types()
        .global_name(FileId::new(32), TextSize::from(name_offset))
        .expect("global type info");
    assert_eq!(global_type.name, "foo");
    assert!(global_type.candidates.iter().any(|candidate| {
        candidate.explicit_type_offsets == decl_type.explicit_type_offsets
            && candidate.initializer_offset == decl_type.initializer_offset
    }));
}

#[test]
fn test_summary_builder_type_query_resolves_member_candidates() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Box
local Box = {
  ---@type integer
  value = 1,
}

---@param x integer
function Box:run(x)
  return self.value + x
end

Box.value = Box.value
print(Box.value)"#;
    set_test_file(&mut compilation, 33, "C:/ws/type_member.lua", source);

    let member_offset = source.rfind("Box.value").expect("member use offset") as u32;
    let member_type = compilation
        .types()
        .member_use(FileId::new(33), TextSize::from(member_offset))
        .expect("member type info");
    assert_eq!(member_type.target.member_name, "value");
    assert!(
        member_type
            .candidates
            .iter()
            .any(|candidate| !candidate.explicit_type_offsets.is_empty())
    );
    assert!(
        member_type
            .candidates
            .iter()
            .any(|candidate| candidate.initializer_offset.is_some())
    );
}

#[test]
fn test_summary_builder_type_query_resolves_program_point_name_type() {
    let mut compilation = setup_compilation();
    let source = r#"---@type string
local value = init()
print(value)"#;
    set_test_file(&mut compilation, 34, "C:/ws/type_program_point.lua", source);

    let name_offset = source.rfind("value").expect("name use offset") as u32;
    let info = compilation
        .types()
        .name_at(
            FileId::new(34),
            TextSize::from(name_offset),
            TextSize::from(name_offset),
        )
        .expect("program point type info");
    assert_eq!(info.syntax_offset, TextSize::from(name_offset));
    assert_eq!(info.program_point_offset, TextSize::from(name_offset));
    assert_eq!(info.name_use.name, "value");
    assert!(info.base_decl_type.is_some());
    assert_eq!(info.candidates.len(), 1);
}

#[test]
fn test_summary_builder_type_query_attaches_class_and_enum_named_types_to_decl() {
    let mut compilation = setup_compilation();
    let source = r#"---@class Box
local Box = { value = 1 }

---@enum Mode: integer
local Mode = { Fast = 1 }"#;
    set_test_file(&mut compilation, 35, "C:/ws/type_named_type.lua", source);

    let decl_tree = compilation
        .file()
        .decl_tree(FileId::new(35))
        .expect("decl tree summary");
    let box_decl = decl_tree
        .decls
        .iter()
        .find(|decl| decl.name == "Box" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
        .expect("Box decl");
    let mode_decl = decl_tree
        .decls
        .iter()
        .find(|decl| decl.name == "Mode" && matches!(decl.kind, SalsaDeclKindSummary::Local { .. }))
        .expect("Mode decl");

    let box_type = compilation
        .types()
        .decl(FileId::new(35), box_decl.id)
        .expect("Box decl type");
    let mode_type = compilation
        .types()
        .decl(FileId::new(35), mode_decl.id)
        .expect("Mode decl type");

    assert_eq!(box_type.named_type_names, vec!["Box"]);
    assert_eq!(mode_type.named_type_names, vec!["Mode"]);
}
