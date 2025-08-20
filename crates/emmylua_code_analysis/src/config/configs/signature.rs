use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcSignature {
    /// Whether to enable signature help.
    #[serde(default = "default_true")]
    #[schemars(extend("x-vscode-setting" = true))]
    pub detail_signature_helper: bool,
}

impl Default for EmmyrcSignature {
    fn default() -> Self {
        Self {
            detail_signature_helper: default_true(),
        }
    }
}

fn default_true() -> bool {
    true
}
