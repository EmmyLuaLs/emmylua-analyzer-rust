//! Type-check failure reasons (detail layer beyond the boolean interface).

#[derive(Debug)]
pub enum TypeCheckFailReason {
    TypeNotMatch,
    TypeRecursion,
    TypeNotMatchWithReason(String),
}

impl TypeCheckFailReason {
    pub fn is_type_not_match(&self) -> bool {
        matches!(
            self,
            TypeCheckFailReason::TypeNotMatch | TypeCheckFailReason::TypeNotMatchWithReason(_)
        )
    }
}
