use std::str::FromStr;

use emmylua_parser::{
    LineIndex, LuaAssignStat, LuaAstNode, LuaCallExprStat, LuaChunk, LuaClosureExpr, LuaComment,
    LuaDocTagDiagnostic, LuaFuncStat, LuaLocalFuncStat, LuaLocalStat, LuaReturnStat, LuaTableField,
};
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;

use super::super::{SalsaLookupBucket, build_lookup_buckets, find_bucket_indices};

use crate::{
    DiagnosticCode, SalsaDocOwnerKindSummary, SalsaDocOwnerSummary, SalsaDocSummary,
    SalsaDocTagKindSummary, SalsaDocTagSummary, SalsaDocVersionConditionSummary,
    SalsaDocVisibilityKindSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTagNameContentSummary {
    pub name: SmolStr,
    pub content: Option<SmolStr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocDiagnosticActionKindSummary {
    Disable,
    DisableNextLine,
    DisableLine,
    Enable,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocDiagnosticActionSummary {
    pub syntax_offset: TextSize,
    pub kind: SalsaDocDiagnosticActionKindSummary,
    pub code_name: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalsaResolvedDocDiagnosticActionSummary {
    pub syntax_offset: TextSize,
    pub kind: SalsaDocDiagnosticActionKindSummary,
    pub code: Option<DiagnosticCode>,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocTagPropertyEntrySummary {
    Visibility(SalsaDocVisibilityKindSummary),
    Source(SmolStr),
    Module(SmolStr),
    Namespace(SmolStr),
    Meta(SmolStr),
    Language(SmolStr),
    Deprecated(Option<SmolStr>),
    Nodiscard(Option<SmolStr>),
    Version(Vec<SalsaDocVersionConditionSummary>),
    Using(SmolStr),
    See(SmolStr),
    Diagnostic(Vec<SalsaDocDiagnosticActionSummary>),
    CustomTag(Box<SalsaDocTagNameContentSummary>),
    AttributeUse(SmolStr),
    Schema(SmolStr),
    Async,
    Readonly,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTagPropertySummary {
    pub owner: SalsaDocOwnerSummary,
    pub entries: Vec<SalsaDocTagPropertyEntrySummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTagQueryIndex {
    pub tags: Vec<SalsaDocTagSummary>,
    pub properties: Vec<SalsaDocTagPropertySummary>,
    by_tag_offset: Vec<SalsaLookupBucket<TextSize>>,
    by_tag_kind: Vec<SalsaLookupBucket<SalsaDocTagKindSummary>>,
    by_tag_owner: Vec<SalsaLookupBucket<SalsaDocOwnerSummary>>,
    by_property_owner: Vec<SalsaLookupBucket<SalsaDocOwnerSummary>>,
    ownerless_property_indices: Vec<usize>,
}

impl SalsaDocTagPropertySummary {
    pub fn visibility(&self) -> Option<&SalsaDocVisibilityKindSummary> {
        self.entries.iter().find_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::Visibility(visibility) => Some(visibility),
            _ => None,
        })
    }

    pub fn source(&self) -> Option<&SmolStr> {
        self.entries.iter().find_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::Source(source) => Some(source),
            _ => None,
        })
    }

    pub fn module(&self) -> Option<&SmolStr> {
        self.entries.iter().find_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::Module(module) => Some(module),
            _ => None,
        })
    }

    pub fn namespace(&self) -> Option<&SmolStr> {
        self.entries.iter().find_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::Namespace(namespace) => Some(namespace),
            _ => None,
        })
    }

    pub fn meta(&self) -> Option<&SmolStr> {
        self.entries.iter().find_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::Meta(meta) => Some(meta),
            _ => None,
        })
    }

    pub fn language(&self) -> Option<&SmolStr> {
        self.entries.iter().find_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::Language(language) => Some(language),
            _ => None,
        })
    }

    pub fn deprecated_message(&self) -> Option<Option<&SmolStr>> {
        self.entries.iter().find_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::Deprecated(message) => Some(message.as_ref()),
            _ => None,
        })
    }

    pub fn nodiscard_message(&self) -> Option<Option<&SmolStr>> {
        self.entries.iter().find_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::Nodiscard(message) => Some(message.as_ref()),
            _ => None,
        })
    }

    pub fn version_conds(&self) -> &[SalsaDocVersionConditionSummary] {
        self.entries
            .iter()
            .find_map(|entry| match entry {
                SalsaDocTagPropertyEntrySummary::Version(version_conds) => {
                    Some(version_conds.as_slice())
                }
                _ => None,
            })
            .unwrap_or(&[])
    }

    pub fn using_names(&self) -> impl Iterator<Item = &SmolStr> {
        self.entries.iter().filter_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::Using(name) => Some(name),
            _ => None,
        })
    }

    pub fn see(&self) -> impl Iterator<Item = &SmolStr> {
        self.entries.iter().filter_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::See(content) => Some(content),
            _ => None,
        })
    }

    pub fn diagnostics(&self) -> impl Iterator<Item = &[SalsaDocDiagnosticActionSummary]> {
        self.entries.iter().filter_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::Diagnostic(diagnostic) => Some(diagnostic.as_slice()),
            _ => None,
        })
    }

    pub fn custom_tags(&self) -> impl Iterator<Item = &SalsaDocTagNameContentSummary> {
        self.entries.iter().filter_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::CustomTag(tag) => Some(tag.as_ref()),
            _ => None,
        })
    }

    pub fn attribute_uses(&self) -> impl Iterator<Item = &SmolStr> {
        self.entries.iter().filter_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::AttributeUse(name) => Some(name),
            _ => None,
        })
    }

    pub fn schemas(&self) -> impl Iterator<Item = &SmolStr> {
        self.entries.iter().filter_map(|entry| match entry {
            SalsaDocTagPropertyEntrySummary::Schema(schema) => Some(schema),
            _ => None,
        })
    }

    pub fn is_deprecated(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry, SalsaDocTagPropertyEntrySummary::Deprecated(_)))
    }

    pub fn is_nodiscard(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry, SalsaDocTagPropertyEntrySummary::Nodiscard(_)))
    }

    pub fn is_async(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry, SalsaDocTagPropertyEntrySummary::Async))
    }

    pub fn is_readonly(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry, SalsaDocTagPropertyEntrySummary::Readonly))
    }
}

