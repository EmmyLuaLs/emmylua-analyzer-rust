//! Function signature definition (one per `ClosureExpr`).
//!
//! All doc details are packed into one `Option<Box<SignatureDoc>>`: zero extra overhead when there is no doc,
//! and no 24-byte Vec header added to every signature when fields are added.

use core::fmt;

use emmylua_parser::{LuaAstNode, LuaClosureExpr, LuaDocFuncType, LuaSyntaxId};
use rowan::TextSize;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};
use smol_str::SmolStr;

use crate::FileId;

use super::{SalsaGenericParam, SemanticId};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Signature {
    /// Globally unique identity (file + closure syntax node).
    pub id: SemanticId,
    pub file_id: FileId,
    pub closure_syntax: LuaSyntaxId,
    pub name: Option<SmolStr>,
    pub is_method: bool,
    /// Owning statement (doc annotation ownership key).
    pub owner_syntax: Option<LuaSyntaxId>,
    /// Parameter names (in declaration order).
    pub param_names: Vec<SmolStr>,
    /// Whether variadic parameters are present (`...` or Lua 5.5 named variadic `...args`).
    pub is_variadic: bool,
    /// All doc details; `None` when there are no doc annotations (zero extra overhead).
    pub docs: Option<Box<SignatureDoc>>,
}

/// `---@[constructor("__init")]` (attribute line immediately after `---@param`).
/// The legacy signature index stored attributes per param, so `meta("Name")` calls use this to
/// resolve `Name()` into a `Name:__init()` constructor call.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ConstructorAttribute {
    /// Constructor method name (`constructor("init")` -> `init`).
    pub name: SmolStr,
    /// Optional root class (`constructor("init", "Base")`).
    pub root_class: Option<SmolStr>,
    /// Whether to strip `self` from class-call arguments (default `true`).
    pub strip_self: bool,
    /// Constructor return strategy: `self` / `doc` / `default`.
    pub return_mode: ConstructorReturnMode,
}

/// Constructor attribute return strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ConstructorReturnMode {
    SelfType,
    Doc,
    #[default]
    Default,
}

impl ConstructorReturnMode {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "self" => Some(Self::SelfType),
            "doc" => Some(Self::Doc),
            "default" => Some(Self::Default),
            _ => None,
        }
    }
}

/// `---@return_cast name Type else Fallback`: true/false branch narrowing when a function is used as a type guard.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SignatureReturnCast {
    /// Narrowed parameter name (`self` is allowed).
    pub name: SmolStr,
    /// True-branch type syntax.
    pub cast: LuaSyntaxId,
    /// False-branch type syntax (type after `else`).
    pub fallback: Option<LuaSyntaxId>,
}

/// Function doc annotation details (collected and merged by owner statement).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SignatureDoc {
    /// `---@param name type`, keyed by name.
    pub param_types: Vec<(SmolStr, LuaSyntaxId)>,
    /// Nullable parameter names from `---@param name? type` (merged with nil during projection).
    pub nullable_params: Vec<SmolStr>,
    /// `---@[constructor("init")]` attributes: parameter name -> constructor configuration.
    /// The attribute line applies to the immediately following `---@param`.
    pub constructor_params: Vec<(SmolStr, ConstructorAttribute)>,
    /// Main return rows `---@return type` / `---@return name type`, in source order (including named returns).
    pub returns: Vec<LuaSyntaxId>,
    /// Named returns from `---@return name type` (in source order, for display).
    pub named_returns: Vec<(SmolStr, LuaSyntaxId)>,
    /// `---@return_overload` return rows; name is always `None`.
    pub return_overloads: Vec<(Option<SmolStr>, LuaSyntaxId)>,
    /// `---@return_cast name Type else Fallback`.
    pub return_cast: Option<SignatureReturnCast>,
    /// Number of types per `---@return_overload` row (`@return true, integer` = 2).
    pub return_overload_rows: Vec<usize>,
    /// `---@overload fun(...)` type nodes.
    pub overloads: Vec<LuaSyntaxId>,
    /// `---@generic T` generic parameters.
    pub generic_params: Vec<SalsaGenericParam>,
    /// `---@deprecated`.
    pub deprecated: bool,
    /// `---@async`.
    pub is_async: bool,
    /// `---@nodiscard [message]` (return value must not be discarded).
    pub nodiscard: Option<SmolStr>,
    /// `---@version 5.1, > 5.2` visibility condition (member selection / completion filtering).
    pub versions: Vec<emmylua_parser::LuaVersionCondition>,
}

impl SignatureDoc {
    pub fn is_empty(&self) -> bool {
        self.param_types.is_empty()
            && self.nullable_params.is_empty()
            && self.constructor_params.is_empty()
            && self.returns.is_empty()
            && self.named_returns.is_empty()
            && self.return_overloads.is_empty()
            && self.return_cast.is_none()
            && self.return_overload_rows.is_empty()
            && self.overloads.is_empty()
            && self.generic_params.is_empty()
            && !self.deprecated
            && !self.is_async
            && self.nodiscard.is_none()
            && self.versions.is_empty()
    }
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub struct LuaSignatureId {
    file_id: FileId,
    position: TextSize,
}

impl Serialize for LuaSignatureId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = format!("{}|{}", self.file_id.id, u32::from(self.position));
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for LuaSignatureId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LuaSignatureIdVisitor;

        impl<'de> Visitor<'de> for LuaSignatureIdVisitor {
            type Value = LuaSignatureId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string with format 'file_id:position'")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let parts: Vec<&str> = value.split('|').collect();
                if parts.len() != 2 {
                    return Err(E::custom("expected format 'file_id:position'"));
                }

                let file_id = FileId {
                    id: parts[0]
                        .parse()
                        .map_err(|e| E::custom(format!("invalid file_id: {}", e)))?,
                };
                let position = TextSize::new(
                    parts[1]
                        .parse()
                        .map_err(|e| E::custom(format!("invalid position: {}", e)))?,
                );

                Ok(LuaSignatureId { file_id, position })
            }
        }

        deserializer.deserialize_str(LuaSignatureIdVisitor)
    }
}

impl LuaSignatureId {
    pub fn from_closure(file_id: FileId, closure: &LuaClosureExpr) -> Self {
        Self {
            file_id,
            position: closure.get_position(),
        }
    }

    pub fn from_doc_func(file_id: FileId, func_type: &LuaDocFuncType) -> Self {
        Self {
            file_id,
            position: func_type.get_position(),
        }
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn get_position(&self) -> TextSize {
        self.position
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignatureReturnStatus {
    UnResolve,
    DocResolve,
    InferResolve,
}
