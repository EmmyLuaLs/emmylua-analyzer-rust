use emmylua_parser::LuaExpr;

use crate::{FileId, InFiled, LuaDeclId, LuaMemberId, LuaSignatureId, LuaTypeDeclId};

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum InferFailReason {
    None,
    RecursiveInfer,
    UnResolveExpr(InFiled<LuaExpr>),
    UnResolveSignatureReturn(LuaSignatureId),
    FieldNotFound,
    UnResolveDeclType(LuaDeclId),
    UnResolveTypeDecl(LuaTypeDeclId),
    UnResolveMemberType(LuaMemberId),
    UnResolveOperatorCall,
    UnResolveModuleExport(FileId),
}

impl InferFailReason {
    pub fn is_need_resolve(&self) -> bool {
        matches!(
            self,
            InferFailReason::UnResolveExpr(_)
                | InferFailReason::UnResolveSignatureReturn(_)
                | InferFailReason::FieldNotFound
                | InferFailReason::UnResolveDeclType(_)
                | InferFailReason::UnResolveTypeDecl(_)
                | InferFailReason::UnResolveMemberType(_)
                | InferFailReason::UnResolveOperatorCall
                | InferFailReason::UnResolveModuleExport(_)
        )
    }
}
