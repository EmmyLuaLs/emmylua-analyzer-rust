use crate::{infer_expr, DbIndex, LuaInferConfig};
use emmylua_parser::{LuaAstNode, LuaExpr, LuaSyntaxId, LuaSyntaxNode};

use super::{type_ops::TypeOps, LuaType};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum TypeAssertion {
    Exist,
    NotExist,
    Narrow(LuaType),
    Add(LuaType),
    Remove(LuaType),
    Reassign((LuaSyntaxId, i32)),
}

#[allow(unused)]
impl TypeAssertion {
    pub fn get_negation(&self) -> Option<TypeAssertion> {
        match self {
            TypeAssertion::Exist => Some(TypeAssertion::NotExist),
            TypeAssertion::NotExist => Some(TypeAssertion::Exist),
            TypeAssertion::Narrow(t) => Some(TypeAssertion::Remove(t.clone())),
            _ => None,
        }
    }

    pub fn tighten_type(
        &self,
        db: &DbIndex,
        config: &mut LuaInferConfig,
        root: &LuaSyntaxNode,
        source: LuaType,
    ) -> Option<LuaType> {
        match self {
            TypeAssertion::Exist => Some(TypeOps::Remove.apply(&source, &LuaType::Nil)),
            TypeAssertion::NotExist => Some(force_nil_or_false(source)),
            TypeAssertion::Narrow(t) => Some(TypeOps::Narrow.apply(&source, t)),
            TypeAssertion::Add(lua_type) => Some(TypeOps::Union.apply(&source, lua_type)),
            TypeAssertion::Remove(lua_type) => Some(TypeOps::Remove.apply(&source, lua_type)),
            TypeAssertion::Reassign((syntax_id, idx)) => {
                let expr = LuaExpr::cast(syntax_id.to_node_from_root(root)?)?;
                let expr_type = infer_expr(db, config, expr)?;
                let expr_type = match &expr_type {
                    LuaType::MuliReturn(multi) => {
                        multi.get_type(*idx as usize).unwrap_or(&LuaType::Nil)
                    }
                    t => t,
                };
                Some(TypeOps::Narrow.apply(&source, &expr_type))
            }
            _ => Some(source),
        }
    }
}

fn force_nil_or_false(t: LuaType) -> LuaType {
    if t.is_boolean() {
        return LuaType::BooleanConst(false);
    }

    return TypeOps::Narrow.apply(&t, &LuaType::Nil);
}
