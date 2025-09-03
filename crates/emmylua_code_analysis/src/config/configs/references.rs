use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcReference {
    /// Enable searching for symbol usages.
    #[serde(default = "default_true")]
    #[schemars(extend("x-vscode-setting" = true))]
    pub enable: bool,

    /// Use fuzzy search when searching for symbol usages
    /// and normal search didn't find anything.
    #[serde(default = "default_true")]
    #[schemars(extend("x-vscode-setting" = true))]
    pub fuzzy_search: bool,

    /// Also search for usages in strings.
    #[serde(default = "default_false")]
    #[schemars(extend("x-vscode-setting" = true))]
    pub short_string_search: bool,
}

impl Default for EmmyrcReference {
    fn default() -> Self {
        Self {
            enable: default_true(),
            fuzzy_search: default_true(),
            short_string_search: default_false(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}
