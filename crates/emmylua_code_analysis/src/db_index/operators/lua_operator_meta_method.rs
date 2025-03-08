#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy)]
pub enum LuaOperatorMetaMethod {
    Add,    // +
    Sub,    // -
    Mul,    // *
    Div,    // /
    Mod,    // %
    Pow,    // ^
    Unm,    // -
    IDiv,   // //
    BAnd,   // &
    BOr,    // |
    BXor,   // ~
    BNot,   // ~
    Shl,    // <<
    Shr,    // >>
    Concat, // ..
    Len,    // #
    Eq,     // ==
    Lt,     // <
    Le,     // <=
    Index,  // __index
    Call,   // __call
}

impl LuaOperatorMetaMethod {
    pub fn from_str(op: &str) -> Option<Self> {
        match op {
            "add" => Some(LuaOperatorMetaMethod::Add),
            "sub" => Some(LuaOperatorMetaMethod::Sub),
            "mul" => Some(LuaOperatorMetaMethod::Mul),
            "div" => Some(LuaOperatorMetaMethod::Div),
            "mod" => Some(LuaOperatorMetaMethod::Mod),
            "pow" => Some(LuaOperatorMetaMethod::Pow),
            "unm" => Some(LuaOperatorMetaMethod::Unm),
            "idiv" => Some(LuaOperatorMetaMethod::IDiv),
            "band" => Some(LuaOperatorMetaMethod::BAnd),
            "bor" => Some(LuaOperatorMetaMethod::BOr),
            "bxor" => Some(LuaOperatorMetaMethod::BXor),
            "bnot" => Some(LuaOperatorMetaMethod::BNot),
            "shl" => Some(LuaOperatorMetaMethod::Shl),
            "shr" => Some(LuaOperatorMetaMethod::Shr),
            "concat" => Some(LuaOperatorMetaMethod::Concat),
            "len" => Some(LuaOperatorMetaMethod::Len),
            "eq" => Some(LuaOperatorMetaMethod::Eq),
            "lt" => Some(LuaOperatorMetaMethod::Lt),
            "le" => Some(LuaOperatorMetaMethod::Le),
            "call" => Some(LuaOperatorMetaMethod::Call),
            _ => None,
        }
    }
}
