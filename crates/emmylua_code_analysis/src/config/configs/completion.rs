use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{DefaultOnError, serde_as};

#[serde_as]
#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcCompletion {
    /// Enable autocompletion.
    #[serde(default = "default_true")]
    #[schemars(extend("x-vscode-setting" = true))]
    pub enable: bool,

    /// When enabled, selecting a completion suggestion from another
    /// module will add the appropriate require statement.
    #[serde(default = "default_true")]
    #[schemars(extend("x-vscode-setting" = true))]
    pub auto_require: bool,

    /// Name of the function that's inserted when auto-requiring modules.
    ///
    /// Default is `"require"`, but can be customized to use any module loader function.
    #[serde(default = "default_require_function")]
    pub auto_require_function: String,

    /// The naming convention for auto-required filenames.
    ///
    /// Controls how the imported module names are formatted in the `require` statement.
    #[serde(default)]
    pub auto_require_naming_convention: EmmyrcFilenameConvention,

    /// Defines the character used to separate path segments in require statements.
    ///
    /// Default is `"."`, but can be changed to other separators like `"/"`.
    #[serde(default = "default_auto_require_separator")]
    pub auto_require_separator: String,

    /// Whether to use call snippets in completions.
    ///
    /// When enabled, function completions will insert a snippet with placeholders
    /// for function arguments, allowing for quick tabbing between parameters.
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    #[schemars(extend("x-vscode-setting" = true))]
    pub call_snippet: bool,

    /// Symbol that's used to trigger postfix autocompletion.
    #[serde(default = "default_postfix")]
    #[schemars(extend("x-vscode-setting" = {
        "type": ["string", "null"],
        "default": null,
        "enum": [null, "@", ".", ":"],
        "enumItemLabels": ["Default"],
        "markdownEnumDescriptions": ["%config.common.enum.default.description%"],
    }))]
    pub postfix: String,

    /// Whether to include the name in the base function in postfix autocompletion.
    ///
    /// Effect: `function () end` -> `function name() end`.
    #[serde(default = "default_true")]
    #[schemars(extend("x-vscode-setting" = true))]
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

/// The naming convention for auto-required filenames.
///
/// Controls how the imported module names are formatted in the `require` statement.
#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum EmmyrcFilenameConvention {
    /// Keep the original filename without any transformation.
    ///
    /// Example: `"my-module"` remains `"my-module"`.
    Keep,

    /// Convert the filename to `snake_case`.
    ///
    /// Example: `"MyModule"` becomes `"my_module"`.
    SnakeCase,

    /// Convert the filename to `PascalCase`.
    ///
    /// Example: `"my_module"` becomes `"MyModule"`.
    PascalCase,

    /// Convert the filename to `camelCase`.
    ///
    /// Example: `"my_module"` becomes `"myModule"`.
    CamelCase,

    /// When returning a class definition, use the class name; otherwise keep the original name.
    ///
    /// This is useful for modules that export a single class with a name that might differ from the filename.
    KeepClass,
}

impl Default for EmmyrcFilenameConvention {
    fn default() -> Self {
        EmmyrcFilenameConvention::Keep
    }
}
