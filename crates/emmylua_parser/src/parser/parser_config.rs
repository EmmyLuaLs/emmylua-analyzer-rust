use std::collections::HashMap;

use rowan::NodeCache;

use crate::{LuaNonStdSymbolSet, kind::LuaLanguageLevel, lexer::LexerConfig};

pub struct ParserConfig<'cache> {
    pub level: LuaLanguageLevel,
    lexer_config: LexerConfig,
    node_cache: Option<&'cache mut NodeCache>,
    special_like: HashMap<String, SpecialFunction>,
    pub enable_emmylua_doc: bool,
}

impl<'cache> ParserConfig<'cache> {
    pub fn new(
        level: LuaLanguageLevel,
        node_cache: Option<&'cache mut NodeCache>,
        special_like: HashMap<String, SpecialFunction>,
        non_std_symbols: LuaNonStdSymbolSet,
        enable_emmylua_doc: bool,
    ) -> Self {
        Self {
            level,
            lexer_config: LexerConfig {
                language_level: level,
                non_std_symbols,
            },
            node_cache,
            special_like,
            enable_emmylua_doc,
        }
    }

    pub fn lexer_config(&self) -> LexerConfig {
        self.lexer_config
    }

    pub fn support_local_attrib(&self) -> bool {
        self.level >= LuaLanguageLevel::Lua54
    }

    pub fn support_emmylua_doc(&self) -> bool {
        self.enable_emmylua_doc
    }

    pub fn support_named_var_args(&self) -> bool {
        self.level >= LuaLanguageLevel::Lua55
    }

    pub fn node_cache(&mut self) -> Option<&mut NodeCache> {
        self.node_cache.as_deref_mut()
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
            lexer_config: LexerConfig {
                language_level: level,
                non_std_symbols: LuaNonStdSymbolSet::new(),
            },
            node_cache: None,
            special_like: HashMap::new(),
            enable_emmylua_doc: true,
        }
    }
}

impl Default for ParserConfig<'_> {
    fn default() -> Self {
        Self {
            level: LuaLanguageLevel::Lua54,
            lexer_config: LexerConfig {
                language_level: LuaLanguageLevel::Lua54,
                non_std_symbols: LuaNonStdSymbolSet::new(),
            },
            node_cache: None,
            special_like: HashMap::new(),
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
