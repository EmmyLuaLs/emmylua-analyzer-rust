use std::{
    collections::HashMap,
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use emmylua_code_analysis::FileId;
use log::{debug, info};
use lsp_types::{Diagnostic, PublishDiagnosticsParams, Uri};
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;

use super::{AnalysisState, ClientProxy, ProgressTask, StatusBar};

pub struct DiagnosticService {
    analysis: Arc<AnalysisState>,
    client: Arc<ClientProxy>,
    status_bar: Arc<StatusBar>,
    diagnostic_tokens: Arc<Mutex<HashMap<FileId, CancellationToken>>>,
    workspace_diagnostic_token: Arc<Mutex<Option<CancellationToken>>>,
    workspace_run_lock: Arc<Mutex<()>>,
    workspace_diagnostic_done: AtomicBool,
    last_workspace_version: Arc<Mutex<Option<i64>>>,
}

impl DiagnosticService {
    pub fn new(
        analysis: Arc<AnalysisState>,
        status_bar: Arc<StatusBar>,
        client: Arc<ClientProxy>,
    ) -> Self {
        Self {
            analysis,
            client,
            diagnostic_tokens: Arc::new(Mutex::new(HashMap::new())),
            workspace_diagnostic_token: Arc::new(Mutex::new(None)),
            workspace_run_lock: Arc::new(Mutex::new(())),
            workspace_diagnostic_done: AtomicBool::new(false),
            last_workspace_version: Arc::new(Mutex::new(None)),
            status_bar,
        }
    }

    pub async fn add_diagnostic_task(&self, file_id: FileId, interval: u64) {
        let mut tokens = self.diagnostic_tokens.lock().await;

        if let Some(token) = tokens.get(&file_id) {
            token.cancel();
            debug!("cancel diagnostic: {:?}", file_id);
        }

        // create new token
        let cancel_token = CancellationToken::new();
        tokens.insert(file_id, cancel_token.clone());
        drop(tokens); // free the lock

        let analysis = self.analysis.clone();
        let client = self.client.clone();
        let diagnostic_tokens = self.diagnostic_tokens.clone();
        let file_id_clone = file_id;

        // Spawn a new task to perform diagnostic
        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(interval)) => {
                    let result = analysis.try_with_snapshot(|analysis| {
                        let uri = analysis.get_uri(file_id_clone)?;
                        let diagnostics = analysis.diagnose_file(file_id_clone, cancel_token)?;
                        Some((uri, diagnostics))
                    });
                    if let Some((uri, diagnostics)) = result {
                        client.publish_diagnostics(PublishDiagnosticsParams {
                            uri,
                            diagnostics,
                            version: None,
                        });
                    } else {
                        info!("file not found or cancelled: {:?}", file_id_clone);
                    }
                    // After completion, remove from HashMap
                    let mut tokens = diagnostic_tokens.lock().await;
                    tokens.remove(&file_id_clone);
                }
                _ = cancel_token.cancelled() => {
                    debug!("cancel diagnostic: {:?}", file_id_clone);
                }
            }
        });
    }

    // todo add message show
    pub async fn add_files_diagnostic_task(&self, file_ids: Vec<FileId>, interval: u64) {
        for file_id in file_ids {
            self.add_diagnostic_task(file_id, interval).await;
        }
    }

    /// Clear diagnostics for the given file.
    pub fn clear_push_file_diagnostics(&self, uri: Uri) {
        let diagnostic_param = PublishDiagnosticsParams {
            uri,
            diagnostics: vec![],
            version: None,
        };
        self.client.publish_diagnostics(diagnostic_param);
    }

    pub async fn add_workspace_diagnostic_task(&self, interval: u64, silent: bool) {
        let mut token = self.workspace_diagnostic_token.lock().await;
        if let Some(token) = token.as_ref() {
            token.cancel();
            debug!("cancel workspace diagnostic");
        }

        let cancel_token = CancellationToken::new();
        token.replace(cancel_token.clone());
        drop(token);

        let analysis = self.analysis.clone();
        let client_proxy = self.client.clone();
        let status_bar = self.status_bar.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(interval)) => {
                    let file_ids = analysis
                        .try_with_snapshot(|analysis| {
                            Some(analysis.salsa.main_workspace_file_ids())
                        })
                        .unwrap_or_default();
                    // Full-workspace diagnostics can be cancelled by salsa's pending-write
                    // (`cancel_others` triggered by Open/DidChange). Don't give up on a required
                    // full pass: after cancellation, wait and rerun with the latest snapshot until done or cancelled.
                    loop {
                        if cancel_token.is_cancelled() {
                            break;
                        }
                        let (result, completed) = run_workspace_batch(
                            analysis.clone(),
                            client_proxy.clone(),
                            status_bar.clone(),
                            file_ids.clone(),
                            cancel_token.clone(),
                            true,
                            !silent,
                        )
                        .await;
                        if completed {
                            let _ = result;
                        }
                        if completed || cancel_token.is_cancelled() {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(interval)).await;
                    }
                }
                _ = cancel_token.cancelled() => {
                    log::info!("cancel workspace diagnostic");
                }
            }
        });
    }

    #[allow(unused)]
    pub async fn cancel_all(&self) {
        let mut tokens = self.diagnostic_tokens.lock().await;
        for token in tokens.values() {
            token.cancel();
        }
        tokens.clear();
    }

    pub async fn cancel_workspace_diagnostic(&self) {
        let mut token = self.workspace_diagnostic_token.lock().await;
        if let Some(token) = token.as_ref() {
            token.cancel();
            debug!("cancel workspace diagnostic");
        }
        token.take();
    }

    pub fn mark_workspace_diagnostic_done(&self) {
        self.workspace_diagnostic_done
            .store(true, Ordering::Release);
    }

    pub fn has_workspace_diagnostic_done(&self) -> bool {
        self.workspace_diagnostic_done.load(Ordering::Acquire)
    }

    pub async fn get_last_workspace_version(&self) -> Option<i64> {
        *self.last_workspace_version.lock().await
    }

    pub async fn set_last_workspace_version(&self, version: i64) {
        *self.last_workspace_version.lock().await = Some(version);
    }

    pub async fn pull_workspace_diagnostics_slow(
        &self,
        cancel_token: CancellationToken,
    ) -> Option<Vec<(Uri, Vec<Diagnostic>)>> {
        let mut token = self.workspace_diagnostic_token.lock().await;
        if let Some(token) = token.as_ref() {
            token.cancel();
            debug!("cancel workspace diagnostic");
        }
        token.replace(cancel_token.clone());
        drop(token);

        // Only one workspace diagnostic runner may run at a time; fast replaces slow.
        let _guard = self.workspace_run_lock.lock().await;

        let mut result = Vec::new();
        let main_workspace_file_ids = self
            .analysis
            .try_with_snapshot(|analysis| Some(analysis.salsa.main_workspace_file_ids()))
            .unwrap_or_default();

        for file_id in main_workspace_file_ids {
            if cancel_token.is_cancelled() {
                return None;
            }
            let (uri, diagnostics) = self.analysis.try_with_snapshot(|analysis| {
                let uri = analysis.get_uri(file_id)?;
                let diagnostics = analysis.diagnose_file(file_id, cancel_token.clone())?;
                Some((uri, diagnostics))
            })?;
            result.push((uri, diagnostics));
        }

        Some(result)
    }

    pub async fn pull_workspace_diagnostics_fast(
        &self,
        cancel_token: CancellationToken,
    ) -> Option<Vec<(Uri, Vec<Diagnostic>)>> {
        let mut token = self.workspace_diagnostic_token.lock().await;
        if let Some(token) = token.as_ref() {
            token.cancel();
            debug!("cancel workspace diagnostic");
        }
        token.replace(cancel_token.clone());
        drop(token);

        // Only one workspace diagnostic runner may run at a time; fast replaces slow.
        let _guard = self.workspace_run_lock.lock().await;

        let main_workspace_file_ids = self
            .analysis
            .try_with_snapshot(|analysis| Some(analysis.salsa.main_workspace_file_ids()))
            .unwrap_or_default();

        let (result, completed) = run_workspace_batch(
            self.analysis.clone(),
            self.client.clone(),
            self.status_bar.clone(),
            main_workspace_file_ids,
            cancel_token,
            false,
            true,
        )
        .await;

        if completed { Some(result) } else { None }
    }
}

