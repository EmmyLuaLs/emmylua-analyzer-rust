use std::ops::Range;

use crate::{LuaMemberKey, LuaType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowKind {
    Recursion,
    Budget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypePathSegment {
    Member(LuaMemberKey),
    Index(LuaType),
    TupleElement(usize),
    ArrayElement,
    FunctionParameter(usize),
    FunctionReturn(usize),
    GenericArgument(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeMismatchKind {
    Incompatible { source: LuaType, target: LuaType },
    Message(String),
    MissingMember { key: LuaMemberKey },
    MissingTupleElement { index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypePathInfo {
    Relation { source: LuaType, target: LuaType },
}

impl TypePathInfo {
    pub fn relation(source: &LuaType, target: &LuaType) -> Self {
        Self::Relation {
            source: source.clone(),
            target: target.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypePathEntry {
    segment: TypePathSegment,
    info_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeMismatch {
    path: Vec<TypePathEntry>,
    path_info: Vec<TypePathInfo>,
    reason: TypeMismatchKind,
}

impl TypeMismatch {
    pub fn new(reason: TypeMismatchKind) -> Self {
        Self {
            path: Vec::new(),
            path_info: Vec::new(),
            reason,
        }
    }

    pub fn incompatible(source: &LuaType, target: &LuaType) -> Self {
        Self::new(TypeMismatchKind::Incompatible {
            source: source.clone(),
            target: target.clone(),
        })
    }

    pub fn at(self, segment: TypePathSegment) -> Self {
        self.at_with_info(segment, std::iter::empty())
    }

    pub fn at_with_info(
        mut self,
        segment: TypePathSegment,
        info: impl IntoIterator<Item = TypePathInfo>,
    ) -> Self {
        let info_start = self.path_info.len();
        self.path_info.extend(info);
        let info_end = self.path_info.len();
        self.path.push(TypePathEntry {
            segment,
            info_range: info_start..info_end,
        });
        self
    }

    pub fn path(
        &self,
    ) -> impl DoubleEndedIterator<Item = TypePathStep<'_>> + ExactSizeIterator + '_ {
        self.path.iter().map(|entry| TypePathStep {
            segment: &entry.segment,
            info: &self.path_info[entry.info_range.clone()],
        })
    }

    pub fn has_path(&self) -> bool {
        !self.path.is_empty()
    }

    pub fn reason(&self) -> &TypeMismatchKind {
        &self.reason
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TypePathStep<'a> {
    segment: &'a TypePathSegment,
    info: &'a [TypePathInfo],
}

impl<'a> TypePathStep<'a> {
    pub fn segment(self) -> &'a TypePathSegment {
        self.segment
    }

    pub fn info(self) -> &'a [TypePathInfo] {
        self.info
    }
}
