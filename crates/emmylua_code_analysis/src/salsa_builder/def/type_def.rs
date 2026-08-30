//! Named type definitions (`---@class` / `@alias` / `@enum`).

use emmylua_parser::LuaSyntaxId;
use rowan::TextRange;
use smol_str::SmolStr;

use crate::{FileId, salsa_builder::def::WorkspaceId};

use super::{SalsaGenericParam, SemanticId};

/// Type scope (corresponding to old `LuaTypeIdentifier`: Global / Internal / File).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeScope {
    Global,
    Internal(WorkspaceId),
    File(FileId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeVisibility {
    Public,
    Internal,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeDefKind {
    Class,
    Alias,
    Enum,
}

/// Type definition flags (mirroring old `LuaTypeFlag::{Partial, Constructor, Meta}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TypeDefFlags {
    /// `---@class Foo(partial)`.
    pub partial: bool,
    /// `---@class Foo(constructor)`.
    pub constructor: bool,
    /// File contains `---@meta`.
    pub meta: bool,
}

/// Named type definition. Types **have scope**: identity = `(scope, full_name)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeDef {
    /// Globally unique identity (scope + full name).
    pub id: SemanticId,
    pub file_id: FileId,
    /// Bare name (`Bar`), for display.
    pub name: SmolStr,
    /// Full name (`ns.Bar`, including `@namespace` qualification).
    pub full_name: SmolStr,
    pub visibility: TypeVisibility,
    pub kind: TypeDefKind,
    /// Name token range (goto-def positioning).
    pub name_range: TextRange,
    /// Parent type full names from `---@class Foo : Bar, Baz`.
    pub super_names: Vec<SmolStr>,
    /// Generic parameters (`---@class Foo<T: Base>` / `---@alias Foo<T>`).
    pub generic_params: Vec<SalsaGenericParam>,
    /// Target type node of `---@alias Dir -1|1` (Alias only).
    pub alias_type: Option<LuaSyntaxId>,
    /// `---@overload fun(...)` (owned by this type; consumed by `---@operator call` / attribute checks).
    pub call_overloads: Vec<LuaSyntaxId>,
    /// Statement that owns the doc comment (the `---@class X` comment's owner).
    /// Used to associate `---@class GenericTest<T>` with the following `local M = {}`.
    pub owner_syntax: Option<LuaSyntaxId>,
    /// `---@deprecated` (same comment block as `@class`/`@alias`/`@enum`).
    pub deprecated: bool,
    /// Flags (partial / constructor / meta).
    pub flags: TypeDefFlags,
}

impl TypeDef {
    pub fn new(
        file_id: FileId,
        workspace_id: WorkspaceId,
        name: SmolStr,
        full_name: SmolStr,
        visibility: TypeVisibility,
        kind: TypeDefKind,
        name_range: TextRange,
        super_names: Vec<SmolStr>,
    ) -> Self {
        let scope = match visibility {
            TypeVisibility::Public => TypeScope::Global,
            TypeVisibility::Internal => TypeScope::Internal(workspace_id),
            TypeVisibility::Private => TypeScope::File(file_id),
        };
        TypeDef {
            id: SemanticId::type_def(scope, full_name.clone()),
            file_id,
            name,
            full_name,
            visibility,
            kind,
            name_range,
            super_names,
            generic_params: Vec::new(),
            alias_type: None,
            call_overloads: Vec::new(),
            owner_syntax: None,
            deprecated: false,
            flags: TypeDefFlags::default(),
        }
    }
}
