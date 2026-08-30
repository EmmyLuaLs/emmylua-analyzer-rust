mod did_rename_files;

pub use did_rename_files::{on_did_rename_files_handler, process_did_rename_files_handler};
use lsp_types::{
    ClientCapabilities, FileOperationFilter, FileOperationPattern, FileOperationPatternOptions,
    FileOperationRegistrationOptions, ServerCapabilities,
    WorkspaceFileOperationsServerCapabilities, WorkspaceServerCapabilities,
};

use crate::handlers::RegisterCapabilities;

pub struct WorkspaceCapabilities;

impl RegisterCapabilities for WorkspaceCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.workspace = Some(WorkspaceServerCapabilities {
            file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                did_rename: Some(FileOperationRegistrationOptions {
                    filters: vec![FileOperationFilter {
                        scheme: Some(String::from("file")),
                        pattern: FileOperationPattern {
                            glob: "**/*".to_string(),
                            matches: None,
                            options: Some(FileOperationPatternOptions {
                                ignore_case: Some(true),
                            }),
                        },
                    }],
                }),
                ..Default::default()
            }),
            ..Default::default()
        });
    }
}
