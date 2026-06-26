//! Declaration and signature lookup projections.
//!
//! Clean, Salsa-first APIs for declaration and signature queries, replacing
//! direct reads of `LuaDeclIndex` and `LuaSignatureIndex` in `semantic/`.
//!
//! ## Naming convention
//! - `find_*` — may return `None` (lookup by ID or name).
//! - `get_*`  — returns owned or reference data; panics / expects existence.
//!
//! ## Salsa coverage
//! - `find_decl_by_id` — ⚠️ Legacy wrapper (returns `LuaDecl`; needs Salsa bridge).
//! - `get_file_decl_tree` — ✅ Salsa-backed via `CompilationDeclTree`.
//! - `find_signature_by_id` — ⚠️ Legacy wrapper (returns `LuaSignature`; needs Salsa bridge).

use crate::{
    DbIndex, FileId, LuaDeclId, LuaSignatureId,
    compilation::CompilationDeclTree,
    db_index::{LuaDecl, LuaSignature},
};

/// Returns the legacy `LuaDecl` for a declaration ID.
///
/// Prefer Salsa-backed alternatives when only kind/is_local/name is needed:
/// - `find_compilation_decl_by_position(db, file_id, position)` → `CompilationDeclInfo`
/// - `CompilationDeclInfo::summary.kind` for param/local/global classification.
pub(crate) fn find_decl_by_id<'a>(db: &'a DbIndex, decl_id: &LuaDeclId) -> Option<&'a LuaDecl> {
    db.get_decl_index().get_decl(decl_id)
}

/// Returns the Salsa-backed declaration tree for a file.
/// Preferred over `find_decl_by_id` for traversing file declarations.
pub(crate) fn get_file_decl_tree(db: &DbIndex, file_id: FileId) -> Option<CompilationDeclTree> {
    Some(CompilationDeclTree::new(
        db.get_summary_db().file().decl_tree(file_id)?,
    ))
}

/// Returns the legacy `LuaSignature` for a signature ID.
///
/// Salsa has `doc().signature().explain(file_id, offset)` but the conversion
/// to `LuaSignature` is pending — this is a thin wrapper for now.
pub(crate) fn find_signature_by_id<'a>(
    db: &'a DbIndex,
    sig_id: &LuaSignatureId,
) -> Option<&'a LuaSignature> {
    db.get_signature_index().get(sig_id)
}
