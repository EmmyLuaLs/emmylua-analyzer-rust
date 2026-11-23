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

/// Operator names accepted in `---@operator` tags.
///
/// These are the operator names defined by LuaCATS: arithmetic, bitwise,
/// comparison and concatenation operators, plus `call`. Bitwise operators are
/// accepted even though the analyzer does not evaluate their overloads yet, so
/// that valid annotations are not reported as unknown. Metamethod names such as
/// `__tostring` are not operators and are documented with `@field` instead.
const OPERATOR_NAMES: &[&str] = &[
    "add", "sub", "mul", "div", "mod", "pow", "unm", "idiv", "band", "bor", "bxor", "bnot", "shl",
    "shr", "concat", "len", "eq", "lt", "le", "call",
];

/// Whether `name` is an accepted `---@operator` name.
pub(crate) fn is_operator_name(name: &str) -> bool {
    OPERATOR_NAMES.contains(&name)
}
