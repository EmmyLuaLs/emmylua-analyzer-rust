use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

#[allow(dead_code)]
fn default_false() -> bool {
    false
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcStrict {
    /// Whether to enable strict mode require path.
    #[serde(default)]
    pub require_path: bool,
    #[serde(default)]
    pub type_call: bool,
    /// Whether to enable strict mode array indexing.
    #[serde(default = "default_true")]
    pub array_index: bool,
    /// meta define overrides file define
    #[serde(default = "default_true")]
    pub meta_override_file_define: bool,
    /// Base constant types defined in doc can match base types, allowing int to match `---@alias id 1|2|3`, same for string.
    #[serde(default = "default_false")]
    pub doc_base_const_match_base_type: bool,
}

impl Default for EmmyrcStrict {
    fn default() -> Self {
        Self {
            require_path: false,
            type_call: false,
            array_index: true,
            meta_override_file_define: true,
            doc_base_const_match_base_type: true,
        }
    }
}
