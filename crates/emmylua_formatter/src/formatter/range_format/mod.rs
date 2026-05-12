#[cfg(test)]
mod test;

use crate::{LuaFormatConfig, Printer, SourceText};
use emmylua_parser::{
    LuaAstNode, LuaCallArgList, LuaChunk, LuaLanguageLevel, LuaParamList, LuaParser, LuaSyntaxKind,
    LuaSyntaxNode, LuaTableExpr, LuaTableField, ParserConfig,
};
use rowan::{TextRange, TextSize};

use super::{FormatContext, format_chunk, layout, model::FormatPlan, model::LayoutNodePlan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeFormatOutput {
    pub replace_range: TextRange,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExplicitFormatTargetKind {
    TableExpr,
    CallArgList,
    ParamList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExplicitFormatTarget {
    replace_range: TextRange,
    kind: ExplicitFormatTargetKind,
}

pub fn reformat_range(
    source: &SourceText,
    selection: TextRange,
    config: &LuaFormatConfig,
) -> Option<RangeFormatOutput> {
    if source.text.is_empty() {
        return None;
    }

    let tree = LuaParser::parse(source.text, ParserConfig::with_level(source.level));
    if tree.has_syntax_errors() {
        return None;
    }

    let chunk = tree.get_chunk_node();
    reformat_range_in_chunk(source.text, &chunk, selection, config, source.level)
}

pub fn reformat_range_in_chunk(
    source_text: &str,
    chunk: &LuaChunk,
    selection: TextRange,
    config: &LuaFormatConfig,
    level: LuaLanguageLevel,
) -> Option<RangeFormatOutput> {
    let selection = clamp_range(selection, chunk.syntax().text_range().end());
    if let Some(target) = select_explicit_format_target(chunk, selection) {
        return format_explicit_target(source_text, target, level, config);
    }

    let selected_range = select_format_range(source_text, chunk, selection, config)?;
    let fragment =
        &source_text[usize::from(selected_range.start())..usize::from(selected_range.end())];
    if fragment.trim().is_empty() {
        return None;
    }

    let source_indent_prefix = line_indent_prefix(source_text, selected_range.start());
    let target_indent_prefix =
        target_indent_prefix(chunk.syntax(), source_text, selected_range, config);
    let dedented = strip_base_indent(fragment, &source_indent_prefix);
    let mut fragment_config = config.clone();
    fragment_config.output.insert_final_newline = fragment.ends_with('\n');
    let formatted = format_fragment(&dedented, level, &fragment_config)?;
    let text = apply_base_indent(&formatted, &target_indent_prefix);

    Some(RangeFormatOutput {
        replace_range: selected_range,
        text,
    })
}

fn select_explicit_format_target(
    chunk: &LuaChunk,
    selection: TextRange,
) -> Option<ExplicitFormatTarget> {
    let root = chunk.syntax();
    let mut best = None;

    if let Some(table_expr) = find_table_expr_target(root, selection) {
        best = Some(ExplicitFormatTarget {
            replace_range: table_expr.get_range(),
            kind: ExplicitFormatTargetKind::TableExpr,
        });
    }

    if let Some(call_arg_list) = find_smallest_containing::<LuaCallArgList>(root, selection) {
        best = pick_better_target(
            best,
            ExplicitFormatTarget {
                replace_range: call_arg_list.get_range(),
                kind: ExplicitFormatTargetKind::CallArgList,
            },
        );
    }

    if let Some(param_list) = find_smallest_containing::<LuaParamList>(root, selection) {
        best = pick_better_target(
            best,
            ExplicitFormatTarget {
                replace_range: param_list.get_range(),
                kind: ExplicitFormatTargetKind::ParamList,
            },
        );
    }

    best
}

fn find_table_expr_target(root: &LuaSyntaxNode, selection: TextRange) -> Option<LuaTableExpr> {
    if let Some(field) = find_smallest_containing::<LuaTableField>(root, selection)
        && let Some(table_expr) = field.get_parent::<LuaTableExpr>()
    {
        return Some(table_expr);
    }

    find_smallest_containing::<LuaTableExpr>(root, selection)
}

fn find_smallest_containing<N: LuaAstNode>(
    root: &LuaSyntaxNode,
    selection: TextRange,
) -> Option<N> {
    root.descendants()
        .filter_map(N::cast)
        .filter(|node| contains_range(node.get_range(), selection))
        .min_by_key(|node| text_range_len(node.get_range()))
}

fn pick_better_target(
    current: Option<ExplicitFormatTarget>,
    candidate: ExplicitFormatTarget,
) -> Option<ExplicitFormatTarget> {
    match current {
        None => Some(candidate),
        Some(existing) => {
            if text_range_len(candidate.replace_range) < text_range_len(existing.replace_range) {
                Some(candidate)
            } else {
                Some(existing)
            }
        }
    }
}

fn text_range_len(range: TextRange) -> u32 {
    u32::from(range.end() - range.start())
}

fn format_explicit_target(
    source_text: &str,
    target: ExplicitFormatTarget,
    level: LuaLanguageLevel,
    config: &LuaFormatConfig,
) -> Option<RangeFormatOutput> {
    let fragment = &source_text
        [usize::from(target.replace_range.start())..usize::from(target.replace_range.end())];
    if fragment.trim().is_empty() {
        return None;
    }

    let wrapped_fragment = wrap_explicit_fragment(target.kind, fragment);
    let formatted_wrapper = format_fragment(&wrapped_fragment, level, config)?;
    let extracted = extract_explicit_fragment(&formatted_wrapper, target.kind, level)?;

    Some(RangeFormatOutput {
        replace_range: target.replace_range,
        text: extracted,
    })
}

fn wrap_explicit_fragment(kind: ExplicitFormatTargetKind, fragment: &str) -> String {
    match kind {
        ExplicitFormatTargetKind::TableExpr => {
            format!("local __emmylua_range_target = {fragment}")
        }
        ExplicitFormatTargetKind::CallArgList => {
            format!("__emmylua_range_target{fragment}")
        }
        ExplicitFormatTargetKind::ParamList => {
            format!("local function __emmylua_range_target{fragment}\nend")
        }
    }
}

fn extract_explicit_fragment(
    formatted_wrapper: &str,
    kind: ExplicitFormatTargetKind,
    level: LuaLanguageLevel,
) -> Option<String> {
    let tree = LuaParser::parse(formatted_wrapper, ParserConfig::with_level(level));
    if tree.has_syntax_errors() {
        return None;
    }

    let chunk = tree.get_chunk_node();
    match kind {
        ExplicitFormatTargetKind::TableExpr => chunk
            .descendants::<LuaTableExpr>()
            .next()
            .map(|node| node.get_text()),
        ExplicitFormatTargetKind::CallArgList => chunk
            .descendants::<LuaCallArgList>()
            .next()
            .map(|node| node.get_text()),
        ExplicitFormatTargetKind::ParamList => chunk
            .descendants::<LuaParamList>()
            .next()
            .map(|node| node.get_text()),
    }
}

fn select_format_range(
    source_text: &str,
    chunk: &LuaChunk,
    selection: TextRange,
    config: &LuaFormatConfig,
) -> Option<TextRange> {
    let ctx = FormatContext::new(config);
    let mut plan = FormatPlan::from_config(config);
    layout::analyze_layout(&ctx, chunk, &mut plan);
    let root = chunk.syntax();

    let expanded = find_deepest_block_slice(root, plan.layout.root_nodes.as_slice(), selection)
        .or_else(|| find_overlapping_slice(root, plan.layout.root_nodes.as_slice(), selection))
        .unwrap_or(selection);

    Some(expand_to_full_lines(source_text, expanded))
}

fn find_deepest_block_slice(
    root: &LuaSyntaxNode,
    nodes: &[LayoutNodePlan],
    selection: TextRange,
) -> Option<TextRange> {
    for node in nodes {
        let LayoutNodePlan::Syntax(syntax_plan) = node else {
            continue;
        };
        let Some(node_range) = syntax_plan
            .syntax_id
            .to_node_from_root(root)
            .map(|node| node.text_range())
        else {
            continue;
        };
        if !contains_range(node_range, selection) {
            continue;
        }

        if let Some(found) = find_deepest_block_slice(root, &syntax_plan.children, selection) {
            return Some(found);
        }

        if syntax_plan.kind == LuaSyntaxKind::Block
            && let Some(found) = find_overlapping_slice(root, &syntax_plan.children, selection)
        {
            return Some(found);
        }
    }

    None
}

fn find_overlapping_slice(
    root: &LuaSyntaxNode,
    nodes: &[LayoutNodePlan],
    selection: TextRange,
) -> Option<TextRange> {
    let mut first = None;
    let mut last = None;

    for (index, node) in nodes.iter().enumerate() {
        let Some(node_range) = layout_node_text_range(root, node) else {
            continue;
        };
        if !intersects_range(node_range, selection) {
            continue;
        }

        first.get_or_insert(index);
        last = Some(index);
    }

    let first = first?;
    let last = last?;
    let start = layout_node_text_range(root, &nodes[first])?.start();
    let end = layout_node_text_range(root, &nodes[last])?.end();
    Some(TextRange::new(start, end))
}

fn layout_node_text_range(root: &LuaSyntaxNode, node: &LayoutNodePlan) -> Option<TextRange> {
    match node {
        LayoutNodePlan::Comment(comment) => comment
            .syntax_id
            .to_node_from_root(root)
            .map(|node| node.text_range()),
        LayoutNodePlan::Syntax(syntax) => syntax
            .syntax_id
            .to_node_from_root(root)
            .map(|node| node.text_range()),
    }
}

fn clamp_range(range: TextRange, upper_bound: TextSize) -> TextRange {
    let start = range.start().min(upper_bound);
    let end = range.end().min(upper_bound).max(start);
    TextRange::new(start, end)
}

fn contains_range(container: TextRange, inner: TextRange) -> bool {
    container.start() <= inner.start() && inner.end() <= container.end()
}

fn intersects_range(left: TextRange, right: TextRange) -> bool {
    if right.is_empty() {
        return left.start() <= right.start() && right.start() <= left.end();
    }

    left.start() < right.end() && right.start() < left.end()
}

fn expand_to_full_lines(text: &str, range: TextRange) -> TextRange {
    let start = line_start_offset(text, usize::from(range.start()));
    let end = line_end_offset(text, usize::from(range.end()));
    TextRange::new(TextSize::new(start as u32), TextSize::new(end as u32))
}

fn line_start_offset(text: &str, offset: usize) -> usize {
    let bytes = text.as_bytes();
    let mut index = offset.min(bytes.len());
    while index > 0 && bytes[index - 1] != b'\n' {
        index -= 1;
    }
    index
}

fn line_end_offset(text: &str, offset: usize) -> usize {
    let bytes = text.as_bytes();
    let mut index = offset.min(bytes.len());
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            return index + 1;
        }
        index += 1;
    }
    bytes.len()
}

fn line_indent_prefix(text: &str, line_start: TextSize) -> String {
    let start = usize::from(line_start);
    let end = line_end_offset(text, start);
    let line = &text[start..end];
    let indent_len = line
        .chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .map(char::len_utf8)
        .sum();
    line[..indent_len].to_string()
}

fn target_indent_prefix(
    root: &LuaSyntaxNode,
    source_text: &str,
    range: TextRange,
    config: &LuaFormatConfig,
) -> String {
    let Some(anchor_offset) = first_non_whitespace_offset_in_range(source_text, range) else {
        return String::new();
    };
    let Some(anchor_node) = find_smallest_containing_offset_node(root, anchor_offset) else {
        return String::new();
    };

    let indent_level = anchor_node
        .ancestors()
        .filter(|node| node.kind() == LuaSyntaxKind::Block.into())
        .count()
        .saturating_sub(1);
    config.indent_str().repeat(indent_level)
}

fn first_non_whitespace_offset_in_range(text: &str, range: TextRange) -> Option<TextSize> {
    let start = usize::from(range.start());
    let end = usize::from(range.end()).min(text.len());
    text[start..end].char_indices().find_map(|(index, ch)| {
        (!ch.is_whitespace()).then(|| TextSize::new((start + index) as u32))
    })
}

fn find_smallest_containing_offset_node(
    root: &LuaSyntaxNode,
    offset: TextSize,
) -> Option<LuaSyntaxNode> {
    root.descendants()
        .filter(|node| contains_offset(node.text_range(), offset))
        .min_by_key(|node| text_range_len(node.text_range()))
}

fn contains_offset(range: TextRange, offset: TextSize) -> bool {
    range.start() <= offset && offset < range.end()
}

fn strip_base_indent(text: &str, indent_prefix: &str) -> String {
    map_lines(text, |content, newline| {
        let stripped = content.strip_prefix(indent_prefix).unwrap_or(content);
        let mut line = String::with_capacity(stripped.len() + newline.len());
        line.push_str(stripped);
        line.push_str(newline);
        line
    })
}

fn apply_base_indent(text: &str, indent_prefix: &str) -> String {
    if indent_prefix.is_empty() {
        return text.to_string();
    }

    map_lines(text, |content, newline| {
        if content.is_empty() {
            return newline.to_string();
        }

        let mut line = String::with_capacity(indent_prefix.len() + content.len() + newline.len());
        line.push_str(indent_prefix);
        line.push_str(content);
        line.push_str(newline);
        line
    })
}

fn map_lines(text: &str, mut map: impl FnMut(&str, &str) -> String) -> String {
    let mut result = String::new();
    for line in text.split_inclusive('\n') {
        let (content, newline) = split_line_ending(line);
        result.push_str(&map(content, newline));
    }

    result
}

fn split_line_ending(line: &str) -> (&str, &str) {
    if let Some(content) = line.strip_suffix("\r\n") {
        (content, "\r\n")
    } else if let Some(content) = line.strip_suffix('\n') {
        (content, "\n")
    } else {
        (line, "")
    }
}

fn format_fragment(
    fragment: &str,
    level: LuaLanguageLevel,
    config: &LuaFormatConfig,
) -> Option<String> {
    let tree = LuaParser::parse(fragment, ParserConfig::with_level(level));
    if tree.has_syntax_errors() {
        return None;
    }

    let ctx = FormatContext::new(config);
    let ir = format_chunk(&ctx, &tree.get_chunk_node());
    let mut printer = Printer::new(config);
    let capacity = (fragment.len() as f64 * 1.2).ceil() as usize;
    printer = printer.with_capacity(capacity);
    Some(printer.print(&ir))
}
