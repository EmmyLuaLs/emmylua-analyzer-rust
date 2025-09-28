use emmylua_parser::LuaExpr;

use crate::{InFiled, LuaDeclId, LuaMemberId, LuaSignatureId};

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum InferFailReason {
    None,
    RecursiveInfer,
    UnResolveExpr(InFiled<LuaExpr>),
    UnResolveSignatureReturn(LuaSignatureId),
    FieldNotFound,
    UnResolveDeclType(LuaDeclId),
    UnResolveMemberType(LuaMemberId),
    UnResolveOperatorCall,
}

impl InferFailReason {
    pub fn is_need_resolve(&self) -> bool {
        matches!(
            self,
            InferFailReason::UnResolveExpr(_)
                | InferFailReason::UnResolveSignatureReturn(_)
                | InferFailReason::FieldNotFound
                | InferFailReason::UnResolveDeclType(_)
                | InferFailReason::UnResolveMemberType(_)
                | InferFailReason::UnResolveOperatorCall
        )
    }
}
