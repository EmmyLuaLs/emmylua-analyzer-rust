//! Salsa inputs and globally interned identities.

use lsp_types::Uri;
use std::collections::HashMap as StdHashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use emmylua_parser::{
    LuaFeatures, LuaFeaturesSet, LuaLanguageLevel, LuaVersionNumber, ParserConfig, SpecialFunction,
};
use rowan::NodeCache;
use smol_str::SmolStr;

use crate::{Emmyrc, FileId, WorkspaceImport};

use super::SalsaDb;
use super::def::{TypeScope, WorkspaceId};

// ──────────────────────────────────────────────
// Inputs
// ──────────────────────────────────────────────

#[salsa::input(debug)]
pub(crate) struct SourceFileInput {
    #[returns(deref)]
    pub(crate) text: Arc<str>,

    #[returns(ref)]
    pub(crate) path: Option<PathBuf>,

    #[returns(ref)]
    pub(crate) uri: Option<Uri>,

    #[returns(copy)]
    pub(crate) file_id: FileId,
}

/// `LuaLanguageLevel` lacks `Hash`; salsa fields require Eq+Hash, so a newtype supplies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LanguageLevel(pub LuaLanguageLevel);

impl Hash for LanguageLevel {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0 as u8).hash(state);
    }
}

impl LanguageLevel {
    /// Language level → runtime version number (used for `---@version` visibility checks).
    pub fn to_lua_version_number(&self) -> LuaVersionNumber {
        match self.0 {
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
}

/// `SpecialFunction` lacks `Hash`; same workaround.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpecialFn(pub SpecialFunction);

impl Hash for SpecialFn {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0 as u8).hash(state);
    }
}

#[salsa::input(debug)]
pub(crate) struct ConfigInput {
    #[returns(copy)]
    pub(crate) language_level: LanguageLevel,

    #[returns(ref)]
    pub(crate) special_like: Vec<(SmolStr, SpecialFn)>,

    #[returns(ref)]
    pub(crate) non_std_symbols: Vec<LuaFeatures>,

    /// require resolution patterns (e.g. `?.lua`, `?/init.lua`).
    #[returns(ref)]
    pub(crate) module_patterns: Vec<SmolStr>,

    /// require name rewrite rules (`(pattern, replace)`, from `workspace.module_map`).
    #[returns(ref)]
    pub(crate) module_replace: Vec<(SmolStr, SmolStr)>,

    /// `emmyrc.doc.known_tags`: additional known doc tags (used by unknown_doc_tag checks).
    #[returns(ref)]
    pub(crate) known_doc_tags: Vec<SmolStr>,

    /// `emmyrc.strict.array_index`: whether array indexes are treated as nullable (strict nil checks).
    #[returns(copy)]
    pub(crate) strict_array_index: bool,

    /// Main workspace root directory (used to derive require module names; set by `add_main_workspace`).
    #[returns(ref)]
    pub(crate) main_root: Option<PathBuf>,
}

