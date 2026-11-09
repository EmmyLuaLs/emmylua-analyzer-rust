//! Module export definition (top-level `return X` export target).

use emmylua_parser::LuaSyntaxId;
use smol_str::SmolStr;

use super::SemanticId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModuleExport {
    /// Returns a declaration (`return M`).
    Decl { decl: SemanticId, name: SmolStr },
    /// Returns a global name.
    Global { name: SmolStr },
    /// Returns a table literal or another expression.
    Expr { value_syntax: LuaSyntaxId },
    /// No explicit top-level `return`.
    None,
}
