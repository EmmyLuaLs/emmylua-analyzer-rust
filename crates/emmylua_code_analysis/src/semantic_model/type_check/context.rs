//! Check context: accesses the type environment only through `SemanticModel` (salsa).
//!
//! The old implementation held `&DbIndex` (type/member/signature/operator indexes);
//! this code resolves everything through salsa: `resolve_type_def` + `super_names` + the member system.
//! Parts still missing from salsa (alias origin types, enum field unions, call operators) fall back to nominal checks.

use crate::LuaType;
use crate::LuaTypeDeclId;
use crate::salsa_builder::def::{TypeDef, TypeDefKind};

use super::super::SemanticModel;
use super::super::render::humanize_type;
use super::fail_reason::TypeCheckFailReason;

#[derive(Clone)]
pub struct TypeCheckContext<'db> {
    pub detail: bool,
    pub model: &'db SemanticModel<'db>,
    /// Assignment semantic mode (`number -> integer`, any matching union target component, etc.).
    pub assign_mode: bool,
    /// Strict subtype mode (legacy `type_check_subtype`: all components of a union target, object field level).
    pub strict_union: bool,
    pub strict_object: bool,
    /// Strict generic inheritance mode: retain parent class generic arguments for precise inheritance checks.
    pub strict_generic: bool,
}

impl<'db> TypeCheckContext<'db> {
    pub fn new(model: &'db SemanticModel<'db>, detail: bool) -> Self {
        Self {
            detail,
            model,
            assign_mode: false,
            strict_union: false,
            strict_object: false,
            strict_generic: false,
        }
    }

    /// Build the failure reason for the current context: when `detail` is enabled, return human-readable type mismatch info.
    pub fn mismatch(&self, source: &LuaType, target: &LuaType) -> TypeCheckFailReason {
        if self.detail {
            TypeCheckFailReason::TypeNotMatchWithReason(format!(
                "expected `{}`, found `{}`",
                humanize_type(self.model, target),
                humanize_type(self.model, source)
            ))
        } else {
            TypeCheckFailReason::TypeNotMatch
        }
    }

    // ── Type environment (salsa resolution) ──

    /// `LuaTypeDeclId` → salsa type definition.
    pub fn type_def_of(&self, id: &LuaTypeDeclId) -> Option<TypeDef> {
        self.model.resolve_type_def(id.get_name())
    }

    /// Direct parent types (A/B in `---@class C : A, B`, resolved as reference types).
    pub fn super_types_of(&self, id: &LuaTypeDeclId) -> Vec<LuaType> {
        let Some(def) = self.type_def_of(id) else {
            return Vec::new();
        };
        def.super_names
            .iter()
            .map(|name| LuaType::Ref(LuaTypeDeclId::global(name)))
            .collect()
    }

    pub fn is_alias(&self, id: &LuaTypeDeclId) -> bool {
        self.type_def_of(id)
            .is_some_and(|def| def.kind == TypeDefKind::Alias)
    }

    /// Alias expansion: the target type (after projection, generic references keep `TplRef`).
    pub fn alias_target_of(&self, id: &LuaTypeDeclId) -> Option<LuaType> {
        let def = self.type_def_of(id)?;
        if def.kind != TypeDefKind::Alias {
            return None;
        }
        self.model.alias_target(&def)
    }

    pub fn is_enum(&self, id: &LuaTypeDeclId) -> bool {
        self.type_def_of(id)
            .is_some_and(|def| def.kind == TypeDefKind::Enum)
    }
}
