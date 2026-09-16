//! Regression test for `---@overload` rendering in the markdown output.
//!
//! The HTML generator renders `---@overload` signatures in a dedicated
//! "Overloads" block, but the markdown generator used to drop them entirely.
//! This test checks that the overload signature appears in the markdown.

use emmylua_doc_cli::{CmdArgs, Format, OutputDestination, run_doc_cli};
use std::path::{Path, PathBuf};

const SOURCE: &str = r#"local M = {}

---@param a number @the first operand
---@return number @the sum
---@overload fun(text: string): string
function M.add(a)
    return a
end

return M
"#;

fn fixture_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("markdown_overloads");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.lua"), SOURCE).unwrap();
    dir
}

/// Generate the markdown for the fixture and return the module page.
fn generate_module_markdown(dir: &Path) -> String {
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

    let modules_dir = out.join("docs").join("modules");
    let mut pages: Vec<PathBuf> = std::fs::read_dir(&modules_dir)
        .expect("modules dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    pages.sort();
    assert_eq!(pages.len(), 1, "expected exactly one module page");
    std::fs::read_to_string(&pages[0]).expect("read module markdown")
}

#[test]
fn markdown_renders_overload_signatures() {
    let dir = fixture_dir();
    let md = generate_module_markdown(&dir);

    assert!(
        md.contains("**Overloads**"),
        "missing `**Overloads**` section:\n{md}"
    );
    assert!(
        md.contains("text: string"),
        "missing overload signature:\n{md}"
    );
}
