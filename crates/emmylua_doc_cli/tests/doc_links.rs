//! Rust-like intra-doc links in generated HTML and Markdown.

mod common;

use emmylua_doc_cli::Format;

const CSV_SOURCE: &str = r#"local csv = {}

---Get CSV-formatted input from `readable`.
---
---See [`Widget`], [`Widget.name`], [`ns.Shape`], [`Shape`], [`Status`], [`Status.Active`],
---[`Handler`], [`fio`], [`fio.open`], [`g_config`], [`g_config.get`],
---[`csv.load`], [`Missing`], [`Duplicate`], [Widget] and [text](https://example.com).
---
---```txt
---See [`FenceOnly`].
---```
---
---Inline `[`InlineOnly`]`.
---@param value string See [`Widget`].
---@return string @See [`Widget.name`].
function csv.load(value) end

return csv
"#;

const FIO_SOURCE: &str = r#"local fio = {}

---Open a file. See [`Widget`].
function fio.open() end

return fio
"#;

const WIDGET_SOURCE: &str = r#"---A widget. See [`Widget.name`], [`fio`] and [`g_config`].
---@class Widget
---@field name string
---@field render fun(self: Widget): string
local Widget = {}

return Widget
"#;

const TYPES_SOURCE: &str = r#"---@class ns.Shape
---@field area number
local Shape = {}

---Status values. See [`Widget`].
---@enum Status
local Status = {
    Active = 1,
    Inactive = 2,
}

---Handler type.
---@alias Handler fun(value: string): boolean

---@class a.Duplicate
local A = {}

---@class b.Duplicate
local B = {}

---@class FenceOnly
---@class InlineOnly
local Hidden = {}
"#;

const GLOBALS_SOURCE: &str = r#"---Global config. See [`Widget`].
g_config = {}

---Get config.
function g_config.get() end
"#;

fn fixture(fixture_name: &str, format: Format) -> std::path::PathBuf {
    common::generate_fixture_files_with_format(
        fixture_name,
        &[
            ("csv.lua", CSV_SOURCE),
            ("fio.lua", FIO_SOURCE),
            ("widget.lua", WIDGET_SOURCE),
            ("types.lua", TYPES_SOURCE),
            ("globals.lua", GLOBALS_SOURCE),
        ],
        format,
    )
}

fn read_page(path: &std::path::Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("read page {}: {err}", path.display()))
}

fn assert_contains_all(text: &str, expected: &[&str]) {
    for expected in expected {
        assert!(text.contains(expected), "missing {expected}:\n{text}");
    }
}

#[test]
fn html_resolves_intra_doc_links() {
    let out = fixture("doc_links_html", Format::Html);
    let html = read_page(&out.join("module").join("csv.html"));

    assert_contains_all(
        &html,
        &[
            r#"<a href="../class/Widget.html"><code>Widget</code></a>"#,
            r#"<a href="../class/Widget.html#Widget.name"><code>Widget.name</code></a>"#,
            r#"<a href="../class/ns.Shape.html"><code>ns.Shape</code></a>"#,
            r#"<a href="../class/ns.Shape.html"><code>Shape</code></a>"#,
            r#"<a href="../enum/Status.html"><code>Status</code></a>"#,
            r#"<a href="../enum/Status.html#Status.Active"><code>Status.Active</code></a>"#,
            r#"<a href="../alias/Handler.html"><code>Handler</code></a>"#,
            r#"<a href="../module/fio.html"><code>fio</code></a>"#,
            r#"<a href="../module/fio.html#fio.open"><code>fio.open</code></a>"#,
            r#"<a href="../global/g_config.html"><code>g_config</code></a>"#,
            r##"<a href="#csv.load"><code>csv.load</code></a>"##,
            r#"<a href="https://example.com">text</a>"#,
            "<code>Missing</code>",
            "<code>Duplicate</code>",
            "[Widget]",
        ],
    );
    assert!(
        !html.contains("href=\"Missing\""),
        "unresolved link must not get an href:\n{html}"
    );
    assert!(
        !html.contains("<code>FenceOnly</code>") && !html.contains("<code>InlineOnly</code>"),
        "links inside code must not be resolved:\n{html}"
    );

    let widget = read_page(&out.join("class").join("Widget.html"));
    assert_contains_all(
        &widget,
        &[
            r##"<a href="#Widget.name"><code>Widget.name</code></a>"##,
            r#"<a href="../module/fio.html"><code>fio</code></a>"#,
            r#"<a href="../global/g_config.html"><code>g_config</code></a>"#,
        ],
    );

    let global = read_page(&out.join("global").join("g_config.html"));
    assert_contains_all(
        &global,
        &[r#"<a href="../class/Widget.html"><code>Widget</code></a>"#],
    );
}

#[test]
fn markdown_resolves_intra_doc_links() {
    let out = fixture("doc_links_markdown", Format::Markdown);
    let md = read_page(&out.join("docs").join("modules").join("csv.md"));

    assert_contains_all(
        &md,
        &[
            "[`Widget`](../types/Widget.md)",
            "[`Widget.name`](../types/Widget.md#Widget.name)",
            "[`ns.Shape`](../types/ns.Shape.md)",
            "[`Shape`](../types/ns.Shape.md)",
            "[`Status`](../types/Status.md)",
            "[`Status.Active`](../types/Status.md#Status.Active)",
            "[`Handler`](../types/Handler.md)",
            "[`fio`](fio.md)",
            "[`fio.open`](fio.md#fio.open)",
            "[`g_config`](../globals/g_config.md)",
            "[`g_config.get`](../globals/g_config.md#g_config.get)",
            "[`csv.load`](#csv.load)",
            "[text](https://example.com)",
            "- **value** (*string*) - See [`Widget`](../types/Widget.md)",
            "- (*string*) - See [`Widget.name`](../types/Widget.md#Widget.name)",
        ],
    );
    assert!(
        md.contains("`Missing`") && !md.contains("[`Missing`]"),
        "unresolved link should become a code span:\n{md}"
    );
    assert!(
        md.contains("`Duplicate`") && !md.contains("[`Duplicate`]"),
        "ambiguous short name should become a code span:\n{md}"
    );
    assert!(
        md.contains("[Widget]"),
        "plain Markdown links must stay unchanged:\n{md}"
    );
    assert!(
        !md.contains("types/FenceOnly.md") && !md.contains("types/InlineOnly.md"),
        "links inside code must not be resolved:\n{md}"
    );

    let widget = read_page(&out.join("docs").join("types").join("Widget.md"));
    assert_contains_all(
        &widget,
        &[
            "[`Widget.name`](#Widget.name)",
            "[`fio`](../modules/fio.md)",
            "[`g_config`](../globals/g_config.md)",
            r#"<a id="Widget.name"></a>"#,
            r#"<a id="Widget.render"></a>"#,
        ],
    );

    let status = read_page(&out.join("docs").join("types").join("Status.md"));
    assert_contains_all(&status, &[r#"<a id="Status.Active"></a>"#]);

    let global = read_page(&out.join("docs").join("globals").join("g_config.md"));
    assert_contains_all(&global, &["[`Widget`](../types/Widget.md)"]);
}

#[test]
fn markdown_does_not_rewrite_code_spans_or_fences() {
    let out = fixture("doc_links_markdown_code", Format::Markdown);
    let md = read_page(&out.join("docs").join("modules").join("csv.md"));
    assert_contains_all(&md, &["See [`FenceOnly`].", "Inline `[`InlineOnly`]`."]);
}
