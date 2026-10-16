//! Type-check failure reasons (detail layer beyond the boolean interface).

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeCheckFailReason {
    TypeNotMatch,
    /// A semantic dependency is unfinished, so neither outcome is proven.
    Unresolved,
    /// This alias's recursive argument transformations are not known finite.
    UnsupportedAlias(crate::LuaTypeDeclId),
    TypeNotMatchWithReason(String),
}

impl TypeCheckFailReason {
    pub(super) fn is_unresolved(&self) -> bool {
        matches!(self, Self::Unresolved | Self::UnsupportedAlias(_))
    }
    pub fn is_type_not_match(&self) -> bool {
        matches!(
            self,
            TypeCheckFailReason::TypeNotMatch | TypeCheckFailReason::TypeNotMatchWithReason(_)
        )
    }
}