pub fn find_doc_tag_at(
    doc: &SalsaDocSummary,
    syntax_offset: TextSize,
) -> Option<SalsaDocTagSummary> {
    let index = build_doc_tag_query_index(doc);
    find_doc_tag_at_in_index(&index, syntax_offset)
}

pub fn collect_doc_tags_for_kind(
    doc: &SalsaDocSummary,
    kind: &SalsaDocTagKindSummary,
) -> Vec<SalsaDocTagSummary> {
    let index = build_doc_tag_query_index(doc);
    collect_doc_tags_for_kind_in_index(&index, kind)
}

pub fn collect_doc_tags_for_owner(
    doc: &SalsaDocSummary,
    owner: &SalsaDocOwnerSummary,
) -> Vec<SalsaDocTagSummary> {
    let index = build_doc_tag_query_index(doc);
    collect_doc_tags_for_owner_in_index(&index, owner)
}

pub fn collect_doc_tag_properties(doc: &SalsaDocSummary) -> Vec<SalsaDocTagPropertySummary> {
    build_doc_tag_query_index(doc).properties
}

pub fn find_doc_tag_property(
    doc: &SalsaDocSummary,
    owner: &SalsaDocOwnerSummary,
) -> Option<SalsaDocTagPropertySummary> {
    let index = build_doc_tag_query_index(doc);
    find_doc_tag_property_in_index(&index, owner)
}

pub fn collect_resolved_doc_tag_diagnostics_for_property(
    property: &SalsaDocTagPropertySummary,
    chunk: &LuaChunk,
    text: &str,
) -> Vec<SalsaResolvedDocDiagnosticActionSummary> {
    let line_index = LineIndex::parse(text);
    property
        .diagnostics()
        .flat_map(|diagnostics| diagnostics.iter())
        .filter_map(|diagnostic| {
            resolve_doc_tag_diagnostic_action(diagnostic, &property.owner, chunk, text, &line_index)
        })
        .collect()
}

