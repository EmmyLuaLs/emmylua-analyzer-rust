use std::sync::Arc;

use lsp_types::{
    NumberOrString, ProgressParams, ProgressParamsValue, WorkDoneProgress, WorkDoneProgressBegin,
    WorkDoneProgressCreateParams, WorkDoneProgressEnd, WorkDoneProgressReport,
};
use serde::{Deserialize, Serialize};

use super::{ClientId, ClientProxy};

pub struct StatusBar {
    client: Arc<ClientProxy>,
}

#[derive(Clone, Copy)]
pub enum ProgressTask {
    LoadWorkspace = 0,
    DiagnoseWorkspace = 1,
    #[allow(dead_code)]
    RefreshIndex = 2,
}

impl ProgressTask {
    pub fn as_i32(&self) -> i32 {
        *self as i32
    }

    pub fn get_task_name(&self) -> &'static str {
        match self {
            ProgressTask::LoadWorkspace => "Load workspace",
            ProgressTask::DiagnoseWorkspace => "Diagnose workspace",
            ProgressTask::RefreshIndex => "Refresh index",
        }
    }
}

impl StatusBar {
    pub fn new(client: Arc<ClientProxy>) -> Self {
        Self { client }
    }

    pub fn create_progress_task(&self, client_id: ClientId, task: ProgressTask) {
        match client_id {
            ClientId::VSCode => {
                self.vscode_set_server_status("ok", true, task.get_task_name());
                self.vscode_report_progress(&task.get_task_name(), 0.0);
            }
            _ => {
                self.client.send_notification(
                    "window/workDoneProgress/create",
                    WorkDoneProgressCreateParams {
                        token: NumberOrString::Number(task.as_i32()),
                    },
                );
                self.client.send_notification(
                    "$/progress",
                    ProgressParams {
                        token: NumberOrString::Number(task as i32),
                        value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(
                            WorkDoneProgressBegin {
                                title: task.get_task_name().to_string(),
                                cancellable: Some(false),
                                message: Some(task.get_task_name().to_string()),
                                percentage: None,
                            },
                        )),
                    },
                )
            }
        }
    }

    pub fn update_progress_task(
        &self,
        client_id: ClientId,
        task: ProgressTask,
        percentage: Option<u32>,
        message: Option<String>,
    ) {
        match client_id {
            ClientId::VSCode => {
                if let Some(message) = message {
                    self.vscode_report_progress(&message, percentage.unwrap_or(0) as f64 / 100.0);
                } else {
                    self.vscode_report_progress(
                        task.get_task_name(),
                        percentage.unwrap_or(0) as f64,
                    );
                }
            }
            _ => self.client.send_notification(
                "$/progress",
                ProgressParams {
                    token: NumberOrString::Number(task.as_i32()),
                    value: ProgressParamsValue::WorkDone(WorkDoneProgress::Report(
                        WorkDoneProgressReport {
                            percentage,
                            cancellable: Some(false),
                            message,
                        },
                    )),
                },
            ),
        }
    }

    pub fn finish_progress_task(
        &self,
        client_id: ClientId,
        task: ProgressTask,
        message: Option<String>,
    ) {
        match client_id {
            ClientId::VSCode => {
                if let Some(message) = message {
                    self.vscode_set_server_status("ok", false, &message);
                } else {
                    self.vscode_set_server_status("ok", false, task.get_task_name());
                }
            }
            _ => self.client.send_notification(
                "$/progress",
                ProgressParams {
                    token: NumberOrString::Number(task.as_i32()),
                    value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(
                        WorkDoneProgressEnd { message },
                    )),
                },
            ),
        }
    }

    fn vscode_set_server_status(&self, health: &str, loading: bool, message: &str) {
        self.client.send_notification(
            "emmy/setServerStatus",
            EmmyServerStatus {
                health: health.to_string(),
                loading,
                message: message.to_string(),
            },
        );
    }

    fn vscode_report_progress(&self, message: &str, percentage: f64) {
        self.client.send_notification(
            "emmy/progressReport",
            EmmyProgress {
                text: message.to_string(),
                percent: percentage,
            },
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmmyServerStatus {
    health: String,
    loading: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmmyProgress {
    text: String,
    percent: f64,
}
