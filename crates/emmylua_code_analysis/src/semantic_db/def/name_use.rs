//! Binding-use site definition (name expressions and bound vararg literals).

use emmylua_parser::LuaSyntaxId;
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NameUse {
    /// Name expression or vararg literal (unique identifier).
    pub syntax: LuaSyntaxId,
    pub name: SmolStr,
}
