//! Type cache projection layer.
//!
//! Centralized entry points for type queries, backed by Salsa where possible
//! with legacy fallback for cases not yet migrated or when Salsa re-enters.
//!
//! ## Salsa coverage status
//! - `LuaTypeOwner::Decl` — ✅ Salsa-backed via `infer_compilation_decl_type`;
//!   falls back to legacy cache on recursion or inference failure.
//!   Recursion guard lives on `DbIndex::salsa_inferring`.
//! - `LuaTypeOwner::Member` — ⚠️ Legacy only (pending member type inference bridge).
//! - `LuaTypeOwner::SyntaxId` — ⚠️ Legacy only (pending name type inference bridge).

use crate::{
    DbIndex, LuaDeclId, LuaType, LuaTypeCache, LuaTypeOwner,
    compilation::decl::infer_compilation_decl_type, find_compilation_decl_by_position,
};

/// Returns the resolved type for a given type owner.
///
/// For `Decl` owners, tries Salsa-backed inference first;
/// falls back to the legacy type_index cache on recursion or inference failure.
/// The recursion guard is stored on `DbIndex` — no global state.
pub(crate) fn get_type_by_owner(db: &DbIndex, owner: &LuaTypeOwner) -> Option<LuaType> {
    match owner {
        LuaTypeOwner::Decl(decl_id) => {
            salsa_decl_type(db, decl_id).or_else(|| legacy_get_type(db, owner))
        }
        LuaTypeOwner::Member(_) | LuaTypeOwner::SyntaxId(_) => legacy_get_type(db, owner),
    }
}

/// Salsa-backed type lookup. Uses `DbIndex::salsa_inferring` as a recursion
/// guard — if we're already computing this declaration, returns `None` so the
/// caller falls back to the legacy cache (which is a simple O(1) lookup).
fn salsa_decl_type(db: &DbIndex, decl_id: &LuaDeclId) -> Option<LuaType> {
    let key = (decl_id.file_id, decl_id.position);
    let guard = db.salsa_inferring();
    {
        let mut set = guard.borrow_mut();
        if !set.insert(key) {
            return None; // recursive; caller falls back to legacy cache
        }
    }

    let result = (|| {
        let decl_info = find_compilation_decl_by_position(db, decl_id.file_id, decl_id.position)?;
        infer_compilation_decl_type(db, &decl_info)
    })();

    guard.borrow_mut().remove(&key);
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
pub(crate) fn get_type_cache<'a>(
    db: &'a DbIndex,
    owner: &LuaTypeOwner,
) -> Option<&'a LuaTypeCache> {
    db.get_type_index().get_type_cache(owner)
}
