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

        // Table.
        if let Some((table, next)) = try_parse_table(&lines, i) {
            close_block(&mut html, &mut in_para, &mut list_tag);
            html.push_str(&table);
            i = next;
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

/// Column alignment from a GFM table delimiter cell (`---`, `:---`, `:---:`,
/// `---:`).
#[derive(Clone, Copy, PartialEq)]
enum TableAlign {
    None,
    Left,
    Center,
    Right,
}

impl TableAlign {
    fn class(self) -> Option<&'static str> {
        match self {
            TableAlign::None => None,
            TableAlign::Left => Some("align-left"),
            TableAlign::Center => Some("align-center"),
            TableAlign::Right => Some("align-right"),
        }
    }
}

/// Parses a GFM table starting at `start` (the header row). Returns the
/// rendered HTML and the index of the first line after the table.
///
/// `start + 1` must be a delimiter row such as `| --- | :---: |`; otherwise
/// the block is not a table and normal paragraph handling applies.
///
/// This follows the small CommonMark subset supported by [`render_markdown`]:
/// cells cannot span lines, and HTML blocks / indented code blocks are not
/// recognized as table-body terminators (only headings, rules, lists,
/// blockquotes and fenced code are).
fn try_parse_table(lines: &[&str], start: usize) -> Option<(String, usize)> {
    let header = split_table_row(lines.get(start)?)?;
    let delimiter = split_table_row(lines.get(start + 1)?)?;
    if header.is_empty() || header.len() != delimiter.len() {
        return None;
    }
    let aligns: Vec<TableAlign> = delimiter
        .iter()
        .map(|cell| parse_table_align(cell))
        .collect::<Option<_>>()?;

    let mut rows = Vec::new();
    let mut i = start + 2;
    while let Some(line) = lines.get(i) {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.contains('|') || interrupts_table(trimmed) {
            break;
        }
        let Some(cells) = split_table_row(trimmed) else {
            break;
        };
        rows.push(cells);
        i += 1;
    }

    Some((render_table(&header, &aligns, &rows), i))
}

/// A block-level construct that ends a table body.
fn interrupts_table(line: &str) -> bool {
    parse_heading(line).is_some()
        || is_horizontal_rule(line)
        || parse_list_item(line).is_some()
        || line.starts_with('>')
        || line.starts_with("```")
}

fn parse_table_align(cell: &str) -> Option<TableAlign> {
    let mut core = cell.trim();
    let left = core.starts_with(':');
    if left {
        core = &core[1..];
    }
    let right = core.ends_with(':');
    if right {
        core = &core[..core.len() - 1];
    }
    if core.is_empty() || !core.chars().all(|c| c == '-') {
        return None;
    }
    Some(match (left, right) {
        (true, true) => TableAlign::Center,
        (true, false) => TableAlign::Left,
        (false, true) => TableAlign::Right,
        (false, false) => TableAlign::None,
    })
}

/// Splits one table row into trimmed cells. Outer pipes are optional, and a
/// backslash-escaped `|` stays inside the cell for `render_inline` to unescape.
///
/// Escape handling is intentionally simple: `\` protects the next character,
/// so `\|` is a literal pipe while `\\|` is a literal backslash followed by a
/// real cell delimiter. There is no separate CommonMark backslash token.
fn split_table_row(line: &str) -> Option<Vec<String>> {
    let line = line.trim();
    if !line.contains('|') {
        return None;
    }

    let mut cells = Vec::new();
    let mut cell = String::new();
    // True when the last character consumed was an unescaped row delimiter.
    let mut ends_with_border = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                cell.push(c);
                if let Some(next) = chars.next() {
                    cell.push(next);
                }
                ends_with_border = false;
            }
            '|' => {
                cells.push(std::mem::take(&mut cell));
                ends_with_border = chars.peek().is_none();
            }
            _ => {
                cell.push(c);
                ends_with_border = false;
            }
        }
    }
    cells.push(cell);

    if line.starts_with('|') {
        cells.remove(0);
    }
    if ends_with_border {
        cells.pop();
    }
    if cells.is_empty() {
        return None;
    }
    Some(cells.into_iter().map(|c| c.trim().to_string()).collect())
}

fn render_table(header: &[String], aligns: &[TableAlign], rows: &[Vec<String>]) -> String {
    let mut html = String::from("<div class=\"doc-table\"><table>\n<thead>\n<tr>");
    for (idx, cell) in header.iter().enumerate() {
        push_cell(&mut html, "th", aligns.get(idx).copied(), cell);
    }
    html.push_str("</tr>\n</thead>\n");

    if !rows.is_empty() {
        html.push_str("<tbody>\n");
        for row in rows {
            html.push_str("<tr>");
            for idx in 0..header.len() {
                let cell = row.get(idx).map(String::as_str).unwrap_or("");
                push_cell(&mut html, "td", aligns.get(idx).copied(), cell);
            }
            html.push_str("</tr>\n");
        }
        html.push_str("</tbody>\n");
    }

    html.push_str("</table></div>");
    html
}

