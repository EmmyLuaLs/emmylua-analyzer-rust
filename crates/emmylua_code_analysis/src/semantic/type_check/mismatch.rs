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
    Incompatible,
    Message(String),
    MissingMember { key: LuaMemberKey },
    MissingTupleElement { index: usize },
    UnresolvedType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MismatchStep {
    pub(super) segment: Option<TypePathSegment>,
    pub(super) source: LuaType,
    pub(super) target: LuaType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeMismatch {
    source: LuaType,
    target: LuaType,
    steps: Vec<MismatchStep>,
    reason: TypeMismatchKind,
}

impl TypeMismatch {
    pub(crate) fn new(source: &LuaType, target: &LuaType, reason: TypeMismatchKind) -> Self {
        Self {
            source: source.clone(),
            target: target.clone(),
            steps: Vec::new(),
            reason,
        }
    }

    pub(crate) fn incompatible(source: &LuaType, target: &LuaType) -> Self {
        Self::new(source, target, TypeMismatchKind::Incompatible)
    }

    pub(crate) fn at(
        mut self,
        segment: TypePathSegment,
        source: &LuaType,
        target: &LuaType,
    ) -> Self {
        self.steps.push(MismatchStep {
            segment: Some(segment),
            source: self.source.clone(),
            target: self.target.clone(),
        });
        self.source = source.clone();
        self.target = target.clone();
        self
    }

    pub(crate) fn with_outer_relation(mut self, source: &LuaType, target: &LuaType) -> Self {
        if self.source == *source && self.target == *target {
            return self;
        }

        self.steps.push(MismatchStep {
            segment: None,
            source: self.source.clone(),
            target: self.target.clone(),
        });
        self.source = source.clone();
        self.target = target.clone();
        self
    }

    pub fn source(&self) -> &LuaType {
        &self.source
    }

    pub fn target(&self) -> &LuaType {
        &self.target
    }

    pub fn reason(&self) -> &TypeMismatchKind {
        &self.reason
    }

    pub(super) fn steps(&self) -> &[MismatchStep] {
        &self.steps
    }

    pub fn locate_in(&self, source_expr: &LuaExpr) -> TextRange {
        locate_mismatch_range(source_expr, self)
    }
}

pub fn render_type_mismatch_reason(db: &DbIndex, mismatch: &TypeMismatch) -> Option<String> {
    let mut lines = Vec::new();
    let mut depth = 0;
    let mut parent_source = mismatch.source();
    let mut parent_target = mismatch.target();

    for step in mismatch.steps.iter().rev() {
        if let Some(segment) = &step.segment {
            if let Some(title) = render_path_title(db, segment) {
                lines.push(format!("{}{}", "  ".repeat(depth), title));
                depth += 1;
            }
        }

        if &step.source != parent_source || &step.target != parent_target {
            lines.push(format!(
                "{}{}",
                "  ".repeat(depth),
                render_relation(db, &step.source, &step.target)
            ));
            depth += 1;
        }

        parent_source = &step.source;
        parent_target = &step.target;
    }

    let terminal_reason = match mismatch.reason() {
        TypeMismatchKind::Incompatible => None,
        TypeMismatchKind::Message(message) => Some(message.clone()),
        TypeMismatchKind::MissingMember { key } => {
            Some(format!("Property '{}' is missing.", key.to_path()))
        }
        TypeMismatchKind::MissingTupleElement { index } => {
            Some(format!("Tuple element {} is missing.", index + 1))
        }
        TypeMismatchKind::UnresolvedType => Some("Type could not be resolved.".into()),
    };
    if let Some(reason) = terminal_reason {
        lines.push(format!("{}{}", "  ".repeat(depth), reason));
    }

    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn render_path_title(db: &DbIndex, segment: &TypePathSegment) -> Option<String> {
    match segment {
        TypePathSegment::Member(key) => Some(format!(
            "Types of property '{}' are incompatible.",
            key.to_path()
        )),
        TypePathSegment::Index(key) => Some(format!(
            "Index type '{}' is incompatible.",
            humanize_type(db, key, RenderLevel::Simple)
        )),
        TypePathSegment::TupleElement(index) => {
            Some(format!("Tuple element {} is incompatible.", index + 1))
        }
        TypePathSegment::ArrayElement => Some("Array element is incompatible.".into()),
        TypePathSegment::FunctionParameter(index) => {
            Some(format!("Function parameter {} is incompatible.", index + 1))
        }
        TypePathSegment::FunctionReturn(index) => {
            Some(format!("Function return {} is incompatible.", index + 1))
        }
        TypePathSegment::GenericArgument(index) => {
            Some(format!("Generic argument {} is incompatible.", index + 1))
        }
        TypePathSegment::SourceUnionMember(_)
        | TypePathSegment::TargetUnionCandidate(_)
        | TypePathSegment::IntersectionMember(_) => None,
    }
}

fn render_relation(db: &DbIndex, source: &LuaType, target: &LuaType) -> String {
    format!(
        "Type '{}' is not assignable to type '{}'.",
        humanize_type(db, source, RenderLevel::Simple),
        humanize_type(db, target, RenderLevel::Simple)
    )
}
