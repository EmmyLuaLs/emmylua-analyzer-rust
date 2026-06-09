use crate::{DbIndex, LuaInferCache, TypeSubstitutor};
use emmylua_parser::LuaCallExpr;

#[derive(Debug)]
pub struct TplContext<'a> {
    pub db: &'a DbIndex,
    pub cache: &'a mut LuaInferCache,
    pub substitutor: &'a mut TypeSubstitutor,
    pub call_expr: Option<LuaCallExpr>,
}