pub fn build_doc_tag_query_index(doc: &SalsaDocSummary) -> SalsaDocTagQueryIndex {
    let tags = doc.tags.clone();
    let mut tag_offset_entries = Vec::with_capacity(tags.len());
    let mut tag_kind_entries = Vec::with_capacity(tags.len());
    let mut tag_owner_entries = Vec::with_capacity(tags.len());

    for (index, tag) in tags.iter().enumerate() {
        tag_offset_entries.push((tag.syntax_offset, index));
        tag_kind_entries.push((tag.kind.clone(), index));
        tag_owner_entries.push((tag.owner.clone(), index));
    }

    let properties = build_doc_tag_properties_from_tags(&tags);
    let mut property_owner_entries = Vec::with_capacity(properties.len());
    let mut ownerless_property_indices = Vec::new();

    for (index, property) in properties.iter().enumerate() {
        property_owner_entries.push((property.owner.clone(), index));
        if property.owner.syntax_offset.is_none() {
            ownerless_property_indices.push(index);
        }
    }

    SalsaDocTagQueryIndex {
        tags,
        properties,
        by_tag_offset: build_lookup_buckets(tag_offset_entries),
        by_tag_kind: build_lookup_buckets(tag_kind_entries),
        by_tag_owner: build_lookup_buckets(tag_owner_entries),
        by_property_owner: build_lookup_buckets(property_owner_entries),
        ownerless_property_indices,
    }
}

pub fn build_doc_tag_query_index_from_properties(
    properties: &[SalsaDocTagPropertySummary],
) -> SalsaDocTagQueryIndex {
    let properties = properties.to_vec();
    let mut property_owner_entries = Vec::with_capacity(properties.len());
    let mut ownerless_property_indices = Vec::new();

    for (index, property) in properties.iter().enumerate() {
        property_owner_entries.push((property.owner.clone(), index));
        if property.owner.syntax_offset.is_none() {
            ownerless_property_indices.push(index);
        }
    }

    SalsaDocTagQueryIndex {
        tags: Vec::new(),
        properties,
        by_tag_offset: Vec::new(),
        by_tag_kind: Vec::new(),
        by_tag_owner: Vec::new(),
        by_property_owner: build_lookup_buckets(property_owner_entries),
        ownerless_property_indices,
    }
}

pub fn find_doc_tag_at_in_index(
    index: &SalsaDocTagQueryIndex,
    syntax_offset: TextSize,
) -> Option<SalsaDocTagSummary> {
    find_bucket_indices(&index.by_tag_offset, &syntax_offset)
        .and_then(|indices| indices.first().copied())
        .map(|tag_index| index.tags[tag_index].clone())
}

pub fn collect_doc_tags_for_kind_in_index(
    index: &SalsaDocTagQueryIndex,
    kind: &SalsaDocTagKindSummary,
) -> Vec<SalsaDocTagSummary> {
    collect_tags(index, find_bucket_indices(&index.by_tag_kind, kind))
}

pub fn collect_doc_tags_for_owner_in_index(
    index: &SalsaDocTagQueryIndex,
    owner: &SalsaDocOwnerSummary,
) -> Vec<SalsaDocTagSummary> {
    collect_tags(index, find_bucket_indices(&index.by_tag_owner, owner))
}

pub fn find_doc_tag_property_in_index(
    index: &SalsaDocTagQueryIndex,
    owner: &SalsaDocOwnerSummary,
) -> Option<SalsaDocTagPropertySummary> {
    find_bucket_indices(&index.by_property_owner, owner)
        .and_then(|indices| indices.first().copied())
        .map(|property_index| index.properties[property_index].clone())
}

pub fn collect_doc_tag_properties_for_owners_in_index(
    index: &SalsaDocTagQueryIndex,
    owners: &[SalsaDocOwnerSummary],
) -> Vec<SalsaDocTagPropertySummary> {
    let mut property_indices = owners
        .iter()
        .filter_map(|owner| find_bucket_indices(&index.by_property_owner, owner))
        .flat_map(|indices: &[usize]| indices.iter().copied())
        .collect::<Vec<_>>();
    property_indices.sort_unstable();
    property_indices.dedup();

    property_indices
        .into_iter()
        .map(|property_index: usize| index.properties[property_index].clone())
        .collect()
}

