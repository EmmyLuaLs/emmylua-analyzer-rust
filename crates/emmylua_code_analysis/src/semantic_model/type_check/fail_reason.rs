//! Type-check failure reasons (detail layer beyond the boolean interface).

use crate::{InFiled, LuaType};

/// The constructor value that failed compatibility, without changing the check result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldMismatch {
    pub location: InFiled<rowan::TextRange>,
    pub source: LuaType,
    pub target: LuaType,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeCheckFailReason {
    TypeNotMatch,
    /// A semantic dependency is unfinished, so neither outcome is proven.
    Unresolved,
    /// Evaluation exceeded the shared resource budget.
    TooComplex,
    TypeNotMatchWithReason(String),
    /// Keep the innermost field failure so recursive comparisons have stable details.
    TypeNotMatchAtField(Box<FieldMismatch>),
}

impl TypeCheckFailReason {
    pub(super) fn is_unresolved(&self) -> bool {
        matches!(self, Self::Unresolved | Self::TooComplex)
    }
    pub fn is_type_not_match(&self) -> bool {
        matches!(
            self,
            TypeCheckFailReason::TypeNotMatch
                | TypeCheckFailReason::TypeNotMatchWithReason(_)
                | TypeCheckFailReason::TypeNotMatchAtField(_)
        )
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::TypeNotMatchWithReason(reason) => Some(reason),
            Self::TypeNotMatchAtField(field) => Some(&field.reason),
            _ => None,
        }
    }
}
