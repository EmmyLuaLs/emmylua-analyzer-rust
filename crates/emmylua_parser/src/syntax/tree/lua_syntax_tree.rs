use rowan::GreenNode;

use crate::{
    LuaSyntaxNode,
    parser_error::{LuaParseError, LuaParseErrorKind},
    syntax::{node::LuaChunk, traits::LuaAstNode},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaSyntaxTree {
    // store GreenNode instead of SyntaxNode, because SyntaxNode is not send and sync
    root: GreenNode,
    errors: Vec<LuaParseError>,
}

impl LuaSyntaxTree {
    pub fn new(root: GreenNode, errors: Vec<LuaParseError>) -> Self {
        LuaSyntaxTree { root, errors }
    }

    // get root node
    pub fn get_red_root(&self) -> LuaSyntaxNode {
        LuaSyntaxNode::new_root(self.root.clone())
    }

    // get chunk node, only can cast to LuaChunk
    pub fn get_chunk_node(&self) -> LuaChunk {
        LuaChunk::cast(self.get_red_root()).unwrap()
    }

    pub fn get_errors(&self) -> &[LuaParseError] {
        &self.errors
    }

    pub fn has_syntax_errors(&self) -> bool {
        self.errors
            .iter()
            .any(|e| e.kind == LuaParseErrorKind::SyntaxError)
    }
}
