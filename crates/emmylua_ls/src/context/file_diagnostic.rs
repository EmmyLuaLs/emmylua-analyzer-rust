use std::{collections::HashMap, sync::Arc, time::Duration};

use emmylua_code_analysis::{EmmyLuaAnalysis, FileId, Profile};
use log::{debug, info};
use lsp_types::{Diagnostic, Uri};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use super::{ClientProxy, ProgressTask, StatusBar};

pub struct FileDiagnostic {
    analysis: Arc<RwLock<EmmyLuaAnalysis>>,
    client: Arc<ClientProxy>,
    status_bar: Arc<StatusBar>,
    diagnostic_tokens: Arc<Mutex<HashMap<FileId, CancellationToken>>>,
    workspace_diagnostic_token: Arc<Mutex<Option<CancellationToken>>>,
}

impl FileDiagnostic {
    pub fn new(
        analysis: Arc<RwLock<EmmyLuaAnalysis>>,
        status_bar: Arc<StatusBar>,
        client: Arc<ClientProxy>,
    ) -> Self {
        Self {
            analysis,
            client,
            diagnostic_tokens: Arc::new(Mutex::new(HashMap::new())),
            workspace_diagnostic_token: Arc::new(Mutex::new(None)),
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
                    let analysis = analysis.read().await;
                    if let Some(uri) = analysis.get_uri(file_id_clone) {
                        let diagnostics = analysis.diagnose_file(file_id_clone, cancel_token);
                        if let Some(diagnostics) = diagnostics {
                            let diagnostic_param = lsp_types::PublishDiagnosticsParams {
                                uri,
                                diagnostics,
                                version: None,
                            };
                            client.publish_diagnostics(diagnostic_param);
                        }
                    } else {
                        info!("file not found: {:?}", file_id_clone);
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

    /// 清除指定文件的诊断信息
    pub fn clear_push_file_diagnostics(&self, uri: Uri) {
        let diagnostic_param = lsp_types::PublishDiagnosticsParams {
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
                    push_workspace_diagnostic(analysis, client_proxy, status_bar, silent, cancel_token).await
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
        for (_, token) in tokens.iter() {
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

    pub async fn pull_file_diagnostics(
        &self,
        uri: Uri,
        cancel_token: CancellationToken,
    ) -> Vec<Diagnostic> {
        let analysis = self.analysis.read().await;
        let Some(file_id) = analysis.get_file_id(&uri) else {
            return vec![];
        };

        let diagnostics = analysis.diagnose_file(file_id, cancel_token);
        diagnostics.unwrap_or_default()
    }

    pub async fn pull_workspace_diagnostics_slow(
        &self,
        cancel_token: CancellationToken,
    ) -> Vec<(Uri, Vec<Diagnostic>)> {
        let mut token = self.workspace_diagnostic_token.lock().await;
        if let Some(token) = token.as_ref() {
            token.cancel();
            debug!("cancel workspace diagnostic");
        }
        token.replace(cancel_token.clone());
        drop(token);

        let mut result = Vec::new();
        let analysis = self.analysis.read().await;
        let main_workspace_file_ids = analysis
            .compilation
            .get_db()
            .get_module_index()
            .get_main_workspace_file_ids();
        drop(analysis);

        for file_id in main_workspace_file_ids {
            if cancel_token.is_cancelled() {
                break;
            }
            let analysis = self.analysis.read().await;
            if let Some(uri) = analysis.get_uri(file_id) {
                let diagnostics = analysis.diagnose_file(file_id, cancel_token.clone());
                if let Some(diagnostics) = diagnostics {
                    result.push((uri, diagnostics));
                }
            }
        }

        result
    }

    pub async fn pull_workspace_diagnostics_fast(
        &self,
        cancel_token: CancellationToken,
    ) -> Vec<(Uri, Vec<Diagnostic>)> {
        let mut token = self.workspace_diagnostic_token.lock().await;
        if let Some(token) = token.as_ref() {
            token.cancel();
            debug!("cancel workspace diagnostic");
        }
        token.replace(cancel_token.clone());
        drop(token);

        let mut result = Vec::new();
        let analysis = self.analysis.read().await;
        let main_workspace_file_ids = analysis
            .compilation
            .get_db()
            .get_module_index()
            .get_main_workspace_file_ids();
        drop(analysis);

        let status_bar = self.status_bar.clone();
        status_bar
            .create_progress_task(ProgressTask::DiagnoseWorkspace)
            .await;

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Option<(Vec<Diagnostic>, Uri)>>(100);
        let valid_file_count = main_workspace_file_ids.len();

        let analysis = self.analysis.clone();
        for file_id in main_workspace_file_ids {
            let analysis = analysis.clone();
            let token = cancel_token.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let analysis = analysis.read().await;
                let diagnostics = analysis.diagnose_file(file_id, token);
                if let Some(diagnostics) = diagnostics {
                    let uri = analysis.get_uri(file_id).unwrap();
                    let _ = tx.send(Some((diagnostics, uri))).await;
                } else {
                    let _ = tx.send(None).await;
                }
            });
        }

        let mut count = 0;
        if valid_file_count != 0 {
            let text = format!("diagnose {} files", valid_file_count);
            let _p = Profile::new(text.as_str());
            let mut last_percentage = 0;
            while let Some(file_diagnostic_result) = rx.recv().await {
                if cancel_token.is_cancelled() {
                    break;
                }

                if let Some((diagnostics, uri)) = file_diagnostic_result {
                    result.push((uri, diagnostics));
                }

                count += 1;
                let percentage_done = ((count as f32 / valid_file_count as f32) * 100.0) as u32;
                if last_percentage != percentage_done {
                    last_percentage = percentage_done;
                    let message = format!("diagnostic {}%", percentage_done);
                    status_bar.update_progress_task(
                        ProgressTask::DiagnoseWorkspace,
                        Some(percentage_done),
                        Some(message),
                    );
                }
                if count == valid_file_count {
                    break;
                }
            }
        }

        status_bar.finish_progress_task(
            ProgressTask::DiagnoseWorkspace,
            Some("Diagnosis complete".to_string()),
        );

        result
    }
}

async fn push_workspace_diagnostic(
    analysis: Arc<RwLock<EmmyLuaAnalysis>>,
    client_proxy: Arc<ClientProxy>,
    status_bar: Arc<StatusBar>,
    silent: bool,
    cancel_token: CancellationToken,
) {
    let read_analysis = analysis.read().await;
    let main_workspace_file_ids = read_analysis
        .compilation
        .get_db()
        .get_module_index()
        .get_main_workspace_file_ids();
    drop(read_analysis);
    // diagnostic files
    let (tx, mut rx) = tokio::sync::mpsc::channel::<FileId>(100);
    let valid_file_count = main_workspace_file_ids.len();
    if !silent {
        status_bar
            .create_progress_task(ProgressTask::DiagnoseWorkspace)
            .await;
    }

    for file_id in main_workspace_file_ids {
        let analysis = analysis.clone();
        let token = cancel_token.clone();
        let client = client_proxy.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let analysis = analysis.read().await;
            let diagnostics = analysis.diagnose_file(file_id, token);
            if let Some(diagnostics) = diagnostics {
                let uri = analysis.get_uri(file_id).unwrap();
                let diagnostic_param = lsp_types::PublishDiagnosticsParams {
                    uri,
                    diagnostics,
                    version: None,
                };
                client.publish_diagnostics(diagnostic_param);
            }
            let _ = tx.send(file_id).await;
        });
    }

    let mut count = 0;
    if valid_file_count != 0 {
        if silent {
            while (rx.recv().await).is_some() {
                count += 1;
                if count == valid_file_count {
                    break;
                }
            }
        } else {
            let text = format!("diagnose {} files", valid_file_count);
            let _p = Profile::new(text.as_str());
            let mut last_percentage = 0;
            while (rx.recv().await).is_some() {
                count += 1;
                let percentage_done = ((count as f32 / valid_file_count as f32) * 100.0) as u32;
                if last_percentage != percentage_done {
                    last_percentage = percentage_done;
                    let message = format!("diagnostic {}%", percentage_done);
                    status_bar.update_progress_task(
                        ProgressTask::DiagnoseWorkspace,
                        Some(percentage_done),
                        Some(message),
                    );
                }
                if count == valid_file_count {
                    break;
                }
            }
        }
    }

    if !silent {
        status_bar.finish_progress_task(
            ProgressTask::DiagnoseWorkspace,
            Some("Diagnosis complete".to_string()),
        );
    }
}
