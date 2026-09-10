//! Canonical cross-file owner / export identities.
//!
//! `SemanticId` identifies a concrete declaration site and is intentionally
//! file-local for `Decl` / `Member`. Workspace indexes, however, need stable
//! identities that survive a declaration moving inside its file:
//!
//! - a global name is always `OwnerId::Global("M")`;
//! - a named type is always `OwnerId::Type(scope, full_name)`;
//! - a module export is always `OwnerId::Module(file_id)`;
//! - a local table / declaration stays file-local, but keeps a stable
//!   `(file_id, range)` pair.
//!
//! `ExportKey` is the canonical map key for a single file contribution. It is
//! the unit the incremental workspace-index layer will add/remove/replace in
//! later phases.

use rowan::TextRange;
use smol_str::SmolStr;

use crate::FileId;

use super::{LuaMemberKey, SemanticId, TypeScope};

/// Canonical owner of members / runtime values.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OwnerId {
    /// Named type: `(scope, full_name)`; `full_name` already includes `@namespace`.
    Type(TypeScope, SmolStr),
    /// Global runtime name (`M`, `a.b`).
    Global(SmolStr),
    /// Module export identity (one per file's top-level `return`).
    Module(FileId),
    /// File-local declaration identity (`local M = {}`).
    Local(FileId, TextRange),
    /// Anonymous / nested table identity (`(file_id, table-range)`).
    Table(FileId, TextRange),
    /// Transitional escape hatch for identities that are not yet canonicalized
    /// (currently `SemanticId::Signature`). Remove once all owner kinds have a
    /// canonical variant.
    Concrete(SemanticId),
}

/// Canonical key for one contribution entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExportKey {
    Type(TypeScope, SmolStr),
    Global(SmolStr),
    RuntimeValue(SmolStr, SemanticId),
    Member(OwnerId, LuaMemberKey),
    Module(FileId),
}
