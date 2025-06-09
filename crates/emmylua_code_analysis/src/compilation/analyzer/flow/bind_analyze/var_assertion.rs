use emmylua_parser::LuaExpr;

use crate::{LuaType, LuaVarRefId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarAssertion {
    pub var_ref_id: LuaVarRefId,
    pub assertion: String,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum TypeAssertion {
    Exist,
    NotExist,
    Narrow(LuaType),
    Add(LuaType),
    Remove(LuaType),
    Assign { expr: LuaExpr, idx: i32 },
    Force(LuaType)
}
