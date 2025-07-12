use std::{fs::OpenOptions, io::Write, sync::Arc};

use emmylua_code_analysis::load_configs;
use lsp_types::Command;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::context::{ServerContextSnapshot, WorkspaceManager};

use super::CommandSpec;

pub struct AddDocTagCommand;

impl CommandSpec for AddDocTagCommand {
    const COMMAND: &str = "emmy.add.doctag";

    async fn handle(context: ServerContextSnapshot, args: Vec<Value>) -> Option<()> {
        let tag_name: String = serde_json::from_value(args.get(0)?.clone()).ok()?;
        add_doc_tag(context.workspace_manager, tag_name).await;
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

async fn add_doc_tag(
    config_manager: Arc<RwLock<WorkspaceManager>>,
    tag_name: String,
) -> Option<()> {
    let config_manager = config_manager.read().await;
    let main_workspace = config_manager.workspace_folders.get(0)?;
    let emmyrc_path = main_workspace.join(".emmyrc.json");
    let mut emmyrc = load_configs(vec![emmyrc_path.clone()], None);
    emmyrc.doc.known_tags.push(tag_name);
    drop(config_manager);

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
