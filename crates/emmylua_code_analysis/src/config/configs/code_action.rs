use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcCodeAction {
    /// Add space after `---` comments when inserting `@diagnostic disable-next-line`.
    #[serde(default = "default_false")]
    pub insert_space: bool,
}

impl Default for EmmyrcCodeAction {
    fn default() -> Self {
        Self {
            insert_space: default_false(),
        }
    }
}

fn default_false() -> bool {
    false
}
