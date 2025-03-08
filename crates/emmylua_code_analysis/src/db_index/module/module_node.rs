use std::collections::HashMap;

use crate::FileId;

#[derive(Debug, Default)]
pub struct ModuleNode {
    pub parent: Option<ModuleNodeId>,
    pub children: HashMap<String, ModuleNodeId>,
    pub file_ids: Vec<FileId>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub struct ModuleNodeId {
    pub id: u32,
}
