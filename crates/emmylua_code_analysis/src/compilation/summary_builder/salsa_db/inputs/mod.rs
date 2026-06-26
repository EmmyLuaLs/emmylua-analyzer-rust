use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use emmylua_parser::{
    LuaChunk, LuaLanguageLevel, LuaNonStdSymbol, LuaNonStdSymbolSet, LuaParser, ParserConfig,
    SpecialFunction,
};
use rowan::NodeCache;

use crate::{Emmyrc, FileId, InFiled};

use super::{SalsaSummaryDatabase, SummaryDb};

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

    /// Build a `ParserConfig` from this summary config.
    pub fn to_parse_config<'a>(&self, node_cache: &'a mut NodeCache) -> ParserConfig<'a> {
        let special_like = self
            .special_like
            .iter()
            .cloned()
            .collect::<std::collections::HashMap<_, _>>();
        let mut non_std_symbols = LuaNonStdSymbolSet::new();
        non_std_symbols.extends(self.non_std_symbols.clone());

        ParserConfig::new(
            self.language_level,
            Some(node_cache),
            special_like,
            non_std_symbols,
            true,
        )
    }
}

#[salsa::input]
pub(crate) struct SummarySourceFileInput {
    pub(crate) file_id: FileId,
    pub(crate) path: Option<PathBuf>,

    #[returns(ref)]
    pub(crate) text: String,

    pub(crate) is_remote: bool,
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

pub(crate) fn config_input(db: &SalsaSummaryDatabase) -> Option<SummaryConfigInput> {
    db.config
}

/// Parse a file into a `LuaChunk`, using the syntax tree cache on the database
/// to avoid redundant parsing across tracked functions.
///
/// Cache is populated by `set_file` (`&mut self`). This function only reads,
/// so no lock needed. If the cache is cold (shouldn't happen in normal flow),
/// we parse on the fly without caching.
pub(crate) fn parse_chunk(
    db: &dyn SummaryDb,
    file_id: FileId,
    text: &str,
    config: &SalsaSummaryConfig,
) -> InFiled<LuaChunk> {
    if let Some(tree) = db.lookup_syntax_tree(file_id) {
        return InFiled::new(file_id, tree.get_chunk_node());
    }
    // Cache miss — parse inline (正常流程不应到达这里，set_file 已预缓存)
    let mut node_cache = NodeCache::default();
    let parse_config = config.to_parse_config(&mut node_cache);
    let tree = LuaParser::parse(text, parse_config);
    InFiled::new(file_id, tree.get_chunk_node())
}
