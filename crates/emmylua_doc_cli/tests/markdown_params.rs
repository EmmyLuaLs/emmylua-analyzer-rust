//! Regression test for parameter / return-value documentation in the
//! markdown output.
//!
//! The HTML generator renders `@param` / `@return` descriptions as a dedicated
//! "Parameters" / "Returns" table, but the markdown generator used to either
//! drop them or dump raw `@param \`name\` - ...` lines into the page.  This test
//! checks that a function member with documented parameters produces a proper
//! `**Parameters**` / `**Returns**` block and no raw `@param` / `@return` lines.

use emmylua_doc_cli::{CmdArgs, Format, OutputDestination, run_doc_cli};
use std::path::{Path, PathBuf};

const SOURCE: &str = r#"local M = {}

---@param a number @the first operand
---@param b number @the second operand
---@return number @the sum
function M.add(a, b)
    return a + b
end

return M
"#;

fn fixture_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("markdown_params");
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
fn markdown_renders_parameter_and_return_descriptions() {
    let dir = fixture_dir();
    let md = generate_module_markdown(&dir);

    assert!(
        md.contains("**Parameters**"),
        "missing `**Parameters**` section:\n{md}"
    );
    assert!(
        md.contains("**Returns**"),
        "missing `**Returns**` section:\n{md}"
    );
    assert!(
        md.contains("the first operand"),
        "missing description of parameter `a`:\n{md}"
    );
    assert!(
        md.contains("the second operand"),
        "missing description of parameter `b`:\n{md}"
    );
    assert!(
        md.contains("the sum"),
        "missing return-value description:\n{md}"
    );

    // The old implementation wrote raw `@param` / `@return` lines into the
    // page; make sure they are gone now that a dedicated block is rendered.
    assert!(
        !md.contains("@param `"),
        "raw `@param` line leaked into the markdown:\n{md}"
    );
    assert!(
        !md.contains("@return "),
        "raw `@return` line leaked into the markdown:\n{md}"
    );
}
