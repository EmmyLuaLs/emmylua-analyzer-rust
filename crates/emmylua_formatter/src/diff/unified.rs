use similar::{ChangeTag, InlineChange, TextDiff, udiff::UnifiedHunkHeader};

use super::color::Colorizer;

/// Options for [`render_unified_diff`].
#[derive(Debug, Clone)]
pub struct DiffRenderOptions {
    pub use_color: bool,
    /// Highlight changed words within modified lines (bold + underline) when
    /// colors are enabled. Off by default; enabled via `--inline`.
    pub inline_highlight: bool,
    pub context_lines: usize,
}

impl Default for DiffRenderOptions {
    fn default() -> Self {
        Self {
            use_color: false,
            inline_highlight: false,
            context_lines: 3,
        }
    }
}

/// Renders a git-apply compatible unified diff between `original` and
/// `formatted`.
///
/// The output follows the unified diff format (`diff --git`, `index`,
/// `--- a/...` / `+++ b/...`, `@@` hunks and `-`/`+`/` ` line prefixes) so it
/// can be consumed by `git apply`. When [`DiffRenderOptions::inline_highlight`]
/// is enabled, changed segments inside modified lines are emphasized with
/// inline highlighting, which plain git diff does not do.
///
/// When `path` is `Some`, a `diff --git` header plus `--- a/...` / `+++ b/...`
/// file headers are emitted with `a/`/`b/` prefixes. Pass `None` for anonymous
/// input such as stdin.
pub fn render_unified_diff(
    path: Option<&str>,
    original: &str,
    formatted: &str,
    options: &DiffRenderOptions,
) -> String {
    let color = Colorizer::new(options.use_color);
    let mut out = String::new();

    let original_norm = normalize_line_endings(original);
    let formatted_norm = normalize_line_endings(formatted);
    let original = original_norm.as_deref().unwrap_or(original);
    let formatted = formatted_norm.as_deref().unwrap_or(formatted);

    match path {
        Some(path) => {
            out.push_str(&color.meta(&format!("diff --git a/{path} b/{path}")));
            out.push('\n');
            // Hash the LF-normalized content so the blob hashes match what git
            // stores (git normalizes CRLF to LF before hashing with autocrlf).
            let old_hash = super::git_blob_hash(original.as_bytes());
            let new_hash = super::git_blob_hash(formatted.as_bytes());
            out.push_str(&color.meta(&format!("index {old_hash}..{new_hash} 100644")));
            out.push('\n');
            out.push_str(&color.file_old(&format!("--- a/{path}")));
            out.push('\n');
            out.push_str(&color.file_new(&format!("+++ b/{path}")));
            out.push('\n');
        }
        None => {
            out.push_str(&color.file_old("---"));
            out.push('\n');
            out.push_str(&color.file_new("+++"));
            out.push('\n');
        }
    }

    let diff = TextDiff::from_lines(original, formatted);
    for ops in diff.grouped_ops(options.context_lines) {
        let header = UnifiedHunkHeader::new(&ops);
        out.push_str(&color.hunk_header(&header.to_string()));
        out.push('\n');
        for op in &ops {
            for change in diff.iter_inline_changes(op) {
                render_change_line(&mut out, &change, options, &color);
            }
        }
    }

    out
}

fn render_change_line(
    out: &mut String,
    change: &InlineChange<'_, str>,
    options: &DiffRenderOptions,
    color: &Colorizer,
) {
    match change.tag() {
        ChangeTag::Equal => out.push(' '),
        ChangeTag::Delete => out.push_str(&color.paint("-", "31")),
        ChangeTag::Insert => out.push_str(&color.paint("+", "32")),
    }

    let inline = options.use_color && options.inline_highlight;

    for (emphasized, value) in change.iter_strings_lossy() {
        let content = value.strip_suffix('\n').unwrap_or(value.as_ref());

        if inline && emphasized {
            out.push_str(&color.line_emphasis(change.tag(), content));
        } else if options.use_color {
            out.push_str(&color.line(change.tag(), content));
        } else {
            out.push_str(content);
        }
    }

    out.push('\n');
    if change.missing_newline() {
        out.push_str(&color.no_newline("\\ No newline at end of file"));
        out.push('\n');
    }
}

/// Normalizes `\r\n` line endings to `\n` so line-ending-only differences are
/// not reported as content changes. Returns `None` when the text is already
/// LF-only.
fn normalize_line_endings(text: &str) -> Option<String> {
    text.contains("\r\n").then(|| text.replace("\r\n", "\n"))
}
