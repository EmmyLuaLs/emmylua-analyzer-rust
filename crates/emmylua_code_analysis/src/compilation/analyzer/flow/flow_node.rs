use std::sync::Arc;

use emmylua_parser::{LuaAst, LuaExpr};
use internment::ArcIntern;
use smol_str::SmolStr;

use crate::{LuaType, LuaVarRefId};

#[allow(unused)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlowNodeType {
    None,
    Unreachable,
    Start,
    BranchLabel,
    LoopLabel,
    NameLabel(ArcIntern<SmolStr>),
    Assignment(Arc<LuaFlowAssignment>),
    Condition(Arc<LuaFlowCondition>)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaFlowAssignment {
    pub var_ref_id: LuaVarRefId,
    pub expr: LuaExpr,
    pub idx: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuaFlowCondition {
    Exist(LuaVarRefId),
    NotExist(LuaVarRefId),
    Narrow(LuaVarRefId, LuaType),
    Add(LuaVarRefId, LuaType),
    Remove(LuaVarRefId, LuaType),
    Force(LuaVarRefId, LuaType),
    And(Arc<LuaFlowCondition>, Arc<LuaFlowCondition>),
    Or(Arc<LuaFlowCondition>, Arc<LuaFlowCondition>),
    Call(),
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Default)]
pub struct FlowId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum FlowAntecedent {
    Node(FlowId),
    Multiple(u32 /* index in multiple_antecedents*/),
}

#[allow(unused)]
#[derive(Debug)]
pub struct FlowNode {
    pub id: FlowId,
    pub kind: FlowNodeType,
    pub antecedent: Option<FlowAntecedent>,
}
