use std::sync::Arc;

use emmylua_parser::{LuaAssignStat, LuaAst, LuaAstPtr, LuaCallExpr, LuaDocTagCast, LuaExpr, LuaForStat, LuaNameExpr, LuaSyntaxId, LuaVarExpr};
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
    Assignment(LuaAstPtr<LuaAssignStat>),
    /// Conditional flow (type guards, existence checks)
    TrueCondition(LuaAstPtr<LuaExpr>),
    /// Conditional flow (type guards, existence checks)
    FalseCondition(LuaAstPtr<LuaExpr>),
    /// Call expression
    Call(LuaAstPtr<LuaCallExpr>),
    /// Variable reference
    Variable(LuaAstPtr<LuaNameExpr>),
    ForIStat(LuaAstPtr<LuaForStat>),
    TagCast(LuaAstPtr<LuaDocTagCast>),
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

    pub fn is_assignment(&self) -> bool {
        matches!(self, FlowNodeKind::Assignment(_))
    }

    pub fn is_conditional(&self) -> bool {
        matches!(self, FlowNodeKind::TrueCondition(_) | FlowNodeKind::FalseCondition(_))
    }

    pub fn is_unreachable(&self) -> bool {
        matches!(self, FlowNodeKind::Unreachable)
    }
}
