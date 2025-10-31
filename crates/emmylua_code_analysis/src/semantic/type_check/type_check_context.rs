use crate::DbIndex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeCheckCheckLevel {
    Normal,
    GenericConditional,
}

#[derive(Debug, Clone)]
pub struct TypeCheckContext<'db> {
    pub detail: bool,
    pub db: &'db DbIndex,
    pub level: TypeCheckCheckLevel,
}

impl<'db> TypeCheckContext<'db> {
    pub fn new(db: &'db DbIndex, detail: bool, level: TypeCheckCheckLevel) -> Self {
        Self { detail, db, level }
    }
}
