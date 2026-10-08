#[cfg(all(test, feature = "slow-tests"))]
mod tests;

use std::collections::HashSet;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, AtomicU64, Ordering};
use std::{path::PathBuf, sync::Arc, time::Duration};

use super::workspace_state::{OpenFilesSnapshot, WorkspaceState};
use super::{AnalysisState, ClientProxy, DiagnosticService};
use crate::context::ServerContextSnapshot;
use crate::context::lsp_features::LspFeatures;
use crate::handlers::{ClientConfig, init_analysis, register_files_watch};
use emmylua_code_analysis::{
    Emmyrc, WorkspaceFolder, load_configs, read_file_with_encoding, uri_to_file_path,
};
use lsp_types::Uri;
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;

pub struct WorkspaceManager {
    client: Arc<ClientProxy>,
    config_reload_token: Arc<PendingTask>,
    reload_lock: Arc<AsyncMutex<()>>,
    reload_generation: Arc<AtomicU64>,
    file_diagnostic: Arc<DiagnosticService>,
    lsp_features: Arc<LspFeatures>,
    pub state: WorkspaceState,
    workspace_diagnostic_level: Arc<AtomicU8>,
    workspace_version: Arc<AtomicI64>,
}

impl WorkspaceManager {
    pub fn new(
        client: Arc<ClientProxy>,
        file_diagnostic: Arc<DiagnosticService>,
        lsp_features: Arc<LspFeatures>,
    ) -> Self {
        Self {
            client,
            config_reload_token: Arc::new(PendingTask::default()),
            reload_lock: Arc::new(AsyncMutex::new(())),
            reload_generation: Arc::new(AtomicU64::new(0)),
            file_diagnostic,
            lsp_features,
            state: WorkspaceState::new(ClientConfig::default()),
            workspace_diagnostic_level: Arc::new(AtomicU8::new(
                WorkspaceDiagnosticLevel::Fast.to_u8(),
            )),
            workspace_version: Arc::new(AtomicI64::new(0)),
        }
    }
}

