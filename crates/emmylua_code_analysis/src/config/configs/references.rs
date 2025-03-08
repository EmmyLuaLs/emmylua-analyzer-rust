use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcReference {
    /// Whether to enable reference search.
    #[serde(default = "default_true")]
    pub enable: bool,
    /// Determines whether to enable fuzzy searching for fields where references cannot be found.
    #[serde(default = "default_true")]
    pub fuzzy_search: bool,
}

impl Default for EmmyrcReference {
    fn default() -> Self {
        Self {
            enable: default_true(),
            fuzzy_search: default_true(),
        }
    }
}

fn default_true() -> bool {
    true
}