pub fn collect_ownerless_doc_tag_properties_in_index(
    index: &SalsaDocTagQueryIndex,
) -> Vec<SalsaDocTagPropertySummary> {
    index
        .ownerless_property_indices
        .iter()
        .map(|property_index| index.properties[*property_index].clone())
        .collect()
}

fn build_doc_tag_properties_from_tags(
    tags: &[SalsaDocTagSummary],
) -> Vec<SalsaDocTagPropertySummary> {
    let mut property_tags = tags
        .iter()
        .filter(|tag| is_property_like_tag(&tag.kind))
        .cloned()
        .collect::<Vec<_>>();
    property_tags.sort_by(|left, right| {
        left.owner
            .cmp(&right.owner)
            .then(left.syntax_offset.cmp(&right.syntax_offset))
    });

    let mut properties = Vec::new();
    for tag in property_tags {
        if properties
            .last()
            .is_none_or(|property: &SalsaDocTagPropertySummary| property.owner != tag.owner)
        {
            properties.push(SalsaDocTagPropertySummary {
                owner: tag.owner.clone(),
                entries: Vec::new(),
            });
        }

        let property = properties.last_mut().expect("property summary just pushed");

        match &tag.kind {
            SalsaDocTagKindSummary::Visibility => {
                if let Some(visibility) = tag.visibility().cloned() {
                    property
                        .entries
                        .push(SalsaDocTagPropertyEntrySummary::Visibility(visibility));
                }
            }
            SalsaDocTagKindSummary::Source => {
                if let Some(source) = tag.content().cloned() {
                    property
                        .entries
                        .push(SalsaDocTagPropertyEntrySummary::Source(source));
                }
            }
            SalsaDocTagKindSummary::Module => {
                if let Some(module) = tag.name().cloned() {
                    property
                        .entries
                        .push(SalsaDocTagPropertyEntrySummary::Module(module));
                }
            }
            SalsaDocTagKindSummary::Namespace => {
                if let Some(namespace) = tag.name().cloned() {
                    property
                        .entries
                        .push(SalsaDocTagPropertyEntrySummary::Namespace(namespace));
                }
            }
            SalsaDocTagKindSummary::Meta => {
                if let Some(meta) = tag.name().cloned() {
                    property
                        .entries
                        .push(SalsaDocTagPropertyEntrySummary::Meta(meta));
                }
            }
            SalsaDocTagKindSummary::Language => {
                if let Some(language) = tag.name().cloned() {
                    property
                        .entries
                        .push(SalsaDocTagPropertyEntrySummary::Language(language));
                }
            }
            SalsaDocTagKindSummary::Deprecated => {
                property
                    .entries
                    .push(SalsaDocTagPropertyEntrySummary::Deprecated(
                        tag.content().cloned(),
                    ));
            }
            SalsaDocTagKindSummary::Nodiscard => {
                property
                    .entries
                    .push(SalsaDocTagPropertyEntrySummary::Nodiscard(
                        tag.content().cloned(),
                    ));
            }
            SalsaDocTagKindSummary::Version => {
                property
                    .entries
                    .push(SalsaDocTagPropertyEntrySummary::Version(
                        tag.version_conds().to_vec(),
                    ));
            }
            SalsaDocTagKindSummary::Using => {
                if let Some(name) = tag.name().cloned() {
                    property
                        .entries
                        .push(SalsaDocTagPropertyEntrySummary::Using(name));
                }
            }
            SalsaDocTagKindSummary::See => {
                if let Some(content) = tag.content().cloned() {
                    property
                        .entries
                        .push(SalsaDocTagPropertyEntrySummary::See(content));
                }
            }
            SalsaDocTagKindSummary::Diagnostic => {
                let actions = build_diagnostic_summaries(&tag);
                if !actions.is_empty() {
                    property
                        .entries
                        .push(SalsaDocTagPropertyEntrySummary::Diagnostic(actions));
                }
            }
            SalsaDocTagKindSummary::Other => {
                if let Some(name) = tag.name().cloned() {
                    property
                        .entries
                        .push(SalsaDocTagPropertyEntrySummary::CustomTag(Box::new(
                            SalsaDocTagNameContentSummary {
                                name,
                                content: tag.content().cloned(),
                            },
                        )));
                }
            }
            SalsaDocTagKindSummary::AttributeUse => {
                property.entries.extend(
                    tag.items()
                        .iter()
                        .cloned()
                        .map(SalsaDocTagPropertyEntrySummary::AttributeUse),
                );
            }
            SalsaDocTagKindSummary::Schema => {
                if let Some(content) = tag.content().cloned() {
                    property
                        .entries
                        .push(SalsaDocTagPropertyEntrySummary::Schema(content));
                }
            }
            SalsaDocTagKindSummary::Async => {
                property
                    .entries
                    .push(SalsaDocTagPropertyEntrySummary::Async);
            }
            SalsaDocTagKindSummary::Readonly => {
                property
                    .entries
                    .push(SalsaDocTagPropertyEntrySummary::Readonly);
            }
            _ => {}
        }
    }

    properties
}

