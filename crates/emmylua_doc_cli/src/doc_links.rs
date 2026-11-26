//! Symbol link resolution for documentation descriptions.
//!
//! Descriptions use Rust-like intra-doc links: ``[`Widget`]``. The link target
//! is resolved against the documented symbols and rendered relative to the
//! page that contains the description.

use std::collections::HashMap;

use emmylua_code_analysis::LuaType;
use emmylua_parser_desc::parse_intra_doc_link;

use crate::doc_model::{DocMember, DocModel, DocType, DocTypeKind};
use crate::markdown_generator::escape_type_name;

/// The generated page a symbol belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocPageRef {
    pub kind: DocPageKind,
    /// Escaped file stem (without extension).
    pub stem: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocPageKind {
    Class,
    Enum,
    Alias,
    Module,
    Global,
}

impl DocPageRef {
    pub fn for_type(doc_type: &DocType) -> Self {
        let kind = match doc_type.kind {
            DocTypeKind::Class => DocPageKind::Class,
            DocTypeKind::Enum => DocPageKind::Enum,
            DocTypeKind::Alias => DocPageKind::Alias,
        };
        Self {
            kind,
            stem: escape_type_name(&doc_type.full_name),
        }
    }

    pub fn for_module(name: &str) -> Self {
        Self {
            kind: DocPageKind::Module,
            stem: escape_type_name(name),
        }
    }

    pub fn for_global(name: &str) -> Self {
        Self {
            kind: DocPageKind::Global,
            stem: escape_type_name(name),
        }
    }

    fn html_dir(&self) -> &'static str {
        match self.kind {
            DocPageKind::Class => "class",
            DocPageKind::Enum => "enum",
            DocPageKind::Alias => "alias",
            DocPageKind::Module => "module",
            DocPageKind::Global => "global",
        }
    }

    fn markdown_dir(&self) -> &'static str {
        match self.kind {
            DocPageKind::Class | DocPageKind::Enum | DocPageKind::Alias => "types",
            DocPageKind::Module => "modules",
            DocPageKind::Global => "globals",
        }
    }

    fn html_path(&self) -> String {
        format!("{}/{}.html", self.html_dir(), self.stem)
    }

    fn markdown_file_name(&self) -> String {
        format!("{}.md", self.stem)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocLinkTarget {
    pub page: DocPageRef,
    pub anchor: Option<String>,
}

/// Index of every documented type, module, global and member.
#[derive(Debug, Default)]
pub struct DocLinkIndex {
    exact: HashMap<String, DocLinkTarget>,
    aliases: HashMap<String, Vec<DocLinkTarget>>,
}

impl DocLinkIndex {
    pub fn build(model: &DocModel) -> Self {
        let mut index = Self::default();

        for doc_type in &model.types {
            let page = DocPageRef::for_type(doc_type);
            let page_target = DocLinkTarget {
                page: page.clone(),
                anchor: None,
            };
            index.add_exact(doc_type.full_name.clone(), page_target.clone());
            let short_name = doc_type
                .full_name
                .rsplit('.')
                .next()
                .unwrap_or(&doc_type.full_name);
            if short_name != doc_type.full_name {
                index.add_alias(short_name.to_string(), page_target);
            }

            for member in public_members(&doc_type.members) {
                let anchor = member.qualified_name(&doc_type.name);
                let target = DocLinkTarget {
                    page: page.clone(),
                    anchor: Some(anchor.clone()),
                };
                index.add_exact(
                    format!("{}.{}", doc_type.full_name, member.name),
                    target.clone(),
                );
                index.add_exact(anchor, target.clone());
                if short_name != doc_type.full_name {
                    index.add_alias(member.qualified_name(short_name), target);
                }
            }
        }

        for module in &model.modules {
            let page = DocPageRef::for_module(&module.name);
            index.add_exact(
                module.name.clone(),
                DocLinkTarget {
                    page: page.clone(),
                    anchor: None,
                },
            );
            for member in public_members(&model.members_for_module(module)) {
                let anchor = member.qualified_name(&module.name);
                index.add_exact(
                    anchor.clone(),
                    DocLinkTarget {
                        page: page.clone(),
                        anchor: Some(anchor),
                    },
                );
            }
        }

        for global in &model.globals {
            if matches!(global.ty, LuaType::Ref(_) | LuaType::Def(_)) {
                continue;
            }
            let page = DocPageRef::for_global(&global.name);
            index.add_exact(
                global.name.clone(),
                DocLinkTarget {
                    page: page.clone(),
                    anchor: None,
                },
            );
            for member in public_members(&model.members_for_global(global)) {
                let anchor = member.qualified_name(&global.name);
                index.add_exact(
                    anchor.clone(),
                    DocLinkTarget {
                        page: page.clone(),
                        anchor: Some(anchor),
                    },
                );
            }
        }

        index
    }

    fn add_exact(&mut self, name: String, target: DocLinkTarget) {
        self.exact.entry(name).or_insert(target);
    }

    fn add_alias(&mut self, name: String, target: DocLinkTarget) {
        let targets = self.aliases.entry(name).or_default();
        if !targets.contains(&target) {
            targets.push(target);
        }
    }

    pub fn resolve(&self, target: &str) -> Option<&DocLinkTarget> {
        if let Some(target) = self.exact.get(target) {
            return Some(target);
        }
        match self.aliases.get(target) {
            Some(targets) if targets.len() == 1 => targets.first(),
            _ => None,
        }
    }
}

fn public_members(members: &[DocMember]) -> impl Iterator<Item = &DocMember> {
    members.iter().filter(|member| {
        member
            .property
            .visibility
            .is_none_or(|visibility| visibility == emmylua_parser::VisibilityKind::Public)
    })
}

/// Per-page link resolution context.
pub struct LinkContext<'a> {
    index: &'a DocLinkIndex,
    page: DocPageRef,
    root_prefix: &'a str,
}

