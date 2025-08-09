use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcHover {
    /// Enable showing documentation on hover.
    #[serde(default = "default_true")]
    #[schemars(extend("x-vscode-setting" = true))]
    pub enable: bool,

    /// Increase verbosity of types shown in hovers.
    ///
    /// Enabling this option will increase maximum nesting of displayed types,
    /// add alias expansions on top level, increase number of shown members.
    #[serde(default)]
    #[schemars(extend("x-vscode-setting" = true))]
    pub verbose: bool,
}

impl Default for EmmyrcHover {
    fn default() -> Self {
        Self {
            enable: default_true(),
            verbose: false,
        }
    }
}

fn default_true() -> bool {
    true
}
