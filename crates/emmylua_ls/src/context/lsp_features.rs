use lsp_types::ClientCapabilities;

#[derive(Debug)]
pub struct LspFeatures {
    client_capabilities: ClientCapabilities,
}

#[allow(unused)]
impl LspFeatures {
    pub fn new(client_capabilities: ClientCapabilities) -> Self {
        Self {
            client_capabilities,
        }
    }

    pub fn supports_multiline_tokens(&self) -> bool {
        if let Some(semantic) = &self.client_capabilities.text_document {
            if let Some(semantic) = &semantic.semantic_tokens {
                if let Some(supports) = semantic.multiline_token_support {
                    return supports;
                }
            }
        }
        false
    }

    pub fn supports_config_request(&self) -> bool {
        if let Some(workspace) = &self.client_capabilities.workspace {
            if let Some(supports) = workspace.configuration {
                return supports;
            }
        }
        false
    }

    pub fn supports_work_done_progress(&self) -> bool {
        self.client_capabilities
            .window
            .as_ref()
            .and_then(|window| window.work_done_progress)
            .unwrap_or_default()
    }

    pub fn supports_pull_diagnostic(&self) -> bool {
        if let Some(text_document) = &self.client_capabilities.text_document {
            return text_document.diagnostic.is_some();
        }
        false
    }

    pub fn supports_workspace_diagnostic(&self) -> bool {
        self.supports_pull_diagnostic()
    }

    pub fn supports_refresh_diagnostic(&self) -> bool {
        if let Some(workspace) = &self.client_capabilities.workspace {
            if let Some(diagnostic) = &workspace.diagnostics {
                if let Some(supports) = diagnostic.refresh_support {
                    return supports;
                }
            }
        }
        false
    }

    pub fn supports_dynamic_watched_files_registration(&self) -> bool {
        self.client_capabilities
            .workspace
            .as_ref()
            .and_then(|ws| ws.did_change_watched_files.as_ref())
            .and_then(|watch| watch.dynamic_registration)
            .unwrap_or_default()
    }
}
