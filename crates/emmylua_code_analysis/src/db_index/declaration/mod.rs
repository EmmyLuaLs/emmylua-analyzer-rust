mod decl;
mod decl_id;
mod decl_tree;
mod scope;

pub use decl::LuaDeclExtra;
pub use decl::{LocalAttribute, LuaDecl};
pub use decl_id::LuaDeclId;
pub use decl_tree::{LuaDeclOrMemberId, LuaDeclarationTree};
use hashbrown::HashMap;
pub use scope::{LuaScope, LuaScopeId, LuaScopeKind, ScopeOrDeclId};

use crate::FileId;

use super::traits::LuaIndex;

#[derive(Debug)]
pub struct LuaDeclIndex {
    decl_trees: HashMap<FileId, LuaDeclarationTree>,
}

impl Default for LuaDeclIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaDeclIndex {
    pub fn new() -> Self {
        Self {
            decl_trees: HashMap::new(),
        }
    }

    pub fn add_decl_tree(&mut self, tree: LuaDeclarationTree) {
        self.decl_trees.insert(tree.file_id(), tree);
    }

    pub fn get_decl_tree(&self, file_id: &FileId) -> Option<&LuaDeclarationTree> {
        self.decl_trees.get(file_id)
    }

    pub fn get_decl(&self, decl_id: &LuaDeclId) -> Option<&LuaDecl> {
        let tree = self.decl_trees.get(&decl_id.file_id)?;
        tree.get_decl(decl_id)
    }

    pub fn get_decl_mut(&mut self, decl_id: &LuaDeclId) -> Option<&mut LuaDecl> {
        let tree = self.decl_trees.get_mut(&decl_id.file_id)?;
        tree.get_decl_mut(*decl_id)
    }
}

impl LuaIndex for LuaDeclIndex {
    fn remove(&mut self, file_id: FileId) {
        self.decl_trees.remove(&file_id);
    }

    fn clear(&mut self) {
        self.decl_trees.clear();
    }
}
