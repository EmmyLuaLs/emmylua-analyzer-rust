use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaAstPtr, LuaCallExprStat, LuaChunk, LuaClosureExpr, LuaDocTagAs,
    LuaDocTagCast, LuaExpr, LuaForStat, LuaFuncStat, LuaSyntaxKind, LuaSyntaxNode,
};
use internment::ArcIntern;
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;

use super::super::def::{LuaMemberKey, SemanticId};

/// Effect summary carried on flow nodes (consumed by semantic_model/flow to avoid reparsing the AST during backtracking).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FlowEffect {
    /// Variable assignment: decl <- value expression.
    AssignDecl {
        decl: SemanticId,
        value_syntax: emmylua_parser::LuaSyntaxId,
    },
    /// Member assignment: `owner[key] = value` / `owner.key = value`.
    AssignMember {
        owner: SemanticId,
        key: LuaMemberKey,
        member: SemanticId,
        value_syntax: emmylua_parser::LuaSyntaxId,
    },
    /// Condition guard: the true/false branch for cond.
    Guard {
        cond: LuaAstPtr<LuaExpr>,
        truthy: bool,
    },
    /// `---@cast` (key expression + target type syntax).
    TagCast(LuaAstPtr<LuaDocTagCast>),
    /// `--[[@as T]]` inline assertion (no key expression; directly replaces the expression type).
    AsCast(LuaAstPtr<LuaDocTagAs>),
}

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
    /// Call expression statement
    CallExprStat(LuaAstPtr<LuaCallExprStat>),
    /// Conditional flow (type guards, existence checks)
    TrueCondition(LuaAstPtr<LuaExpr>),
    /// Conditional flow (type guards, existence checks)
    FalseCondition(LuaAstPtr<LuaExpr>),
    /// impl function
    ImplFunc(LuaAstPtr<LuaFuncStat>),
    /// For loop initialization
    ForIStat(LuaAstPtr<LuaForStat>),
    /// Tag cast comment
    TagCast(LuaAstPtr<LuaDocTagCast>),
    /// `--[[@as T]]` inline assertion
    AsCast(LuaAstPtr<LuaDocTagAs>),
    /// Break statement
    Break,
    /// Continue statement
    Continue,
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
        matches!(
            self,
            FlowNodeKind::Break | FlowNodeKind::Return | FlowNodeKind::Continue
        )
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

    #[allow(unused)]
    pub fn get_position(&self) -> TextSize {
        self.0.start()
    }

    #[allow(unused)]
    pub fn get_range(&self) -> TextRange {
        self.0
    }
}
