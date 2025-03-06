use std::{path::PathBuf, sync::Arc, time::Duration};

use super::{ClientProxy, FileDiagnostic, StatusBar};
use crate::handlers::{init_analysis, ClientConfig};
use emmylua_code_analysis::update_code_style;
use emmylua_code_analysis::{load_configs, EmmyLuaAnalysis, Emmyrc};
use log::{debug, info};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

pub struct WorkspaceManager {
    analysis: Arc<RwLock<EmmyLuaAnalysis>>,
    client: Arc<ClientProxy>,
    status_bar: Arc<StatusBar>,
    update_token: Arc<Mutex<Option<Arc<ReindexToken>>>>,
    file_diagnostic: Arc<FileDiagnostic>,
    pub client_config: ClientConfig,
    pub workspace_folders: Vec<PathBuf>,
    pub watcher: Option<notify::RecommendedWatcher>,
}

impl WorkspaceManager {
    pub fn new(
        analysis: Arc<RwLock<EmmyLuaAnalysis>>,
        client: Arc<ClientProxy>,
        status_bar: Arc<StatusBar>,
        file_diagnostic: Arc<FileDiagnostic>,
    ) -> Self {
        Self {
            analysis,
            client,
            status_bar,
            client_config: ClientConfig::default(),
            workspace_folders: Vec::new(),
            update_token: Arc::new(Mutex::new(None)),
            file_diagnostic,
            watcher: None,
        }
    }

    pub async fn add_update_emmyrc_task(&self, file_dir: PathBuf) {
        let mut update_token = self.update_token.lock().await;
        if let Some(token) = update_token.as_ref() {
            token.cancel();
            debug!("cancel update config: {:?}", file_dir);
        }

        let cancel_token = Arc::new(ReindexToken::new(Duration::from_secs(2)));
        update_token.replace(cancel_token.clone());
        drop(update_token);

        let analysis = self.analysis.clone();
        let client = self.client.clone();
        let workspace_folders = self.workspace_folders.clone();
        let config_update_token = self.update_token.clone();
        let client_config = self.client_config.clone();
        let status_bar = self.status_bar.clone();
        let client_id = client_config.client_id;
        let file_diagnostic = self.file_diagnostic.clone();
        tokio::spawn(async move {
            cancel_token.wait_for_reindex().await;
            if cancel_token.is_cancelled() {
                return;
            }

            let emmyrc = load_emmy_config(Some(file_dir.clone()), client_config);
            init_analysis(
                analysis,
                client,
                &status_bar,
                workspace_folders,
                emmyrc,
                client_id,
                file_diagnostic,
            )
            .await;
            // After completion, remove from HashMap
            let mut tokens = config_update_token.lock().await;
            tokens.take();
        });
    }

    pub fn update_editorconfig(&self, path: PathBuf) {
        let parent_dir = path
            .parent()
            .unwrap()
            .to_path_buf()
            .to_string_lossy()
            .to_string()
            .replace("\\", "/");
        let file_normalized = path.to_string_lossy().to_string().replace("\\", "/");
        log::info!("update code style: {:?}", file_normalized);
        update_code_style(&parent_dir, &file_normalized);
    }

    pub async fn reload_workspace(&self) -> Option<()> {
        let config_root: Option<PathBuf> = match self.workspace_folders.first() {
            Some(root) => Some(PathBuf::from(root)),
            None => None,
        };

        let emmyrc = load_emmy_config(config_root, self.client_config.clone());
        let analysis = self.analysis.clone();
        let client = self.client.clone();
        let workspace_folders = self.workspace_folders.clone();
        let status_bar = self.status_bar.clone();
        let client_id = self.client_config.client_id;
        let file_diagnostic = self.file_diagnostic.clone();
        init_analysis(
            analysis,
            client,
            &status_bar,
            workspace_folders,
            emmyrc,
            client_id,
            file_diagnostic,
        )
        .await;

        Some(())
    }

    pub async fn extend_reindex_delay(&self) -> Option<()> {
        let update_token = self.update_token.lock().await;
        if let Some(token) = update_token.as_ref() {
            token.set_resleep().await;
        }

        Some(())
    }

    pub async fn reindex_workspace(&self, delay: Duration) -> Option<()> {
        let mut update_token = self.update_token.lock().await;
        if let Some(token) = update_token.as_ref() {
            token.cancel();
            log::info!("cancel reindex workspace");
        }

        let cancel_token = Arc::new(ReindexToken::new(delay));
        update_token.replace(cancel_token.clone());
        drop(update_token);
        let analysis = self.analysis.clone();
        let file_diagnostic = self.file_diagnostic.clone();
        let client_id = self.client_config.client_id;

        tokio::spawn(async move {
            cancel_token.wait_for_reindex().await;
            if cancel_token.is_cancelled() {
                return;
            }

            let mut analysis = analysis.write().await;
            analysis.reindex();
            file_diagnostic
                .add_workspace_diagnostic_task(client_id, 500, true)
                .await;
        });

        Some(())
    }
}

pub fn load_emmy_config(config_root: Option<PathBuf>, client_config: ClientConfig) -> Arc<Emmyrc> {
    let mut config_files = Vec::new();
    if let Some(config_root) = &config_root {
        let luarc_path = config_root.join(".luarc.json");
        if luarc_path.exists() {
            info!("load config from: {:?}", luarc_path);
            config_files.push(luarc_path);
        }
        let emmyrc_path = config_root.join(".emmyrc.json");
        if emmyrc_path.exists() {
            info!("load config from: {:?}", emmyrc_path);
            config_files.push(emmyrc_path);
        }
    }

    let mut emmyrc = load_configs(config_files, client_config.partial_emmyrcs.clone());
    merge_client_config(client_config, &mut emmyrc);
    if let Some(workspace_root) = &config_root {
        emmyrc.pre_process_emmyrc(workspace_root);
    }

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

#[derive(Debug)]
pub struct ReindexToken {
    cancel_token: CancellationToken,
    time_sleep: Duration,
    need_re_sleep: Mutex<bool>,
}

impl ReindexToken {
    pub fn new(time_sleep: Duration) -> Self {
        Self {
            cancel_token: CancellationToken::new(),
            time_sleep,
            need_re_sleep: Mutex::new(false),
        }
    }

    pub async fn wait_for_reindex(&self) {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.time_sleep) => {
                    // 获取锁来安全地访问和修改 need_re_sleep
                    let mut need_re_sleep = self.need_re_sleep.lock().await;
                    if *need_re_sleep {
                        *need_re_sleep = false;
                    } else {
                        break;
                    }
                }
                _ = self.cancel_token.cancelled() => {
                    break;
                }
            }
        }
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    pub async fn set_resleep(&self) {
        // 获取锁来安全地修改 need_re_sleep
        let mut need_re_sleep = self.need_re_sleep.lock().await;
        *need_re_sleep = true;
    }
}
