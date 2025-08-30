use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcResource {
    /// List of resource directories used in a project. Files from these
    /// directories will be added to autocompletion when completing
    /// file paths.
    ///
    /// This list can contain anything, like directories with game assets,
    /// template files, and so on. No special interpretation beyond
    /// autocompletion is given to these paths.
    #[serde(default)]
    pub paths: Vec<String>,
}
