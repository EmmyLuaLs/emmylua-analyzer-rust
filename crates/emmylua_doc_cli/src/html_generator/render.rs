/// Escapes a plain text string for safe embedding in HTML.
pub fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Wraps a rendered signature body in a `<pre class="signature">` block.
pub fn signature_pre(inner: String) -> String {
    format!("<pre class=\"signature\"><code>{inner}</code></pre>")
}
