use emmylua_parser::LuaVersionNumber;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcRuntime {
    /// Lua version.
    #[serde(default)]
    pub version: EmmyrcLuaVersion,
    #[serde(default)]
    /// Functions that like require.
    pub require_like_function: Vec<String>,
    #[serde(default)]
    /// Framework versions.
    pub framework_versions: Vec<String>,
    #[serde(default)]
    /// file Extensions. eg: .lua, .lua.txt
    pub extensions: Vec<String>,
    #[serde(default)]
    /// Require pattern. eg. "?.lua", "?/init.lua"
    pub require_pattern: Vec<String>,
}

impl Default for EmmyrcRuntime {
    fn default() -> Self {
        Self {
            version: Default::default(),
            require_like_function: Default::default(),
            framework_versions: Default::default(),
            extensions: Default::default(),
            require_pattern: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Copy, PartialEq, Eq)]
pub enum EmmyrcLuaVersion {
    /// Lua 5.1
    #[serde(rename = "Lua5.1", alias = "Lua 5.1")]
    Lua51,
    /// LuaJIT
    #[serde(rename = "LuaJIT")]
    LuaJIT,
    /// Lua 5.2
    #[serde(rename = "Lua5.2", alias = "Lua 5.2")]
    Lua52,
    /// Lua 5.3
    #[serde(rename = "Lua5.3", alias = "Lua 5.3")]
    Lua53,
    /// Lua 5.4
    #[serde(rename = "Lua5.4", alias = "Lua 5.4")]
    Lua54,
    /// Lua Latest
    #[serde(rename = "LuaLatest", alias = "Lua Latest")]
    LuaLatest,
}

impl Default for EmmyrcLuaVersion {
    fn default() -> Self {
        EmmyrcLuaVersion::LuaLatest
    }
}

impl EmmyrcLuaVersion {
    pub fn to_lua_version_number(&self) -> LuaVersionNumber {
        match self {
            EmmyrcLuaVersion::Lua51 => LuaVersionNumber::new(5, 1, 0),
            EmmyrcLuaVersion::LuaJIT => LuaVersionNumber::LUA_JIT,
            EmmyrcLuaVersion::Lua52 => LuaVersionNumber::new(5, 2, 0),
            EmmyrcLuaVersion::Lua53 => LuaVersionNumber::new(5, 3, 0),
            EmmyrcLuaVersion::Lua54 => LuaVersionNumber::new(5, 4, 0),
            EmmyrcLuaVersion::LuaLatest => LuaVersionNumber::new(5, 4, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emmyrc_runtime() {
        let json1 = r#"{
            "version": "Lua5.1"
        }"#;
        let runtime: EmmyrcRuntime = serde_json::from_str(json1).unwrap();
        assert_eq!(runtime.version, EmmyrcLuaVersion::Lua51);

        let json2 = r#"{
            "version": "Lua 5.1"
        }"#;

        let runtime: EmmyrcRuntime = serde_json::from_str(json2).unwrap();
        assert_eq!(runtime.version, EmmyrcLuaVersion::Lua51);
    }
}
