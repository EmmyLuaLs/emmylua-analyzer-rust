//! Generic parameters (doc references, collected per file; resolution deferred to the query layer).

use emmylua_parser::LuaSyntaxId;
use smol_str::SmolStr;

/// Generic parameters.
///
/// Type definition: `---@class Foo<T: Base, U = Default>`; function: `---@generic T`.
/// `constraint` / `default` store doc type node references; resolved in the query layer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SalsaGenericParam {
    pub name: SmolStr,
    /// `T: Constraint` constraint type node.
    pub constraint: Option<LuaSyntaxId>,
    /// `T = Default` default type node.
    pub default: Option<LuaSyntaxId>,
    /// `---@generic const T`.
    pub is_const: bool,
    /// `---@generic T...` (variadic generic).
    pub is_variadic: bool,
}

impl SalsaGenericParam {
    pub fn new(
        name: SmolStr,
        constraint: Option<LuaSyntaxId>,
        default: Option<LuaSyntaxId>,
        is_const: bool,
        is_variadic: bool,
    ) -> Self {
        Self {
            name,
            constraint,
            default,
            is_const,
            is_variadic,
        }
    }
}
