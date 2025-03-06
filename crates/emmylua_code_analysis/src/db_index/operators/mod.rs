mod lua_operator;
mod lua_operator_meta_method;

use std::collections::HashMap;

use crate::FileId;

use super::{traits::LuaIndex, LuaTypeDeclId};
pub use lua_operator::{LuaOperator, LuaOperatorId};
pub use lua_operator_meta_method::LuaOperatorMetaMethod;

#[derive(Debug)]
pub struct LuaOperatorIndex {
    operators: HashMap<LuaOperatorId, LuaOperator>,
    type_operators_map: HashMap<LuaTypeDeclId, HashMap<LuaOperatorMetaMethod, Vec<LuaOperatorId>>>,
    in_filed_operator_map: HashMap<FileId, Vec<LuaOperatorId>>,
}

impl LuaOperatorIndex {
    pub fn new() -> Self {
        Self {
            operators: HashMap::new(),
            type_operators_map: HashMap::new(),
            in_filed_operator_map: HashMap::new(),
        }
    }

    pub fn add_operator(&mut self, operator: LuaOperator) {
        let id = operator.get_id();
        let owner = operator.get_owner().clone();
        let op = operator.get_op();
        self.operators.insert(id, operator);
        self.type_operators_map
            .entry(owner)
            .or_insert_with(HashMap::new)
            .entry(op)
            .or_insert_with(Vec::new)
            .push(id);
        self.in_filed_operator_map
            .entry(id.file_id)
            .or_insert_with(Vec::new)
            .push(id);
    }

    pub fn get_operators_by_type(
        &self,
        type_id: &LuaTypeDeclId,
    ) -> Option<&HashMap<LuaOperatorMetaMethod, Vec<LuaOperatorId>>> {
        self.type_operators_map.get(type_id)
    }

    pub fn get_operator(&self, id: &LuaOperatorId) -> Option<&LuaOperator> {
        self.operators.get(id)
    }
}

impl LuaIndex for LuaOperatorIndex {
    fn remove(&mut self, file_id: FileId) {
        if let Some(operator_ids) = self.in_filed_operator_map.remove(&file_id) {
            for id in operator_ids {
                let operator = self.operators.remove(&id).unwrap();
                let owner = operator.get_owner();
                let op = operator.get_op();
                let operators_map = self.type_operators_map.get_mut(owner).unwrap();
                let operators = operators_map.get_mut(&op).unwrap();
                operators.retain(|x| x != &id);
                if operators.is_empty() {
                    operators_map.remove(&op);
                }

                if operators_map.is_empty() {
                    self.type_operators_map.remove(owner);
                }
            }
        }
    }

    fn clear(&mut self) {
        self.operators.clear();
        self.type_operators_map.clear();
        self.in_filed_operator_map.clear();
    }
}
