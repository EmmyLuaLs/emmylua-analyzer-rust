use emmylua_parser::LuaCallExpr;

use crate::{DbIndex, LuaCompilation, LuaInferCache, TypeSubstitutor};

#[derive(Debug)]
pub struct TplContext<'a> {
    pub compilation: Option<&'a LuaCompilation>,
    legacy_db: Option<&'a DbIndex>,
    pub cache: &'a mut LuaInferCache,
    pub substitutor: &'a mut TypeSubstitutor,
    pub call_expr: Option<LuaCallExpr>,
}

impl<'a> TplContext<'a> {
    pub fn from_compilation(
        compilation: &'a LuaCompilation,
        cache: &'a mut LuaInferCache,
        substitutor: &'a mut TypeSubstitutor,
        call_expr: Option<LuaCallExpr>,
    ) -> Self {
        Self {
            compilation: Some(compilation),
            legacy_db: None,
            cache,
            substitutor,
            call_expr,
        }
    }

    pub fn from_db(
        db: &'a DbIndex,
        cache: &'a mut LuaInferCache,
        substitutor: &'a mut TypeSubstitutor,
        call_expr: Option<LuaCallExpr>,
    ) -> Self {
        Self {
            compilation: None,
            legacy_db: Some(db),
            cache,
            substitutor,
            call_expr,
        }
    }

    pub fn db(&self) -> &'a DbIndex {
        self.compilation
            .map(LuaCompilation::legacy_db)
            .or(self.legacy_db)
            .expect("TplContext requires either compilation or legacy_db")
    }
}
