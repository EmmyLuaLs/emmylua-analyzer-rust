use similar::ChangeTag;

/// ANSI colorizer for diff output.
///
/// Every method is a no-op when colors are disabled. The palette follows the
/// standard git unified diff colors.
#[derive(Debug, Clone, Copy)]
pub struct Colorizer {
    enabled: bool,
}

impl Colorizer {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Wraps `text` in the given ANSI SGR code when colors are enabled.
    pub fn paint(&self, text: &str, code: &str) -> String {
        if !self.enabled || text.is_empty() {
            text.to_string()
        } else {
            format!("\x1b[{code}m{text}\x1b[0m")
        }
    }

    /// Bold metadata line such as `diff --git a/x b/x`.
    pub fn meta(&self, text: &str) -> String {
        self.paint(text, "1")
    }

    /// Old file header (`--- a/x`).
    pub fn file_old(&self, text: &str) -> String {
        self.paint(text, "1;31")
    }

    /// New file header (`+++ b/x`).
    pub fn file_new(&self, text: &str) -> String {
        self.paint(text, "1;32")
    }

    /// Hunk header (`@@ -1,5 +1,5 @@`).
    pub fn hunk_header(&self, text: &str) -> String {
        self.paint(text, "1;36")
    }

    /// A deleted or added line body.
    pub fn line(&self, tag: ChangeTag, text: &str) -> String {
        match tag {
            ChangeTag::Equal => text.to_string(),
            ChangeTag::Delete => self.paint(text, "31"),
            ChangeTag::Insert => self.paint(text, "32"),
        }
    }

    /// Inline-emphasized segment within a deleted or added line.
    ///
    /// Uses bold + underline so the exact changed words stand out from the
    /// rest of the line body.
    pub fn line_emphasis(&self, tag: ChangeTag, text: &str) -> String {
        match tag {
            ChangeTag::Equal => text.to_string(),
            ChangeTag::Delete => self.paint(text, "1;4;91"),
            ChangeTag::Insert => self.paint(text, "1;4;92"),
        }
    }

    /// `\ No newline at end of file` hint.
    pub fn no_newline(&self, text: &str) -> String {
        self.paint(text, "36")
    }
}
