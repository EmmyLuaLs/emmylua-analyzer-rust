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

fn generate_fixture(fixture_name: &str, source: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(fixture_name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    std::fs::write(dir.join("mod.lua"), source).expect("write fixture");

    let out = dir.join("out");
    let args = CmdArgs {
        config: None,
        workspace: vec![dir.to_path_buf()],
        exclude_pattern: None,
        include_pattern: None,
        output_format: Format::Markdown,
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
