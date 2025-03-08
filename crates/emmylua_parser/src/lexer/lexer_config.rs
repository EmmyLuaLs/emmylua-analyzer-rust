use crate::kind::LuaLanguageLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexerConfig {
    pub language_level: LuaLanguageLevel,
}

impl LexerConfig {
    pub fn support_goto(&self) -> bool {
        matches!(
            self.language_level,
            LuaLanguageLevel::Lua52
                | LuaLanguageLevel::Lua53
                | LuaLanguageLevel::Lua54
                | LuaLanguageLevel::LuaJIT
        )
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
        matches!(
            self.language_level,
            LuaLanguageLevel::Lua53 | LuaLanguageLevel::Lua54
        )
    }

    pub fn support_pow_operator(&self) -> bool {
        matches!(
            self.language_level,
            LuaLanguageLevel::Lua53 | LuaLanguageLevel::Lua54 | LuaLanguageLevel::LuaJIT
        )
    }
}

impl Default for LexerConfig {
    fn default() -> Self {
        LexerConfig {
            language_level: LuaLanguageLevel::Lua54,
        }
    }
}
