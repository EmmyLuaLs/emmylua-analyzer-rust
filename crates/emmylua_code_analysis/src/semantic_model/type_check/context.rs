//! Check context: accesses the type environment only through `SemanticModel` (semantic).
//!
//! The old implementation held `&DbIndex` (type/member/signature/operator indexes);
//! 类型定义, 直接父类型和成员均通过语义模型查询.
//! Parts still missing from semantic (alias origin types, enum field unions, call operators) fall back to nominal checks.

use crate::LuaType;
use crate::LuaTypeDeclId;
use crate::semantic_db::def::{TypeDef, TypeDefKind};

use super::fail_reason::TypeCheckFailReason;
use crate::semantic_model::SemanticModel;
use crate::semantic_model::render::humanize_type;

#[derive(Clone)]
pub struct TypeCheckContext<'db> {
    pub detail: bool,
    pub model: &'db SemanticModel<'db>,
    /// Subtype proofs also check object fields and generic inheritance arguments.
    pub strict_subtype: bool,
    pub(super) comparisons: std::rc::Rc<std::cell::RefCell<super::comparison::Comparisons>>,
}

impl<'db> TypeCheckContext<'db> {
    pub fn new(model: &'db SemanticModel<'db>, detail: bool) -> Self {
        Self {
            detail,
            model,
            strict_subtype: false,
            comparisons: Default::default(),
        }
    }

    /// Do not use an unfinished semantic dependency as comparison evidence.
    pub(super) fn require_complete<T>(
        &self,
        query: impl FnOnce() -> T,
    ) -> Result<T, TypeCheckFailReason> {
        let revision = self.model.cache.borrow().revision();
        let result = query();
        let cache = self.model.cache.borrow();
        if cache.evaluation_exhausted {
            Err(TypeCheckFailReason::TooComplex)
        } else if cache.is_complete(revision) {
            Ok(result)
        } else {
            Err(TypeCheckFailReason::Unresolved)
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

    // ── Type environment (semantic resolution) ──

    /// `LuaTypeDeclId` → semantic type definition.
    pub fn type_def_of(&self, id: &LuaTypeDeclId) -> Option<TypeDef> {
        self.model.type_def_of(id)
    }

    /// Direct parent types (A/B in `---@class C : A, B`).
    pub fn super_types_of(&self, id: &LuaTypeDeclId) -> Vec<LuaType> {
        self.model.base_types(id)
    }

    pub fn is_enum(&self, id: &LuaTypeDeclId) -> bool {
        self.type_def_of(id)
            .is_some_and(|def| def.kind == TypeDefKind::Enum)
    }
}
