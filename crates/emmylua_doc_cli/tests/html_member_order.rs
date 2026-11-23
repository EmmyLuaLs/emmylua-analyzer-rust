//! Regression tests for alphabetical ordering in the generated HTML site
//! (#1407).
//!
//! Members used to be rendered in declaration order, so the class page TOC and
//! the "Methods" section did not match the alphabetical listing users expect.
//! Type index entries used to be sorted by their display name (`class Foo`,
//! `alias Bar`), which broke the alphabetical order of the letter groups.

mod common;

const MEMBERS_SOURCE: &str = r#"---@class Assert
local M = {}

function M.test() end
function M.plan() end
function M.check() end
function M.diag() end
function M.ok() end
function M.fail() end
function M.skip() end
function M.is() end
function M.isnt() end
function M.is_deeply() end
function M.like() end
function M.unlike() end

M.zebra = ""
M.apple = ""

return M
"#;

const INDEX_GROUPS_SOURCE: &str = r#"---@class Apple

---@class Avocado

---@alias Zebra string
"#;

const INDEX_ITEMS_SOURCE: &str = r#"---@class Apple

---@alias Avocado string
"#;

const EXPECTED_METHODS: &[&str] = &[
    "check",
    "diag",
    "fail",
    "is",
    "is_deeply",
    "isnt",
    "like",
    "ok",
    "plan",
    "skip",
    "test",
    "unlike",
];

const EXPECTED_FIELDS: &[&str] = &["apple", "zebra"];

fn read_html(out: &std::path::Path, relative: &str) -> String {
    let page = out.join(relative);
    std::fs::read_to_string(&page)
        .unwrap_or_else(|err| panic!("read html page {}: {err}", page.display()))
}

fn assert_member_order(html: &str, owner: &str, names: &[&str]) {
    let mut cursor = 0;
    for name in names {
        let needle = format!("id=\"{owner}.{name}\"");
        let rel = html[cursor..]
            .find(&needle)
            .unwrap_or_else(|| panic!("missing member {owner}.{name} in order:\n{html}"));
        cursor += rel + needle.len();
    }
}

fn toc_members<'a>(html: &'a str, owner: &str) -> Vec<&'a str> {
    let prefix = format!("<li class=\"toc-sub\"><a href=\"#{owner}.");
    html.match_indices(&prefix)
        .map(|(start, _)| {
            let rest = &html[start + prefix.len()..];
            let end = rest.find('"').expect("unterminated TOC href");
            &rest[..end]
        })
        .collect()
}

fn index_group<'a>(html: &'a str, letter: &str) -> &'a str {
    let marker = format!("id=\"types-{letter}\"");
    let start = html
        .find(&marker)
        .unwrap_or_else(|| panic!("missing type group {letter}:\n{html}"));
    let rest = &html[start..];
    let end = rest[1..]
        .find("id=\"types-")
        .map(|idx| idx + 1)
        .unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn class_page_sorts_methods_and_fields() {
    let out = common::generate_html_fixture("html_member_order_class", MEMBERS_SOURCE);
    let html = read_html(&out, "class/Assert.html");

    let methods = common::html_section(&html, "methods")
        .unwrap_or_else(|| panic!("class methods section was not rendered:\n{html}"));
    assert_member_order(methods, "Assert", EXPECTED_METHODS);

    let fields = common::html_section(&html, "fields")
        .unwrap_or_else(|| panic!("class fields section was not rendered:\n{html}"));
    assert_member_order(fields, "Assert", EXPECTED_FIELDS);

    let toc = toc_members(&html, "Assert");
    let expected: Vec<&str> = EXPECTED_METHODS
        .iter()
        .chain(EXPECTED_FIELDS.iter())
        .copied()
        .collect();
    assert_eq!(toc, expected, "class TOC is not sorted:\n{html}");
}

#[test]
fn module_page_sorts_methods_and_fields() {
    let out = common::generate_html_fixture("html_member_order_module", MEMBERS_SOURCE);
    let html = read_html(&out, "module/mod.html");

    let methods = common::html_section(&html, "methods")
        .unwrap_or_else(|| panic!("module methods section was not rendered:\n{html}"));
    assert_member_order(methods, "mod", EXPECTED_METHODS);

    let fields = common::html_section(&html, "fields")
        .unwrap_or_else(|| panic!("module fields section was not rendered:\n{html}"));
    assert_member_order(fields, "mod", EXPECTED_FIELDS);

    let toc = toc_members(&html, "mod");
    let expected: Vec<&str> = EXPECTED_METHODS
        .iter()
        .chain(EXPECTED_FIELDS.iter())
        .copied()
        .collect();
    assert_eq!(toc, expected, "module TOC is not sorted:\n{html}");
}

#[test]
fn type_index_groups_are_alphabetical() {
    let out = common::generate_html_fixture("html_member_order_index_groups", INDEX_GROUPS_SOURCE);
    let html = read_html(&out, "index.html");

    // `alias Zebra` used to sort before `class Apple` because the sort key was
    // the display name with its kind prefix.
    let a = html.find("id=\"types-A\"").expect("missing type group A");
    let z = html.find("id=\"types-Z\"").expect("missing type group Z");
    assert!(a < z, "type groups are not alphabetical:\n{html}");
}

#[test]
fn type_index_items_use_bare_names() {
    let out = common::generate_html_fixture("html_member_order_index_items", INDEX_ITEMS_SOURCE);
    let html = read_html(&out, "index.html");

    // `alias Avocado` must sort by the bare name, not by the `alias ` prefix,
    // so both entries in group A stay in name order regardless of their kind.
    let group_a = index_group(&html, "A");
    let apple = group_a.find("Apple</a>").expect("missing Apple in group A");
    let avocado = group_a
        .find("Avocado</a>")
        .expect("missing Avocado in group A");
    assert!(
        apple < avocado,
        "items in type group A are not alphabetical:\n{group_a}"
    );
}
