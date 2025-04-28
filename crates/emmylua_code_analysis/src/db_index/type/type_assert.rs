use std::sync::Arc;

use crate::{infer_expr, DbIndex, InferFailReason, LuaInferCache};
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
    Force(LuaType),
    And(Arc<Vec<TypeAssertion>>),
    Or(Arc<Vec<TypeAssertion>>),
}

#[allow(unused)]
impl TypeAssertion {
    pub fn get_negation(&self) -> Option<TypeAssertion> {
        match self {
            TypeAssertion::Exist => Some(TypeAssertion::NotExist),
            TypeAssertion::NotExist => Some(TypeAssertion::Exist),
            TypeAssertion::Narrow(t) => Some(TypeAssertion::Remove(t.clone())),
            TypeAssertion::Force(t) => Some(TypeAssertion::Remove(t.clone())),
            TypeAssertion::Remove(t) => Some(TypeAssertion::Narrow(t.clone())),
            TypeAssertion::Add(t) => Some(TypeAssertion::Remove(t.clone())),
            TypeAssertion::And(a) => {
                let negations: Vec<_> = a.iter().filter_map(|x| x.get_negation()).collect();
                Some(TypeAssertion::Or(negations.into()))
            }
            TypeAssertion::Or(a) => {
                let negations: Vec<_> = a.iter().filter_map(|x| x.get_negation()).collect();
                Some(TypeAssertion::And(negations.into()))
            }
            _ => None,
        }
    }

    pub fn tighten_type(
        &self,
        db: &DbIndex,
        config: &mut LuaInferCache,
        root: &LuaSyntaxNode,
        source: LuaType,
    ) -> Result<LuaType, InferFailReason> {
        match self {
            TypeAssertion::Exist => Ok(TypeOps::RemoveNilOrFalse.apply_source(db, &source)),
            TypeAssertion::NotExist => Ok(TypeOps::NarrowFalseOrNil.apply_source(db, &source)),
            TypeAssertion::Narrow(t) => Ok(TypeOps::Narrow.apply(db, &source, t)),
            TypeAssertion::Add(lua_type) => Ok(TypeOps::Union.apply(db, &source, lua_type)),
            TypeAssertion::Remove(lua_type) => Ok(TypeOps::Remove.apply(db, &source, lua_type)),
            TypeAssertion::Force(t) => Ok(t.clone()),
            TypeAssertion::Reassign((syntax_id, idx)) => {
                let expr = LuaExpr::cast(
                    syntax_id
                        .to_node_from_root(root)
                        .ok_or(InferFailReason::None)?,
                )
                .ok_or(InferFailReason::None)?;
                let expr_type = infer_expr(db, config, expr)?;
                let expr_type = match &expr_type {
                    LuaType::MuliReturn(multi) => {
                        multi.get_type(*idx as usize).unwrap_or(&LuaType::Nil)
                    }
                    t => t,
                };
                Ok(TypeOps::Narrow.apply(db, &source, &expr_type))
            }
            TypeAssertion::And(a) => {
                let mut result = vec![];
                for assertion in a.iter() {
                    result.push(assertion.tighten_type(db, config, root, source.clone())?);
                }

                match result.len() {
                    0 => Ok(source),
                    1 => Ok(result.remove(0)),
                    _ => {
                        let mut result_type = result.remove(0);
                        for t in result {
                            result_type = TypeOps::And.apply(db, &result_type, &t);
                            if result_type.is_nil() {
                                return Ok(LuaType::Nil);
                            }
                        }

                        Ok(result_type)
                    }
                }
            }
            TypeAssertion::Or(a) => {
                let mut result = vec![];
                for assertion in a.iter() {
                    result.push(assertion.tighten_type(db, config, root, source.clone())?);
                }

                match result.len() {
                    0 => Ok(source),
                    1 => Ok(result.remove(0)),
                    _ => {
                        let mut result_type = result.remove(0);
                        for t in result {
                            result_type = TypeOps::Union.apply(db, &result_type, &t);
                        }

                        Ok(result_type)
                    }
                }
            }
            _ => Ok(source),
        }
    }

    pub fn is_reassign(&self) -> bool {
        matches!(self, TypeAssertion::Reassign(_))
    }

    pub fn is_and(&self) -> bool {
        matches!(self, TypeAssertion::And(_))
    }

    pub fn is_or(&self) -> bool {
        matches!(self, TypeAssertion::Or(_))
    }

    pub fn is_exist(&self) -> bool {
        matches!(self, TypeAssertion::Exist)
    }

    pub fn and_assert(&self, assertion: TypeAssertion) -> TypeAssertion {
        if let TypeAssertion::And(a) = self {
            let mut vecs = a.as_ref().clone();
            vecs.push(assertion);
            TypeAssertion::And(Arc::new(vecs))
        } else {
            TypeAssertion::And(Arc::new(vec![self.clone(), assertion]))
        }
    }

    pub fn or_assert(&self, assertion: TypeAssertion) -> TypeAssertion {
        if let TypeAssertion::Or(a) = self {
            let mut vecs = a.as_ref().clone();
            vecs.push(assertion);
            TypeAssertion::Or(Arc::new(vecs))
        } else {
            TypeAssertion::Or(Arc::new(vec![self.clone(), assertion]))
        }
    }
}
