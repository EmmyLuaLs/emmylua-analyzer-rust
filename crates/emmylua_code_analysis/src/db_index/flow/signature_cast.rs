use emmylua_parser::{LuaAstPtr, LuaDocOpType};

#[derive(Debug, Clone)]
pub struct LuaSignatureCast {
    pub name: String,
    pub cast: LuaAstPtr<LuaDocOpType>,
    pub fallback_cast: Option<LuaAstPtr<LuaDocOpType>>,
}
