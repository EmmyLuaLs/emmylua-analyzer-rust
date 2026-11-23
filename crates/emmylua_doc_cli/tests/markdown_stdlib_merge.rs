//! Regression test for stdlib members merged into workspace-declared classes.
//!
//! `DocModel` builds pages only for types declared in the main workspace, but
//! must merge matching stdlib definitions into them: a workspace
//! `---@class string` has to keep `string.gsub`, `string.format`, etc.
//!
//! Global classes also need their runtime table members (`function MyLib.foo`)
//! attached to the page; those members are owned by `SemanticId::Name`, not by
//! the type definition itself.

mod common;

const STDLIB_SOURCE: &str = r#"--- Workspace string class.
---@class string
---@field tarantool_extra fun(): string
string = {}
"#;

const GLOBAL_CLASS_SOURCE: &str = r#"---@class mylib
mylib = {}

---@param x number
---@return number
function mylib.foo(x)
    return x
end
"#;

const UNRELATED_SOURCE: &str = r#"---@class mylib
local M = {}

return M
"#;

#[test]
fn workspace_class_page_includes_stdlib_members() {
    let md = common::generate_type_markdown("markdown_stdlib_merge", STDLIB_SOURCE, "string");

    assert!(
        md.contains("tarantool_extra"),
        "workspace field was dropped:\n{md}"
    );
    assert!(
        md.contains("string.gsub"),
        "stdlib member was dropped:\n{md}"
    );
    assert!(
        md.contains("string.format"),
        "stdlib member was dropped:\n{md}"
    );
    assert_eq!(
        md.lines().filter(|line| *line == "### string.gsub").count(),
        1,
        "expected exactly one stdlib member heading: {md}"
    );

    // Page metadata must come from the workspace declaration, not from stdlib.
    assert!(
        md.contains("Workspace string class."),
        "workspace type description was dropped:\n{md}"
    );
    assert!(
        !md.contains("immutable sequences of bytes"),
        "stdlib description overrode the workspace one:\n{md}"
    );
}

#[test]
fn global_class_page_includes_runtime_members() {
    let md =
        common::generate_type_markdown("markdown_global_class_merge", GLOBAL_CLASS_SOURCE, "mylib");

    assert!(
        md.contains("mylib.foo"),
        "global runtime member was dropped:\n{md}"
    );
    assert_eq!(
        md.lines().filter(|line| *line == "### mylib.foo").count(),
        1,
        "expected exactly one global runtime member heading: {md}"
    );
}

#[test]
fn stdlib_only_types_do_not_get_pages() {
    let out = common::generate_fixture("markdown_stdlib_only", UNRELATED_SOURCE);
    let types = out.join("docs").join("types");
    let mut pages: Vec<String> = std::fs::read_dir(&types)
        .unwrap_or_else(|err| panic!("read types dir {}: {err}", types.display()))
        .map(|entry| entry.expect("read types dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    pages.sort();

    assert_eq!(
        pages,
        vec!["mylib.md"],
        "stdlib-only types must not produce pages"
    );
}
