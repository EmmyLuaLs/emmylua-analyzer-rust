mod client_config;
mod codestyle;
mod collect_files;
mod locale;

use std::{path::PathBuf, str::FromStr, sync::Arc};

use crate::{
    cmd_args::CmdArgs,
    context::{
        FileDiagnostic, LspFeatures, ProgressTask, ServerContextSnapshot, StatusBar,
        WorkspaceFileMatcher, get_client_id, load_emmy_config,
    },
    handlers::{
        initialized::collect_files::calculate_include_and_exclude,
        text_document::register_files_watch,
    },
    logger::init_logger,
};
pub use client_config::{ClientConfig, get_client_config};
use codestyle::load_editorconfig;
use collect_files::collect_files;
use emmylua_code_analysis::{EmmyLuaAnalysis, Emmyrc, file_path_to_uri, uri_to_file_path};
use lsp_types::{InitializeParams, Uri};
use tokio::sync::RwLock;

pub async fn initialized_handler(
    context: ServerContextSnapshot,
    params: InitializeParams,
    cmd_args: CmdArgs,
) -> Option<()> {
    // init locale
    locale::set_ls_locale(&params);
    let workspace_folders = get_workspace_folders(&params);
    let main_root: Option<&str> = match workspace_folders.first() {
        Some(path) => path.to_str(),
        None => None,
    };

    // init logger
    init_logger(main_root, &cmd_args);
    log::info!("main root: {:?}", main_root);

    let client_id = if let Some(editor) = &cmd_args.editor {
        editor.clone().into()
    } else {
        get_client_id(&params.client_info)
    };
    let supports_config_request = params
        .capabilities
        .workspace
        .as_ref()?
        .configuration
        .unwrap_or_default();
    log::info!("client_id: {:?}", client_id);

    {
        log::info!("set workspace folders: {:?}", workspace_folders);
        let mut workspace_manager = context.workspace_manager().write().await;
        workspace_manager.workspace_folders = workspace_folders.clone();
        log::info!("workspace folders set");
    }

    let client_config = get_client_config(&context, client_id, supports_config_request).await;
    log::info!("client_config: {:?}", client_config);

    let params_json = serde_json::to_string_pretty(&params).unwrap();
    log::info!("initialization_params: {}", params_json);

    // init config
    // todo! support multi config
    let config_root: Option<PathBuf> = main_root.map(PathBuf::from);

    let emmyrc = load_emmy_config(config_root, client_config.clone());
    load_editorconfig(workspace_folders.clone());

    // init std lib
    init_std_lib(context.analysis(), &cmd_args, emmyrc.clone()).await;

    {
        let mut workspace_manager = context.workspace_manager().write().await;
        workspace_manager.client_config = client_config.clone();
        let (include, exclude, exclude_dir) = calculate_include_and_exclude(&emmyrc);
        workspace_manager.match_file_pattern =
            WorkspaceFileMatcher::new(include, exclude, exclude_dir);
        log::info!("workspace manager updated with client config and watch file patterns")
    }

    init_analysis(
        context.analysis(),
        context.status_bar(),
        context.file_diagnostic(),
        context.lsp_features(),
        workspace_folders,
        emmyrc.clone(),
    )
    .await;

    register_files_watch(context.clone(), &params.capabilities).await;
    Some(())
}

pub async fn init_analysis(
    analysis: &RwLock<EmmyLuaAnalysis>,
    status_bar: &StatusBar,
    file_diagnostic: &FileDiagnostic,
    lsp_features: &LspFeatures,
    workspace_folders: Vec<PathBuf>,
    emmyrc: Arc<Emmyrc>,
) {
    let mut mut_analysis = analysis.write().await;

    // update config
    mut_analysis.update_config(emmyrc.clone());

    if let Ok(emmyrc_json) = serde_json::to_string_pretty(emmyrc.as_ref()) {
        log::info!("current config : {}", emmyrc_json);
    }

    status_bar
        .create_progress_task(ProgressTask::LoadWorkspace)
        .await;
    status_bar.update_progress_task(
        ProgressTask::LoadWorkspace,
        None,
        Some("Loading workspace files".to_string()),
    );

    let mut workspace_folders = workspace_folders;
    for workspace_root in &workspace_folders {
        log::info!("add workspace root: {:?}", workspace_root);
        mut_analysis.add_main_workspace(workspace_root.clone());
    }

    for workspace_root in &emmyrc.workspace.workspace_roots {
        log::info!("add workspace root: {:?}", workspace_root);
        mut_analysis.add_main_workspace(PathBuf::from_str(workspace_root).unwrap());
    }

    for lib in &emmyrc.workspace.library {
        log::info!("add library: {:?}", lib);
        mut_analysis.add_library_workspace(PathBuf::from_str(lib).unwrap());
        workspace_folders.push(PathBuf::from_str(lib).unwrap());
    }

    status_bar.update_progress_task(
        ProgressTask::LoadWorkspace,
        None,
        Some(String::from("Collecting files")),
    );

    // load files
    let files: Vec<emmylua_code_analysis::LuaFileInfo> = collect_files(&workspace_folders, &emmyrc);
    let files: Vec<(Uri, Option<String>)> = files
        .into_iter()
        .filter_map(|file| {
            let (path, text) = file.into_tuple();
            let uri = file_path_to_uri(&path)?;
            Some((uri, text))
        })
        .collect();
    let file_count = files.len();
    if file_count != 0 {
        mut_analysis.update_files_by_uri_with_progress(files, |idx, total_count| {
            let percentage = (idx as u32) * 100 / (total_count as u32) + 1;
            status_bar.update_progress_task(
                ProgressTask::LoadWorkspace,
                Some(percentage.clamp(0, 100)),
                Some(format!("Indexing {} files", total_count)),
            );
        });
    }

    status_bar.update_progress_task(
        ProgressTask::LoadWorkspace,
        None,
        Some(String::from("Finished loading workspace files")),
    );
    status_bar.finish_progress_task(
        ProgressTask::LoadWorkspace,
        Some("Indexing complete".to_string()),
    );

    drop(mut_analysis);

    if !lsp_features.supports_workspace_diagnostic() {
        file_diagnostic
            .add_workspace_diagnostic_task(0, false)
            .await;
    }
}

pub fn get_workspace_folders(params: &InitializeParams) -> Vec<PathBuf> {
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
            let root_workspace = uri_to_file_path(uri);
            if let Some(path) = root_workspace {
                workspace_folders.push(path);
            }
        }
    }

    workspace_folders
}

pub async fn init_std_lib(
    analysis: &RwLock<EmmyLuaAnalysis>,
    cmd_args: &CmdArgs,
    emmyrc: Arc<Emmyrc>,
) {
    log::info!(
        "initializing std lib with resources path: {:?}",
        cmd_args.resources_path
    );
    let mut analysis = analysis.write().await;
    if cmd_args.load_stdlib.0 {
        // double update config
        analysis.update_config(emmyrc);
        analysis.init_std_lib(cmd_args.resources_path.0.clone());
    }

    log::info!("initialized std lib complete");
}
