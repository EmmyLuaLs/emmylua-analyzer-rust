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
pub struct TypeMismatch {
    path: Vec<TypePathSegment>,
    reason: TypeMismatchKind,
}

impl TypeMismatch {
    pub fn new(reason: TypeMismatchKind) -> Self {
        Self {
            path: Vec::new(),
            reason,
        }
    }

    pub fn incompatible(source: &LuaType, target: &LuaType) -> Self {
        Self::new(TypeMismatchKind::Incompatible {
            source: source.clone(),
            target: target.clone(),
        })
    }

    pub fn at(mut self, segment: TypePathSegment) -> Self {
        self.path.push(segment);
        self
    }

    pub fn path(&self) -> &[TypePathSegment] {
        &self.path
    }

    pub fn reason(&self) -> &TypeMismatchKind {
        &self.reason
    }
}
