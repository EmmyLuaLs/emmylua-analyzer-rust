use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DefaultOnError};

#[serde_as]
#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
/// Configuration for EmmyLua code completion.
pub struct EmmyrcCompletion {
    /// Whether to enable code completion.
    #[serde(default = "default_true")]
    pub enable: bool,
    /// Whether to automatically require modules.
    #[serde(default = "default_true")]
    pub auto_require: bool,
    /// The function used for auto-requiring modules.
    #[serde(default = "default_require_function")]
    pub auto_require_function: String,
    /// The naming convention for auto-required filenames.
    #[serde(default)]
    pub auto_require_naming_convention: EmmyrcFilenameConvention,
    /// A separator used in auto-require paths.
    #[serde(default = "default_auto_require_separator")]
    pub auto_require_separator: String,
    /// Whether to use call snippets in completions.
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub call_snippet: bool,
    /// The postfix trigger used in completions.
    #[serde(default = "default_postfix")]
    pub postfix: String,
    /// Whether to include the name in the base function completion. effect: `function () end` -> `function name() end`.
    #[serde(default = "default_true")]
    pub base_function_includes_name: bool,
}

impl Default for EmmyrcCompletion {
    fn default() -> Self {
        Self {
            enable: default_true(),
            auto_require: default_true(),
            auto_require_function: default_require_function(),
            auto_require_naming_convention: Default::default(),
            call_snippet: false,
            auto_require_separator: default_auto_require_separator(),
            postfix: default_postfix(),
            base_function_includes_name: default_true(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_require_function() -> String {
    "require".to_string()
}

fn default_postfix() -> String {
    "@".to_string()
}

fn default_auto_require_separator() -> String {
    ".".to_string()
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum EmmyrcFilenameConvention {
    /// Keep the original filename.
    Keep,
    /// Convert the filename to snake_case.
    SnakeCase,
    /// Convert the filename to PascalCase.
    PascalCase,
    /// Convert the filename to camelCase.
    CamelCase,
}

impl Default for EmmyrcFilenameConvention {
    fn default() -> Self {
        EmmyrcFilenameConvention::Keep
    }
}
