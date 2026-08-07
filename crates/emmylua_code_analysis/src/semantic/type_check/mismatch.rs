use emmylua_parser::LuaExpr;
use rowan::TextRange;

use crate::{
    DbIndex, LuaMemberKey, LuaType, RenderLevel, humanize_type,
    semantic::type_check::locator::locate_mismatch_range,
};
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
    SourceUnionMember(usize),
    TargetUnionCandidate(usize),
    IntersectionMember(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeMismatchKind {
    Incompatible { source: LuaType, target: LuaType },
    Message(String),
    MissingMember { key: LuaMemberKey },
    MissingTupleElement { index: usize },
    UnresolvedType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeMismatch {
    source: LuaType,
    target: LuaType,
    frames: Vec<TypePathSegment>,
    leaf: TypeMismatchKind,
}

impl TypeMismatch {
    pub(crate) fn new(source: &LuaType, target: &LuaType, leaf: TypeMismatchKind) -> Self {
        Self {
            source: source.clone(),
            target: target.clone(),
            frames: Vec::new(),
            leaf,
        }
    }
    pub(crate) fn incompatible(source: &LuaType, target: &LuaType) -> Self {
        Self::new(
            source,
            target,
            TypeMismatchKind::Incompatible {
                source: source.clone(),
                target: target.clone(),
            },
        )
    }

    pub(crate) fn at(mut self, frame: TypePathSegment, source: &LuaType, target: &LuaType) -> Self {
        self.source = source.clone();
        self.target = target.clone();
        self.frames.push(frame);
        self
    }

    pub fn source(&self) -> &LuaType {
        &self.source
    }

    pub fn target(&self) -> &LuaType {
        &self.target
    }

    pub fn frames(&self) -> &[TypePathSegment] {
        &self.frames
    }

    pub fn leaf(&self) -> &TypeMismatchKind {
        &self.leaf
    }

    pub fn locate_in(&self, source_expr: &LuaExpr) -> TextRange {
        locate_mismatch_range(source_expr, self.frames())
    }
}

pub fn render_type_mismatch(db: &DbIndex, mismatch: &TypeMismatch) -> String {
    let mut lines = vec![format!(
        "Type '{}' is not assignable to type '{}'.",
        humanize_type(db, &mismatch.source, RenderLevel::Simple),
        humanize_type(db, &mismatch.target, RenderLevel::Simple)
    )];
    for frame in mismatch.frames.iter().rev() {
        lines.push(match frame {
            TypePathSegment::Member(key) => {
                format!("Types of property '{}' are incompatible.", key.to_path())
            }
            TypePathSegment::Index(key) => format!(
                "Index type '{}' is incompatible.",
                humanize_type(db, key, RenderLevel::Simple)
            ),
            TypePathSegment::TupleElement(i) => format!("Tuple element {} is incompatible.", i + 1),
            TypePathSegment::ArrayElement => "Array element is incompatible.".into(),
            TypePathSegment::FunctionParameter(i) => {
                format!("Function parameter {} is incompatible.", i + 1)
            }
            TypePathSegment::FunctionReturn(i) => {
                format!("Function return {} is incompatible.", i + 1)
            }
            TypePathSegment::GenericArgument(i) => {
                format!("Generic argument {} is incompatible.", i + 1)
            }
            TypePathSegment::SourceUnionMember(i) => {
                format!("Source union member {} is incompatible.", i + 1)
            }
            TypePathSegment::TargetUnionCandidate(i) => {
                format!("Target union candidate {} is incompatible.", i + 1)
            }
            TypePathSegment::IntersectionMember(i) => {
                format!("Intersection member {} is incompatible.", i + 1)
            }
        });
    }
    lines.push(match &mismatch.leaf {
        TypeMismatchKind::Incompatible { source, target } => format!(
            "Type '{}' is not assignable to type '{}'.",
            humanize_type(db, source, RenderLevel::Simple),
            humanize_type(db, target, RenderLevel::Simple)
        ),
        TypeMismatchKind::Message(message) => message.clone(),
        TypeMismatchKind::MissingMember { key } => {
            format!("Property '{}' is missing.", key.to_path())
        }
        TypeMismatchKind::MissingTupleElement { index } => {
            format!("Tuple element {} is missing.", index + 1)
        }
        TypeMismatchKind::UnresolvedType => "Type could not be resolved.".into(),
    });
    lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                line
            } else {
                format!("{}{}", "  ".repeat(i), line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
