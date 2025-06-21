use emmylua_parser::{LuaAstNode, LuaDocTagCast, LuaSyntaxId, LuaVarExpr};
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;

use crate::{InFiled, LuaDeclId};

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum LuaVarRefId {
    DeclId(LuaDeclId),
    Name(SmolStr),
    SyntaxId(InFiled<LuaSyntaxId>),
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum LuaVarRefNode {
    UseRef(LuaVarExpr),
    AssignRef(LuaVarExpr),
    CastRef(LuaDocTagCast),
}

#[allow(unused)]
impl LuaVarRefNode {
    pub fn get_range(&self) -> TextRange {
        match self {
            LuaVarRefNode::UseRef(id) => id.get_range(),
            LuaVarRefNode::AssignRef(id) => id.get_range(),
            LuaVarRefNode::CastRef(id) => id.get_range(),
        }
    }

    pub fn get_position(&self) -> TextSize {
        self.get_range().start()
    }

    pub fn is_use_ref(&self) -> bool {
        matches!(self, LuaVarRefNode::UseRef(_))
    }

    pub fn is_assign_ref(&self) -> bool {
        matches!(self, LuaVarRefNode::AssignRef(_))
    }

    pub fn is_cast_ref(&self) -> bool {
        matches!(self, LuaVarRefNode::CastRef(_))
    }
}
