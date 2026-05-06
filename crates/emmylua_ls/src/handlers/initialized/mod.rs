mod client_config;
mod locale;
mod std_i18n;

use std::{path::PathBuf, sync::Arc};

use crate::{
    cmd_args::CmdArgs,
    context::{
        AnalysisLock, FileDiagnostic, LspFeatures, ProgressTask, ServerContextSnapshot, StatusBar,
        get_client_id, load_emmy_config,
    },
    handlers::{
        initialized::std_i18n::try_generate_translated_std, text_document::register_files_watch,
    },
    logger::init_logger,
};
pub use client_config::{ClientConfig, get_client_config};
use emmylua_code_analysis::{
    Emmyrc, WorkspaceFolder, build_workspace_folders, collect_workspace_files, uri_to_file_path,
};
use lsp_types::InitializeParams;

pub async fn initialized_handler(
    context: ServerContextSnapshot,
    params: InitializeParams,
    cmd_args: CmdArgs,
) -> Option<()> {
    // init locale
    locale::set_ls_locale(&params);
    let workspace_folders = get_workspace_folders(&params);
    let main_root: Option<&str> = match workspace_folders.first() {
        Some(path) => path.root.to_str(),
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

    // init std lib
    init_std_lib(context.analysis(), &cmd_args, emmyrc.clone()).await;

    {
        let mut workspace_manager = context.workspace_manager().write().await;
        workspace_manager.client_config = client_config.clone();
        workspace_manager.update_match_state(emmyrc.as_ref());
        log::info!("workspace manager updated with client config and watch file patterns")
    }

    init_analysis(
        context.analysis(),
        context.status_bar(),
        context.file_diagnostic(),
        context.lsp_features(),
        workspace_folders,
        emmyrc.clone(),
        Vec::new(),
    )
    .await;

    register_files_watch(context.clone()).await;
    Some(())
}

pub async fn init_analysis(
    analysis: &AnalysisLock,
    status_bar: &StatusBar,
    file_diagnostic: &FileDiagnostic,
    lsp_features: &LspFeatures,
    workspace_folders: Vec<WorkspaceFolder>,
    emmyrc: Arc<Emmyrc>,
    open_files: Vec<(lsp_types::Uri, String)>,
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

    let workspace_folders = build_workspace_folders(&workspace_folders, emmyrc.as_ref());
    for workspace in &workspace_folders {
        if workspace.is_library {
            log::info!("add library workspace: {:?}", workspace);
            mut_analysis.add_library_workspace(workspace);
        } else {
            log::info!("add workspace root: {:?}", workspace.root);
            mut_analysis.add_main_workspace(workspace.root.clone());
        }
    }

    status_bar.update_progress_task(
        ProgressTask::LoadWorkspace,
        None,
        Some(String::from("Collecting files")),
    );

    // load files
    let files = collect_workspace_files(&workspace_folders, &emmyrc, None, None);
    let files: Vec<(PathBuf, Option<String>)> =
        files.into_iter().map(|file| file.into_tuple()).collect();
    let file_count = files.len();
    if file_count != 0 {
        status_bar.update_progress_task(
            ProgressTask::LoadWorkspace,
            None,
            Some(format!("Indexing {} files", file_count)),
        );
    }
    let removed_uris = mut_analysis.reload_workspace_files(files, open_files);

    status_bar.update_progress_task(
        ProgressTask::LoadWorkspace,
        None,
        Some(String::from("Finished loading workspace files")),
    );
    status_bar.finish_progress_task(
        ProgressTask::LoadWorkspace,
        Some("Indexing complete".to_string()),
    );

    if mut_analysis.check_schema_update() {
        mut_analysis.update_schema().await;
    }

    drop(mut_analysis);

    if !lsp_features.supports_pull_diagnostic() {
        for uri in removed_uris {
            file_diagnostic.clear_push_file_diagnostics(uri);
        }
    }

    if !lsp_features.supports_workspace_diagnostic() {
        file_diagnostic
            .add_workspace_diagnostic_task(0, false)
            .await;
    }
}

pub fn get_workspace_folders(params: &InitializeParams) -> Vec<WorkspaceFolder> {
    let mut workspace_folders = Vec::new();
    if let Some(workspaces) = &params.workspace_folders {
        for workspace in workspaces {
            if let Some(path) = uri_to_file_path(&workspace.uri) {
                workspace_folders.push(WorkspaceFolder::new(path, false));
            }
        }
    }

    if workspace_folders.is_empty() {
        // However, most LSP clients still provide this field
        #[allow(deprecated)]
        if let Some(uri) = &params.root_uri {
            let root_workspace = uri_to_file_path(uri);
            if let Some(path) = root_workspace {
                workspace_folders.push(WorkspaceFolder::new(path, false));
            }
        }
    }

    workspace_folders
}

pub async fn init_std_lib(analysis: &AnalysisLock, cmd_args: &CmdArgs, emmyrc: Arc<Emmyrc>) {
    log::info!(
        "initializing std lib with resources path: {:?}",
        cmd_args.resources_path
    );
    let mut analysis = analysis.write().await;
    if cmd_args.load_stdlib.0 {
        // double update config
        analysis.update_config(emmyrc);
        try_generate_translated_std();
        analysis.init_std_lib(cmd_args.resources_path.0.clone());
    }

    log::info!("initialized std lib complete");
}
