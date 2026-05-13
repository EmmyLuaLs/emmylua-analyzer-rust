use emmylua_parser::LuaCallExpr;

use super::instantiate_type::TplCandidateSource;
use crate::{
    DbIndex, GenericTplId, LuaInferCache, LuaType, TypeSubstitutor,
    semantic::generic::type_substitutor::TplBinding,
};

#[derive(Debug)]
pub struct TplContext<'a> {
    pub db: &'a DbIndex,
    pub cache: &'a mut LuaInferCache,
    pub substitutor: &'a mut TypeSubstitutor,
    pub call_expr: Option<LuaCallExpr>,
    inference_top_level: bool,
}

impl<'a> TplContext<'a> {
    pub fn new(
        db: &'a DbIndex,
        cache: &'a mut LuaInferCache,
        substitutor: &'a mut TypeSubstitutor,
        call_expr: Option<LuaCallExpr>,
    ) -> Self {
        Self {
            db,
            cache,
            substitutor,
            call_expr,
            inference_top_level: true,
        }
    }

    pub fn with_inference_top_level<R>(
        &mut self,
        top_level: bool,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let previous = self.inference_top_level;
        self.inference_top_level = previous && top_level;
        let result = f(self);
        self.inference_top_level = previous;
        result
    }

    pub(in crate::semantic::generic) fn insert_type(
        &mut self,
        tpl_id: GenericTplId,
        replace_type: LuaType,
        source: TplCandidateSource,
    ) {
        self.substitutor.bind(
            tpl_id,
            TplBinding::InferredType {
                ty: replace_type,
                source,
                top_level: self.inference_top_level,
            },
        );
    }

    pub(in crate::semantic::generic) fn insert_multi_types(
        &mut self,
        tpl_id: GenericTplId,
        types: Vec<LuaType>,
        source: TplCandidateSource,
    ) {
        self.substitutor.bind(
            tpl_id,
            TplBinding::InferredMultiTypes {
                types,
                source,
                top_level: self.inference_top_level,
            },
        );
    }
}
