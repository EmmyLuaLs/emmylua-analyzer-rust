use std::{collections::HashMap, sync::Arc};

use rowan::TextRange;

use crate::{
    compilation::analyzer::flow::flow_tree::{FlowTree, VarRefNode},
    LuaFlowChain, LuaFlowChainInfo, LuaFlowId, TypeAssertion, VarRefId,
};

use super::{
    unresolve_trace::{UnResolveTraceId, UnResolveTraceInfo},
    var_trace_info::VarTraceInfo,
};

#[derive(Debug, Clone)]
pub struct VarTrace<'a> {
    var_ref_id: VarRefId,
    var_refs: Vec<(VarRefNode, LuaFlowId)>,
    assertions: Vec<LuaFlowChainInfo>,
    current_flow_id: Option<LuaFlowId>,
    unresolve_traces: HashMap<UnResolveTraceId, (LuaFlowId, UnResolveTraceInfo)>,
    flow_tree: &'a FlowTree,
}

#[allow(unused)]
impl<'a> VarTrace<'a> {
    pub fn new(
        var_ref_id: VarRefId,
        var_refs: Vec<(VarRefNode, LuaFlowId)>,
        flow_tree: &'a FlowTree,
    ) -> Self {
        Self {
            var_ref_id,
            var_refs,
            assertions: Vec::new(),
            current_flow_id: None,
            unresolve_traces: HashMap::new(),
            flow_tree,
        }
    }

    pub fn set_current_flow_id(&mut self, flow_id: LuaFlowId) {
        self.current_flow_id = Some(flow_id);
    }

    pub fn get_current_flow_id(&self) -> Option<LuaFlowId> {
        self.current_flow_id.clone()
    }

    pub fn get_var_ref_id(&self) -> &VarRefId {
        &self.var_ref_id
    }

    pub fn add_assert(&mut self, assertion: TypeAssertion, effect_range: TextRange) -> Option<()> {
        let current_flow_id = self.current_flow_id?;
        let mut assert_info = LuaFlowChainInfo {
            range: effect_range,
            type_assert: assertion.clone(),
            allow_flow_id: vec![current_flow_id],
        };

        if let Some(flow_node) = self.flow_tree.get_flow_node(current_flow_id) {
            let children = flow_node.get_children();
            for child in children {
                let range = child.get_range();
                let mut allow_add_flow_id = true;
                for (var_ref, flow_id) in &self.var_refs {
                    if flow_id == &current_flow_id
                        && var_ref.is_assign_ref()
                        && var_ref.get_position() > range.end()
                    {
                        allow_add_flow_id = false;
                        break;
                    }
                }
                if allow_add_flow_id {
                    assert_info.allow_flow_id.push(*child);
                }
            }
        }

        self.assertions.push(assert_info);
        Some(())
    }

    pub fn add_unresolve_trace(
        &mut self,
        trace_id: UnResolveTraceId,
        trace_info: Arc<VarTraceInfo>,
    ) {
        if let Some(old_info) = self.unresolve_traces.get_mut(&trace_id) {
            old_info.1.add_trace_info(trace_info);
        } else {
            let trace_info = UnResolveTraceInfo::Trace(trace_info);
            self.unresolve_traces.insert(trace_id, (self.current_flow_id.unwrap(), trace_info));
        }
    }

    pub fn check_var_use_in_range(&self, range: TextRange) -> bool {
        for (node, _) in &self.var_refs {
            if node.is_use_ref() && range.contains(node.get_position()) {
                return true;
            }
        }

        false
    }

    pub fn pop_unresolve_trace(
        &mut self,
        trace_id: &UnResolveTraceId,
    ) -> Option<(LuaFlowId, UnResolveTraceInfo)> {
        self.unresolve_traces.remove(trace_id)
    }

    pub fn pop_all_unresolve_traces(&mut self) -> HashMap<UnResolveTraceId, (LuaFlowId, UnResolveTraceInfo)> {
        std::mem::take(&mut self.unresolve_traces)
    }

    pub fn has_unresolve_traces(&self) -> bool {
        !self.unresolve_traces.is_empty()
    }

    pub fn finish(self) -> LuaFlowChain {
        let mut asserts = self.assertions;
        asserts.sort_by(|a, b| a.range.start().cmp(&b.range.start()));

        LuaFlowChain::new(self.var_ref_id, asserts)
    }
}
