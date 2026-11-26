use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct EmmyrcDoc {
    /// Treat specific field names as private, e.g. `m_*` means `XXX.m_id` and `XXX.m_type` are private, witch can only be accessed in the class where the definition is located.
    #[serde(default)]
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

impl EmmyrcDoc {
    /// Whether `name` matches a `private_name` pattern (`m_*`, `*_private` or an exact name).
    pub fn is_private_name(&self, name: &str) -> bool {
        self.private_name.iter().any(|pattern| {
            if let Some(prefix) = pattern.strip_suffix('*') {
                name.starts_with(prefix)
            } else if let Some(suffix) = pattern.strip_prefix('*') {
                name.ends_with(suffix)
            } else {
                name == pattern
            }
        })
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum DocSyntax {
    None,
    #[default]
    Md,
    Myst,
    Rst,
}
