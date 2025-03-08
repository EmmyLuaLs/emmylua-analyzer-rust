use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcCodeLen {
    /// Whether to enable code lens.
    #[serde(default = "default_true")]
    pub enable: bool,
}

impl Default for EmmyrcCodeLen {
    fn default() -> Self {
        Self {
            enable: default_true(),
        }
    }
}

fn default_true() -> bool {
    true
}
