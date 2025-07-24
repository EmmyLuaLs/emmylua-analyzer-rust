use emmylua_parser::LuaSyntaxNode;

use crate::{DbIndex, LuaInferCache, TypeSubstitutor};

#[derive(Debug)]
pub struct TplContext<'a> {
    pub db: &'a DbIndex,
    pub cache: &'a mut LuaInferCache,
    pub substitutor: &'a mut TypeSubstitutor,
    pub root: LuaSyntaxNode,
}
