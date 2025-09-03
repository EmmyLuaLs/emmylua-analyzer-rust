use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcReformat {
    /// Configuration for external formatting tool.
    #[serde(default)]
    pub external_tool: Option<EmmyrcExternalTool>,

    /// Whether to use the diff algorithm for formatting.
    #[serde(default = "default_false")]
    pub use_diff: bool,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcExternalTool {
    /// The command to run the external tool.
    #[serde(default)]
    pub program: String,

    /// List of arguments to pass to the external tool.
    ///
    /// Each argument can contain the following patterns:
    ///
    /// - `${file}` expands to the file path that needs formatting;
    ///
    /// - `${indent_size}` expands to numeric value for indentation size;
    ///
    /// - `${use_tabs?<on_yes>:<on_no>}` expands to `<on_yes>` placeholder or
    ///   `<on_no>` placeholder depending on whether tabs are used
    ///   for indentation.
    ///
    ///   For example, `${use_tabs?--tabs}` will expand to `--tabs` if tabs
    ///   are required, or an empty string if tabs are not required.
    ///
    /// - `${insert_final_newline?<on_yes>:<on_no>}` expands to `<on_yes>`
    ///   placeholder or `<on_no>` placeholder depending on whether the tool
    ///   should insert final newline.
    ///
    /// - `${non_standard_symbol?<on_yes>:<on_no>}` expands to `<on_yes>`
    ///   placeholder or `<on_no>` placeholder depending on whether
    ///   non-standard symbols are enabled.
    #[serde(default)]
    pub args: Vec<String>,

    /// Command timeout, in milliseconds.
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 {
    5000
}

fn default_false() -> bool {
    false
}
