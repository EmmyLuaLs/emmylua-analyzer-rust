use std::collections::HashSet;

use crate::{DbIndex, LuaCompilation, LuaMemberKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeCheckCheckLevel {
    Normal,
    GenericConditional,
}

#[derive(Debug, Clone)]
pub struct TypeCheckContext<'db> {
    pub compilation: Option<&'db LuaCompilation>,
    pub detail: bool,
    legacy_db: Option<&'db DbIndex>,
    pub level: TypeCheckCheckLevel,
    pub table_member_checked: Option<HashSet<LuaMemberKey>>,
}

impl<'db> TypeCheckContext<'db> {
    pub fn from_compilation(
        compilation: &'db LuaCompilation,
        detail: bool,
        level: TypeCheckCheckLevel,
    ) -> Self {
        Self {
            compilation: Some(compilation),
            detail,
            legacy_db: None,
            level,
            table_member_checked: None,
        }
    }

    pub fn from_db(
        db: &'db DbIndex,
        detail: bool,
        level: TypeCheckCheckLevel,
    ) -> Self {
        Self {
            compilation: None,
            detail,
            legacy_db: Some(db),
            level,
            table_member_checked: None,
        }
    }

    pub fn fresh_branch(&self) -> Self {
        if let Some(compilation) = self.compilation {
            Self::from_compilation(compilation, self.detail, self.level.clone())
        } else {
            Self::from_db(
                self.db(),
                self.detail,
                self.level.clone(),
            )
        }
    }

    pub fn db(&self) -> &'db DbIndex {
        self.compilation
            .map(LuaCompilation::legacy_db)
            .or(self.legacy_db)
            .expect("TypeCheckContext requires either compilation or legacy_db")
    }

    pub fn is_key_checked(&self, key: &LuaMemberKey) -> bool {
        if let Some(checked) = &self.table_member_checked {
            checked.contains(key)
        } else {
            false
        }
    }

    pub fn mark_key_checked(&mut self, key: LuaMemberKey) {
        if self.table_member_checked.is_none() {
            self.table_member_checked = Some(HashSet::new());
        }
        if let Some(checked) = &mut self.table_member_checked {
            checked.insert(key);
        }
    }
}
