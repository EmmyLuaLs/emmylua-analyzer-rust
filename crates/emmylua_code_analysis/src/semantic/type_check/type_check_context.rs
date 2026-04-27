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
    legacy_db: &'db DbIndex,
    pub level: TypeCheckCheckLevel,
    pub table_member_checked: Option<HashSet<LuaMemberKey>>,
}

impl<'db> TypeCheckContext<'db> {
    pub fn new(
        compilation: Option<&'db LuaCompilation>,
        db: &'db DbIndex,
        detail: bool,
        level: TypeCheckCheckLevel,
    ) -> Self {
        Self {
            compilation,
            detail,
            legacy_db: db,
            level,
            table_member_checked: None,
        }
    }

    pub fn db(&self) -> &'db DbIndex {
        self.compilation
            .map(LuaCompilation::legacy_db)
            .unwrap_or(self.legacy_db)
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