impl ConfigInput {
    /// Extract configuration from `Emmyrc` (salsa inputs must be built with `ConfigInput::new`).
    #[allow(clippy::type_complexity)]
    pub(crate) fn parts_from_emmyrc(
        emmyrc: &Emmyrc,
    ) -> (
        LanguageLevel,
        Vec<(SmolStr, SpecialFn)>,
        Vec<LuaFeatures>,
        Vec<SmolStr>,
        Vec<(SmolStr, SmolStr)>,
        Vec<SmolStr>,
        bool,
    ) {
        let mut special_like = Vec::new();
        for (name, func) in &emmyrc.runtime.special {
            if let Some(func) = (*func).into() {
                special_like.push((SmolStr::new(name), SpecialFn(func)));
            }
        }
        for name in &emmyrc.runtime.require_like_function {
            special_like.push((SmolStr::new(name), SpecialFn(SpecialFunction::Require)));
        }

        let mut non_std_symbols = emmyrc
            .runtime
            .nonstandard_symbol
            .iter()
            .map(|symbol| LuaFeatures::from(*symbol))
            .collect::<Vec<_>>();
        non_std_symbols.sort_by_key(|symbol| *symbol as u64);
        non_std_symbols.dedup();

        // Module resolution patterns: `?.lua` + `?/init.lua` (or custom require_pattern).
        let mut extensions: Vec<SmolStr> = emmyrc
            .runtime
            .extensions
            .iter()
            .map(|ext| {
                SmolStr::new(
                    ext.strip_prefix(".")
                        .or_else(|| ext.strip_prefix("*."))
                        .unwrap_or(ext),
                )
            })
            .collect();
        if !extensions.iter().any(|e| e == "lua") {
            extensions.push(SmolStr::new("lua"));
        }
        let mut module_patterns: Vec<SmolStr> = extensions
            .iter()
            .map(|ext| SmolStr::new(format!("?.{}", ext)))
            .collect();
        if emmyrc.runtime.require_pattern.is_empty() {
            for ext in &extensions {
                module_patterns.push(SmolStr::new(format!("?/init.{}", ext)));
            }
        } else {
            module_patterns.extend(emmyrc.runtime.require_pattern.iter().map(SmolStr::new));
        }

        let module_replace = emmyrc
            .workspace
            .module_map
            .iter()
            .map(|m| (SmolStr::new(&m.pattern), SmolStr::new(&m.replace)))
            .collect::<Vec<_>>();

        let known_doc_tags = emmyrc
            .doc
            .known_tags
            .iter()
            .map(|tag| SmolStr::new(tag.as_str()))
            .collect::<Vec<_>>();

        (
            LanguageLevel(emmyrc.get_language_level()),
            special_like,
            non_std_symbols,
            module_patterns,
            module_replace,
            known_doc_tags,
            emmyrc.strict.array_index,
        )
    }

    pub(crate) fn to_parse_config<'a>(
        &self,
        db: &dyn SalsaDb,
        node_cache: &'a mut NodeCache,
    ) -> ParserConfig<'a> {
        let mut special_like = StdHashMap::new();
        for (name, func) in self.special_like(db) {
            special_like.insert(name.as_str().to_string(), func.0);
        }
        let mut non_std_symbols = LuaFeaturesSet::default();
        non_std_symbols.extends(self.non_std_symbols(db).to_vec());
        ParserConfig::new(
            self.language_level(db).0,
            Some(node_cache),
            special_like,
            non_std_symbols,
            true,
        )
    }
}

/// Workspace root metadata for std / main / library.
///
/// File sets remain managed by `WorkspaceInput.file_ids`; workspace_id is derived by
/// matching path prefixes against `roots`, while `import` controls which relative paths participate in module indexing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub(crate) struct WorkspaceRoot {
    pub(crate) id: WorkspaceId,
    pub(crate) root: PathBuf,
    pub(crate) import: WorkspaceImport,
}

// `WorkspaceImport` is pure `'static` data and is safe for salsa to retain.
unsafe impl salsa::SalsaValue for WorkspaceImport {}

/// Workspace file-set input. Refreshed as files are added or removed.
///
/// This is the core file-list input on the salsa side; it only stores a lightweight `FileId` set and
/// workspace root metadata, while `FileId -> SourceFileInput` mappings and URI/Path indexes
/// are handled by `VfsSnapshot`. Thus file-set changes only replace one `Arc<[FileId]>`,
/// without copying a large HashMap.
#[salsa::input(debug)]
pub(crate) struct WorkspaceInput {
    #[returns(ref)]
    pub(crate) file_ids: Arc<[FileId]>,

    #[returns(ref)]
    pub(crate) roots: Arc<[WorkspaceRoot]>,
}

// ──────────────────────────────────────────────
// Globally interned identities
// ──────────────────────────────────────────────

/// Named types (`---@class Foo` etc.). Interned by `(scope, full_name)`, sharing the same id across files.
#[salsa::interned(debug)]
pub struct TypeName<'db> {
    #[returns(copy)]
    pub scope: TypeScope,

    #[returns(deref)]
    pub name: SmolStr,
}