fn collect_tags(
    index: &SalsaDocTagQueryIndex,
    tag_indices: Option<&[usize]>,
) -> Vec<SalsaDocTagSummary> {
    tag_indices
        .into_iter()
        .flatten()
        .map(|tag_index| index.tags[*tag_index].clone())
        .collect()
}

fn build_diagnostic_summaries(tag: &SalsaDocTagSummary) -> Vec<SalsaDocDiagnosticActionSummary> {
    let Some(kind) = tag
        .name()
        .and_then(|name| diagnostic_action_kind_from_name(name.as_str()))
    else {
        return Vec::new();
    };

    if matches!(kind, SalsaDocDiagnosticActionKindSummary::Enable) {
        return tag
            .items()
            .iter()
            .cloned()
            .map(|code_name| SalsaDocDiagnosticActionSummary {
                syntax_offset: tag.syntax_offset,
                kind,
                code_name: Some(code_name),
            })
            .collect();
    }

    if tag.items().is_empty() {
        return vec![SalsaDocDiagnosticActionSummary {
            syntax_offset: tag.syntax_offset,
            kind,
            code_name: None,
        }];
    }

    tag.items()
        .iter()
        .cloned()
        .map(|code_name| SalsaDocDiagnosticActionSummary {
            syntax_offset: tag.syntax_offset,
            kind,
            code_name: Some(code_name),
        })
        .collect()
}

fn diagnostic_action_kind_from_name(name: &str) -> Option<SalsaDocDiagnosticActionKindSummary> {
    match name {
        "disable" => Some(SalsaDocDiagnosticActionKindSummary::Disable),
        "disable-next-line" => Some(SalsaDocDiagnosticActionKindSummary::DisableNextLine),
        "disable-line" => Some(SalsaDocDiagnosticActionKindSummary::DisableLine),
        "enable" => Some(SalsaDocDiagnosticActionKindSummary::Enable),
        _ => None,
    }
}

fn resolve_doc_tag_diagnostic_action(
    diagnostic: &SalsaDocDiagnosticActionSummary,
    owner: &SalsaDocOwnerSummary,
    chunk: &LuaChunk,
    text: &str,
    line_index: &LineIndex,
) -> Option<SalsaResolvedDocDiagnosticActionSummary> {
    let diagnostic_tag = find_doc_diagnostic_tag_at(chunk, diagnostic.syntax_offset)?;
    let comment = diagnostic_tag.ancestors::<LuaComment>().next()?;
    let comment_range = comment.get_range();
    let range = match diagnostic.kind {
        SalsaDocDiagnosticActionKindSummary::Disable => {
            resolve_doc_owner_range(owner, chunk).unwrap_or_else(|| document_range(text))
        }
        SalsaDocDiagnosticActionKindSummary::DisableNextLine => {
            let comment_end_line = line_index.get_line(comment_range.end())?;
            let next_line_range = line_range(line_index, text, comment_end_line + 1)?;
            TextRange::new(comment_range.start(), next_line_range.end())
        }
        SalsaDocDiagnosticActionKindSummary::DisableLine => {
            let comment_end_line = line_index.get_line(comment_range.end())?;
            line_range(line_index, text, comment_end_line)?
        }
        SalsaDocDiagnosticActionKindSummary::Enable => {
            if owner.syntax_offset.is_none() {
                document_range(text)
            } else {
                comment_range
            }
        }
    };

    let code = diagnostic
        .code_name
        .as_ref()
        .and_then(|code_name| DiagnosticCode::from_str(code_name.as_str()).ok())
        .filter(|code| *code != DiagnosticCode::None);

    Some(SalsaResolvedDocDiagnosticActionSummary {
        syntax_offset: diagnostic.syntax_offset,
        kind: diagnostic.kind,
        code,
        range,
    })
}

