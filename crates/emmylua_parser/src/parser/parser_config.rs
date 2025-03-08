use rowan::NodeCache;

use crate::{kind::LuaLanguageLevel, lexer::LexerConfig};

pub struct ParserConfig<'cache> {
    pub level: LuaLanguageLevel,
    lexer_config: LexerConfig,
    node_cache: Option<&'cache mut NodeCache>,
}

impl<'cache> ParserConfig<'cache> {
    pub fn new(level: LuaLanguageLevel, node_cache: Option<&'cache mut NodeCache>) -> Self {
        Self {
            level,
            lexer_config: LexerConfig {
                language_level: level,
            },
            node_cache,
        }
    }

    pub fn lexer_config(&self) -> LexerConfig {
        self.lexer_config
    }

    pub fn support_local_attrib(&self) -> bool {
        self.level == LuaLanguageLevel::Lua54
    }

    pub fn node_cache(&mut self) -> Option<&mut NodeCache> {
        self.node_cache.as_deref_mut()
    }
}

impl<'cache> Default for ParserConfig<'cache> {
    fn default() -> Self {
        Self {
            level: LuaLanguageLevel::Lua54,
            lexer_config: LexerConfig {
                language_level: LuaLanguageLevel::Lua54,
            },
            node_cache: None,
        }
    }
}
