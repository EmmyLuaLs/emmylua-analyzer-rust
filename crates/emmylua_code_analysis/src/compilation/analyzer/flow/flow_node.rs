use emmylua_parser::{LuaCallExpr, LuaExpr};
use internment::ArcIntern;
use smol_str::SmolStr;

use crate::{LuaType, LuaVarRefId};

/// Unique identifier for flow nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FlowId(pub u32);

impl FlowId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
    
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

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

impl FlowNode {
    pub fn new(id: FlowId, kind: FlowNodeKind, antecedent: Option<FlowAntecedent>) -> Self {
        Self { id, kind, antecedent }
    }
    
    pub fn is_reachable(&self) -> bool {
        !matches!(self.kind, FlowNodeKind::Unreachable)
    }
    
    pub fn is_assignment(&self) -> bool {
        matches!(self.kind, FlowNodeKind::Assignment(_))
    }
    
    pub fn is_condition(&self) -> bool {
        matches!(self.kind, FlowNodeKind::Condition(_))
    }
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
    Condition(Box<FlowCondition>),
    /// Break statement
    Break,
    /// Return statement
    Return,
    /// Variable reference (used in expressions)
    VariableRef(LuaVarRefId),
}

/// Represents a variable assignment in the flow
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowAssignment {
    /// The variable being assigned
    pub var_ref_id: LuaVarRefId,
    /// The expression being assigned
    pub expr: LuaExpr,
    pub idx: u32,
}

impl FlowAssignment {
    pub fn new(var_ref_id: LuaVarRefId, expr: LuaExpr, idx: u32) -> Self {
        Self {
            var_ref_id,
            expr,
            idx,
        }
    }
}

/// Represents conditional flow constraints for type analysis
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowCondition {
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
    /// Logical AND of conditions
    And(Box<FlowCondition>, Box<FlowCondition>),
    /// Logical OR of conditions
    Or(Box<FlowCondition>, Box<FlowCondition>),
    /// Negation of condition
    Not(Box<FlowCondition>),
}

impl FlowCondition {
    pub fn and(left: FlowCondition, right: FlowCondition) -> Self {
        Self::And(Box::new(left), Box::new(right))
    }
    
    pub fn or(left: FlowCondition, right: FlowCondition) -> Self {
        Self::Or(Box::new(left), Box::new(right))
    }
    
    pub fn not(condition: FlowCondition) -> Self {
        Self::Not(Box::new(condition))
    }
    
    /// Get all variables referenced in this condition
    pub fn get_referenced_vars(&self) -> Vec<LuaVarRefId> {
        let mut vars = Vec::new();
        self.collect_vars(&mut vars);
        vars
    }
    
    fn collect_vars(&self, vars: &mut Vec<LuaVarRefId>) {
        match self {
            FlowCondition::Truthy(var) 
            | FlowCondition::Falsy(var)
            | FlowCondition::TypeGuard(var, _)
            | FlowCondition::TypeAdd(var, _)
            | FlowCondition::TypeRemove(var, _)
            | FlowCondition::TypeForce(var, _) => {
                vars.push(var.clone());
            }
            FlowCondition::And(left, right) | FlowCondition::Or(left, right) => {
                left.collect_vars(vars);
                right.collect_vars(vars);
            }
            FlowCondition::Not(inner) => {
                inner.collect_vars(vars);
            }
        }
    }
}

/// Represents a function call that may affect control flow
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowCall {
    /// The function being called
    pub call_expr: LuaCallExpr,
    /// Arguments passed to the function
    pub args: Vec<Option<LuaVarRefId>>,
    /// The "self" variable for method calls
    pub self_var_ref: Option<LuaVarRefId>,
}
