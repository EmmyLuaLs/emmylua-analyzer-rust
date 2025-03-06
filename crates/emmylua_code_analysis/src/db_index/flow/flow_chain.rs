use std::collections::HashMap;

use emmylua_parser::{LuaAstNode, LuaClosureExpr, LuaSyntaxKind, LuaSyntaxNode};
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;

use crate::db_index::TypeAssertion;

#[derive(Debug)]
pub struct LuaFlowChain {
    flow_id: LuaFlowId,
    type_asserts: HashMap<SmolStr, Vec<(TypeAssertion, TextRange)>>,
}

impl LuaFlowChain {
    pub fn new(flow_id: LuaFlowId) -> Self {
        Self {
            flow_id,
            type_asserts: HashMap::new(),
        }
    }

    pub fn get_flow_id(&self) -> LuaFlowId {
        self.flow_id
    }

    pub fn add_type_assert(&mut self, path: &str, type_assert: TypeAssertion, range: TextRange) {
        self.type_asserts
            .entry(SmolStr::new(path))
            .or_insert_with(Vec::new)
            .push((type_assert, range));
    }

    pub fn get_type_asserts(
        &self,
        path: &str,
        position: TextSize,
    ) -> impl Iterator<Item = &TypeAssertion> {
        self.type_asserts
            .get(path)
            .into_iter()
            .flat_map(move |asserts| {
                asserts.iter().filter_map(move |(assert, range)| {
                    if range.contains(position) {
                        Some(assert)
                    } else {
                        None
                    }
                })
            })
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct LuaFlowId(TextSize);

impl LuaFlowId {
    pub fn from_closure(closure_expr: LuaClosureExpr) -> Self {
        Self(closure_expr.get_position())
    }

    pub fn chunk() -> Self {
        Self(TextSize::from(0))
    }

    pub fn from_node(node: &LuaSyntaxNode) -> Self {
        let flow_id = node.ancestors().find_map(|node| match node.kind().into() {
            LuaSyntaxKind::ClosureExpr => LuaClosureExpr::cast(node).map(LuaFlowId::from_closure),
            _ => None,
        });

        flow_id.unwrap_or_else(|| LuaFlowId::chunk())
    }
}
