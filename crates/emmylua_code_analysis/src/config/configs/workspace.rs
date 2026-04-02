use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcWorkspace {
    /// Ignore directories.
    #[serde(default)]
    pub ignore_dir: Vec<String>,
    /// Ignore globs. eg: ["**/*.lua"]
    #[serde(default)]
    pub ignore_globs: Vec<String>,
    #[serde(default)]
    /// Library paths. Can be a string path or an object with path and ignore rules.
    /// eg: ["/usr/local/share/lua/5.1"] or [{"path": "/usr/local/share/lua/5.1", "ignoreDir": ["test"], "ignoreGlobs": ["**/*.spec.lua"]}]
    pub library: Vec<EmmyrcWorkspacePathItem>,
    #[serde(default)]
    /// Package directories. Can be a string path or an object with path and ignore rules.
    /// Treat the parent directory as a `library`, but only add files from the specified directory.
    /// eg: ["/usr/local/share/lua/5.1/module"] or [{"path": "/usr/local/share/lua/5.1/module", "ignoreDir": ["test"], "ignoreGlobs": ["**/*.spec.lua"]}]
    pub packages: Vec<EmmyrcWorkspacePathItem>,
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
            packages: Vec::new(),
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
    pub pattern: String,
    pub replace: String,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Hash, PartialEq, Eq)]
#[serde(untagged)]
pub enum EmmyrcWorkspacePathItem {
    /// Simple workspace entry path string
    Path(String),
    /// Workspace entry configuration with path and ignore rules
    Config(EmmyrcWorkspacePathConfig),
}

impl EmmyrcWorkspacePathItem {
    pub fn get_path(&self) -> &String {
        match self {
            EmmyrcWorkspacePathItem::Path(p) => p,
            EmmyrcWorkspacePathItem::Config(c) => &c.path,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Hash, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcWorkspacePathConfig {
    /// Workspace entry path
    pub path: String,
    /// Ignore directories within this entry
    #[serde(default)]
    pub ignore_dir: Vec<String>,
    /// Ignore globs within this entry. eg: ["**/*.lua"]
    #[serde(default)]
    pub ignore_globs: Vec<String>,
}

#[doc(hidden)]
pub type EmmyLibraryItem = EmmyrcWorkspacePathItem;

#[doc(hidden)]
pub type EmmyLibraryConfig = EmmyrcWorkspacePathConfig;

fn encoding_default() -> String {
    "utf-8".to_string()
}

fn reindex_duration_default() -> u64 {
    5000
}

fn enable_reindex_default() -> bool {
    false
}
