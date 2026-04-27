use emmylua_parser::LuaCallExpr;

use crate::{DbIndex, LuaCompilation, LuaInferCache, TypeSubstitutor};

#[derive(Debug)]
pub struct TplContext<'a> {
    pub compilation: Option<&'a LuaCompilation>,
    pub(crate) legacy_db: &'a DbIndex,
    pub cache: &'a mut LuaInferCache,
    pub substitutor: &'a mut TypeSubstitutor,
    pub call_expr: Option<LuaCallExpr>,
}

impl<'a> TplContext<'a> {
    pub fn db(&self) -> &'a DbIndex {
        self.compilation
            .map(LuaCompilation::legacy_db)
            .unwrap_or(self.legacy_db)
    }
}
