pub(crate) mod alignment;
mod test;

use std::collections::HashMap;

use crate::config::LuaFormatConfig;
use crate::ir::{AlignEntry, DocIR, GroupId, ir_flat_width};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrintMode {
    Flat,
    Break,
}

pub struct Printer {
    max_line_width: usize,
    indent_str: String,
    indent_width: usize,
    newline_str: &'static str,
    output: String,
    current_column: usize,
    indent_level: usize,
    group_break_map: HashMap<GroupId, bool>,
    line_suffixes: Vec<Vec<DocIR>>,
}

impl Printer {
    pub fn new(config: &LuaFormatConfig) -> Self {
        Self {
            max_line_width: config.max_line_width,
            indent_str: config.indent_str(),
            indent_width: config.indent_width(),
            newline_str: config.newline_str(),
            output: String::new(),
            current_column: 0,
            indent_level: 0,
            group_break_map: HashMap::new(),
            line_suffixes: Vec::new(),
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
                self.line_suffixes.push(contents.clone());
            }
            DocIR::AlignGroup(group) => {
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

    /// Check whether contents fit within the remaining line width in Flat mode
    fn fits_on_line(&self, docs: &[DocIR], _current_mode: PrintMode) -> bool {
        let remaining = self.max_line_width.saturating_sub(self.current_column);
        self.fits(docs, remaining as isize)
    }

    fn fits(&self, docs: &[DocIR], mut remaining: isize) -> bool {
        let mut stack: Vec<(&DocIR, PrintMode)> =
            docs.iter().rev().map(|d| (d, PrintMode::Flat)).collect();

        while let Some((doc, mode)) = stack.pop() {
            if remaining < 0 {
                return false;
            }

            match doc {
                DocIR::Text(s) => {
                    remaining -= s.len() as isize;
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
                        self.group_break_map.get(gid).copied().unwrap_or(false)
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
                            AlignEntry::Aligned { before, after } => {
                                for d in before.iter().rev() {
                                    stack.push((d, mode));
                                }
                                for d in after.iter().rev() {
                                    stack.push((d, mode));
                                }
                            }
                            AlignEntry::Line(content) => {
                                for d in content.iter().rev() {
                                    stack.push((d, mode));
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
    fn has_hard_line(&self, docs: &[DocIR]) -> bool {
        for doc in docs {
            match doc {
                DocIR::HardLine => return true,
                DocIR::List(contents) | DocIR::Indent(contents) => {
                    if self.has_hard_line(contents) {
                        return true;
                    }
                }
                DocIR::Group { contents, .. } => {
                    if self.has_hard_line(contents) {
                        return true;
                    }
                }
                DocIR::AlignGroup(group) => {
                    // Alignment groups with 2+ entries always produce hard lines
                    if group.entries.len() >= 2 {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Fill: greedy fill
    fn print_fill(&mut self, parts: &[DocIR], mode: PrintMode) {
        let mut i = 0;
        while i < parts.len() {
            let content = &parts[i];
            let content_fits = self.fits(
                std::slice::from_ref(content),
                (self.max_line_width.saturating_sub(self.current_column)) as isize,
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
                self.fits(
                    &combo,
                    (self.max_line_width.saturating_sub(self.current_column)) as isize,
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

    /// Print an alignment group: pad each entry's `before` to the max width so `after` parts align.
    fn print_align_group(&mut self, entries: &[AlignEntry], mode: PrintMode) {
        // Compute max flat width of `before` parts across all Aligned entries
        let max_before = entries
            .iter()
            .filter_map(|e| match e {
                AlignEntry::Aligned { before, .. } => Some(ir_flat_width(before)),
                AlignEntry::Line(_) => None,
            })
            .max()
            .unwrap_or(0);

        for (i, entry) in entries.iter().enumerate() {
            if i > 0 {
                self.flush_line_suffixes();
                self.push_newline();
            }
            match entry {
                AlignEntry::Aligned { before, after } => {
                    let before_width = ir_flat_width(before);
                    self.print_docs(before, mode);
                    let padding = max_before - before_width;
                    if padding > 0 {
                        self.push_text(&" ".repeat(padding));
                    }
                    self.push_text(" ");
                    self.print_docs(after, mode);
                }
                AlignEntry::Line(content) => {
                    self.print_docs(content, mode);
                }
            }
        }
    }
}
