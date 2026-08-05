use super::render::html_escape;
use emmylua_parser::{LexerState, Reader, SourceRange};
use emmylua_parser_desc::{
    CodeBlockHighlightKind, CodeBlockLang, DescItem, DescItemKind, ResultContainer, process_code,
};

/// Collects `DescItem`s from a standalone code block for syntax highlighting.
struct HighlightCollector {
    items: Vec<DescItem>,
}

impl ResultContainer for HighlightCollector {
    fn results(&self) -> &Vec<DescItem> {
        &self.items
    }
    fn results_mut(&mut self) -> &mut Vec<DescItem> {
        &mut self.items
    }
    fn cursor_position(&self) -> Option<usize> {
        None
    }
}

fn hl_class(kind: CodeBlockHighlightKind) -> &'static str {
    match kind {
        CodeBlockHighlightKind::None => "hl-none",
        CodeBlockHighlightKind::String => "hl-str",
        CodeBlockHighlightKind::Number => "hl-num",
        CodeBlockHighlightKind::Keyword => "hl-kw",
        CodeBlockHighlightKind::Operators => "hl-op",
        CodeBlockHighlightKind::Comment => "hl-comment",
        CodeBlockHighlightKind::Function => "hl-fn",
        CodeBlockHighlightKind::Class => "hl-class",
        CodeBlockHighlightKind::Enum => "hl-enum",
        CodeBlockHighlightKind::Variable => "hl-var",
        CodeBlockHighlightKind::Property => "hl-prop",
        CodeBlockHighlightKind::Decorator => "hl-decorator",
    }
}

/// Highlights a fenced code block using the parser-desc lexers. Falls back to
/// plain escaped text for unknown languages.
pub fn highlight_code(lang: &str, code: &str) -> String {
    let code_lang = CodeBlockLang::try_parse(lang).unwrap_or(CodeBlockLang::None);
    let range = SourceRange::new(0, code.len());
    let reader = Reader::new_with_range(code, range);
    let mut collector = HighlightCollector { items: Vec::new() };
    process_code(&mut collector, range, reader, LexerState::Normal, code_lang);

    collector
        .items
        .sort_by_key(|item| u32::from(item.range.start()));

    let mut out = String::new();
    let mut pos = 0usize;
    for item in &collector.items {
        let DescItemKind::CodeBlockHl(kind) = item.kind else {
            continue;
        };
        let start: usize = item.range.start().into();
        let end: usize = item.range.end().into();
        let start = start.max(pos);
        if start >= end {
            continue;
        }
        if start > pos {
            out.push_str(&html_escape(&code[pos..start]));
        }
        let text = html_escape(&code[start..end]);
        out.push_str(&format!("<span class=\"{}\">{text}</span>", hl_class(kind)));
        pos = end;
    }
    out.push_str(&html_escape(&code[pos..]));
    out
}

/// Renders a subset of CommonMark (bold, italic, inline code, links, headings,
/// lists, blockquotes, fenced code, horizontal rules, paragraphs) to HTML.
///
/// This operates on the plain description strings produced by the doc index, so
/// it is self-contained and does not require re-parsing source files.
pub fn render_markdown(text: &str) -> String {
    let mut html = String::new();
    let mut in_code = false;
    let mut code_lang = String::new();
    let mut code_buf = String::new();
    let mut list_tag: Option<&str> = None;
    let mut in_para = false;

    let lines: Vec<&str> = text.split('\n').collect();
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];

        // Fenced code block.
        if raw.trim_start().starts_with("```") {
            if in_code {
                html.push_str(&format!(
                    "<pre class=\"doc-code\"><code data-lang=\"{}\">{}</code></pre>",
                    html_escape(&code_lang),
                    highlight_code(&code_lang, &code_buf)
                ));
                in_code = false;
                code_buf.clear();
            } else {
                in_code = true;
                close_block(&mut html, &mut in_para, &mut list_tag);
                code_lang = raw.trim_start().trim_start_matches('`').trim().to_string();
            }
            i += 1;
            continue;
        }
        if in_code {
            code_buf.push_str(raw);
            code_buf.push('\n');
            i += 1;
            continue;
        }

        let trimmed = raw.trim();
        if trimmed.is_empty() {
            close_block(&mut html, &mut in_para, &mut list_tag);
            i += 1;
            continue;
        }

        // Heading.
        if let Some((level, content)) = parse_heading(trimmed) {
            close_block(&mut html, &mut in_para, &mut list_tag);
            html.push_str(&format!("<h{level}>{}</h{level}>", render_inline(content)));
            i += 1;
            continue;
        }

        // Horizontal rule.
        if is_horizontal_rule(trimmed) {
            close_block(&mut html, &mut in_para, &mut list_tag);
            html.push_str("<hr>");
            i += 1;
            continue;
        }

        // Blockquote.
        if let Some(content) = trimmed.strip_prefix('>') {
            close_block(&mut html, &mut in_para, &mut list_tag);
            html.push_str(&format!(
                "<blockquote>{}</blockquote>",
                render_inline(content.trim())
            ));
            i += 1;
            continue;
        }

        // List items.
        if let Some((tag, content)) = parse_list_item(trimmed) {
            if list_tag != Some(tag) {
                close_list(&mut html, &mut list_tag);
                html.push_str(if tag == "ul" { "<ul>" } else { "<ol>" });
                list_tag = Some(tag);
            }
            close_para(&mut html, &mut in_para);
            html.push_str(&format!("<li>{}</li>", render_inline(content)));
            i += 1;
            continue;
        }

        // Paragraph text.
        if !in_para {
            html.push_str("<p>");
            in_para = true;
        } else {
            html.push(' ');
        }
        html.push_str(&render_inline(trimmed));
        i += 1;
    }

    close_block(&mut html, &mut in_para, &mut list_tag);
    if in_code {
        html.push_str(&format!(
            "<pre class=\"doc-code\"><code data-lang=\"{}\">{}</code></pre>",
            html_escape(&code_lang),
            highlight_code(&code_lang, &code_buf)
        ));
    }
    html
}

