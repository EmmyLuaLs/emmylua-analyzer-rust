use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcDoc {
    #[serde(default)]
    /// Treat specific field names as private, e.g. `m_*` means `XXX.m_id` and `XXX.m_type` are private, witch can only be accessed in the class where the definition is located.
    pub private_name: Vec<String>,

    /// List of known documentation tags.
    #[serde(default)]
    pub known_tags: Vec<String>,
}

impl Default for EmmyrcDoc {
    fn default() -> Self {
        Self {
            private_name: Default::default(),
            known_tags: Default::default(),
        }
    }
}
