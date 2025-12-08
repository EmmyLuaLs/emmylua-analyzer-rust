use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicI64, AtomicU8, Ordering};
use std::{path::PathBuf, sync::Arc, time::Duration};

use super::{ClientProxy, FileDiagnostic, StatusBar};
use crate::context::lsp_features::LspFeatures;
use crate::handlers::{ClientConfig, init_analysis};
use emmylua_code_analysis::{EmmyLuaAnalysis, Emmyrc, load_configs};
use emmylua_code_analysis::{update_code_style, uri_to_file_path};
use log::{debug, info};
use lsp_types::Uri;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use wax::Pattern;

#[derive(Clone, Debug)]
pub enum WorkspaceImport {
    All,
    SubPaths(Vec<PathBuf>),
}

#[derive(Clone, Debug)]
pub struct WorkspaceFolder {
    pub root: PathBuf,
    pub import: WorkspaceImport,
    pub is_library: bool,
}

impl WorkspaceFolder {
    pub fn new(root: PathBuf, is_library: bool) -> Self {
        Self {
            root,
            import: WorkspaceImport::All,
            is_library,
        }
    }

    pub fn with_sub_paths(root: PathBuf, sub_paths: Vec<PathBuf>, is_library: bool) -> Self {
        Self {
            root,
            import: WorkspaceImport::SubPaths(sub_paths),
            is_library,
        }
    }
}

pub struct WorkspaceManager {
    analysis: Arc<RwLock<EmmyLuaAnalysis>>,
    client: Arc<ClientProxy>,
    status_bar: Arc<StatusBar>,
    update_token: Arc<Mutex<Option<Arc<ReindexToken>>>>,
    file_diagnostic: Arc<FileDiagnostic>,
    lsp_features: Arc<LspFeatures>,
    pub client_config: ClientConfig,
    pub workspace_folders: Vec<WorkspaceFolder>,
    pub watcher: Option<notify::RecommendedWatcher>,
    pub current_open_files: HashSet<Uri>,
    pub match_file_pattern: WorkspaceFileMatcher,
    workspace_diagnostic_level: Arc<AtomicU8>,
    workspace_version: Arc<AtomicI64>,
}