impl Deref for WorkspaceManager {
    type Target = WorkspaceState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for WorkspaceManager {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl WorkspaceManager {
    pub fn get_workspace_diagnostic_level(&self) -> WorkspaceDiagnosticLevel {
        let value = self.workspace_diagnostic_level.load(Ordering::Acquire);
        WorkspaceDiagnosticLevel::from_u8(value)
    }

    pub fn update_workspace_version(&self, level: WorkspaceDiagnosticLevel, add_version: bool) {
        self.workspace_diagnostic_level
            .store(level.to_u8(), Ordering::Release);
        if add_version {
            self.workspace_version.fetch_add(1, Ordering::AcqRel);
        }
    }

    pub fn get_workspace_version(&self) -> i64 {
        self.workspace_version.load(Ordering::Acquire)
    }

    pub async fn add_update_emmyrc_task(
        &self,
        context: ServerContextSnapshot,
        config_path: PathBuf,
    ) {
        let Some(config_root) = self.config_root() else {
            return;
        };
        if config_path.parent() != Some(config_root.as_path()) {
            return;
        }

        let (cancel_token, cancelled_existing) =
            self.config_reload_token.replace(CONFIG_RELOAD_DELAY).await;
        if cancelled_existing {
            log::debug!("cancel pending config reload: {:?}", config_path);
        }

        let workspace_folders = self.workspace_folders.clone();
        let client_config = self.client_config.clone();
        let config_reload_token = self.config_reload_token.clone();
        let reload_task_handles = self.reload_task_handles();
        tokio::spawn(async move {
            cancel_token.wait().await;
            if cancel_token.is_cancelled() {
                config_reload_token.clear(&cancel_token).await;
                return;
            }

            let emmyrc = load_emmy_config(Some(config_root), client_config);
            spawn_workspace_reload_task(reload_task_handles, context, workspace_folders, emmyrc);
            config_reload_token.clear(&cancel_token).await;
        });
    }

    pub fn add_reload_workspace_task(&self, context: ServerContextSnapshot) {
        let emmyrc = load_emmy_config(self.config_root(), self.client_config.clone());
        spawn_workspace_reload_task(
            self.reload_task_handles(),
            context,
            self.workspace_folders.clone(),
            emmyrc,
        );
    }

    // pub async fn reindex_workspace(&self, delay: Duration) {
    //     log::info!("reindex workspace with delay: {:?}", delay);
    //     let (cancel_token, cancelled_existing) = self.reindex_token.replace(delay).await;
    //     if cancelled_existing {
    //         log::info!("cancel reindex workspace");
    //     }

    //     let analysis = self.analysis.clone();
    //     let client = self.client.clone();
    //     let file_diagnostic = self.file_diagnostic.clone();
    //     let lsp_features = self.lsp_features.clone();
    //     let reindex_token = self.reindex_token.clone();
    //     let workspace_diagnostic_level = self.workspace_diagnostic_level.clone();
    //     tokio::spawn(async move {
    //         cancel_token.wait().await;
    //         if cancel_token.is_cancelled() {
    //             reindex_token.clear(&cancel_token).await;
    //             return;
    //         }

    //         // Perform reindex as a single serialized analysis update.
    //         analysis
    //             .update(|analysis| {
    //                 // Clean up nonexistent files before reindexing.
    //                 analysis.cleanup_nonexistent_files();
    //                 analysis.reindex();
    //             })
    //             .await;

    //         refresh_workspace_diagnostics(
    //             file_diagnostic,
    //             lsp_features,
    //             client,
    //             workspace_diagnostic_level,
    //         )
    //         .await;
    //         reindex_token.clear(&cancel_token).await;
    //     });
    // }

    fn reload_task_handles(&self) -> ReloadTaskHandles {
        ReloadTaskHandles {
            client: self.client.clone(),
            file_diagnostic: self.file_diagnostic.clone(),
            lsp_features: self.lsp_features.clone(),
            reload_lock: self.reload_lock.clone(),
            reload_generation: self.reload_generation.clone(),
            workspace_diagnostic_level: self.workspace_diagnostic_level.clone(),
        }
    }
}

const CONFIG_FILE_NAMES: [&str; 3] = [".luarc.json", ".emmyrc.json", ".emmyrc.lua"];
const CONFIG_RELOAD_DELAY: Duration = Duration::from_secs(2);

pub fn load_emmy_config(config_root: Option<PathBuf>, client_config: ClientConfig) -> Arc<Emmyrc> {
    let mut config_files = Vec::new();

    extend_config_files(&mut config_files, dirs::home_dir());
    extend_config_files(
        &mut config_files,
        dirs::config_dir().map(|path| path.join("emmylua_ls")),
    );

    if let Ok(path) = std::env::var("EMMYLUALS_CONFIG") {
        let path = PathBuf::from(path);
        if path.exists() {
            log::info!("load config from: {:?}", path);
            config_files.push(path);
        }
    }

    extend_config_files(&mut config_files, config_root.clone());

    let mut emmyrc = load_configs(config_files, client_config.partial_emmyrcs.clone());
    merge_client_config(client_config, &mut emmyrc);
    if let Some(workspace_root) = &config_root {
        emmyrc.pre_process_emmyrc(workspace_root);
    }

    log::info!("loaded emmyrc complete");
    emmyrc.into()
}

fn merge_client_config(client_config: ClientConfig, emmyrc: &mut Emmyrc) -> Option<()> {
    emmyrc.runtime.extensions.extend(client_config.extensions);
    emmyrc.workspace.ignore_globs.extend(client_config.exclude);
    if client_config.encoding != "utf-8" {
        emmyrc.workspace.encoding = client_config.encoding;
    }

    Some(())
}

fn extend_config_files(config_files: &mut Vec<PathBuf>, dir: Option<PathBuf>) {
    let Some(dir) = dir else {
        return;
    };

    for file_name in CONFIG_FILE_NAMES {
        let path = dir.join(file_name);
        if path.exists() {
            log::info!("load config from: {:?}", path);
            config_files.push(path);
        }
    }
}

#[derive(Debug)]
struct DebounceToken {
    cancel_token: CancellationToken,
    time_sleep: Duration,
    need_re_sleep: AtomicBool,
}

impl DebounceToken {
    fn new(time_sleep: Duration) -> Self {
        Self {
            cancel_token: CancellationToken::new(),
            time_sleep,
            need_re_sleep: AtomicBool::new(false),
        }
    }

    async fn wait(&self) {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.time_sleep) => {
                    if !self.need_re_sleep.swap(false, Ordering::AcqRel) {
                        break;
                    }
                }
                _ = self.cancel_token.cancelled() => break,
            }
        }
    }

    fn cancel(&self) {
        self.cancel_token.cancel();
    }

    fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }
}