fn find_doc_diagnostic_tag_at(
    chunk: &LuaChunk,
    syntax_offset: TextSize,
) -> Option<LuaDocTagDiagnostic> {
    chunk
        .descendants::<LuaDocTagDiagnostic>()
        .find(|diagnostic| TextSize::from(u32::from(diagnostic.get_position())) == syntax_offset)
}

fn resolve_doc_owner_range(owner: &SalsaDocOwnerSummary, chunk: &LuaChunk) -> Option<TextRange> {
    let offset = owner.syntax_offset?;
    match owner.kind {
        SalsaDocOwnerKindSummary::AssignStat => find_node_range_at::<LuaAssignStat>(chunk, offset),
        SalsaDocOwnerKindSummary::LocalStat => find_node_range_at::<LuaLocalStat>(chunk, offset),
        SalsaDocOwnerKindSummary::FuncStat => find_node_range_at::<LuaFuncStat>(chunk, offset),
        SalsaDocOwnerKindSummary::LocalFuncStat => {
            find_node_range_at::<LuaLocalFuncStat>(chunk, offset)
        }
        SalsaDocOwnerKindSummary::TableField => find_node_range_at::<LuaTableField>(chunk, offset),
        SalsaDocOwnerKindSummary::Closure => find_node_range_at::<LuaClosureExpr>(chunk, offset),
        SalsaDocOwnerKindSummary::CallExprStat => {
            find_node_range_at::<LuaCallExprStat>(chunk, offset)
        }
        SalsaDocOwnerKindSummary::ReturnStat => find_node_range_at::<LuaReturnStat>(chunk, offset),
        SalsaDocOwnerKindSummary::None | SalsaDocOwnerKindSummary::Other => None,
    }
}

fn find_node_range_at<T>(chunk: &LuaChunk, offset: TextSize) -> Option<TextRange>
where
    T: LuaAstNode,
{
    chunk
        .descendants::<T>()
        .find(|node| TextSize::from(u32::from(node.get_position())) == offset)
        .map(|node| node.get_range())
}

fn line_range(line_index: &LineIndex, text: &str, line: usize) -> Option<TextRange> {
    let start = line_index.get_line_offset(line)?;
    if let Some(end) = line_index.get_line_offset(line + 1) {
        Some(TextRange::new(start, end))
    } else {
        let end = TextSize::new(text.len() as u32);
        (end > start).then(|| TextRange::new(start, end))
    }
}

fn document_range(text: &str) -> TextRange {
    TextRange::new(TextSize::new(0), TextSize::new(text.len() as u32))
}

fn is_property_like_tag(kind: &SalsaDocTagKindSummary) -> bool {
    matches!(
        kind,
        SalsaDocTagKindSummary::AttributeUse
            | SalsaDocTagKindSummary::Module
            | SalsaDocTagKindSummary::See
            | SalsaDocTagKindSummary::Diagnostic
            | SalsaDocTagKindSummary::Deprecated
            | SalsaDocTagKindSummary::Version
            | SalsaDocTagKindSummary::Source
            | SalsaDocTagKindSummary::Schema
            | SalsaDocTagKindSummary::Other
            | SalsaDocTagKindSummary::Namespace
            | SalsaDocTagKindSummary::Using
            | SalsaDocTagKindSummary::Meta
            | SalsaDocTagKindSummary::Nodiscard
            | SalsaDocTagKindSummary::Readonly
            | SalsaDocTagKindSummary::Async
            | SalsaDocTagKindSummary::Visibility
            | SalsaDocTagKindSummary::Language
    )
}