impl WorkspaceManager {
    pub fn new(
        analysis: Arc<RwLock<EmmyLuaAnalysis>>,
        client: Arc<ClientProxy>,
        status_bar: Arc<StatusBar>,
        file_diagnostic: Arc<FileDiagnostic>,
        lsp_features: Arc<LspFeatures>,
    ) -> Self {
        Self {
            analysis,
            client,
            status_bar,
            client_config: ClientConfig::default(),
            workspace_folders: Vec::new(),
            update_token: Arc::new(Mutex::new(None)),
            file_diagnostic,
            lsp_features,
            watcher: None,
            current_open_files: HashSet::new(),
            match_file_pattern: WorkspaceFileMatcher::default(),
            workspace_diagnostic_level: Arc::new(AtomicU8::new(
                WorkspaceDiagnosticLevel::Fast.to_u8(),
            )),
            workspace_version: Arc::new(AtomicI64::new(0)),
        }
    }

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
        let workspace_folders = self.workspace_folders.clone();
        let config_update_token = self.update_token.clone();
        let client_config = self.client_config.clone();
        let status_bar = self.status_bar.clone();
        let file_diagnostic = self.file_diagnostic.clone();
        let lsp_features = self.lsp_features.clone();
        tokio::spawn(async move {
            cancel_token.wait_for_reindex().await;
            if cancel_token.is_cancelled() {
                return;
            }

            let emmyrc = load_emmy_config(Some(file_dir.clone()), client_config);
            init_analysis(
                &analysis,
                &status_bar,
                &file_diagnostic,
                &lsp_features,
                workspace_folders,
                emmyrc,
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

    pub fn add_reload_workspace_task(&self) -> Option<()> {
        let config_root: Option<PathBuf> = self.workspace_folders.first().map(|wf| wf.root.clone());

        let emmyrc = load_emmy_config(config_root, self.client_config.clone());
        let analysis = self.analysis.clone();
        let workspace_folders = self.workspace_folders.clone();
        let status_bar = self.status_bar.clone();
        let file_diagnostic = self.file_diagnostic.clone();
        let lsp_features = self.lsp_features.clone();
        let client = self.client.clone();
        let workspace_diagnostic_status = self.workspace_diagnostic_level.clone();
        tokio::spawn(async move {
            // Perform reindex with minimal lock holding time
            init_analysis(
                &analysis,
                &status_bar,
                &file_diagnostic,
                &lsp_features,
                workspace_folders,
                emmyrc,
            )
            .await;

            // Cancel diagnostics and update status without holding analysis lock
            file_diagnostic.cancel_workspace_diagnostic().await;
            workspace_diagnostic_status
                .store(WorkspaceDiagnosticLevel::Fast.to_u8(), Ordering::Release);

            // Trigger diagnostics refresh
            if lsp_features.supports_workspace_diagnostic() {
                client.refresh_workspace_diagnostics();
            } else {
                file_diagnostic
                    .add_workspace_diagnostic_task(500, true)
                    .await;
            }
        });

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
        log::info!("reindex workspace with delay: {:?}", delay);
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
        let lsp_features = self.lsp_features.clone();
        let client = self.client.clone();
        let workspace_diagnostic_status = self.workspace_diagnostic_level.clone();
        tokio::spawn(async move {
            cancel_token.wait_for_reindex().await;
            if cancel_token.is_cancelled() {
                return;
            }

            // Perform reindex with minimal lock holding time
            {
                let mut analysis = analysis.write().await;
                // 在重新索引之前清理不存在的文件
                analysis.cleanup_nonexistent_files();
                analysis.reindex();
                // Release lock immediately after reindex
            }

            // Cancel diagnostics and update status without holding analysis lock
            file_diagnostic.cancel_workspace_diagnostic().await;
            workspace_diagnostic_status
                .store(WorkspaceDiagnosticLevel::Fast.to_u8(), Ordering::Release);

            // Trigger diagnostics refresh
            if lsp_features.supports_workspace_diagnostic() {
                client.refresh_workspace_diagnostics();
            } else {
                file_diagnostic
                    .add_workspace_diagnostic_task(500, true)
                    .await;
            }
        });

        Some(())
    }

    pub fn is_workspace_file(&self, uri: &Uri) -> bool {
        if self.workspace_folders.is_empty() {
            return true;
        }

        let Some(file_path) = uri_to_file_path(uri) else {
            return true;
        };

        for workspace in &self.workspace_folders {
            if let Ok(relative) = file_path.strip_prefix(&workspace.root) {
                let inside_import = match &workspace.import {
                    WorkspaceImport::All => true,
                    WorkspaceImport::SubPaths(paths) => {
                        paths.iter().any(|p| relative.starts_with(p))
                    }
                };

                if !inside_import {
                    continue;
                }

                if self.match_file_pattern.is_match(&file_path, relative) {
                    return true;
                }
            }
        }

        false
    }
}

pub fn load_emmy_config(config_root: Option<PathBuf>, client_config: ClientConfig) -> Arc<Emmyrc> {
    // Config load priority.
    // * Global `<os-specific home-dir>/.luarc.json`.
    // * Global `<os-specific home-dir>/.emmyrc.json`.
    // * Global `<os-specific config-dir>/emmylua_ls/.luarc.json`.
    // * Global `<os-specific config-dir>/emmylua_ls/.emmyrc.json`.
    // * Environment-specified config at the $EMMYLUALS_CONFIG path.
    // * Local `.luarc.json`.
    // * Local `.emmyrc.json`.
    let luarc_file = ".luarc.json";
    let emmyrc_file = ".emmyrc.json";
    let mut config_files = Vec::new();

    let home_dir = dirs::home_dir();
    if let Some(home_dir) = home_dir {
        let global_luarc_path = home_dir.join(luarc_file);
        if global_luarc_path.exists() {
            info!("load config from: {:?}", global_luarc_path);
            config_files.push(global_luarc_path);
        }
        let global_emmyrc_path = home_dir.join(emmyrc_file);
        if global_emmyrc_path.exists() {
            info!("load config from: {:?}", global_emmyrc_path);
            config_files.push(global_emmyrc_path);
        }
    };

    let emmylua_config_dir = "emmylua_ls";
    let config_dir = dirs::config_dir().map(|path| path.join(emmylua_config_dir));
    if let Some(config_dir) = config_dir {
        let global_luarc_path = config_dir.join(luarc_file);
        if global_luarc_path.exists() {
            info!("load config from: {:?}", global_luarc_path);
            config_files.push(global_luarc_path);
        }
        let global_emmyrc_path = config_dir.join(emmyrc_file);
        if global_emmyrc_path.exists() {
            info!("load config from: {:?}", global_emmyrc_path);
            config_files.push(global_emmyrc_path);
        }
    };

    std::env::var("EMMYLUALS_CONFIG")
        .inspect(|path| {
            let config_path = std::path::PathBuf::from(path);
            if config_path.exists() {
                info!("load config from: {:?}", config_path);
                config_files.push(config_path);
            }
        })
        .ok();

    if let Some(config_root) = &config_root {
        let luarc_path = config_root.join(luarc_file);
        if luarc_path.exists() {
            info!("load config from: {:?}", luarc_path);
            config_files.push(luarc_path);
        }
        let emmyrc_path = config_root.join(emmyrc_file);
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

#[derive(Debug, Clone)]
pub struct WorkspaceFileMatcher {
    include: Vec<String>,
    exclude: Vec<String>,
    exclude_dir: Vec<PathBuf>,
}

impl WorkspaceFileMatcher {
    pub fn new(include: Vec<String>, exclude: Vec<String>, exclude_dir: Vec<PathBuf>) -> Self {
        Self {
            include,
            exclude,
            exclude_dir,
        }
    }
    pub fn is_match(&self, path: &Path, relative_path: &Path) -> bool {
        if self.exclude_dir.iter().any(|dir| path.starts_with(dir)) {
            return false;
        }

        // let path_str = path.to_string_lossy().to_string().replace("\\", "/");
        let exclude_matcher = wax::any(self.exclude.iter().map(|s| s.as_str()));
        if let Ok(exclude_set) = exclude_matcher {
            if exclude_set.is_match(relative_path) {
                return false;
            }
        } else {
            log::error!("Invalid exclude pattern");
        }

        let include_matcher = wax::any(self.include.iter().map(|s| s.as_str()));
        if let Ok(include_set) = include_matcher {
            return include_set.is_match(relative_path);
        } else {
            log::error!("Invalid include pattern");
        }

        true
    }
}

impl Default for WorkspaceFileMatcher {
    fn default() -> Self {
        let include_pattern = vec!["**/*.lua".to_string()];
        Self::new(include_pattern, vec![], vec![])
    }
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
