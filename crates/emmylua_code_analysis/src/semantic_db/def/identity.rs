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
/// Identity-level dependency key (P7).
///
/// A file's references register the canonical keys they read; workspace writes
/// publish the canonical keys they changed. Invalidation is the intersection of
/// the two, not a string-name scan.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DependencyKey {
    Global(SmolStr),
    Type(TypeScope, SmolStr),
    /// Runtime value implementing a named type (`local M = {}` for `@class M`).
    RuntimeValue(SmolStr),
    Member(OwnerId, LuaMemberKey),
    Module(FileId),
    /// Module name referenced by a require literal.
    ///
    /// Unlike Module(FileId), this is a negative dependency: it is recorded
    /// even when the module cannot be resolved yet, so adding, removing or
    /// renaming the module file can refresh the consumer require aliases and
    /// reference index through the normal changed-key intersection.
    ModuleName(SmolStr),
    /// Bare or qualified type name referenced before the type exists.
    ///
    /// This is a negative dependency recorded by unresolved name and member
    /// owner uses. Adding, renaming or removing a matching type publishes it,
    /// so the reference index can re-resolve without editing the consumer.
    TypeName(SmolStr),
}

/// Canonical dependency set of one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileDependencies {
    pub keys: hashbrown::HashSet<DependencyKey>,
}

impl FileDependencies {
    pub fn insert(&mut self, key: DependencyKey) {
        self.keys.insert(key);
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &DependencyKey> {
        self.keys.iter()
    }
}

/// Canonical keys changed by a file write (export surface delta).
pub type ChangedKeys = hashbrown::HashSet<DependencyKey>;
