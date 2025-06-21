mod flow_chain;
mod flow_var_ref_id;
mod type_assert;

use std::collections::HashMap;

pub use flow_chain::{LuaClosureId, LuaFlowChain, LuaFlowChainInfo};
pub use flow_var_ref_id::{LuaVarRefId, LuaVarRefNode};
pub use type_assert::TypeAssertion;

use crate::FileId;

use super::{traits::LuaIndex, LuaSignatureId};

#[derive(Debug)]
pub struct LuaFlowIndex {
    chains_map: HashMap<FileId, HashMap<LuaVarRefId, LuaFlowChain>>,
    call_cast: HashMap<FileId, HashMap<LuaSignatureId, HashMap<String, TypeAssertion>>>,
}

impl LuaFlowIndex {
    pub fn new() -> Self {
        Self {
            chains_map: HashMap::new(),
            call_cast: HashMap::new(),
        }
    }

    pub fn add_flow_chain(&mut self, file_id: FileId, chain: LuaFlowChain) {
        self.chains_map
            .entry(file_id)
            .or_insert_with(HashMap::new)
            .insert(chain.get_var_ref_id(), chain);
    }

    pub fn get_flow_chain(
        &self,
        file_id: FileId,
        var_ref_id: LuaVarRefId,
    ) -> Option<&LuaFlowChain> {
        self.chains_map
            .get(&file_id)
            .and_then(|map| map.get(&var_ref_id))
    }

    pub fn add_call_cast(
        &mut self,
        signature_id: LuaSignatureId,
        name: &str,
        assertion: TypeAssertion,
    ) {
        let file_id = signature_id.get_file_id();
        self.call_cast
            .entry(file_id)
            .or_insert_with(HashMap::new)
            .entry(signature_id)
            .or_insert_with(HashMap::new)
            .insert(name.to_string(), assertion);
    }

    pub fn get_call_cast(
        &self,
        signature_id: LuaSignatureId,
    ) -> Option<&HashMap<String, TypeAssertion>> {
        let file_id = signature_id.get_file_id();
        self.call_cast
            .get(&file_id)
            .and_then(|map| map.get(&signature_id))
    }
}

impl LuaIndex for LuaFlowIndex {
    fn remove(&mut self, file_id: crate::FileId) {
        self.chains_map.remove(&file_id);
        self.call_cast.remove(&file_id);
    }

    fn clear(&mut self) {
        self.chains_map.clear();
    }
}
