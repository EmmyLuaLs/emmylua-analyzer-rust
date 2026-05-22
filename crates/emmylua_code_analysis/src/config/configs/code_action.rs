use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcCodeAction {
    /// Add space after `---` comments when inserting `@diagnostic disable-next-line`.
    ///
    /// When omitted, this follows the formatter's resolved
    /// `emmy_doc.space_between_tag_columns` setting.
    #[serde(default)]
    #[schemars(extend("x-vscode-setting" = true))]
    pub insert_space: Option<bool>,
}
