use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaAstPtr, LuaChunk, LuaClosureExpr, LuaDocTagCast, LuaExpr,
    LuaForStat, LuaSyntaxKind, LuaSyntaxNode,
};
use internment::ArcIntern;
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;

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
    /// Declaration position
    DeclPosition(TextSize),
    /// Variable assignment
    Assignment(LuaAstPtr<LuaAssignStat>),
    /// Conditional flow (type guards, existence checks)
    TrueCondition(LuaAstPtr<LuaExpr>),
    /// Conditional flow (type guards, existence checks)
    FalseCondition(LuaAstPtr<LuaExpr>),
    /// For loop initialization
    ForIStat(LuaAstPtr<LuaForStat>),
    /// Tag cast comment
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
        matches!(
            self,
            FlowNodeKind::TrueCondition(_) | FlowNodeKind::FalseCondition(_)
        )
    }

    pub fn is_unreachable(&self) -> bool {
        matches!(self, FlowNodeKind::Unreachable)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct LuaClosureId(TextRange);

impl LuaClosureId {
    pub fn from_closure(closure_expr: LuaClosureExpr) -> Self {
        Self(closure_expr.get_range())
    }

    pub fn from_chunk(chunk: LuaChunk) -> Self {
        Self(chunk.get_range())
    }

    pub fn from_node(node: &LuaSyntaxNode) -> Self {
        let flow_id = node.ancestors().find_map(|node| match node.kind().into() {
            LuaSyntaxKind::ClosureExpr => {
                LuaClosureExpr::cast(node).map(LuaClosureId::from_closure)
            }
            LuaSyntaxKind::Chunk => LuaChunk::cast(node).map(LuaClosureId::from_chunk),
            _ => None,
        });

        flow_id.unwrap_or_else(|| LuaClosureId(TextRange::default()))
    }

    pub fn get_position(&self) -> TextSize {
        self.0.start()
    }

    pub fn get_range(&self) -> TextRange {
        self.0
    }
}