#[derive(Debug, Default)]
struct PendingTask(AsyncMutex<Option<Arc<DebounceToken>>>);

impl PendingTask {
    async fn replace(&self, delay: Duration) -> (Arc<DebounceToken>, bool) {
        let mut current = self.0.lock().await;
        let had_existing_token = current.is_some();
        if let Some(token) = current.as_ref() {
            token.cancel();
        }

        let next = Arc::new(DebounceToken::new(delay));
        current.replace(next.clone());
        (next, had_existing_token)
    }

    async fn clear(&self, finished_token: &Arc<DebounceToken>) {
        let mut current = self.0.lock().await;
        if current
            .as_ref()
            .is_some_and(|token| Arc::ptr_eq(token, finished_token))
        {
            current.take();
        }
    }
}

async fn refresh_workspace_diagnostics(
    file_diagnostic: Arc<DiagnosticService>,
    lsp_features: Arc<LspFeatures>,
    client: Arc<ClientProxy>,
    workspace_diagnostic_level: Arc<AtomicU8>,
) {
    file_diagnostic.cancel_workspace_diagnostic().await;
    workspace_diagnostic_level.store(WorkspaceDiagnosticLevel::Fast.to_u8(), Ordering::Release);

    if lsp_features.supports_workspace_diagnostic() {
        client.refresh_workspace_diagnostics();
    } else {
        file_diagnostic
            .add_workspace_diagnostic_task(500, true)
            .await;
    }
}

#[derive(Clone)]
struct ReloadTaskHandles {
    client: Arc<ClientProxy>,
    file_diagnostic: Arc<DiagnosticService>,
    lsp_features: Arc<LspFeatures>,
    reload_lock: Arc<AsyncMutex<()>>,
    reload_generation: Arc<AtomicU64>,
    workspace_diagnostic_level: Arc<AtomicU8>,
}

fn spawn_workspace_reload_task(
    handles: ReloadTaskHandles,
    context: ServerContextSnapshot,
    workspace_folders: Vec<WorkspaceFolder>,
    emmyrc: Arc<Emmyrc>,
) {
    let generation = handles.reload_generation.fetch_add(1, Ordering::AcqRel) + 1;
    tokio::spawn(async move {
        let _reload_guard = handles.reload_lock.lock().await;
        if generation != handles.reload_generation.load(Ordering::Acquire) {
            return;
        }

        apply_workspace_reload(context, workspace_folders, emmyrc).await;
        if generation != handles.reload_generation.load(Ordering::Acquire) {
            return;
        }

        refresh_workspace_diagnostics(
            handles.file_diagnostic,
            handles.lsp_features,
            handles.client,
            handles.workspace_diagnostic_level,
        )
        .await;
    });
}