impl<'a> LinkContext<'a> {
    pub fn new(index: &'a DocLinkIndex, page: DocPageRef, root_prefix: &'a str) -> Self {
        Self {
            index,
            page,
            root_prefix,
        }
    }

    pub fn resolve(&self, target: &str) -> Option<&DocLinkTarget> {
        self.index.resolve(target)
    }

    /// Root-relative HTML href, prefixed with `root_prefix`.
    pub fn html_href(&self, target: &str) -> Option<String> {
        let target = self.resolve(target)?;
        if target.page == self.page
            && let Some(anchor) = &target.anchor
        {
            return Some(format!("#{anchor}"));
        }

        let mut href = format!("{}{}", self.root_prefix, target.page.html_path());
        if let Some(anchor) = &target.anchor {
            href.push('#');
            href.push_str(anchor);
        }
        Some(href)
    }

    /// Relative Markdown link for the current page.
    pub fn markdown_href(&self, target: &str) -> Option<String> {
        let target = self.resolve(target)?;
        if target.page == self.page
            && let Some(anchor) = &target.anchor
        {
            return Some(format!("#{anchor}"));
        }

        let mut href = if target.page.markdown_dir() == self.page.markdown_dir() {
            target.page.markdown_file_name()
        } else {
            format!(
                "../{}/{}",
                target.page.markdown_dir(),
                target.page.markdown_file_name()
            )
        };
        if let Some(anchor) = &target.anchor {
            href.push('#');
            href.push_str(anchor);
        }
        Some(href)
    }

    /// Rewrites intra-doc links in a Markdown description.
    ///
    /// Fenced code blocks and inline code spans are left untouched. Resolved
    /// references become normal Markdown links; unresolved ones are reduced to
    /// their code span so raw brackets never leak into the generated docs.
    pub fn rewrite_markdown(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut in_fenced_code = false;

        for line in text.split_inclusive('\n') {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") {
                in_fenced_code = !in_fenced_code;
                out.push_str(line);
                continue;
            }
            if in_fenced_code {
                out.push_str(line);
                continue;
            }
            out.push_str(&self.rewrite_markdown_line(line));
        }

        out
    }

    fn rewrite_markdown_line(&self, line: &str) -> String {
        let mut out = String::with_capacity(line.len());
        let mut rest = line;
        while !rest.is_empty() {
            if let Some(after_tick) = rest.strip_prefix('`') {
                let span_len = after_tick
                    .find('`')
                    .map(|index| index + 2)
                    .unwrap_or(rest.len());
                let (span, tail) = rest.split_at(span_len);
                out.push_str(span);
                rest = tail;
                continue;
            }

            if let Some(after_open) = rest.strip_prefix('[') {
                if let Some(close) = after_open.find(']') {
                    let label = &after_open[..close];
                    let after_label = &after_open[close + 1..];
                    if after_label.starts_with('(') {
                        // Ordinary explicit Markdown link: keep it as is.
                        if let Some(end) = after_label.find(')') {
                            let len = 1 + close + 1 + end + 1;
                            let (link, tail) = rest.split_at(len);
                            out.push_str(link);
                            rest = tail;
                            continue;
                        }
                    } else if let Some(link) = parse_intra_doc_link(label) {
                        match self.markdown_href(link.target) {
                            Some(href) => {
                                out.push('[');
                                out.push_str(label);
                                out.push_str("](");
                                out.push_str(&href);
                                out.push(')');
                            }
                            None => out.push_str(label),
                        }
                        rest = &after_label;
                        continue;
                    }
                }
            }

            let Some(ch) = rest.chars().next() else {
                break;
            };
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_drops_unresolved_link_brackets() {
        let index = DocLinkIndex::default();
        let page = DocPageRef::for_module("mod");
        let links = LinkContext::new(&index, page, "../");
        assert_eq!(links.rewrite_markdown("[`Missing`]"), "`Missing`");
    }

    #[test]
    fn rewrite_keeps_code_and_explicit_links() {
        let index = DocLinkIndex::default();
        let page = DocPageRef::for_module("mod");
        let links = LinkContext::new(&index, page, "../");
        assert_eq!(
            links.rewrite_markdown("`[`Widget`]` and [text](url)"),
            "`[`Widget`]` and [text](url)"
        );
    }
}
