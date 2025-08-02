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

    /// Syntax for highlighting documentation.
    #[serde(default)]
    pub syntax: DocSyntax,

    /// When `syntax` is `Myst` or `Rst`, specifies primary domain used
    /// with RST processor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rst_primary_domain: Option<String>,

    /// When `syntax` is `Myst` or `Rst`, specifies default role used
    /// with RST processor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rst_default_role: Option<String>,
}

impl Default for EmmyrcDoc {
    fn default() -> Self {
        Self {
            private_name: Default::default(),
            known_tags: Default::default(),
            syntax: Default::default(),
            rst_primary_domain: None,
            rst_default_role: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum DocSyntax {
    None,
    Md,
    Myst,
    Rst,
}

impl Default for DocSyntax {
    fn default() -> Self {
        DocSyntax::None
    }
}
