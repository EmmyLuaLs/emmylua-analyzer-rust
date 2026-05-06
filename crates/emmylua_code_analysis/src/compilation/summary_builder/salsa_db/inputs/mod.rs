use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use emmylua_parser::{
    LuaChunk, LuaLanguageLevel, LuaNonStdSymbol, LuaNonStdSymbolSet, LuaParser, ParserConfig,
    SpecialFunction,
};
use hashbrown::HashMap;
use rowan::NodeCache;

use crate::{Emmyrc, FileId, InFiled, Vfs, db_index::Workspace};

use super::SalsaSummaryDatabase;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalsaSummaryConfig {
    language_level: LuaLanguageLevel,
    special_like: Vec<(String, SpecialFunction)>,
    non_std_symbols: Vec<LuaNonStdSymbol>,
    module_extract_patterns: Vec<String>,
    module_replace_patterns: Vec<(String, String)>,
}

impl SalsaSummaryConfig {
    pub fn from_emmyrc(emmyrc: Arc<Emmyrc>) -> Self {
        let mut special_like = BTreeMap::new();
        for (name, func) in &emmyrc.runtime.special {
            if let Some(func) = (*func).into() {
                special_like.insert(name.clone(), func);
            }
        }
        for name in &emmyrc.runtime.require_like_function {
            special_like.insert(name.clone(), SpecialFunction::Require);
        }

        let mut non_std_symbols = emmyrc
            .runtime
            .nonstandard_symbol
            .iter()
            .map(|symbol| LuaNonStdSymbol::from(*symbol))
            .collect::<Vec<_>>();
        non_std_symbols.sort_by_key(|symbol| *symbol as u64);
        non_std_symbols.dedup();

        let mut module_extract_patterns = Vec::new();
        let mut extensions = emmyrc.runtime.extensions.clone();
        if !extensions.iter().any(|it| it == "lua") {
            extensions.push("lua".to_string());
        }
        for extension in &extensions {
            let stripped = extension
                .strip_prefix('.')
                .or_else(|| extension.strip_prefix("*."))
                .unwrap_or(extension);
            module_extract_patterns.push(format!("?.{}", stripped));
        }
        if emmyrc.runtime.require_pattern.is_empty() {
            for extension in &extensions {
                let stripped = extension
                    .strip_prefix('.')
                    .or_else(|| extension.strip_prefix("*."))
                    .unwrap_or(extension);
                module_extract_patterns.push(format!("?/init.{}", stripped));
            }
        } else {
            module_extract_patterns.extend(emmyrc.runtime.require_pattern.clone());
        }

        let module_replace_patterns = emmyrc
            .workspace
            .module_map
            .iter()
            .map(|item| (item.pattern.clone(), item.replace.clone()))
            .collect();

        Self {
            language_level: emmyrc.get_language_level(),
            special_like: special_like.into_iter().collect(),
            non_std_symbols,
            module_extract_patterns,
            module_replace_patterns,
        }
    }

    pub fn module_extract_patterns(&self) -> Vec<String> {
        self.module_extract_patterns.clone()
    }

    pub fn module_replace_patterns(&self) -> HashMap<String, String> {
        self.module_replace_patterns.iter().cloned().collect()
    }
}

#[salsa::input]
pub(crate) struct SummarySourceFileInput {
    pub(crate) file_id: FileId,
    pub(crate) path: Option<PathBuf>,
    pub(crate) text: String,
    pub(crate) is_remote: bool,
}

#[salsa::input]
pub(crate) struct SummaryWorkspaceInput {
    pub(crate) workspaces: Vec<Workspace>,
}

#[salsa::input]
pub(crate) struct SummaryConfigInput {
    pub(crate) config: SalsaSummaryConfig,
}

pub(crate) fn file_input(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<SummarySourceFileInput> {
    db.files.get(&file_id).copied()
}

pub(crate) fn workspace_input(db: &SalsaSummaryDatabase) -> Option<SummaryWorkspaceInput> {
    db.workspaces
}

pub(crate) fn config_input(db: &SalsaSummaryDatabase) -> Option<SummaryConfigInput> {
    db.config
}

pub(crate) fn parse_chunk(
    file_id: FileId,
    text: &str,
    config: &SalsaSummaryConfig,
) -> InFiled<LuaChunk> {
    let mut node_cache = NodeCache::default();
    let special_like = config
        .special_like
        .iter()
        .cloned()
        .collect::<std::collections::HashMap<_, _>>();
    let mut non_std_symbols = LuaNonStdSymbolSet::new();
    non_std_symbols.extends(config.non_std_symbols.clone());

    let parse_config = ParserConfig::new(
        config.language_level,
        Some(&mut node_cache),
        special_like,
        non_std_symbols,
        true,
    );
    let tree = LuaParser::parse(text, parse_config);
    InFiled::new(file_id, tree.get_chunk_node())
}

pub(crate) fn snapshot_vfs_file(
    vfs: &Vfs,
    file_id: FileId,
) -> Option<(Option<PathBuf>, String, bool)> {
    let text = vfs.get_file_content(&file_id)?.clone();
    let path = vfs.get_file_path(&file_id).cloned();
    let is_remote = vfs.is_remote_file(&file_id);
    Some((path, text, is_remote))
}
