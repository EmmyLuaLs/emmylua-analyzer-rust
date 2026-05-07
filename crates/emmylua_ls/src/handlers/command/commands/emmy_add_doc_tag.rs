use std::{fs::OpenOptions, io::Write};

use emmylua_code_analysis::load_config_json_unprocessed;
use lsp_types::Command;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::context::{ServerContextSnapshot, WorkspaceManager};

use super::CommandSpec;

pub struct AddDocTagCommand;

impl CommandSpec for AddDocTagCommand {
    const COMMAND: &str = "emmy.add.doctag";

    async fn handle(context: ServerContextSnapshot, args: Vec<Value>) -> Option<()> {
        let tag_name: String = serde_json::from_value(args.first()?.clone()).ok()?;
        add_doc_tag(context.workspace_manager(), tag_name).await;
        Some(())
    }
}

pub fn make_auto_doc_tag_command(title: &str, tag_name: &str) -> Command {
    let args = vec![serde_json::to_value(tag_name).unwrap()];

    Command {
        title: title.to_string(),
        command: AddDocTagCommand::COMMAND.to_string(),
        arguments: Some(args),
    }
}

async fn add_doc_tag(workspace_manager: &RwLock<WorkspaceManager>, tag_name: String) -> Option<()> {
    let workspace_manager = workspace_manager.read().await;
    let main_workspace = workspace_manager.workspace_folders.first()?;
    let emmyrc_path = main_workspace.root.join(".emmyrc.json");
    let mut emmyrc = load_config_json_unprocessed(emmyrc_path.clone());
    drop(workspace_manager);

    emmyrc
        .as_object_mut()?
        .entry("doc")
        .or_insert_with(|| Value::Object(Default::default()))
        .as_object_mut()?
        .entry("knownTags")
        .or_insert_with(|| Value::Array(Default::default()))
        .as_array_mut()?
        .push(Value::String(tag_name));

    let emmyrc_json = serde_json::to_string_pretty(&emmyrc).ok()?;
    if let Ok(mut file) = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&emmyrc_path)
    {
        if let Err(err) = file.write_all(emmyrc_json.as_bytes()) {
            log::error!("write emmyrc file failed: {:?}", err);
            return None;
        }
    } else {
        log::error!("Failed to open/create emmyrc file: {:?}", emmyrc_path);
        return None;
    }

    Some(())
}
