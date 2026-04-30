mod test;

use hashbrown::HashMap;

use crate::config::LuaFormatConfig;
use crate::ir::{AlignEntry, DocIR, GroupId, ir_flat_width, syntax_text_trimmed_end};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrintedDocMetrics {
    pub line_widths: Vec<usize>,
}

pub fn measure_docs(config: &LuaFormatConfig, docs: &[DocIR]) -> PrintedDocMetrics {
    MeasuringPrinter::new(config).measure(docs)
}

pub fn measure_docs_with_limits(
    config: &LuaFormatConfig,
    docs: &[DocIR],
    limits: MeasureLimits,
) -> (PrintedDocMetrics, bool) {
    MeasuringPrinter::new(config)
        .with_limits(limits)
        .measure_with_truncation(docs)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrintMode {
    Flat,
    Break,
}

#[derive(Debug, Default, Clone)]
pub struct PrinterProfile {
    pub fits_calls: u64,
    pub fits_nodes_visited: u64,
    pub has_hard_line_calls: u64,
    pub has_hard_line_nodes_visited: u64,
    pub print_fill_calls: u64,
    pub group_count: u64,
    pub align_group_count: u64,
    pub line_suffix_clones: u64,
}

pub struct Printer {
    max_line_width: usize,
    indent_str: String,
    indent_width: usize,
    newline_str: &'static str,
    line_comment_min_spaces_before: usize,
    line_comment_min_column: usize,
    output: String,
    current_column: usize,
    indent_level: usize,
    group_break_map: HashMap<GroupId, bool>,
    line_suffixes: Vec<Vec<DocIR>>,
    profile: PrinterProfile,
}

struct MeasuringPrinter {
    max_line_width: usize,
    indent_width: usize,
    line_comment_min_spaces_before: usize,
    line_comment_min_column: usize,
    current_column: usize,
    indent_level: usize,
    trailing_spaces: usize,
    group_break_map: HashMap<GroupId, bool>,
    line_suffixes: Vec<Vec<DocIR>>,
    metrics: PrintedDocMetrics,
    limits: MeasureLimits,
    truncated: bool,
    overflow_penalty: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MeasureLimits {
    pub max_lines: Option<usize>,
    pub max_overflow_penalty: Option<usize>,
    pub first_line_prefix_width: usize,
}

impl Printer {
    pub fn new(config: &LuaFormatConfig) -> Self {
        Self {
            max_line_width: config.layout.max_line_width,
            indent_str: config.indent_str(),
            indent_width: config.indent_width(),
            newline_str: config.newline_str(),
            line_comment_min_spaces_before: config.comments.line_comment_min_spaces_before.max(1),
            line_comment_min_column: config.comments.line_comment_min_column,
            output: String::new(),
            current_column: 0,
            indent_level: 0,
            group_break_map: HashMap::new(),
            line_suffixes: Vec::new(),
            profile: PrinterProfile::default(),
        }
    }

    pub fn print(mut self, docs: &[DocIR]) -> String {
        self.print_docs(docs, PrintMode::Break);

        // Flush any remaining line suffixes
        if !self.line_suffixes.is_empty() {
            let suffixes = std::mem::take(&mut self.line_suffixes);
            for suffix in &suffixes {
                self.print_docs(suffix, PrintMode::Break);
            }
        }

        self.output
    }

    pub fn print_with_profile(mut self, docs: &[DocIR]) -> (String, PrinterProfile) {
        self.print_docs(docs, PrintMode::Break);

        if !self.line_suffixes.is_empty() {
            let suffixes = std::mem::take(&mut self.line_suffixes);
            for suffix in &suffixes {
                self.print_docs(suffix, PrintMode::Break);
            }
        }

        (self.output, self.profile)
    }

    fn print_docs(&mut self, docs: &[DocIR], mode: PrintMode) {
        for doc in docs {
            self.print_doc(doc, mode);
        }
    }

    fn print_doc(&mut self, doc: &DocIR, mode: PrintMode) {
        match doc {
            DocIR::Text(s) => {
                self.push_text(s);
            }
            DocIR::SourceNode { node, trim_end } => {
                let text = node.text();
                if *trim_end {
                    let end = syntax_text_trimmed_end(&text);
                    self.push_syntax_text(&text.slice(..end));
                } else {
                    self.push_syntax_text(&text);
                }
            }
            DocIR::SourceToken(token) => {
                self.push_text(token.text());
            }
            DocIR::SyntaxToken(kind) => {
                if let Some(text) = kind.syntax_text() {
                    self.push_text(text);
                }
            }
            DocIR::Space => {
                self.push_text(" ");
            }
            DocIR::HardLine => {
                self.flush_line_suffixes();
                self.push_newline();
            }
            DocIR::SoftLine => match mode {
                PrintMode::Flat => self.push_text(" "),
                PrintMode::Break => {
                    self.flush_line_suffixes();
                    self.push_newline();
                }
            },
            DocIR::SoftLineOrEmpty => {
                if mode == PrintMode::Break {
                    self.flush_line_suffixes();
                    self.push_newline();
                }
            }
            DocIR::Group {
                contents,
                should_break,
                id,
            } => {
                self.profile.group_count += 1;
                let should_break = *should_break || self.has_hard_line(contents);
                let child_mode = if should_break {
                    PrintMode::Break
                } else if self.fits_on_line(contents, mode) {
                    PrintMode::Flat
                } else {
                    PrintMode::Break
                };

                if let Some(gid) = id {
                    self.group_break_map
                        .insert(*gid, child_mode == PrintMode::Break);
                }

                self.print_docs(contents, child_mode);
            }
            DocIR::Indent(contents) => {
                self.indent_level += 1;
                self.print_docs(contents, mode);
                self.indent_level -= 1;
            }
            DocIR::List(contents) => {
                self.print_docs(contents, mode);
            }
            DocIR::IfBreak {
                break_contents,
                flat_contents,
                group_id,
            } => {
                let is_break = if let Some(gid) = group_id {
                    self.group_break_map.get(gid).copied().unwrap_or(false)
                } else {
                    mode == PrintMode::Break
                };
                let d = if is_break {
                    break_contents.as_ref()
                } else {
                    flat_contents.as_ref()
                };
                self.print_doc(d, mode);
            }
            DocIR::Fill { parts } => {
                self.print_fill(parts, mode);
            }
            DocIR::LineSuffix(contents) => {
                self.profile.line_suffix_clones += 1;
                self.line_suffixes.push(contents.clone());
            }
            DocIR::AlignGroup(group) => {
                self.profile.align_group_count += 1;
                self.print_align_group(&group.entries, mode);
            }
        }
    }

    fn push_text(&mut self, s: &str) {
        self.output.push_str(s);
        if let Some(last_newline) = s.rfind('\n') {
            self.current_column = s.len() - last_newline - 1;
        } else {
            self.current_column += s.len();
        }
    }

    fn push_syntax_text(&mut self, text: &rowan::SyntaxText) {
        text.for_each_chunk(|chunk| self.push_text(chunk));
    }

    fn push_newline(&mut self) {
        // Trim trailing spaces
        let trimmed = self.output.trim_end_matches(' ');
        let trimmed_len = trimmed.len();
        if trimmed_len < self.output.len() {
            self.output.truncate(trimmed_len);
        }

        self.output.push_str(self.newline_str);
        let indent = self.indent_str.repeat(self.indent_level);
        self.output.push_str(&indent);
        self.current_column = self.indent_level * self.indent_width;
    }

    fn flush_line_suffixes(&mut self) {
        if self.line_suffixes.is_empty() {
            return;
        }
        let suffixes = std::mem::take(&mut self.line_suffixes);
        for suffix in &suffixes {
            self.print_docs(suffix, PrintMode::Break);
        }
    }

    fn trailing_comment_padding(
        &self,
        content_width: usize,
        aligned_content_width: usize,
    ) -> usize {
        let natural_padding = aligned_content_width.saturating_sub(content_width)
            + self.line_comment_min_spaces_before;

        if self.line_comment_min_column == 0 {
            natural_padding
        } else {
            natural_padding.max(self.line_comment_min_column.saturating_sub(content_width))
        }
    }

    /// Check whether contents fit within the remaining line width in Flat mode
    fn fits_on_line(&self, docs: &[DocIR], _current_mode: PrintMode) -> bool {
        let remaining = self.max_line_width.saturating_sub(self.current_column);
        Self::fits_impl(docs, remaining as isize, &self.group_break_map, None)
    }

    fn fits_impl(
        docs: &[DocIR],
        mut remaining: isize,
        group_break_map: &HashMap<GroupId, bool>,
        mut profile: Option<&mut PrinterProfile>,
    ) -> bool {
        if let Some(profile) = profile.as_deref_mut() {
            profile.fits_calls += 1;
        }
        let mut stack: Vec<(&DocIR, PrintMode)> =
            docs.iter().rev().map(|d| (d, PrintMode::Flat)).collect();

        while let Some((doc, mode)) = stack.pop() {
            if let Some(profile) = profile.as_deref_mut() {
                profile.fits_nodes_visited += 1;
            }
            if remaining < 0 {
                return false;
            }

            match doc {
                DocIR::Text(s) => {
                    remaining -= s.len() as isize;
                }
                DocIR::SourceNode { node, trim_end } => {
                    let text = node.text();
                    let width = if *trim_end {
                        let end = syntax_text_trimmed_end(&text);
                        let end: u32 = end.into();
                        end as isize
                    } else {
                        let len: u32 = text.len().into();
                        len as isize
                    };
                    remaining -= width;
                }
                DocIR::SourceToken(token) => {
                    remaining -= token.text().len() as isize;
                }
                DocIR::SyntaxToken(kind) => {
                    remaining -= kind.syntax_text().map(str::len).unwrap_or(0) as isize;
                }
                DocIR::Space => {
                    remaining -= 1;
                }
                DocIR::HardLine => {
                    return true;
                }
                DocIR::SoftLine => {
                    if mode == PrintMode::Break {
                        return true;
                    }
                    remaining -= 1;
                }
                DocIR::SoftLineOrEmpty => {
                    if mode == PrintMode::Break {
                        return true;
                    }
                }
                DocIR::Group {
                    contents,
                    should_break,
                    ..
                } => {
                    let child_mode = if *should_break {
                        PrintMode::Break
                    } else {
                        PrintMode::Flat
                    };
                    for d in contents.iter().rev() {
                        stack.push((d, child_mode));
                    }
                }
                DocIR::Indent(contents) | DocIR::List(contents) => {
                    for d in contents.iter().rev() {
                        stack.push((d, mode));
                    }
                }
                DocIR::IfBreak {
                    break_contents,
                    flat_contents,
                    group_id,
                } => {
                    let is_break = if let Some(gid) = group_id {
                        group_break_map.get(gid).copied().unwrap_or(false)
                    } else {
                        mode == PrintMode::Break
                    };
                    let d = if is_break {
                        break_contents.as_ref()
                    } else {
                        flat_contents.as_ref()
                    };
                    stack.push((d, mode));
                }
                DocIR::Fill { parts } => {
                    for d in parts.iter().rev() {
                        stack.push((d, mode));
                    }
                }
                DocIR::LineSuffix(_) => {}
                DocIR::AlignGroup(group) => {
                    // For fit checking, treat as all entries printed flat
                    for entry in &group.entries {
                        match entry {
                            AlignEntry::Aligned {
                                before,
                                after,
                                trailing,
                            } => {
                                for d in before.iter().rev() {
                                    stack.push((d, mode));
                                }
                                for d in after.iter().rev() {
                                    stack.push((d, mode));
                                }
                                if let Some(trail) = trailing {
                                    for d in trail.iter().rev() {
                                        stack.push((d, mode));
                                    }
                                }
                            }
                            AlignEntry::Line { content, trailing } => {
                                for d in content.iter().rev() {
                                    stack.push((d, mode));
                                }
                                if let Some(trail) = trailing {
                                    for d in trail.iter().rev() {
                                        stack.push((d, mode));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        remaining >= 0
    }

    /// Check whether an IR list contains HardLine
    fn has_hard_line(&mut self, docs: &[DocIR]) -> bool {
        self.profile.has_hard_line_calls += 1;
        for doc in docs {
            self.profile.has_hard_line_nodes_visited += 1;
            match doc {
                DocIR::HardLine => return true,
                DocIR::List(contents) | DocIR::Indent(contents)
                    if self.has_hard_line(contents) => {
                        return true;
                    }
                DocIR::Group { contents, .. }
                    if self.has_hard_line(contents) => {
                        return true;
                    }
                DocIR::AlignGroup(group)
                    // Alignment groups with 2+ entries always produce hard lines
                    if group.entries.len() >= 2 => {
                        return true;
                    }
                _ => {}
            }
        }
        false
    }

    /// Fill: greedy fill
    fn print_fill(&mut self, parts: &[DocIR], mode: PrintMode) {
        self.profile.print_fill_calls += 1;
        let mut i = 0;
        while i < parts.len() {
            let content = &parts[i];
            let content_fits = Self::fits_impl(
                std::slice::from_ref(content),
                (self.max_line_width.saturating_sub(self.current_column)) as isize,
                &self.group_break_map,
                Some(&mut self.profile),
            );

            if content_fits {
                self.print_doc(content, PrintMode::Flat);
            } else {
                self.print_doc(content, PrintMode::Break);
            }

            i += 1;
            if i >= parts.len() {
                break;
            }

            let separator = &parts[i];
            i += 1;

            let next_fits = if i < parts.len() {
                let combo = vec![separator.clone(), parts[i].clone()];
                Self::fits_impl(
                    &combo,
                    (self.max_line_width.saturating_sub(self.current_column)) as isize,
                    &self.group_break_map,
                    Some(&mut self.profile),
                )
            } else {
                true
            };

            if next_fits {
                self.print_doc(separator, PrintMode::Flat);
            } else {
                self.print_doc(separator, PrintMode::Break);
            }
        }
        let _ = mode;
    }

    /// Print an alignment group with up to three-column alignment:
    /// Column 1: `before` (padded to max_before)
    /// Column 2: `after`
    /// Column 3: `trailing` comment (padded to max content width)
    fn print_align_group(&mut self, entries: &[AlignEntry], mode: PrintMode) {
        // Phase 1: Compute max flat width of `before` parts across all Aligned entries
        let max_before = entries
            .iter()
            .filter_map(|e| match e {
                AlignEntry::Aligned { before, .. } => Some(ir_flat_width(before)),
                AlignEntry::Line { .. } => None,
            })
            .max()
            .unwrap_or(0);

        // Phase 2: Compute max content width for trailing comment alignment
        let has_any_trailing = entries.iter().any(|e| match e {
            AlignEntry::Aligned { trailing, .. } | AlignEntry::Line { trailing, .. } => {
                trailing.is_some()
            }
        });

        let max_content_width = if has_any_trailing {
            entries
                .iter()
                .map(|e| match e {
                    AlignEntry::Aligned { after, .. } => {
                        // before is padded to max_before, then " ", then after
                        max_before + 1 + ir_flat_width(after)
                    }
                    AlignEntry::Line { content, .. } => ir_flat_width(content),
                })
                .max()
                .unwrap_or(0)
        } else {
            0
        };

        // Phase 3: Print each entry
        for (i, entry) in entries.iter().enumerate() {
            if i > 0 {
                self.flush_line_suffixes();
                self.push_newline();
            }
            match entry {
                AlignEntry::Aligned {
                    before,
                    after,
                    trailing,
                } => {
                    let before_width = ir_flat_width(before);
                    self.print_docs(before, mode);
                    let padding = max_before - before_width;
                    if padding > 0 {
                        self.push_text(&" ".repeat(padding));
                    }
                    self.push_text(" ");
                    self.print_docs(after, mode);

                    if let Some(trail) = trailing {
                        let content_width = max_before + 1 + ir_flat_width(after);
                        let trail_padding =
                            self.trailing_comment_padding(content_width, max_content_width);
                        if trail_padding > 0 {
                            self.push_text(&" ".repeat(trail_padding));
                        }
                        self.print_docs(trail, mode);
                    }
                }
                AlignEntry::Line { content, trailing } => {
                    self.print_docs(content, mode);

                    if let Some(trail) = trailing {
                        let content_width = ir_flat_width(content);
                        let trail_padding =
                            self.trailing_comment_padding(content_width, max_content_width);
                        if trail_padding > 0 {
                            self.push_text(&" ".repeat(trail_padding));
                        }
                        self.print_docs(trail, mode);
                    }
                }
            }
        }
    }
}

impl MeasuringPrinter {
    fn new(config: &LuaFormatConfig) -> Self {
        Self {
            max_line_width: config.layout.max_line_width,
            indent_width: config.indent_width(),
            line_comment_min_spaces_before: config.comments.line_comment_min_spaces_before.max(1),
            line_comment_min_column: config.comments.line_comment_min_column,
            current_column: 0,
            indent_level: 0,
            trailing_spaces: 0,
            group_break_map: HashMap::new(),
            line_suffixes: Vec::new(),
            metrics: PrintedDocMetrics {
                line_widths: Vec::new(),
            },
            limits: MeasureLimits::default(),
            truncated: false,
            overflow_penalty: 0,
        }
    }

    fn with_limits(mut self, limits: MeasureLimits) -> Self {
        self.limits = limits;
        self
    }

    fn measure(self, docs: &[DocIR]) -> PrintedDocMetrics {
        self.measure_with_truncation(docs).0
    }

    fn measure_with_truncation(mut self, docs: &[DocIR]) -> (PrintedDocMetrics, bool) {
        self.measure_docs(docs, PrintMode::Break);
        if !self.truncated && !self.line_suffixes.is_empty() {
            let suffixes = std::mem::take(&mut self.line_suffixes);
            for suffix in &suffixes {
                self.measure_docs(suffix, PrintMode::Break);
            }
        }

        if !self.truncated && (self.current_column > 0 || self.metrics.line_widths.is_empty()) {
            self.record_line();
        }

        (self.metrics, self.truncated)
    }

    fn measure_docs(&mut self, docs: &[DocIR], mode: PrintMode) {
        for doc in docs {
            if self.truncated {
                break;
            }
            self.measure_doc(doc, mode);
        }
    }

    fn measure_doc(&mut self, doc: &DocIR, mode: PrintMode) {
        if self.truncated {
            return;
        }

        match doc {
            DocIR::Text(s) => self.measure_text(s),
            DocIR::SourceNode { node, trim_end } => {
                let text = node.text();
                if *trim_end {
                    let end = syntax_text_trimmed_end(&text);
                    self.measure_syntax_text(&text.slice(..end));
                } else {
                    self.measure_syntax_text(&text);
                }
            }
            DocIR::SourceToken(token) => self.measure_text(token.text()),
            DocIR::SyntaxToken(kind) => {
                if let Some(text) = kind.syntax_text() {
                    self.measure_text(text);
                }
            }
            DocIR::Space => self.measure_text(" "),
            DocIR::HardLine => {
                self.flush_line_suffixes();
                self.push_newline();
            }
            DocIR::SoftLine => match mode {
                PrintMode::Flat => self.measure_text(" "),
                PrintMode::Break => {
                    self.flush_line_suffixes();
                    self.push_newline();
                }
            },
            DocIR::SoftLineOrEmpty => {
                if mode == PrintMode::Break {
                    self.flush_line_suffixes();
                    self.push_newline();
                }
            }
            DocIR::Group {
                contents,
                should_break,
                id,
            } => {
                let should_break = *should_break || self.has_hard_line(contents);
                let child_mode = if should_break {
                    PrintMode::Break
                } else if self.fits_on_line(contents) {
                    PrintMode::Flat
                } else {
                    PrintMode::Break
                };

                if let Some(gid) = id {
                    self.group_break_map
                        .insert(*gid, child_mode == PrintMode::Break);
                }

                self.measure_docs(contents, child_mode);
            }
            DocIR::Indent(contents) => {
                self.indent_level += 1;
                self.measure_docs(contents, mode);
                self.indent_level -= 1;
            }
            DocIR::List(contents) => self.measure_docs(contents, mode),
            DocIR::IfBreak {
                break_contents,
                flat_contents,
                group_id,
            } => {
                let is_break = if let Some(gid) = group_id {
                    self.group_break_map.get(gid).copied().unwrap_or(false)
                } else {
                    mode == PrintMode::Break
                };
                let d = if is_break {
                    break_contents.as_ref()
                } else {
                    flat_contents.as_ref()
                };
                self.measure_doc(d, mode);
            }
            DocIR::Fill { parts } => self.measure_fill(parts),
            DocIR::LineSuffix(contents) => self.line_suffixes.push(contents.clone()),
            DocIR::AlignGroup(group) => self.measure_align_group(&group.entries, mode),
        }
    }

    fn measure_text(&mut self, s: &str) {
        let mut start = 0usize;
        while let Some(rel_idx) = s[start..].find('\n') {
            let end = start + rel_idx;
            let line = s[start..end].strip_suffix('\r').unwrap_or(&s[start..end]);
            self.measure_line_fragment(line);
            self.push_newline();
            start = end + 1;
        }

        if start < s.len() {
            self.measure_line_fragment(&s[start..]);
        }
    }

    fn measure_syntax_text(&mut self, text: &rowan::SyntaxText) {
        text.for_each_chunk(|chunk| self.measure_text(chunk));
    }

    fn measure_line_fragment(&mut self, fragment: &str) {
        self.current_column += fragment.len();
        self.trailing_spaces = fragment
            .bytes()
            .rev()
            .take_while(|byte| *byte == b' ')
            .count();
    }

    fn push_newline(&mut self) {
        self.record_line();
        self.current_column = self.indent_level * self.indent_width;
        self.trailing_spaces = self.current_column;
    }

    fn record_line(&mut self) {
        let line_index = self.metrics.line_widths.len();
        let line_width = self.current_column.saturating_sub(self.trailing_spaces);
        self.metrics.line_widths.push(line_width);

        let effective_width = if line_index == 0 {
            line_width + self.limits.first_line_prefix_width
        } else {
            line_width
        };
        if effective_width > self.max_line_width {
            self.overflow_penalty += effective_width - self.max_line_width;
        }

        if self
            .limits
            .max_lines
            .is_some_and(|max_lines| self.metrics.line_widths.len() > max_lines)
        {
            self.truncated = true;
            return;
        }

        if self
            .limits
            .max_overflow_penalty
            .is_some_and(|max_overflow_penalty| self.overflow_penalty > max_overflow_penalty)
        {
            self.truncated = true;
        }
    }

    fn flush_line_suffixes(&mut self) {
        if self.line_suffixes.is_empty() {
            return;
        }
        let suffixes = std::mem::take(&mut self.line_suffixes);
        for suffix in &suffixes {
            self.measure_docs(suffix, PrintMode::Break);
        }
    }

    fn trailing_comment_padding(
        &self,
        content_width: usize,
        aligned_content_width: usize,
    ) -> usize {
        let natural_padding = aligned_content_width.saturating_sub(content_width)
            + self.line_comment_min_spaces_before;

        if self.line_comment_min_column == 0 {
            natural_padding
        } else {
            natural_padding.max(self.line_comment_min_column.saturating_sub(content_width))
        }
    }

    fn fits_on_line(&self, docs: &[DocIR]) -> bool {
        let remaining = self.max_line_width.saturating_sub(self.current_column);
        Printer::fits_impl(docs, remaining as isize, &self.group_break_map, None)
    }

    fn has_hard_line(&self, docs: &[DocIR]) -> bool {
        docs.iter().any(|doc| match doc {
            DocIR::HardLine => true,
            DocIR::List(contents) | DocIR::Indent(contents) => self.has_hard_line(contents),
            DocIR::Group { contents, .. } => self.has_hard_line(contents),
            DocIR::AlignGroup(group) => group.entries.len() >= 2,
            _ => false,
        })
    }

    fn measure_fill(&mut self, parts: &[DocIR]) {
        let mut i = 0;
        while i < parts.len() {
            let content = &parts[i];
            let content_fits = Printer::fits_impl(
                std::slice::from_ref(content),
                (self.max_line_width.saturating_sub(self.current_column)) as isize,
                &self.group_break_map,
                None,
            );

            if content_fits {
                self.measure_doc(content, PrintMode::Flat);
            } else {
                self.measure_doc(content, PrintMode::Break);
            }

            i += 1;
            if i >= parts.len() {
                break;
            }

            let separator = &parts[i];
            i += 1;

            let next_fits = if i < parts.len() {
                let combo = vec![separator.clone(), parts[i].clone()];
                Printer::fits_impl(
                    &combo,
                    (self.max_line_width.saturating_sub(self.current_column)) as isize,
                    &self.group_break_map,
                    None,
                )
            } else {
                true
            };

            if next_fits {
                self.measure_doc(separator, PrintMode::Flat);
            } else {
                self.measure_doc(separator, PrintMode::Break);
            }
        }
    }

    fn measure_align_group(&mut self, entries: &[AlignEntry], mode: PrintMode) {
        let max_before = entries
            .iter()
            .filter_map(|e| match e {
                AlignEntry::Aligned { before, .. } => Some(ir_flat_width(before)),
                AlignEntry::Line { .. } => None,
            })
            .max()
            .unwrap_or(0);

        let has_any_trailing = entries.iter().any(|e| match e {
            AlignEntry::Aligned { trailing, .. } | AlignEntry::Line { trailing, .. } => {
                trailing.is_some()
            }
        });

        let max_content_width = if has_any_trailing {
            entries
                .iter()
                .map(|e| match e {
                    AlignEntry::Aligned { after, .. } => max_before + 1 + ir_flat_width(after),
                    AlignEntry::Line { content, .. } => ir_flat_width(content),
                })
                .max()
                .unwrap_or(0)
        } else {
            0
        };

        for (i, entry) in entries.iter().enumerate() {
            if i > 0 {
                self.flush_line_suffixes();
                self.push_newline();
            }
            match entry {
                AlignEntry::Aligned {
                    before,
                    after,
                    trailing,
                } => {
                    let before_width = ir_flat_width(before);
                    self.measure_docs(before, mode);
                    let padding = max_before - before_width;
                    if padding > 0 {
                        self.measure_text(&" ".repeat(padding));
                    }
                    self.measure_text(" ");
                    self.measure_docs(after, mode);

                    if let Some(trail) = trailing {
                        let content_width = max_before + 1 + ir_flat_width(after);
                        let trail_padding =
                            self.trailing_comment_padding(content_width, max_content_width);
                        if trail_padding > 0 {
                            self.measure_text(&" ".repeat(trail_padding));
                        }
                        self.measure_docs(trail, mode);
                    }
                }
                AlignEntry::Line { content, trailing } => {
                    self.measure_docs(content, mode);

                    if let Some(trail) = trailing {
                        let content_width = ir_flat_width(content);
                        let trail_padding =
                            self.trailing_comment_padding(content_width, max_content_width);
                        if trail_padding > 0 {
                            self.measure_text(&" ".repeat(trail_padding));
                        }
                        self.measure_docs(trail, mode);
                    }
                }
            }
        }
    }
}
