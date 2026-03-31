use hashbrown::HashMap;

use rowan::TextRange;

use crate::{FileId, InFiled};

use super::LuaIndex;

#[derive(Debug)]
pub struct LuaMetatableIndex {
    pub metatables: HashMap<InFiled<TextRange>, InFiled<TextRange>>,
}

impl Default for LuaMetatableIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaMetatableIndex {
    pub fn new() -> Self {
        Self {
            metatables: HashMap::new(),
        }
    }

    pub fn add(&mut self, table: InFiled<TextRange>, metatable: InFiled<TextRange>) {
        self.metatables.insert(table, metatable);
    }

    pub fn get(&self, table: &InFiled<TextRange>) -> Option<&InFiled<TextRange>> {
        self.metatables.get(table)
    }
}

impl LuaIndex for LuaMetatableIndex {
    fn remove(&mut self, file_id: FileId) {
        self.metatables.retain(|key, _| key.file_id != file_id);
    }

    fn clear(&mut self) {
        self.metatables.clear();
    }
}
