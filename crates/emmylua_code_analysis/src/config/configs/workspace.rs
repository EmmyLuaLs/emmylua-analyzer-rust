use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcWorkspace {
    /// List of ignored directories.
    #[serde(default)]
    pub ignore_dir: Vec<String>,

    /// List of globs for ignored files.
    #[serde(default)]
    pub ignore_globs: Vec<String>,

    /// List of library roots.
    ///
    /// Example: `"/usr/local/share/lua/5.1"`.
    #[serde(default)]
    pub library: Vec<String>,

    /// List of workspace roots.
    ///
    /// Example: `["src", "test"]`.
    #[serde(default)]
    pub workspace_roots: Vec<String>,

    // unused
    #[serde(default)]
    pub preload_file_size: i32,

    /// File encoding.
    #[serde(default = "encoding_default")]
    pub encoding: String,

    /// Module map. Allows customizing conversion from file paths
    /// to module names and require paths.
    ///
    /// This is a list of objects, each containing a regular expression
    /// and a replace string. When generating module name for a file,
    /// EmmyLua will reverse-match file path with require patterns,
    /// generate an appropriate module name, then run it through these replace
    /// patterns to get the final module name.
    ///
    /// Example:
    ///
    /// ```json
    /// {
    ///     "workspace": {
    ///         "moduleMap": [
    ///             {
    ///                 "pattern": "^_core\\.public\\.(.*)$",
    ///                 "replace": "@core.$1"
    ///             }
    ///         ]
    ///     }
    /// }
    /// ```
    #[serde(default)]
    pub module_map: Vec<EmmyrcWorkspaceModuleMap>,

    /// Delay between changing a file and full project reindex, in milliseconds.
    #[serde(default = "reindex_duration_default")]
    #[schemars(extend("x-vscode-setting" = true))]
    pub reindex_duration: u64,

    /// Enable full project reindex after changing a file.
    #[serde(default = "enable_reindex_default")]
    #[schemars(extend("x-vscode-setting" = true))]
    pub enable_reindex: bool,
}

impl Default for EmmyrcWorkspace {
    fn default() -> Self {
        Self {
            ignore_dir: Vec::new(),
            ignore_globs: Vec::new(),
            library: Vec::new(),
            workspace_roots: Vec::new(),
            preload_file_size: 0,
            encoding: encoding_default(),
            module_map: Vec::new(),
            reindex_duration: 5000,
            enable_reindex: false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
pub struct EmmyrcWorkspaceModuleMap {
    /// Regular expression that will be matched against the generated module
    /// name. See [regex] crate for details about syntax.
    ///
    /// [regex]: https://docs.rs/regex/latest/regex/#syntax
    pub pattern: String,

    /// Replace string. Use `$name` to substitute capturing groups from regex.
    pub replace: String,
}

fn encoding_default() -> String {
    "utf-8".to_string()
}

fn reindex_duration_default() -> u64 {
    5000
}

fn enable_reindex_default() -> bool {
    false
}
