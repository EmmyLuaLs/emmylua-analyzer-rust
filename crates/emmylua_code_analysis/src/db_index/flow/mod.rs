mod flow_node;
mod flow_tree;

use std::collections::HashMap;

use crate::FileId;
pub use flow_node::*;
pub use flow_tree::FlowTree;

use super::traits::LuaIndex;

#[derive(Debug)]
pub struct LuaFlowIndex {
    file_flow_tree: HashMap<FileId, FlowTree>,
}

impl LuaFlowIndex {
    pub fn new() -> Self {
        Self {
            file_flow_tree: HashMap::new(),
        }
    }

    pub fn add_flow_tree(&mut self, file_id: FileId, flow_tree: FlowTree) {
        self.file_flow_tree.insert(file_id, flow_tree);
    }

    pub fn get_flow_tree(&self, file_id: &FileId) -> Option<&FlowTree> {
        self.file_flow_tree.get(file_id)
    }
}

impl LuaIndex for LuaFlowIndex {
    fn remove(&mut self, file_id: FileId) {
        self.file_flow_tree.remove(&file_id);
    }

    fn clear(&mut self) {
        self.file_flow_tree.clear();
    }
}
