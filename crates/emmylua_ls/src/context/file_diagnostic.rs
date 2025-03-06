use std::{collections::HashMap, sync::Arc, time::Duration};

use emmylua_code_analysis::{EmmyLuaAnalysis, FileId, Profile};
use log::{debug, info};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use super::{ClientId, ClientProxy, ProgressTask, StatusBar};

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
        tokens.insert(file_id.clone(), cancel_token.clone());
        drop(tokens); // free the lock

        let analysis = self.analysis.clone();
        let client = self.client.clone();
        let diagnostic_tokens = self.diagnostic_tokens.clone();
        let file_id_clone = file_id.clone();

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

    pub async fn add_workspace_diagnostic_task(
        &self,
        client_id: ClientId,
        interval: u64,
        silent: bool,
    ) {
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
                    workspace_diagnostic(analysis, client_proxy, client_id, status_bar, silent, cancel_token).await
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
}

async fn workspace_diagnostic(
    analysis: Arc<RwLock<EmmyLuaAnalysis>>,
    client_proxy: Arc<ClientProxy>,
    client_id: ClientId,
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
            while let Some(_) = rx.recv().await {
                count += 1;
                if count == valid_file_count {
                    break;
                }
            }
        } else {
            let text = format!("diagnose {} files", valid_file_count);
            let _p = Profile::new(text.as_str());
            status_bar.create_progress_task(client_id, ProgressTask::DiagnoseWorkspace);
            while let Some(_) = rx.recv().await {
                count += 1;

                let message = format!("diagnostic {}/{}", count, valid_file_count);
                let percentage_done = ((count as f32 / valid_file_count as f32) * 100.0) as u32;
                status_bar.update_progress_task(
                    client_id,
                    ProgressTask::DiagnoseWorkspace,
                    Some(percentage_done),
                    Some(message),
                );

                if count == valid_file_count {
                    status_bar.finish_progress_task(
                        client_id,
                        ProgressTask::DiagnoseWorkspace,
                        None,
                    );
                    break;
                }
            }
        }
    }
}
