use std::collections::HashMap;

use rowan::NodeCache;

use crate::parser::desc_parser::DescParserType;
use crate::{kind::LuaLanguageLevel, lexer::LexerConfig};

pub struct ParserConfig<'cache> {
    pub level: LuaLanguageLevel,
    lexer_config: LexerConfig,
    node_cache: Option<&'cache mut NodeCache>,
    special_like: HashMap<String, SpecialFunction>,
    desc_parser_type: DescParserType,
}

impl<'cache> ParserConfig<'cache> {
    pub fn new(
        level: LuaLanguageLevel,
        node_cache: Option<&'cache mut NodeCache>,
        special_like: HashMap<String, SpecialFunction>,
        desc_parser_type: DescParserType,
    ) -> Self {
        Self {
            level,
            lexer_config: LexerConfig {
                language_level: level,
            },
            node_cache,
            special_like,
            desc_parser_type,
        }
    }

    pub fn lexer_config(&self) -> LexerConfig {
        self.lexer_config
    }

    pub fn support_local_attrib(&self) -> bool {
        self.level >= LuaLanguageLevel::Lua54
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
            "setmetatable" => SpecialFunction::Setmatable,
            _ => *self
                .special_like
                .get(name)
                .unwrap_or(&SpecialFunction::None),
        }
    }

    pub fn desc_parser_type(&self) -> &DescParserType {
        &self.desc_parser_type
    }

    pub fn with_level(level: LuaLanguageLevel) -> Self {
        Self {
            level,
            lexer_config: LexerConfig {
                language_level: level,
                ..Default::default()
            },
            node_cache: None,
            special_like: HashMap::new(),
            desc_parser_type: DescParserType::default(),
        }
    }

    pub fn with_desc_parser_type(desc_parser_type: DescParserType) -> Self {
        Self {
            level: LuaLanguageLevel::Lua54,
            lexer_config: LexerConfig {
                language_level: LuaLanguageLevel::Lua54,
                ..Default::default()
            },
            node_cache: None,
            special_like: HashMap::new(),
            desc_parser_type,
        }
    }
}

impl Default for ParserConfig<'_> {
    fn default() -> Self {
        Self {
            level: LuaLanguageLevel::Lua54,
            lexer_config: LexerConfig {
                language_level: LuaLanguageLevel::Lua54,
                ..Default::default()
            },
            node_cache: None,
            special_like: HashMap::new(),
            desc_parser_type: DescParserType::default(),
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
    Setmatable,
}
