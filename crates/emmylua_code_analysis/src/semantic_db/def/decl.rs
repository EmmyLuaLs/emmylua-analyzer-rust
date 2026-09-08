use emmylua_parser::LuaSyntaxId;
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;

use crate::FileId;

use super::SemanticId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclKind {
    /// `local x` / `local x <const>` / for-loop variables.
    /// `is_const`: `<const>` attribute; `is_iter`: `for` loop variable (old `LocalAttribute::IterConst`).
    Local {
        is_const: bool,
        is_iter: bool,
    },
    Param,
    Global,
}

impl DeclKind {
    pub fn is_local(&self) -> bool {
        matches!(self, DeclKind::Local { .. } | DeclKind::Param)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Decl {
    /// Globally unique identity (file + name range).
    pub id: SemanticId,
    pub file_id: FileId,
    pub name: SmolStr,
    pub kind: DeclKind,
    /// Name token range.
    pub name_range: TextRange,
    pub scope_id: u32,
    /// Initializer expression (uniquely identified by `LuaSyntaxId`; position alone cannot distinguish parent/child expressions).
    pub value_expr_syntax: Option<LuaSyntaxId>,
    /// Multi-return assignment slot (`payload = 1` for `local ok, payload = f()`).
    pub multi_return_index: Option<usize>,
    /// Name index within the same `local` statement (`b = 1` for `local a, b ---@type A, B`).
    pub doc_type_index: Option<usize>,
    /// Owning statement (uniquely identified by `LuaSyntaxId`; doc-comment ownership key).
    pub owner_syntax: Option<LuaSyntaxId>,
    /// True for the implicit `self` parameter of colon-defined methods.
    /// Its name range points at the colon so unused-self diagnostics can grey out the colon.
    pub is_implicit_self: bool,
    /// `---@type` annotation type node (uniquely identified by `LuaSyntaxId`).
    pub doc_type_syntax: Option<LuaSyntaxId>,
    /// Module name from `---@module "name"` (resolved as ModuleRef type).
    pub module_path: Option<SmolStr>,
    /// `---@deprecated` (same comment block as the declaration).
    pub deprecated: bool,
    /// `---@readonly` (annotation owned by this declaration).
    pub readonly: bool,
    /// `---@[lsp_optimization("delayed_definition")]`: type resolution is deferred to a later assignment.
    pub delayed_definition: bool,
}

impl Decl {
    pub fn name_offset(&self) -> TextSize {
        self.name_range.start()
    }

    pub fn new(file_id: FileId, name: SmolStr, kind: DeclKind, name_range: TextRange) -> Self {
        Decl {
            id: SemanticId::decl(file_id, name_range),
            file_id,
            name,
            kind,
            name_range,
            scope_id: 0,
            value_expr_syntax: None,
            multi_return_index: None,
            doc_type_index: None,
            owner_syntax: None,
            doc_type_syntax: None,
            is_implicit_self: false,
            module_path: None,
            deprecated: false,
            readonly: false,
            delayed_definition: false,
        }
    }
}
