//! Salsa inputs.

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

use super::SalsaDatabase;
use super::def::WorkspaceId;

// ──────────────────────────────────────────────
// Inputs
// ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(crate) struct SourceFileInputData {
    pub(crate) text: Arc<str>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) uri: Option<Uri>,
}

impl SourceFileInputData {
    pub(crate) fn new(text: Arc<str>, path: Option<PathBuf>, uri: Option<Uri>) -> Self {
        Self { text, path, uri }
    }
}

impl FileId {
    pub(crate) fn text(self, db: &SalsaDatabase) -> &str {
        &db.source_file_data(self)
            .expect("file data must exist")
            .text
    }

    pub(crate) fn path(self, db: &SalsaDatabase) -> &Option<PathBuf> {
        &db.source_file_data(self)
            .expect("file data must exist")
            .path
    }

    pub(crate) fn uri(self, db: &SalsaDatabase) -> &Option<Uri> {
        &db.source_file_data(self).expect("file data must exist").uri
    }

    pub(crate) fn file_id(self, _db: &SalsaDatabase) -> FileId {
        self
    }
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

#[derive(Debug, Clone)]
pub(crate) struct ConfigInputData {
    pub(crate) language_level: LanguageLevel,
    pub(crate) special_like: Vec<(SmolStr, SpecialFn)>,
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
        language_level: LanguageLevel,
        special_like: Vec<(SmolStr, SpecialFn)>,
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

    pub(crate) fn language_level(&self) -> LanguageLevel {
        self.language_level
    }

    pub(crate) fn special_like(&self) -> &[(SmolStr, SpecialFn)] {
        &self.special_like
    }

    pub(crate) fn non_std_symbols(&self) -> &[LuaFeatures] {
        &self.non_std_symbols
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

    pub(crate) fn to_parse_config<'a>(&self, node_cache: &'a mut NodeCache) -> ParserConfig<'a> {
        let mut special_like = StdHashMap::new();
        for (name, func) in &self.special_like {
            special_like.insert(name.as_str().to_string(), func.0);
        }
        let mut non_std_symbols = LuaFeaturesSet::default();
        non_std_symbols.extends(self.non_std_symbols.to_vec());
        ParserConfig::new(
            self.language_level.0,
            Some(node_cache),
            special_like,
            non_std_symbols,
            true,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WorkspaceInput;


