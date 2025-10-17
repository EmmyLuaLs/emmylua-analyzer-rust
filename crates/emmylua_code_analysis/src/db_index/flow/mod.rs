mod flow_node;
mod flow_tree;
mod signature_cast;

use std::collections::HashMap;

use crate::{FileId, LuaSignatureId};
use emmylua_parser::{LuaAstPtr, LuaDocOpType};
pub use flow_node::*;
pub use flow_tree::FlowTree;
pub use signature_cast::LuaSignatureCast;

use super::traits::LuaIndex;

#[derive(Debug)]
pub struct LuaFlowIndex {
    file_flow_tree: HashMap<FileId, FlowTree>,
    signature_cast_cache: HashMap<FileId, HashMap<LuaSignatureId, LuaSignatureCast>>,
}

impl Default for LuaFlowIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaFlowIndex {
    pub fn new() -> Self {
        Self {
            file_flow_tree: HashMap::new(),
            signature_cast_cache: HashMap::new(),
        }
    }

    pub fn add_flow_tree(&mut self, file_id: FileId, flow_tree: FlowTree) {
        self.file_flow_tree.insert(file_id, flow_tree);
    }

    pub fn get_flow_tree(&self, file_id: &FileId) -> Option<&FlowTree> {
        self.file_flow_tree.get(file_id)
    }

    pub fn get_signature_cast(&self, signature_id: &LuaSignatureId) -> Option<&LuaSignatureCast> {
        self.signature_cast_cache
            .get(&signature_id.get_file_id())?
            .get(signature_id)
    }

    pub fn add_signature_cast(
        &mut self,
        file_id: FileId,
        signature_id: LuaSignatureId,
        name: String,
        cast: LuaAstPtr<LuaDocOpType>,
        fallback_cast: Option<LuaAstPtr<LuaDocOpType>>,
    ) {
        self.signature_cast_cache
            .entry(file_id)
            .or_default()
            .insert(
                signature_id,
                LuaSignatureCast {
                    name,
                    cast,
                    fallback_cast,
                },
            );
    }
}

impl LuaIndex for LuaFlowIndex {
    fn remove(&mut self, file_id: FileId) {
        self.file_flow_tree.remove(&file_id);
        self.signature_cast_cache.remove(&file_id);
    }

    fn clear(&mut self) {
        self.file_flow_tree.clear();
        self.signature_cast_cache.clear();
    }
}
