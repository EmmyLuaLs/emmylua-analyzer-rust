//! Operator overload definition (`---@operator add(Vector): Vector`).

use emmylua_parser::LuaSyntaxId;
use smol_str::SmolStr;

use super::SemanticId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperatorDef {
    /// Owner type (the `TypeDef` id of `@class`).
    pub owner: SemanticId,
    /// Operator name (`add`/`sub`/`mul`/`div`/`mod`/`pow`/`unm`/`concat`/`len`/`eq`/`lt`/`le`).
    pub name: SmolStr,
    /// Operand type nodes (`(Vector)`).
    pub params: Vec<LuaSyntaxId>,
    /// Return type node.
    pub returns: LuaSyntaxId,
}
