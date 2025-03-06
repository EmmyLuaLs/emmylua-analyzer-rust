use std::collections::HashMap;

use emmylua_parser::{LuaIndexExpr, LuaNameExpr};
use smol_str::SmolStr;

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum FlowRef {
    NameExpr(LuaNameExpr),
    IndexExpr(LuaIndexExpr),
}

#[allow(unused)]
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum FlowNode {
    UseRef(FlowRef),
    AssignRef(FlowRef),
}

#[derive(Debug)]
pub struct FlowNodes {
    var_flow_ref: HashMap<SmolStr, Vec<FlowNode>>,
}

#[allow(unused)]
impl FlowNodes {
    pub fn new() -> FlowNodes {
        FlowNodes {
            var_flow_ref: HashMap::new(),
        }
    }

    pub fn add_var_ref(&mut self, name: &str, node: FlowNode) {
        self.var_flow_ref
            .entry(SmolStr::new(name))
            .or_insert_with(Vec::new)
            .push(node);
    }

    pub fn get_var_flow_nodes(&self) -> Vec<(&str, &Vec<FlowNode>)> {
        self.var_flow_ref
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    }
}
