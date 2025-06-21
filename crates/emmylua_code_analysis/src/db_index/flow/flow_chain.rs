use emmylua_parser::{LuaAstNode, LuaChunk, LuaClosureExpr, LuaSyntaxKind, LuaSyntaxNode};
use rowan::{TextRange, TextSize};

use super::{type_assert::TypeAssertion, LuaVarRefId};

#[derive(Debug)]
pub struct LuaFlowChain {
    var_ref_id: LuaVarRefId,
    type_asserts: Vec<LuaFlowChainInfo>,
}

#[derive(Debug, Clone)]
pub struct LuaFlowChainInfo {
    pub range: TextRange,
    pub type_assert: TypeAssertion,
    pub allow_flow_id: Vec<LuaFlowId>,
}

impl LuaFlowChain {
    pub fn new(var_ref_id: LuaVarRefId, asserts: Vec<LuaFlowChainInfo>) -> Self {
        Self {
            var_ref_id,
            type_asserts: asserts,
        }
    }

    pub fn get_var_ref_id(&self) -> LuaVarRefId {
        self.var_ref_id.clone()
    }

    pub fn get_type_asserts(
        &self,
        position: TextSize,
        flow_id: LuaFlowId,
    ) -> impl Iterator<Item = &TypeAssertion> {
        self.type_asserts
            .iter()
            .filter(move |assert| {
                assert.allow_flow_id.contains(&flow_id) && assert.range.contains(position)
            })
            .map(|assert| &assert.type_assert)
    }

    pub fn get_all_type_asserts(&self) -> impl Iterator<Item = &TypeAssertion> {
        self.type_asserts.iter().map(|assert| &assert.type_assert)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct LuaFlowId(TextRange);

impl LuaFlowId {
    pub fn from_closure(closure_expr: LuaClosureExpr) -> Self {
        Self(closure_expr.get_range())
    }

    pub fn from_chunk(chunk: LuaChunk) -> Self {
        Self(chunk.get_range())
    }

    pub fn from_node(node: &LuaSyntaxNode) -> Self {
        let flow_id = node.ancestors().find_map(|node| match node.kind().into() {
            LuaSyntaxKind::ClosureExpr => LuaClosureExpr::cast(node).map(LuaFlowId::from_closure),
            LuaSyntaxKind::Chunk => LuaChunk::cast(node).map(LuaFlowId::from_chunk),
            _ => None,
        });

        flow_id.unwrap_or_else(|| LuaFlowId(TextRange::default()))
    }

    pub fn get_position(&self) -> TextSize {
        self.0.start()
    }

    pub fn get_range(&self) -> TextRange {
        self.0
    }
}
