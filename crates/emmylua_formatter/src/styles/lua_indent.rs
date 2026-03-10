use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LuaIndent {
    /// Use tabs for indentation
    Tab,
    /// Use spaces for indentation
    Space(usize),
}

impl Default for LuaIndent {
    fn default() -> Self {
        LuaIndent::Space(4)
    }
}