fn push_cell(html: &mut String, tag: &str, align: Option<TableAlign>, content: &str) {
    html.push('<');
    html.push_str(tag);
    if let Some(class) = align.and_then(TableAlign::class) {
        html.push_str(" class=\"");
        html.push_str(class);
        html.push('"');
    }
    html.push('>');
    html.push_str(&render_inline(content));
    html.push_str("</");
    html.push_str(tag);
    html.push('>');
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

#[cfg(test)]
mod tests {
    use super::{render_markdown, split_table_row};

    #[test]
    fn renders_pipe_table() {
        let html = render_markdown("| aaa | bbb |\n| --- | --- |\n| 1 | 2 |");
        assert!(html.contains("<table>"), "{html}");
        assert!(html.contains("<th>aaa</th>"), "{html}");
        assert!(html.contains("<th>bbb</th>"), "{html}");
        assert!(html.contains("<td>1</td>"), "{html}");
        assert!(html.contains("<td>2</td>"), "{html}");
        assert!(!html.contains("<p>|"), "{html}");
    }

    #[test]
    fn renders_table_without_outer_pipes() {
        let html = render_markdown("aaa | bbb\n--- | ---\n1 | 2");
        assert!(html.contains("<th>aaa</th>"), "{html}");
        assert!(html.contains("<td>2</td>"), "{html}");
    }

    #[test]
    fn renders_inline_markdown_inside_cells() {
        let html = render_markdown("| **bold** | `code` |\n| --- | --- |\n| [x](y) | plain |");
        assert!(html.contains("<th><strong>bold</strong></th>"), "{html}");
        assert!(html.contains("<th><code>code</code></th>"), "{html}");
        assert!(html.contains("<td><a href=\"y\">x</a></td>"), "{html}");
    }

    #[test]
    fn renders_alignment_classes() {
        let html = render_markdown("| a | b | c |\n|:---|:---:|---:|\n| 1 | 2 | 3 |");
        assert!(html.contains("<th class=\"align-left\">a</th>"), "{html}");
        assert!(html.contains("<th class=\"align-center\">b</th>"), "{html}");
        assert!(html.contains("<th class=\"align-right\">c</th>"), "{html}");
        assert!(html.contains("<td class=\"align-center\">2</td>"), "{html}");
    }

    #[test]
    fn escaped_pipe_stays_inside_cell() {
        let html = render_markdown("| a \\| b | c |\n| --- | --- |\n| 1 | 2 |");
        assert!(html.contains("<th>a | b</th>"), "{html}");
        assert!(html.contains("<th>c</th>"), "{html}");
    }

    #[test]
    fn table_without_delimiter_row_falls_back_to_paragraph() {
        let html = render_markdown("| aaa | bbb |\n| 1 | 2 |");
        assert!(!html.contains("<table>"), "{html}");
        assert!(html.contains("<p>"), "{html}");
    }

    #[test]
    fn table_ends_at_blank_line() {
        let html = render_markdown("| a | b |\n| --- | --- |\n| 1 | 2 |\n\nafter");
        assert!(html.contains("</table>"), "{html}");
        assert!(html.contains("<p>after</p>"), "{html}");
    }

    #[test]
    fn table_pads_missing_cells_and_drops_extra_ones() {
        let html = render_markdown("| a | b |\n| --- | --- |\n| 1 |\n| 2 | 3 | 4 |");
        assert!(html.contains("<tr><td>1</td><td></td></tr>"), "{html}");
        assert!(html.contains("<tr><td>2</td><td>3</td></tr>"), "{html}");
        assert!(!html.contains("<td>4</td>"), "{html}");
    }

    #[test]
    fn invalid_delimiter_cell_falls_back_to_paragraph() {
        let html = render_markdown("| a | b |\n| -- | x |\n| 1 | 2 |");
        assert!(!html.contains("<table>"), "{html}");
        assert!(html.contains("<p>| a | b |"), "{html}");
    }

    #[test]
    fn mismatched_column_count_falls_back_to_paragraph() {
        let html = render_markdown("| a | b |\n| --- |\n| 1 | 2 |");
        assert!(!html.contains("<table>"), "{html}");
        assert!(html.contains("<p>"), "{html}");
    }

    #[test]
    fn table_body_stops_at_heading() {
        let html = render_markdown("| a | b |\n| --- | --- |\n| 1 | 2 |\n### next");
        assert!(html.contains("</table>"), "{html}");
        assert!(html.contains("<h3>next</h3>"), "{html}");
        assert!(!html.contains("<td>### next</td>"), "{html}");
    }

    #[test]
    fn split_table_row_handles_border_pipes() {
        assert_eq!(
            split_table_row("| a | b |"),
            Some(vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(
            split_table_row("a | b"),
            Some(vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(split_table_row("|"), None);
        assert_eq!(split_table_row("||"), Some(vec![String::new()]));
        assert_eq!(
            split_table_row("a \\| b | c"),
            Some(vec!["a \\| b".to_string(), "c".to_string()])
        );
    }
}