async fn apply_workspace_reload(
    context: ServerContextSnapshot,
    workspace_folders: Vec<WorkspaceFolder>,
    emmyrc: Arc<Emmyrc>,
) {
    let open_files = {
        let mut workspace_manager = context.workspace_manager().lock().await;
        workspace_manager.update_match_state(emmyrc.as_ref());
        workspace_manager.workspace_open_files_snapshot()
    };

    {
        context
            .analysis()
            .update(|analysis| analysis.clear_non_std_workspaces())
            .await;
    }

    init_analysis(
        context.analysis(),
        context.status_bar(),
        context.file_diagnostic(),
        context.lsp_features(),
        workspace_folders,
        emmyrc,
        open_files.files.clone(),
    )
    .await;
    sync_reloaded_open_files(context.clone(), open_files).await;

    register_files_watch(context).await;
}

async fn sync_reloaded_open_files(
    context: ServerContextSnapshot,
    mut applied_snapshot: OpenFilesSnapshot,
) {
    loop {
        let snapshot_update = {
            let workspace_manager = context.workspace_manager().lock().await;
            let next_snapshot = workspace_manager.workspace_open_files_snapshot();
            if next_snapshot.version == applied_snapshot.version {
                None
            } else {
                let next_open_uris = next_snapshot
                    .files
                    .iter()
                    .map(|(uri, _)| uri.clone())
                    .collect::<HashSet<_>>();
                let removed_actions = applied_snapshot
                    .files
                    .iter()
                    .filter_map(|(uri, _)| {
                        if next_open_uris.contains(uri) {
                            return None;
                        }

                        if workspace_manager.is_workspace_file(uri)
                            && let Some(path) = uri_to_file_path(uri)
                            && path.exists()
                        {
                            return Some(OpenFileSyncAction::RestoreFromDisk(uri.clone(), path));
                        }

                        Some(OpenFileSyncAction::Remove(uri.clone()))
                    })
                    .collect::<Vec<_>>();
                Some((next_snapshot, removed_actions))
            }
        };
        let Some((next_snapshot, removed_actions)) = snapshot_update else {
            return;
        };

        let removed_uris = apply_open_file_sync(
            context.analysis(),
            next_snapshot.files.clone(),
            removed_actions,
        )
        .await;
        if !context.lsp_features().supports_pull_diagnostic() {
            for uri in removed_uris {
                context.file_diagnostic().clear_push_file_diagnostics(uri);
            }
        }

        applied_snapshot = next_snapshot;
    }
}

async fn apply_open_file_sync(
    analysis: &AnalysisState,
    current_open_files: Vec<(Uri, String)>,
    removed_actions: Vec<OpenFileSyncAction>,
) -> Vec<Uri> {
    if current_open_files.is_empty() && removed_actions.is_empty() {
        return Vec::new();
    }

    let encoding = analysis
        .with_snapshot(|analysis| analysis.get_emmyrc().workspace.encoding.clone())
        .unwrap_or_default();
    let mut updates = current_open_files
        .into_iter()
        .map(|(uri, text)| (uri, Some(text)))
        .collect::<Vec<_>>();
    let mut removed_uris = Vec::new();

    for action in removed_actions {
        match action {
            OpenFileSyncAction::RestoreFromDisk(uri, path) => {
                if let Some(text) = read_file_with_encoding(&path, &encoding) {
                    updates.push((uri, Some(text)));
                } else {
                    removed_uris.push(uri);
                }
            }
            OpenFileSyncAction::Remove(uri) => {
                removed_uris.push(uri);
            }
        }
    }

    analysis
        .update(|analysis| {
            for uri in &removed_uris {
                analysis.remove_file_by_uri(uri);
            }
            if !updates.is_empty() {
                analysis.update_files_by_uri(updates);
            }
        })
        .await;

    removed_uris
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceDiagnosticLevel {
    None = 0,
    Fast = 1,
    Slow = 2,
}

impl WorkspaceDiagnosticLevel {
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Fast,
            2 => Self::Slow,
            _ => Self::None,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone)]
enum OpenFileSyncAction {
    RestoreFromDisk(Uri, PathBuf),
    Remove(Uri),
}
