use crate::diff::{DiffRenderOptions, render_unified_diff};

#[test]
fn test_git_diff_style_has_full_header_and_hunks() {
    let rendered = render_unified_diff(
        Some("src/test.lua"),
        "local x=1\n",
        "local x = 1\n",
        &DiffRenderOptions {
            use_color: false,
            inline_highlight: false,
            context_lines: 3,
        },
    );

    assert!(rendered.contains("diff --git a/src/test.lua b/src/test.lua"));
    assert!(rendered.contains("--- a/src/test.lua"));
    assert!(rendered.contains("+++ b/src/test.lua"));
    assert!(rendered.contains("@@ -1 +1 @@"));
    assert!(!rendered.contains("[-"));
    assert!(!rendered.contains("{+"));
}

#[test]
fn test_plain_output_keeps_clean_git_lines() {
    let rendered = render_unified_diff(
        None,
        "local x=1\n",
        "local x = 1\n",
        &DiffRenderOptions {
            use_color: false,
            inline_highlight: false,
            context_lines: 3,
        },
    );

    assert!(rendered.contains("-local x=1\n"));
    assert!(rendered.contains("+local x = 1\n"));
    assert!(!rendered.contains("\x1b["));
}

#[test]
fn test_color_diff_uses_ansi_without_inline_markers() {
    let rendered = render_unified_diff(
        None,
        "local x=1\n",
        "local x = 1\n",
        &DiffRenderOptions {
            use_color: true,
            inline_highlight: false,
            context_lines: 3,
        },
    );

    assert!(rendered.contains("\x1b["));
    assert!(!rendered.contains("[-"));
    assert!(!rendered.contains("{+"));
    assert!(!rendered.contains("1;4;91"));
}

#[test]
fn test_inline_highlight_is_off_by_default() {
    let rendered = render_unified_diff(
        None,
        "local x=1\n",
        "local x = 1\n",
        &DiffRenderOptions::default(),
    );

    assert!(!rendered.contains("1;4;91"));
    assert!(!rendered.contains("1;4;92"));
}

#[test]
fn test_inline_highlight_applies_emphasis_when_enabled() {
    let rendered = render_unified_diff(
        None,
        "local x=1\n",
        "local x = 1\n",
        &DiffRenderOptions {
            use_color: true,
            inline_highlight: true,
            context_lines: 3,
        },
    );

    assert!(rendered.contains("1;4;91"));
    assert!(rendered.contains("1;4;92"));
}

#[test]
fn test_hunk_header_omits_single_line_count() {
    let rendered = render_unified_diff(
        None,
        "a\nb\nc\nd\n",
        "a\nB\nc\nd\n",
        &DiffRenderOptions {
            use_color: false,
            inline_highlight: false,
            context_lines: 0,
        },
    );

    assert!(rendered.contains("@@ -2 +2 @@"));
    assert!(!rendered.contains("@@ -2,1 +2,1 @@"));
}

#[test]
fn test_diff_marks_missing_final_newline() {
    let rendered = render_unified_diff(
        None,
        "a\nb",
        "a\nc",
        &DiffRenderOptions {
            use_color: false,
            inline_highlight: false,
            context_lines: 3,
        },
    );

    assert!(rendered.contains("\\ No newline at end of file"));
}

#[test]
fn test_empty_diff_renders_no_hunks() {
    let rendered = render_unified_diff(
        Some("src/test.lua"),
        "local x = 1\n",
        "local x = 1\n",
        &DiffRenderOptions {
            use_color: false,
            inline_highlight: false,
            context_lines: 3,
        },
    );

    assert!(!rendered.contains("@@"));
}

#[test]
fn test_diff_normalizes_crlf_line_endings() {
    let rendered = render_unified_diff(
        None,
        "local x = 1\r\nlocal y = 2\r\n",
        "local x = 1\nlocal y = 2\n",
        &DiffRenderOptions {
            use_color: false,
            inline_highlight: false,
            context_lines: 3,
        },
    );

    assert!(!rendered.contains("@@"));
}

#[test]
fn test_pure_insertion_at_start_matches_git_header() {
    let rendered = render_unified_diff(
        None,
        "line1\nline2\n",
        "NEW\nline1\nline2\n",
        &DiffRenderOptions {
            use_color: false,
            inline_highlight: false,
            context_lines: 0,
        },
    );

    assert!(rendered.contains("@@ -0,0 +1 @@"));
}

#[test]
fn test_pure_deletion_at_start_matches_git_header() {
    let rendered = render_unified_diff(
        None,
        "OLD\nline1\nline2\n",
        "line1\nline2\n",
        &DiffRenderOptions {
            use_color: false,
            inline_highlight: false,
            context_lines: 0,
        },
    );

    assert!(rendered.contains("@@ -1 +0,0 @@"));
}

#[test]
fn test_index_line_has_git_blob_hashes() {
    let rendered = render_unified_diff(
        Some("src/test.lua"),
        "local x=1\n",
        "local x = 1\n",
        &DiffRenderOptions {
            use_color: false,
            inline_highlight: false,
            context_lines: 3,
        },
    );

    assert!(
        rendered.contains(&format!(
            "index {}..{} 100644",
            super::git_blob_hash(b"local x=1\n"),
            super::git_blob_hash(b"local x = 1\n")
        )),
        "missing index line in: {rendered}"
    );
}

#[test]
fn test_index_line_absent_for_anonymous_path() {
    let rendered = render_unified_diff(
        None,
        "local x=1\n",
        "local x = 1\n",
        &DiffRenderOptions {
            use_color: false,
            inline_highlight: false,
            context_lines: 3,
        },
    );

    assert!(!rendered.contains("index "));
}
