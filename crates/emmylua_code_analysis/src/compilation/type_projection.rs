//! Type cache projection layer.
//!
//! Centralized entry points for type queries, backed by Salsa where possible
//! with legacy fallback for cases not yet migrated or when Salsa re-enters.
//!
//! ## Salsa coverage status
//! - `LuaTypeOwner::Decl` — ✅ Salsa-backed via `infer_compilation_decl_type`;
//!   falls back to legacy cache on recursion or inference failure.
//! - `LuaTypeOwner::Member` — ⚠️ Legacy only (pending member type inference bridge).
//! - `LuaTypeOwner::SyntaxId` — ⚠️ Legacy only (pending name type inference bridge).

use std::cell::RefCell;

use hashbrown::HashSet;
use rowan::TextSize;

use crate::{
    DbIndex, FileId, LuaDeclId, LuaType, LuaTypeCache, LuaTypeOwner,
    compilation::decl::infer_compilation_decl_type,
    find_compilation_decl_by_position,
};

thread_local! {
    /// Set of `(file_id, position)` currently being inferred via Salsa.
    /// Guards against infinite recursion when inference re-enters
    /// via `get_type_by_owner` for the same declaration.
    static SALSA_INFERRING: RefCell<HashSet<(FileId, TextSize)>> =
        RefCell::new(HashSet::new());
}

/// Returns the resolved type for a given type owner.
///
/// For `Decl` owners, tries Salsa-backed inference first;
/// falls back to the legacy type_index cache on recursion or inference failure.
pub(crate) fn get_type_by_owner(db: &DbIndex, owner: &LuaTypeOwner) -> Option<LuaType> {
    match owner {
        LuaTypeOwner::Decl(decl_id) => {
            salsa_decl_type(db, decl_id).or_else(|| legacy_get_type(db, owner))
        }
        LuaTypeOwner::Member(_) | LuaTypeOwner::SyntaxId(_) => legacy_get_type(db, owner),
    }
}

/// Salsa-backed type lookup. Uses existing compilation infrastructure
/// (`find_compilation_decl_by_position` + `infer_compilation_decl_type`)
/// which both query Salsa summaries internally.  A thread-local guard
/// prevents infinite recursion when inference triggers a re-lookup.
fn salsa_decl_type(db: &DbIndex, decl_id: &LuaDeclId) -> Option<LuaType> {
    let key = (decl_id.file_id, decl_id.position);
    let already = SALSA_INFERRING.with(|set| !set.borrow_mut().insert(key));
    if already {
        return None; // recursive; caller falls back to legacy cache
    }

    let result = (|| {
        let decl_info =
            find_compilation_decl_by_position(db, decl_id.file_id, decl_id.position)?;
        infer_compilation_decl_type(db, &decl_info)
    })();

    SALSA_INFERRING.with(|set| {
        set.borrow_mut().remove(&key);
    });
    result
}

fn legacy_get_type(db: &DbIndex, owner: &LuaTypeOwner) -> Option<LuaType> {
    db.get_type_index()
        .get_type_cache(owner)
        .map(|cache| cache.as_type().clone())
}

/// Returns the type cache entry. Legacy-only;
/// prefer `get_type_by_owner` when possible.
///
/// Only remaining caller: `infer_table.rs` which needs `is_doc()` distinction.
pub(crate) fn get_type_cache<'a>(db: &'a DbIndex, owner: &LuaTypeOwner) -> Option<&'a LuaTypeCache> {
    db.get_type_index().get_type_cache(owner)
}
