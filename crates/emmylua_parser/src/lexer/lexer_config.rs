use crate::{LuaNonStdSymbolSet, kind::LuaLanguageLevel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexerConfig {
    pub language_level: LuaLanguageLevel,
    pub non_std_symbols: LuaNonStdSymbolSet,
}

impl LexerConfig {
    pub fn support_goto(&self) -> bool {
        self.language_level >= LuaLanguageLevel::Lua52
            || self.language_level == LuaLanguageLevel::LuaJIT
    }

    pub fn support_complex_number(&self) -> bool {
        matches!(self.language_level, LuaLanguageLevel::LuaJIT)
    }

    pub fn support_ll_integer(&self) -> bool {
        matches!(self.language_level, LuaLanguageLevel::LuaJIT)
    }

    pub fn support_binary_integer(&self) -> bool {
        matches!(self.language_level, LuaLanguageLevel::LuaJIT)
    }

    pub fn support_integer_operation(&self) -> bool {
        self.language_level >= LuaLanguageLevel::Lua53
    }

    pub fn support_global_decl(&self) -> bool {
        self.language_level >= LuaLanguageLevel::Lua55
    }
}

impl Default for LexerConfig {
    fn default() -> Self {
        LexerConfig {
            language_level: LuaLanguageLevel::Lua54,
            non_std_symbols: LuaNonStdSymbolSet::new(),
        }
    }
}
