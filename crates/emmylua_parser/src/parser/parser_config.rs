use std::collections::HashMap;

use rowan::NodeCache;

use crate::{LuaFeatures, LuaFeaturesSet, kind::LuaLanguageLevel, lexer::LexerConfig};

pub struct ParserConfig<'cache> {
    pub level: LuaLanguageLevel,
    lexer_config: LexerConfig,
    node_cache: Option<&'cache mut NodeCache>,
    special_like: HashMap<String, SpecialFunction>,
    /// Whether `special_like` contains a dotted name such as `VFS.Include`.
    has_dotted_special_function: bool,
    pub enable_emmylua_doc: bool,
}

impl<'cache> ParserConfig<'cache> {
    pub fn new(
        level: LuaLanguageLevel,
        node_cache: Option<&'cache mut NodeCache>,
        special_like: HashMap<String, SpecialFunction>,
        ext_features: LuaFeaturesSet,
        enable_emmylua_doc: bool,
    ) -> Self {
        let has_dotted_special_function = special_like.keys().any(|name| name.contains('.'));
        Self {
            level,
            lexer_config: LexerConfig::new_with_extended_features(level, ext_features),
            node_cache,
            special_like,
            has_dotted_special_function,
            enable_emmylua_doc,
        }
    }

    pub fn lexer_config(&self) -> LexerConfig {
        self.lexer_config
    }

    pub fn support(&self, symbol: LuaFeatures) -> bool {
        self.lexer_config.support(symbol)
    }

    pub fn support_emmylua_doc(&self) -> bool {
        self.enable_emmylua_doc
    }

    pub fn node_cache(&mut self) -> Option<&mut NodeCache> {
        self.node_cache.as_deref_mut()
    }

    /// Returns true when a special function is configured with a dotted name
    /// (e.g. `VFS.Include`), in which case the parser needs to track dotted
    /// call paths.
    pub fn has_dotted_special_function(&self) -> bool {
        self.has_dotted_special_function
    }

    pub fn get_special_function(&self, name: &str) -> SpecialFunction {
        match name {
            "require" => SpecialFunction::Require,
            "error" => SpecialFunction::Error,
            "assert" => SpecialFunction::Assert,
            "type" => SpecialFunction::Type,
            "setmetatable" => SpecialFunction::Setmetaatable,
            _ => *self
                .special_like
                .get(name)
                .unwrap_or(&SpecialFunction::None),
        }
    }

    pub fn with_level(level: LuaLanguageLevel) -> Self {
        Self {
            level,
            lexer_config: LexerConfig::new(level),
            node_cache: None,
            special_like: HashMap::new(),
            has_dotted_special_function: false,
            enable_emmylua_doc: true,
        }
    }
}

impl Default for ParserConfig<'_> {
    fn default() -> Self {
        Self {
            level: LuaLanguageLevel::Lua55,
            lexer_config: LexerConfig::new(LuaLanguageLevel::Lua55),
            node_cache: None,
            special_like: HashMap::new(),
            has_dotted_special_function: false,
            enable_emmylua_doc: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialFunction {
    None,
    Require,
    Error,
    Assert,
    Type,
    Setmetaatable,
}
