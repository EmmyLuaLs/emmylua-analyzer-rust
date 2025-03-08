use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LuaLanguageLevel {
    Lua51,
    Lua52,
    Lua53,
    Lua54,
    LuaJIT,
}

impl fmt::Display for LuaLanguageLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LuaLanguageLevel::Lua51 => write!(f, "Lua 5.1"),
            LuaLanguageLevel::Lua52 => write!(f, "Lua 5.2"),
            LuaLanguageLevel::Lua53 => write!(f, "Lua 5.3"),
            LuaLanguageLevel::Lua54 => write!(f, "Lua 5.4"),
            LuaLanguageLevel::LuaJIT => write!(f, "LuaJIT"),
        }
    }
}
