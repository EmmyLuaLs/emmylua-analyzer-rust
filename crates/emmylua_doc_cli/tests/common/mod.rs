#![allow(dead_code)]

use emmylua_doc_cli::{CmdArgs, Format, OutputDestination, run_doc_cli};
use std::path::{Path, PathBuf};

/// Generates the markdown for a single-module fixture and returns the module page.
pub fn generate_module_markdown(fixture_name: &str, source: &str) -> String {
    let out = generate_fixture(fixture_name, source);
    read_single_page(&out.join("docs").join("modules"), "module")
}

/// Generates the markdown for a global-only fixture and returns the global page.
pub fn generate_global_markdown(fixture_name: &str, source: &str) -> String {
    let out = generate_fixture(fixture_name, source);
    read_single_page(&out.join("docs").join("globals"), "global")
}

/// Generates the markdown for a type-only fixture and returns its type page.
pub fn generate_type_markdown(fixture_name: &str, source: &str, type_name: &str) -> String {
    let out = generate_fixture(fixture_name, source);
    let page = out
        .join("docs")
        .join("types")
        .join(format!("{type_name}.md"));
    std::fs::read_to_string(&page)
        .unwrap_or_else(|err| panic!("read type page {}: {err}", page.display()))
}

pub fn generate_fixture(fixture_name: &str, source: &str) -> PathBuf {
    generate_fixture_with_format(fixture_name, source, Format::Markdown)
}

/// Generates an HTML fixture and returns the output directory.
pub fn generate_html_fixture(fixture_name: &str, source: &str) -> PathBuf {
    generate_fixture_with_format(fixture_name, source, Format::Html)
}

/// Reads the generated HTML search index as a list of JSON entries.
pub fn read_search_index(out: &Path) -> Vec<serde_json::Value> {
    let path = out.join("static").join("search-index.js");
    let js = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read search index {}: {err}", path.display()));
    let json = js
        .strip_prefix("window.SEARCH_INDEX = ")
        .and_then(|rest| rest.strip_suffix(';'))
        .unwrap_or_else(|| panic!("unexpected search index format: {js}"));
    serde_json::from_str(json)
        .unwrap_or_else(|err| panic!("parse search index {}: {err}", path.display()))
}

/// Generates a JSON fixture and returns the generated `doc.json` contents.
pub fn generate_json_fixture(fixture_name: &str, source: &str) -> serde_json::Value {
    let out = generate_fixture_with_format(fixture_name, source, Format::Json);
    let page = out.join("doc.json");
    let json = std::fs::read_to_string(&page)
        .unwrap_or_else(|err| panic!("read json file {}: {err}", page.display()));
    serde_json::from_str(&json)
        .unwrap_or_else(|err| panic!("parse generated json {}: {err}", page.display()))
}

/// Extracts the content of a `<section ... id="...">...</section>` block.
///
/// This keeps assertions scoped to a single section, so a `section-count` in
/// the fields section cannot accidentally satisfy a methods-section check.
/// Assumes generated sections are flat (not nested).
pub fn html_section<'a>(html: &'a str, id: &str) -> Option<&'a str> {
    let marker = format!("id=\"{id}\"");
    let mut cursor = 0;
    while let Some(rel) = html[cursor..].find("<section") {
        let open = cursor + rel;
        let tag_end = html[open..].find('>')? + open;
        if html[open..tag_end].contains(&marker) {
            let end = html[tag_end..].find("</section>")? + tag_end;
            return Some(&html[open..end]);
        }
        cursor = tag_end;
    }
    None
}

/// Member names exported in a JSON module or global object, in export order.
pub fn json_member_names(value: &serde_json::Value) -> Vec<String> {
    value["members"]
        .as_array()
        .unwrap_or_else(|| panic!("missing members array: {value}"))
        .iter()
        .map(|member| {
            member["name"]
                .as_str()
                .unwrap_or_else(|| panic!("member without a name: {member}"))
                .to_string()
        })
        .collect()
}

/// Member names in sorted order, for order-insensitive assertions.
pub fn json_member_names_sorted(value: &serde_json::Value) -> Vec<String> {
    let mut names = json_member_names(value);
    names.sort();
    names
}

fn generate_fixture_with_format(fixture_name: &str, source: &str, format: Format) -> PathBuf {
    generate_fixture_files_with_format(fixture_name, &[("mod.lua", source)], format)
}

pub fn generate_fixture_files_with_format(
    fixture_name: &str,
    files: &[(&str, &str)],
    format: Format,
) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(fixture_name);
    // The fixture directory does not exist on the first run.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    for (name, source) in files {
        std::fs::write(dir.join(name), source).expect("write fixture");
    }

    let out = dir.join("out");
    let args = CmdArgs {
        config: None,
        workspace: vec![dir.to_path_buf()],
        exclude_pattern: None,
        include_pattern: None,
        output_format: format,
        output: OutputDestination::File(out.clone()),
        override_template: None,
        site_name: None,
        mixin: None,
        verbose: false,
    };
    run_doc_cli(args).expect("run_doc_cli failed");

    out
}

fn read_single_page(dir: &Path, kind: &str) -> String {
    let mut pages: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("read {kind} dir {}: {err}", dir.display()))
        .map(|entry| entry.expect("read docs dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    pages.sort();
    assert_eq!(pages.len(), 1, "expected exactly one {kind} page");
    std::fs::read_to_string(&pages[0]).expect("read page markdown")
}