fn parse_heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = line[level..].trim_start();
    if rest.is_empty() {
        None
    } else {
        Some((level, rest))
    }
}

fn is_horizontal_rule(line: &str) -> bool {
    let chars: Vec<char> = line.chars().filter(|c| *c != ' ' && *c != '\t').collect();
    if chars.is_empty() {
        return false;
    }
    let marker = chars[0];
    if marker != '-' && marker != '*' && marker != '_' {
        return false;
    }
    chars.len() >= 3 && chars.iter().all(|c| *c == marker)
}

fn parse_list_item(line: &str) -> Option<(&'static str, &str)> {
    if let Some(rest) = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
    {
        return Some(("ul", rest.trim()));
    }
    let bytes = line.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx > 0 && bytes.get(idx) == Some(&b'.') && bytes.get(idx + 1) == Some(&b' ') {
        let content = line[idx + 2..].trim();
        return Some(("ol", content));
    }
    None
}

fn close_block(html: &mut String, in_para: &mut bool, list_tag: &mut Option<&str>) {
    close_para(html, in_para);
    close_list(html, list_tag);
}

fn close_para(html: &mut String, in_para: &mut bool) {
    if *in_para {
        html.push_str("</p>");
        *in_para = false;
    }
}

fn close_list(html: &mut String, list_tag: &mut Option<&str>) {
    if let Some(tag) = list_tag.take() {
        html.push_str(if tag == "ul" { "</ul>" } else { "</ol>" });
    }
}

/// Renders inline markdown: `**bold**`, `*italic*`, `` `code` ``, `[text](url)`.
fn render_inline(text: &str) -> String {
    let mut out = String::new();
    let mut plain = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        let special = match c {
            '`' => try_inline_code(&chars, i),
            '[' => try_link(&chars, i),
            '*' | '_' => try_emph(&chars, i),
            '\\' => try_escape(&chars, i),
            _ => None,
        };

        if let Some((tag, next)) = special {
            flush_plain(&mut out, &mut plain);
            out.push_str(&tag);
            i = next;
        } else {
            plain.push(c);
            i += 1;
        }
    }
    flush_plain(&mut out, &mut plain);
    out
}

fn flush_plain(out: &mut String, plain: &mut String) {
    if !plain.is_empty() {
        out.push_str(&html_escape(plain));
        plain.clear();
    }
}

fn try_inline_code(chars: &[char], i: usize) -> Option<(String, usize)> {
    let end = chars[i + 1..].iter().position(|&c| c == '`')?;
    let end = i + 1 + end;
    let code: String = chars[i + 1..end].iter().collect();
    Some((format!("<code>{}</code>", html_escape(&code)), end + 1))
}

fn try_link(chars: &[char], i: usize) -> Option<(String, usize)> {
    let close_bracket = chars[i + 1..].iter().position(|&c| c == ']')?;
    let close_bracket = i + 1 + close_bracket;
    if chars.get(close_bracket + 1) != Some(&'(') {
        return None;
    }
    let open_paren = close_bracket + 1;
    let close_paren = chars[open_paren + 1..].iter().position(|&c| c == ')')?;
    let close_paren = open_paren + 1 + close_paren;
    let label: String = chars[i + 1..close_bracket].iter().collect();
    let href: String = chars[open_paren + 1..close_paren].iter().collect();
    let html = format!(
        "<a href=\"{}\">{}</a>",
        html_escape(&href),
        html_escape(&label)
    );
    Some((html, close_paren + 1))
}

fn try_emph(chars: &[char], i: usize) -> Option<(String, usize)> {
    let marker = chars[i];
    let double = chars.get(i + 1) == Some(&marker);
    let marker_len = if double { 2 } else { 1 };
    let mut j = i + marker_len;
    while j < chars.len() {
        if chars[j] == marker && (double == (chars.get(j + 1) == Some(&marker))) {
            let inner: String = chars[i + marker_len..j].iter().collect();
            if double {
                return Some((
                    format!("<strong>{}</strong>", render_inline(&inner)),
                    j + marker_len,
                ));
            }
            return Some((
                format!("<em>{}</em>", render_inline(&inner)),
                j + marker_len,
            ));
        }
        j += 1;
    }
    None
}

fn try_escape(chars: &[char], i: usize) -> Option<(String, usize)> {
    let next = chars.get(i + 1)?;
    Some((html_escape(&next.to_string()), i + 2))
}