async fn run_workspace_batch(
    analysis: Arc<AnalysisState>,
    client: Arc<ClientProxy>,
    status_bar: Arc<StatusBar>,
    file_ids: Vec<FileId>,
    cancel_token: CancellationToken,
    publish: bool,
    progress: bool,
) -> (Vec<(Uri, Vec<Diagnostic>)>, bool) {
    let valid_file_count = file_ids.len();
    if progress {
        status_bar
            .create_progress_task(ProgressTask::DiagnoseWorkspace)
            .await;
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Option<(Uri, Vec<Diagnostic>)>>(100);
    let semaphore = Arc::new(Semaphore::new(64));
    let mut result = Vec::new();

    for file_id in file_ids {
        let analysis = analysis.clone();
        let token = cancel_token.clone();
        let client = client.clone();
        let tx = tx.clone();
        let semaphore = semaphore.clone();
        tokio::spawn(async move {
            let _permit = semaphore.acquire_owned().await.expect("semaphore closed");
            let result = analysis.try_with_snapshot(|analysis| {
                let diagnostics = analysis.diagnose_file(file_id, token)?;
                let uri = analysis.get_uri(file_id)?;
                Some((uri, diagnostics))
            });
            let Some((uri, diagnostics)) = result else {
                let _ = tx.send(None).await;
                return;
            };
            if publish {
                client.publish_diagnostics(PublishDiagnosticsParams {
                    uri: uri.clone(),
                    diagnostics: diagnostics.clone(),
                    version: None,
                });
            }
            let _ = tx.send(Some((uri, diagnostics))).await;
        });
    }

    let mut count = 0;
    if valid_file_count != 0 {
        if progress {
            let mut last_percentage = 0;
            while let Some(item) = rx.recv().await {
                if cancel_token.is_cancelled() {
                    break;
                }
                if let Some((uri, diagnostics)) = item {
                    result.push((uri, diagnostics));
                }
                count += 1;
                let percentage_done = ((count as f32 / valid_file_count as f32) * 100.0) as u32;
                if last_percentage != percentage_done {
                    last_percentage = percentage_done;
                    status_bar.update_progress_task(
                        ProgressTask::DiagnoseWorkspace,
                        Some(percentage_done),
                        Some(format!("diagnostic {}%", percentage_done)),
                    );
                }
                if count == valid_file_count {
                    break;
                }
            }
        } else {
            while rx.recv().await.is_some() {
                count += 1;
                if count == valid_file_count {
                    break;
                }
            }
        }
    }

    if progress {
        status_bar.finish_progress_task(
            ProgressTask::DiagnoseWorkspace,
            Some("Diagnosis complete".to_string()),
        );
    }

    (result, count == valid_file_count)
}
