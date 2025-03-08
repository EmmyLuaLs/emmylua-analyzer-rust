use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcWorkspace {
    /// Ignore directories.
    #[serde(default)]
    pub ignore_dir: Vec<String>,
    /// Ignore globs. eg: ["**/*.lua"]
    #[serde(default)]
    pub ignore_globs: Vec<String>,
    #[serde(default)]
    /// Library paths. eg: "/usr/local/share/lua/5.1"
    pub library: Vec<String>,
    #[serde(default)]
    /// Workspace roots. eg: ["src", "test"]
    pub workspace_roots: Vec<String>,
    // unused
    #[serde(default)]
    pub preload_file_size: i32,
    /// Encoding. eg: "utf-8"
    #[serde(default = "encoding_default")]
    pub encoding: String,
    /// Module map. key is regex, value is new module regex
    /// eg: {
    ///     "^(.*)$": "module_$1"
    ///     "^lib(.*)$": "script$1"
    /// }
    #[serde(default)]
    pub module_map: Vec<EmmyrcWorkspaceModuleMap>,

    /// when save a file, ls will reindex the workspace after reindex_duration milliseconds.
    #[serde(default = "reindex_duration_default")]
    pub reindex_duration: u64,
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
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct EmmyrcWorkspaceModuleMap {
    pub pattern: String,
    pub replace: String,
}

fn encoding_default() -> String {
    "utf-8".to_string()
}

fn reindex_duration_default() -> u64 {
    5000
}
