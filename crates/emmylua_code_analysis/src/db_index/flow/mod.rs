mod flow_chain;

use std::collections::HashMap;

pub use flow_chain::{LuaFlowChain, LuaFlowId};

use crate::FileId;

use super::traits::LuaIndex;

#[derive(Debug)]
pub struct LuaFlowIndex {
    chains_map: HashMap<FileId, HashMap<LuaFlowId, LuaFlowChain>>,
}

impl LuaFlowIndex {
    pub fn new() -> Self {
        Self {
            chains_map: HashMap::new(),
        }
    }

    pub fn add_flow_chain(&mut self, file_id: FileId, chain: LuaFlowChain) {
        let id = chain.get_flow_id();
        self.chains_map
            .entry(file_id)
            .or_insert_with(HashMap::new)
            .insert(id, chain);
    }

    pub fn get_flow_chain(&self, file_id: FileId, flow_id: LuaFlowId) -> Option<&LuaFlowChain> {
        self.chains_map
            .get(&file_id)
            .and_then(|map| map.get(&flow_id))
    }

    pub fn get_or_create_flow_chain(
        &mut self,
        file_id: FileId,
        flow_id: LuaFlowId,
    ) -> &mut LuaFlowChain {
        self.chains_map
            .entry(file_id)
            .or_insert_with(HashMap::new)
            .entry(flow_id)
            .or_insert_with(|| LuaFlowChain::new(flow_id))
    }
}

impl LuaIndex for LuaFlowIndex {
    fn remove(&mut self, file_id: crate::FileId) {
        self.chains_map.remove(&file_id);
    }

    fn clear(&mut self) {
        self.chains_map.clear();
    }
}
