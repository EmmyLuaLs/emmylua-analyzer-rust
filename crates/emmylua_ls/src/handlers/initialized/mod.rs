mod client_config;
mod codestyle;
mod collect_files;
mod locale;

use std::{path::PathBuf, str::FromStr, sync::Arc};

use crate::{
    cmd_args::CmdArgs,
    context::{
        get_client_id, load_emmy_config, ClientId, ClientProxy, FileDiagnostic, ProgressTask,
        ServerContextSnapshot, StatusBar,
    },
    handlers::text_document::register_files_watch,
    logger::init_logger,
};
pub use client_config::{get_client_config, ClientConfig};
use codestyle::load_editorconfig;
use collect_files::collect_files;
use emmylua_code_analysis::{uri_to_file_path, EmmyLuaAnalysis, Emmyrc};
use log::info;
use lsp_types::InitializeParams;
use tokio::sync::RwLock;

pub async fn initialized_handler(
    context: ServerContextSnapshot,
    params: InitializeParams,
    cmd_args: CmdArgs,
) -> Option<()> {
    let client_id = get_client_id(&params.client_info);
    let client_config = get_client_config(&context, client_id).await;
    let workspace_folders = get_workspace_folders(&params);
    let main_root: Option<&str> = match workspace_folders.first() {
        Some(path) => path.to_str(),
        None => None,
    };
    // init locale
    locale::set_ls_locale(&params);

    // init logger
    init_logger(main_root, &cmd_args);
    info!("client_id: {:?}", client_id);
    let params_json = serde_json::to_string_pretty(&params).unwrap();
    info!("initialization_params: {}", params_json);

    // init config
    // todo! support multi config
    let config_root: Option<PathBuf> = match main_root {
        Some(root) => Some(PathBuf::from(root)),
        None => None,
    };

    let emmyrc = load_emmy_config(config_root, client_config.clone());
    load_editorconfig(workspace_folders.clone());

    let mut workspace_manager = context.workspace_manager.write().await;
    workspace_manager.workspace_folders = workspace_folders.clone();
    workspace_manager.client_config = client_config.clone();
    drop(workspace_manager);
    // let file_diagnostic = context.file_diagnostic.clone();

    init_analysis(
        context.analysis.clone(),
        context.client.clone(),
        &context.status_bar,
        workspace_folders,
        emmyrc,
        client_id,
        context.file_diagnostic.clone(),
    )
    .await;

    register_files_watch(context.clone(), &params.capabilities).await;
    Some(())
}

pub async fn init_analysis(
    analysis: Arc<RwLock<EmmyLuaAnalysis>>,
    _: Arc<ClientProxy>,
    status_bar: &StatusBar,
    workspace_folders: Vec<PathBuf>,
    emmyrc: Arc<Emmyrc>,
    client_id: ClientId,
    file_diagnostic: Arc<FileDiagnostic>,
) {
    let mut mut_analysis = analysis.write().await;
    // update config
    mut_analysis.update_config(emmyrc.clone());

    let emmyrc_json = serde_json::to_string_pretty(emmyrc.as_ref()).unwrap();
    info!("current config : {}", emmyrc_json);

    status_bar.create_progress_task(client_id, ProgressTask::LoadWorkspace);
    status_bar.update_progress_task(
        client_id,
        ProgressTask::LoadWorkspace,
        None,
        Some("Loading workspace files".to_string()),
    );

    let mut workspace_folders = workspace_folders;
    for workspace_root in &workspace_folders {
        info!("add workspace root: {:?}", workspace_root);
        mut_analysis.add_main_workspace(workspace_root.clone());
    }

    for workspace_root in &emmyrc.workspace.workspace_roots {
        info!("add workspace root: {:?}", workspace_root);
        mut_analysis.add_main_workspace(PathBuf::from_str(workspace_root).unwrap());
    }

    for lib in &emmyrc.workspace.library {
        info!("add library: {:?}", lib);
        mut_analysis.add_library_workspace(PathBuf::from_str(lib).unwrap());
        workspace_folders.push(PathBuf::from_str(lib).unwrap());
    }

    status_bar.update_progress_task(
        client_id,
        ProgressTask::LoadWorkspace,
        None,
        Some(String::from("Collecting files")),
    );

    // load files
    let files = collect_files(&workspace_folders, &emmyrc);
    let files: Vec<(PathBuf, Option<String>)> =
        files.into_iter().map(|file| file.into_tuple()).collect();

    let file_count = files.len();
    status_bar.update_progress_task(
        client_id,
        ProgressTask::LoadWorkspace,
        None,
        Some(format!("Indexing {} files", file_count)),
    );

    mut_analysis.update_files_by_path(files);

    status_bar.finish_progress_task(
        client_id,
        ProgressTask::LoadWorkspace,
        Some(String::from("Finished loading workspace files")),
    );

    drop(mut_analysis);

    file_diagnostic.add_workspace_diagnostic_task(client_id, 0).await;
}

fn get_workspace_folders(params: &InitializeParams) -> Vec<PathBuf> {
    let mut workspace_folders = Vec::new();
    if let Some(workspaces) = &params.workspace_folders {
        for workspace in workspaces {
            if let Some(path) = uri_to_file_path(&workspace.uri) {
                workspace_folders.push(path);
            }
        }
    }

    if workspace_folders.is_empty() {
        // However, most LSP clients still provide this field
        #[allow(deprecated)]
        if let Some(uri) = &params.root_uri {
            let root_workspace = uri_to_file_path(&uri);
            if let Some(path) = root_workspace {
                workspace_folders.push(path);
            }
        }
    }

    workspace_folders
}
