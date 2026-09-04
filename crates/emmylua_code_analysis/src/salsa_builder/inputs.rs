//! Salsa inputs.

use lsp_types::Uri;
use std::collections::HashMap as StdHashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use emmylua_parser::{
    LuaFeatures, LuaFeaturesSet, LuaLanguageLevel, LuaVersionNumber, ParserConfig, SpecialFunction,
};
use rowan::NodeCache;
use smol_str::SmolStr;

use crate::{Emmyrc, FileId, WorkspaceImport};

use super::SalsaDb;
use super::def::WorkspaceId;

// ──────────────────────────────────────────────
// Inputs
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SourceFileInput {
    file_id: FileId,
}

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

static NO_PATH: Option<PathBuf> = None;
static NO_URI: Option<Uri> = None;

impl SourceFileInput {
    pub(crate) fn new(file_id: FileId) -> Self {
        Self { file_id }
    }

    pub(crate) fn text<'a>(&self, db: &'a dyn SalsaDb) -> &'a str {
        db.source_file_data(self.file_id)
            .map(|data| data.text.as_ref())
            .unwrap_or("")
    }

    pub(crate) fn path<'a>(&self, db: &'a dyn SalsaDb) -> &'a Option<PathBuf> {
        db.source_file_data(self.file_id)
            .map(|data| &data.path)
            .unwrap_or(&NO_PATH)
    }

    pub(crate) fn uri<'a>(&self, db: &'a dyn SalsaDb) -> &'a Option<Uri> {
        db.source_file_data(self.file_id)
            .map(|data| &data.uri)
            .unwrap_or(&NO_URI)
    }

    pub(crate) fn file_id(&self, _db: &dyn SalsaDb) -> FileId {
        self.file_id
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ConfigInput;

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
}

impl ConfigInput {
    pub(crate) fn language_level(&self, db: &dyn SalsaDb) -> LanguageLevel {
        db.config_data()
            .map(|data| data.language_level)
            .unwrap_or(LanguageLevel(LuaLanguageLevel::Lua51))
    }

    pub(crate) fn special_like<'a>(&self, db: &'a dyn SalsaDb) -> &'a [(SmolStr, SpecialFn)] {
        db.config_data()
            .map(|data| data.special_like.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn non_std_symbols<'a>(&self, db: &'a dyn SalsaDb) -> &'a [LuaFeatures] {
        db.config_data()
            .map(|data| data.non_std_symbols.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn module_patterns<'a>(&self, db: &'a dyn SalsaDb) -> &'a [SmolStr] {
        db.config_data()
            .map(|data| data.module_patterns.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn module_replace<'a>(&self, db: &'a dyn SalsaDb) -> &'a [(SmolStr, SmolStr)] {
        db.config_data()
            .map(|data| data.module_replace.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn known_doc_tags<'a>(&self, db: &'a dyn SalsaDb) -> &'a [SmolStr] {
        db.config_data()
            .map(|data| data.known_doc_tags.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn main_root<'a>(&self, db: &'a dyn SalsaDb) -> &'a Option<PathBuf> {
        db.config_data()
            .map(|data| &data.main_root)
            .unwrap_or(&NO_PATH)
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct WorkspaceRoot {
    pub(crate) id: WorkspaceId,
    pub(crate) root: PathBuf,
    pub(crate) import: WorkspaceImport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WorkspaceInput;

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceInputData {
    pub(crate) file_ids: Arc<[FileId]>,
    pub(crate) roots: Arc<[WorkspaceRoot]>,
    pub(crate) revision: u64,
}

impl WorkspaceInputData {
    pub(crate) fn new(file_ids: Arc<[FileId]>, roots: Arc<[WorkspaceRoot]>, revision: u64) -> Self {
        Self {
            file_ids,
            roots,
            revision,
        }
    }
}

impl WorkspaceInput {
    pub(crate) fn file_ids<'a>(&self, db: &'a dyn SalsaDb) -> &'a Arc<[FileId]> {
        db.workspace_data()
            .map(|data| &data.file_ids)
            .unwrap_or(&*EMPTY_FILE_IDS)
    }

    pub(crate) fn roots<'a>(&self, db: &'a dyn SalsaDb) -> &'a Arc<[WorkspaceRoot]> {
        db.workspace_data()
            .map(|data| &data.roots)
            .unwrap_or(&*EMPTY_ROOTS)
    }

    pub(crate) fn revision(&self, db: &dyn SalsaDb) -> u64 {
        db.workspace_data().map(|data| data.revision).unwrap_or(0)
    }
}

static EMPTY_FILE_IDS: LazyLock<Arc<[FileId]>> = LazyLock::new(|| Arc::from([]));
static EMPTY_ROOTS: LazyLock<Arc<[WorkspaceRoot]>> = LazyLock::new(|| Arc::from([]));
