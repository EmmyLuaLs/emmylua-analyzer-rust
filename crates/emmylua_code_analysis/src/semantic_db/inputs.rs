//! Inputs.

use std::path::PathBuf;

use emmylua_parser::{LuaLanguageLevel, LuaVersionNumber};

use crate::WorkspaceImport;

use super::def::WorkspaceId;

// ──────────────────────────────────────────────
// Inputs
// ──────────────────────────────────────────────

pub(crate) fn language_level_to_version(level: LuaLanguageLevel) -> LuaVersionNumber {
    match level {
        LuaLanguageLevel::Lua51 => LuaVersionNumber::new(5, 1, 0),
        LuaLanguageLevel::Lua52 => LuaVersionNumber::new(5, 2, 0),
        LuaLanguageLevel::Lua53 => LuaVersionNumber::new(5, 3, 0),
        LuaLanguageLevel::Lua54 => LuaVersionNumber::new(5, 4, 0),
        LuaLanguageLevel::Lua55 => LuaVersionNumber::new(5, 5, 0),
        LuaLanguageLevel::LuaJIT | LuaLanguageLevel::LuaJIT2 | LuaLanguageLevel::LuaJIT3 => {
            LuaVersionNumber::LUA_JIT
        }
    }
}

/// Workspace root metadata for std / main / library.
///
/// File sets are managed by `SemanticDatabase`; workspace_id is derived by
/// matching path prefixes against `roots`, while `import` controls which relative paths participate in module indexing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct WorkspaceRoot {
    pub(crate) id: WorkspaceId,
    pub(crate) root: PathBuf,
    pub(crate) import: WorkspaceImport,
}
