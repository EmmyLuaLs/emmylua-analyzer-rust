//! Name-use site definition (`LuaNameExpr`).

use emmylua_parser::LuaSyntaxId;
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NameUse {
    /// Name expression (unique identifier).
    pub syntax: LuaSyntaxId,
    pub name: SmolStr,
}
