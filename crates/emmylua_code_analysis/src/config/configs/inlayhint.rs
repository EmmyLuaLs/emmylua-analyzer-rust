use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcInlayHint {
    /// Whether to enable inlay hints.
    #[serde(default = "default_true")]
    pub enable: bool,
    /// Whether to enable parameter hints.
    #[serde(default = "default_true")]
    pub param_hint: bool,
    /// Whether to enable index hints.
    #[serde(default = "default_true")]
    pub index_hint: bool,
    /// Whether to enable local hints.
    #[serde(default = "default_true")]
    /// Whether to enable override hints.
    pub local_hint: bool,
    /// Whether to enable override hints.
    #[serde(default = "default_true")]
    pub override_hint: bool,
    /// Whether to enable meta __call operator hints.
    #[serde(default = "default_true")]
    pub meta_call_hint: bool,
    /// Whether to enable enum parameter hints.
    #[serde(default = "default_false")]
    pub enum_param_hint: bool,
}

impl Default for EmmyrcInlayHint {
    fn default() -> Self {
        Self {
            enable: default_true(),
            param_hint: default_true(),
            index_hint: default_true(),
            local_hint: default_true(),
            override_hint: default_true(),
            meta_call_hint: default_true(),
            enum_param_hint: default_false(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}
