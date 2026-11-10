//! Regression test for Markdown tables in the HTML output (#1276).
//!
//! Descriptions are rendered by a small CommonMark subset renderer. It used to
//! treat table rows as paragraph lines, so `| a | b |` tables ended up as a
//! single `<p>` of pipe-separated text instead of an HTML table.

mod common;

const SOURCE: &str = r#"--- Module xxx
---
---### xxx
---
--- | aaa | bbb |
--- | --- | --- |
--- | 1 | 2 |
---
function new()
end

return new
"#;

#[test]
fn html_renders_markdown_table() {
    let out = common::generate_html_fixture("html_tables", SOURCE);
    let page = out.join("module").join("mod.html");
    let html = std::fs::read_to_string(&page)
        .unwrap_or_else(|err| panic!("read html page {}: {err}", page.display()));

    assert!(
        html.contains("<div class=\"doc-table\"><table>"),
        "table wrapper was not rendered:\n{html}"
    );
    assert!(html.contains("<th>aaa</th>"), "{html}");
    assert!(html.contains("<th>bbb</th>"), "{html}");
    assert!(html.contains("<td>1</td>"), "{html}");
    assert!(html.contains("<td>2</td>"), "{html}");
    assert!(!html.contains("<p>| aaa"), "{html}");
}
