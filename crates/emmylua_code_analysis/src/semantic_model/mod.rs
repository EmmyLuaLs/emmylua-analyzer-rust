pub(crate) mod cache;
pub mod flow;
pub mod infer;
pub mod member;
pub mod render;
pub mod type_check;
pub mod type_eval;

mod access;
mod doc_types;
mod prelude;
mod signature;
mod types;

#[cfg(test)]
mod legacy_visibility_tests;

use hashbrown::HashSet;
use std::cell::{Ref, RefCell, RefMut};

use emmylua_parser::{LuaSyntaxId, VisibilityKind};
use smol_str::SmolStr;

use crate::semantic_db::def::SemanticId;
use crate::semantic_db::{AnalysisView, FileView, SemanticDatabase};
use crate::{FileId, LuaFunctionType, LuaType};

/// Lazily allocated per-model local query cache.
///
/// A typical `SemanticModel` query only touches a few cache maps (or none at
/// all). Keeping `SemanticLocalCache` inline makes every model ~1KB; this
/// wrapper keeps the model small and allocates the cache on first use.
#[derive(Default)]
struct LazyCache {
    inner: RefCell<Option<Box<cache::SemanticLocalCache>>>,
}

impl LazyCache {
    fn ensure(&self) {
        if self.inner.borrow().is_none() {
            *self.inner.borrow_mut() = Some(Box::new(cache::SemanticLocalCache::default()));
        }
    }

    fn borrow(&self) -> Ref<'_, cache::SemanticLocalCache> {
        self.ensure();
        Ref::map(self.inner.borrow(), |inner| {
            inner.as_deref().expect("lazy cache initialized")
        })
    }

    fn borrow_mut(&self) -> RefMut<'_, cache::SemanticLocalCache> {
        self.ensure();
        RefMut::map(self.inner.borrow_mut(), |inner| {
            inner.as_deref_mut().expect("lazy cache initialized")
        })
    }
}

/// Lazily allocated in-progress set (same motivation as [`LazyCache`]).
struct LazySet<T> {
    inner: RefCell<Option<Box<HashSet<T>>>>,
}

impl<T> Default for LazySet<T> {
    fn default() -> Self {
        Self {
            inner: RefCell::new(None),
        }
    }
}

impl<T> LazySet<T> {
    fn ensure(&self) {
        if self.inner.borrow().is_none() {
            *self.inner.borrow_mut() = Some(Box::new(HashSet::new()));
        }
    }

    fn borrow(&self) -> Ref<'_, HashSet<T>> {
        self.ensure();
        Ref::map(self.inner.borrow(), |inner| {
            inner.as_deref().expect("lazy set initialized")
        })
    }

    fn borrow_mut(&self) -> RefMut<'_, HashSet<T>> {
        self.ensure();
        RefMut::map(self.inner.borrow_mut(), |inner| {
            inner.as_deref_mut().expect("lazy set initialized")
        })
    }
}

/// Semantic model: a per-file access handle, only through the semantic analysis layer.
pub struct SemanticModel<'db> {
    view: FileView<'db>,

    /// Per-model local query cache. Recursion-in-progress state is stored in
    /// cache entries, so no separate O(n) guard stacks are needed.
    cache: LazyCache,
    /// Closure-return inference depends on the VM closure environment, so its
    /// result cannot be globally memoized; only O(1) in-progress tracking is kept.
    closure_return_in_progress: LazySet<LuaSyntaxId>,
}

/// Member-reference resolution result: index expression -> actual member declaration.
#[derive(Debug, Clone)]
pub struct ResolvedMember {
    /// Member declaration id (`None` when no declaration resolved).
    pub member_id: Option<SemanticId>,
    /// File containing the member declaration (always present when `member_id` has a value).
    pub file_id: Option<FileId>,
    /// Owner before resolution (`SemanticId`).
    pub owner: SemanticId,
    /// Member name.
    pub name: SmolStr,
    /// Member declaration type (`type_of_member` projection).
    pub member_type: Option<LuaType>,
    /// Member visibility (@field/@private tags).
    pub visibility: Option<VisibilityKind>,
    /// Whether this is a method definition (runtime closure signature `is_method`).
    pub is_method: bool,
}

impl ResolvedMember {
    /// Lazy member type (avoids re-entrant recursion between resolve_member and type_of_member).
    pub fn member_type(&self, model: &SemanticModel<'_>) -> Option<LuaType> {
        let member_id = self.member_id.as_ref()?;
        model.type_of_member(member_id)
    }
}

/// Semantic info at a syntax location (node / token): type + semantic declaration identity.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticInfo {
    /// Type at this location (`Unknown` = no type / inference failed).
    pub typ: LuaType,
    /// Semantic declaration identity (Decl / Member / TypeDef).
    pub decl: Option<SemanticId>,
}

/// Shared call-site analysis computed once per Lua call expression during a file check.
#[derive(Clone)]
pub(crate) struct CallSiteAnalysis {
    /// Callable signatures extracted from the callee (including overloads and member fallbacks).
    pub(crate) candidates: Vec<LuaFunctionType>,
    /// Flow-sensitive types of the actual call arguments.
    pub(crate) arg_types: Vec<LuaType>,
    /// Whether the call uses `:` syntax.
    pub(crate) colon_call: bool,
    /// Type of the implicit receiver for colon calls (`Unknown` otherwise).
    pub(crate) receiver_ty: LuaType,
    /// Explicit generic arguments written in call syntax (`f<T>(...)`).
    pub(crate) explicit_generics: Vec<LuaSyntaxId>,
}

impl<'db> SemanticModel<'db> {
    pub fn new(db: &'db SemanticDatabase, file_id: FileId) -> Self {
        Self {
            view: AnalysisView::new(db).file(file_id),
            cache: LazyCache::default(),
            closure_return_in_progress: LazySet::default(),
        }
    }

    pub(crate) fn db(&self) -> &'db SemanticDatabase {
        self.view.db()
    }
    pub(crate) fn analysis(&self) -> AnalysisView<'db> {
        self.view.analysis()
    }

    pub(crate) fn is_closure_return_in_progress(&self, closure_syntax: LuaSyntaxId) -> bool {
        self.closure_return_in_progress
            .borrow()
            .contains(&closure_syntax)
    }

    pub(crate) fn begin_closure_return_infer(&self, closure_syntax: LuaSyntaxId) {
        self.closure_return_in_progress
            .borrow_mut()
            .insert(closure_syntax);
    }

    pub(crate) fn end_closure_return_infer(&self, closure_syntax: LuaSyntaxId) {
        self.closure_return_in_progress
            .borrow_mut()
            .remove(&closure_syntax);
    }
}

#[cfg(test)]
mod size_tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn semantic_model_stays_small() {
        let size = size_of::<SemanticModel<'static>>();
        assert!(
            size <= 64,
            "SemanticModel grew to {size} bytes; keep heavy caches boxed/lazy"
        );
    }
}
