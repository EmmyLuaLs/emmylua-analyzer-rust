use std::sync::Arc;

use emmylua_parser::LuaSyntaxId;
use internment::ArcIntern;
use smol_str::SmolStr;

use crate::{LuaType, LuaVarRefId};

/// Unique identifier for flow nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FlowId(pub u32);

/// Represents how flow nodes are connected
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowAntecedent {
    /// Single predecessor node
    Single(FlowId),
    /// Multiple predecessor nodes (stored externally by index)
    Multiple(u32),
}

/// Main flow node structure containing all flow analysis information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowNode {
    pub id: FlowId,
    pub kind: FlowNodeKind,
    pub antecedent: Option<FlowAntecedent>,
}

/// Different types of flow nodes in the control flow graph
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowNodeKind {
    /// Entry point of the flow
    Start,
    /// Unreachable code
    Unreachable,
    /// Label for branching (if/else, switch cases)
    BranchLabel,
    /// Label for loops (while, for, repeat)
    LoopLabel,
    /// Named label (goto target)
    NamedLabel(ArcIntern<SmolStr>),
    /// Variable assignment
    Assignment(Box<FlowAssignment>),
    /// Conditional flow (type guards, existence checks)
    Assertion(Box<FlowAssertion>),
    /// Break statement
    Break,
    /// Return statement
    Return,
}

#[allow(unused)]
impl FlowNodeKind {
    pub fn is_branch_label(&self) -> bool {
        matches!(self, FlowNodeKind::BranchLabel)
    }

    pub fn is_loop_label(&self) -> bool {
        matches!(self, FlowNodeKind::LoopLabel)
    }

    pub fn is_named_label(&self) -> bool {
        matches!(self, FlowNodeKind::NamedLabel(_))
    }

    pub fn is_change_flow(&self) -> bool {
        matches!(self, FlowNodeKind::Break | FlowNodeKind::Return)
    }
}

/// Represents a variable assignment in the flow
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowAssignment {
    /// The variable being assigned
    pub var_ref_id: LuaVarRefId,
    /// The expression being assigned
    pub expr: LuaSyntaxId,
    pub idx: u32,
}

impl FlowAssignment {
    pub fn new(var_ref_id: LuaVarRefId, expr: LuaSyntaxId, idx: u32) -> Self {
        Self {
            var_ref_id,
            expr,
            idx,
        }
    }
}

/// Represents conditional flow constraints for type analysis
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowAssertion {
    /// Variable exists (not nil)
    Truthy(LuaVarRefId),
    /// Variable is falsy (nil or false)
    Falsy(LuaVarRefId),
    /// Type narrowing (variable is of specific type)
    TypeGuard(LuaVarRefId, LuaType),
    /// Type addition (union with new type)
    TypeAdd(LuaVarRefId, LuaType),
    /// Type removal (subtract from union)
    TypeRemove(LuaVarRefId, LuaType),
    /// Force type (override existing type)
    TypeForce(LuaVarRefId, LuaType),

    TruthyCall(Arc<FlowCall>),
    FalsyCall(Arc<FlowCall>),

    And(Vec<Arc<FlowAssertion>>),

    Or(Vec<Arc<FlowAssertion>>),
}

impl FlowAssertion {
    pub fn get_negation(&self) -> Self {
        match self {
            FlowAssertion::Truthy(var) => FlowAssertion::Falsy(var.clone()),
            FlowAssertion::Falsy(var) => FlowAssertion::Truthy(var.clone()),
            FlowAssertion::TypeGuard(var, ty) => FlowAssertion::TypeRemove(var.clone(), ty.clone()),
            FlowAssertion::TypeAdd(var, ty) => FlowAssertion::TypeRemove(var.clone(), ty.clone()),
            FlowAssertion::TypeRemove(var, ty) => FlowAssertion::TypeGuard(var.clone(), ty.clone()),
            FlowAssertion::TypeForce(var, ty) => FlowAssertion::TypeRemove(var.clone(), ty.clone()),
            FlowAssertion::And(assertions) => {
                FlowAssertion::Or(assertions.iter().map(|a| a.get_negation().into()).collect())
            }
            FlowAssertion::Or(assertions) => {
                FlowAssertion::And(assertions.iter().map(|a| a.get_negation().into()).collect())
            }
            FlowAssertion::TruthyCall(call) => FlowAssertion::FalsyCall(call.clone()),
            FlowAssertion::FalsyCall(call) => FlowAssertion::TruthyCall(call.clone()),
        }
    }
}

/// Represents a function call that may affect control flow
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowCall {
    /// The function being called
    pub call_expr: LuaSyntaxId,
    /// Arguments passed to the function
    pub args: Vec<Option<LuaVarRefId>>,
    /// The "self" variable for method calls
    pub self_var_ref: Option<LuaVarRefId>,
}
