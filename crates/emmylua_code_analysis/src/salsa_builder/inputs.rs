//! Salsa inputs.

use std::path::PathBuf;

use emmylua_parser::{LuaFeatures, LuaLanguageLevel, LuaVersionNumber, SpecialFunction};
use smol_str::SmolStr;

use crate::{Emmyrc, FileId, WorkspaceImport};

use super::SemanticDatabase;
use super::def::WorkspaceId;

// ──────────────────────────────────────────────
// Inputs
// ──────────────────────────────────────────────

impl FileId {
    pub(crate) fn path(self, db: &SemanticDatabase) -> &Option<PathBuf> {
        &db.source_file_data(self)
            .expect("file data must exist")
            .path
    }

    pub(crate) fn file_id(self, _db: &SemanticDatabase) -> FileId {
        self
    }
}

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

#[derive(Debug, Clone)]
pub(crate) struct ConfigInputData {
    pub(crate) language_level: LuaLanguageLevel,
    pub(crate) special_like: Vec<(SmolStr, SpecialFunction)>,
    pub(crate) non_std_symbols: Vec<LuaFeatures>,
    pub(crate) module_patterns: Vec<SmolStr>,
    pub(crate) module_replace: Vec<(SmolStr, SmolStr)>,
    pub(crate) known_doc_tags: Vec<SmolStr>,
    pub(crate) strict_array_index: bool,
    pub(crate) main_root: Option<PathBuf>,
}

impl ConfigInputData {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        language_level: LuaLanguageLevel,
        special_like: Vec<(SmolStr, SpecialFunction)>,
        non_std_symbols: Vec<LuaFeatures>,
        module_patterns: Vec<SmolStr>,
        module_replace: Vec<(SmolStr, SmolStr)>,
        known_doc_tags: Vec<SmolStr>,
        strict_array_index: bool,
        main_root: Option<PathBuf>,
    ) -> Self {
        Self {
            language_level,
            special_like,
            non_std_symbols,
            module_patterns,
            module_replace,
            known_doc_tags,
            strict_array_index,
            main_root,
        }
    }

    pub(crate) fn module_patterns(&self) -> &[SmolStr] {
        &self.module_patterns
    }

    pub(crate) fn module_replace(&self) -> &[(SmolStr, SmolStr)] {
        &self.module_replace
    }

    pub(crate) fn known_doc_tags(&self) -> &[SmolStr] {
        &self.known_doc_tags
    }

    pub(crate) fn main_root(&self) -> &Option<PathBuf> {
        &self.main_root
    }

    /// Extract configuration from `Emmyrc`.
    #[allow(clippy::type_complexity)]
    pub(crate) fn parts_from_emmyrc(
        emmyrc: &Emmyrc,
    ) -> (
        LuaLanguageLevel,
        Vec<(SmolStr, SpecialFunction)>,
        Vec<LuaFeatures>,
        Vec<SmolStr>,
        Vec<(SmolStr, SmolStr)>,
        Vec<SmolStr>,
        bool,
    ) {
        let mut special_like = Vec::new();
        for (name, func) in &emmyrc.runtime.special {
            if let Some(func) = (*func).into() {
                special_like.push((SmolStr::new(name), func));
            }
        }
        for name in &emmyrc.runtime.require_like_function {
            special_like.push((SmolStr::new(name), SpecialFunction::Require));
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
            emmyrc.get_language_level(),
            special_like,
            non_std_symbols,
            module_patterns,
            module_replace,
            known_doc_tags,
            emmyrc.strict.array_index,
        )
    }
}

/// Workspace root metadata for std / main / library.
///
/// File sets are managed by `SalsaDatabase`; workspace_id is derived by
/// matching path prefixes against `roots`, while `import` controls which relative paths participate in module indexing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct WorkspaceRoot {
    pub(crate) id: WorkspaceId,
    pub(crate) root: PathBuf,
    pub(crate) import: WorkspaceImport,
}
