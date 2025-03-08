#[derive(Debug)]
pub enum TypeCheckFailReason {
    TypeNotMatch,
    TypeRecursion,
    TypeNotMatchWithReason(String),
}
