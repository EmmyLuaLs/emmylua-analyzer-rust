use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum LuaLanguageLevel {
    Lua51,
    LuaJIT2,
    LuaJIT,
    LuaJIT3,
    Lua52,
    Lua53,
    Lua54,
    #[default]
    Lua55,
}

impl fmt::Display for LuaLanguageLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LuaLanguageLevel::Lua51 => write!(f, "Lua 5.1"),
            LuaLanguageLevel::Lua52 => write!(f, "Lua 5.2"),
            LuaLanguageLevel::Lua53 => write!(f, "Lua 5.3"),
            LuaLanguageLevel::Lua54 => write!(f, "Lua 5.4"),
            LuaLanguageLevel::LuaJIT2 => write!(f, "LuaJIT2"),
            LuaLanguageLevel::LuaJIT => write!(f, "LuaJIT"),
            LuaLanguageLevel::LuaJIT3 => write!(f, "LuaJIT3"),
            LuaLanguageLevel::Lua55 => write!(f, "Lua 5.5"),
        }
    }
}
