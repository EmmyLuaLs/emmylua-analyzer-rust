use super::*;
use crate::context::{ServerContext, ServerContextSnapshot};
use emmylua_code_analysis::{Emmyrc, FileId, file_path_to_uri};
use lsp_server::{Connection, Message};
use lsp_types::{
    ClientCapabilities, DidChangeWatchedFilesClientCapabilities, PublishDiagnosticsParams,
    WorkspaceClientCapabilities,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static TEST_WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TestWorkspace {
    root: PathBuf,
}

impl TestWorkspace {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = TEST_WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "emmylua-workspace-manager-{}-{}-{}",
            std::process::id(),
            unique,
            counter,
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn write_file(&self, relative_path: &str) -> PathBuf {
        self.write_file_with_contents(relative_path, "return true\n")
    }

    fn write_file_with_contents(&self, relative_path: &str, contents: &str) -> PathBuf {
        let path = self.root.join(relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    fn path(&self, relative_path: &str) -> PathBuf {
        self.root.join(relative_path)
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap()
}

fn emmyrc_from_json(json: &str) -> Emmyrc {
    serde_json::from_str(json).unwrap()
}

fn dynamic_watch_capabilities() -> ClientCapabilities {
    ClientCapabilities {
        workspace: Some(WorkspaceClientCapabilities {
            did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
                dynamic_registration: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn collect_watch_registration_methods(client: &Connection, expected: usize) -> Vec<String> {
    let mut methods = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);

    while methods.len() < expected {
        let now = Instant::now();
        if now >= deadline {
            break;
        }

        let remaining = deadline.saturating_duration_since(now);
        match client.receiver.recv_timeout(remaining) {
            Ok(Message::Request(request))
                if request.method == "client/registerCapability"
                    || request.method == "client/unregisterCapability" =>
            {
                methods.push(request.method);
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    methods
}

fn recv_publish_diagnostics_for_uri(
    client: &Connection,
    uri: &lsp_types::Uri,
    timeout: Duration,
) -> Option<PublishDiagnosticsParams> {
    let deadline = Instant::now() + timeout;

    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match client.receiver.recv_timeout(remaining) {
            Ok(Message::Notification(notification))
                if notification.method == "textDocument/publishDiagnostics" =>
            {
                let params: PublishDiagnosticsParams =
                    serde_json::from_value(notification.params).ok()?;
                if &params.uri == uri {
                    return Some(params);
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    None
}

async fn file_text(snapshot: &ServerContextSnapshot, uri: &lsp_types::Uri) -> Option<String> {
    let analysis = snapshot.analysis().read().await;
    let file_id = analysis.get_file_id(uri)?;
    analysis
        .compilation
        .get_db()
        .get_vfs()
        .get_file_content(&file_id)
        .map(|text| text.to_string())
}

// Run the same initialization path that startup uses for a single workspace root.
async fn run_init_analysis(snapshot: &ServerContextSnapshot, workspace_root: PathBuf) {
    init_analysis(
        snapshot.analysis(),
        snapshot.status_bar(),
        snapshot.file_diagnostic(),
        snapshot.lsp_features(),
        vec![WorkspaceFolder::new(workspace_root, false)],
        Arc::new(Emmyrc::default()),
        Vec::new(),
    )
    .await;
}

// Read the generated remote schema file and confirm it is fully resolved.
async fn resolved_remote_schema(snapshot: &ServerContextSnapshot) -> (FileId, String) {
    let analysis = snapshot.analysis().read().await;
    let vfs = analysis.compilation.get_db().get_vfs();
    let remote_file_ids = vfs
        .get_all_file_ids()
        .into_iter()
        .filter(|file_id| vfs.is_remote_file(file_id))
        .collect::<Vec<_>>();
    assert_eq!(remote_file_ids.len(), 1);

    let remote_file_id = remote_file_ids[0];
    let remote_content = vfs
        .get_file_content(&remote_file_id)
        .cloned()
        .expect("resolved schema content should be present");
    assert!(remote_content.contains("auto-generated from JSON Schema"));
    assert!(remote_content.contains("---@field name"));
    assert!(!analysis.check_schema_update());

    (remote_file_id, remote_content)
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_workspace_reload_rebuilds_external_root_watchers() {
    let workspace = TestWorkspace::new();
    let external_library = TestWorkspace::new();
    let library_root = external_library.path("runtime/lua/vim");
    fs::create_dir_all(&library_root).unwrap();
    let (server, client) = Connection::memory();
    let context = ServerContext::new(server, dynamic_watch_capabilities());
    let snapshot = context.snapshot();
    let workspace_folders = vec![WorkspaceFolder::new(workspace.root.clone(), false)];

    {
        let mut workspace_manager = snapshot.workspace_manager().write().await;
        workspace_manager.workspace_folders = workspace_folders.clone();
    }

    apply_workspace_reload(
        snapshot.clone(),
        workspace_folders,
        Arc::new(emmyrc_from_json(&format!(
            r#"{{
                "workspace": {{
                    "library": [{}]
                }}
            }}"#,
            json_string(&to_string(&library_root)),
        ))),
    )
    .await;

    assert_eq!(
        collect_watch_registration_methods(&client, 2),
        vec![
            "client/unregisterCapability".to_string(),
            "client/registerCapability".to_string(),
        ]
    );
    assert!(snapshot.workspace_manager().read().await.watcher.is_some());

    snapshot
        .file_diagnostic()
        .cancel_workspace_diagnostic()
        .await;
    context.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn config_reload_is_not_cancelled_by_reindex() {
    let workspace = TestWorkspace::new();
    let library_root = workspace.path(".test-deps/runtime/lua/vim");
    let library_file = workspace.write_file(".test-deps/runtime/lua/vim/shared.lua");
    let library_uri = file_path_to_uri(&library_file).unwrap();
    let config_path = workspace.path(".emmyrc.json");
    fs::write(
        &config_path,
        format!(
            r#"{{
                "workspace": {{
                    "library": [{}]
                }}
            }}"#,
            json_string(&to_string(&library_root)),
        ),
    )
    .unwrap();
    let (server, _client) = Connection::memory();
    let context = ServerContext::new(server, dynamic_watch_capabilities());
    let snapshot = context.snapshot();

    {
        let mut workspace_manager = snapshot.workspace_manager().write().await;
        workspace_manager.workspace_folders =
            vec![WorkspaceFolder::new(workspace.root.clone(), false)];
    }

    snapshot
        .workspace_manager()
        .read()
        .await
        .add_update_emmyrc_task(snapshot.clone(), config_path);
    snapshot
        .workspace_manager()
        .read()
        .await
        .reindex_workspace(Duration::from_millis(50));

    tokio::time::sleep(CONFIG_RELOAD_DELAY + Duration::from_millis(250)).await;

    assert!(
        snapshot
            .analysis()
            .read()
            .await
            .get_file_id(&library_uri)
            .is_some()
    );

    snapshot
        .file_diagnostic()
        .cancel_workspace_diagnostic()
        .await;
    context.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_workspace_reload_loads_configured_workspace_roots() {
    let workspace = TestWorkspace::new();
    let extra_workspace = TestWorkspace::new();
    let extra_file = extra_workspace.write_file("lua/shared.lua");
    let extra_uri = file_path_to_uri(&extra_file).unwrap();
    let (server, _client) = Connection::memory();
    let context = ServerContext::new(server, ClientCapabilities::default());
    let snapshot = context.snapshot();
    let workspace_folders = vec![WorkspaceFolder::new(workspace.root.clone(), false)];

    {
        let mut workspace_manager = snapshot.workspace_manager().write().await;
        workspace_manager.workspace_folders = workspace_folders.clone();
    }

    apply_workspace_reload(
        snapshot.clone(),
        workspace_folders,
        Arc::new(emmyrc_from_json(&format!(
            r#"{{
                "workspace": {{
                    "workspaceRoots": [{}]
                }}
            }}"#,
            json_string(&to_string(&extra_workspace.root)),
        ))),
    )
    .await;

    assert!(
        snapshot
            .analysis()
            .read()
            .await
            .get_file_id(&extra_uri)
            .is_some()
    );

    snapshot
        .file_diagnostic()
        .cancel_workspace_diagnostic()
        .await;
    context.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_workspace_reload_removes_library_files_and_clears_diagnostics() {
    let workspace = TestWorkspace::new();
    let external_library = TestWorkspace::new();
    let library_root = external_library.path("runtime/lua/vim");
    let library_file = external_library.write_file("runtime/lua/vim/shared.lua");
    let library_uri = file_path_to_uri(&library_file).unwrap();
    let (server, client) = Connection::memory();
    let context = ServerContext::new(server, ClientCapabilities::default());
    let snapshot = context.snapshot();
    let workspace_folders = vec![WorkspaceFolder::new(workspace.root.clone(), false)];

    {
        let mut workspace_manager = snapshot.workspace_manager().write().await;
        workspace_manager.workspace_folders = workspace_folders.clone();
    }

    let library_emmyrc = Arc::new(emmyrc_from_json(&format!(
        r#"{{
            "workspace": {{
                "library": [{}]
            }}
        }}"#,
        json_string(&to_string(&library_root)),
    )));

    apply_workspace_reload(snapshot.clone(), workspace_folders.clone(), library_emmyrc).await;
    assert!(
        snapshot
            .analysis()
            .read()
            .await
            .get_file_id(&library_uri)
            .is_some()
    );

    let _ = recv_publish_diagnostics_for_uri(&client, &library_uri, Duration::from_millis(100));

    apply_workspace_reload(
        snapshot.clone(),
        workspace_folders,
        Arc::new(Emmyrc::default()),
    )
    .await;

    assert!(
        snapshot
            .analysis()
            .read()
            .await
            .get_file_id(&library_uri)
            .is_none()
    );
    let cleared = recv_publish_diagnostics_for_uri(&client, &library_uri, Duration::from_secs(5))
        .expect("expected cleared diagnostics for removed library file");
    assert!(cleared.diagnostics.is_empty());

    snapshot
        .file_diagnostic()
        .cancel_workspace_diagnostic()
        .await;
    context.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_workspace_reload_preserves_unsaved_open_text_across_membership_changes() {
    let workspace = TestWorkspace::new();
    let external_library = TestWorkspace::new();
    let library_root = external_library.path("runtime/lua/vim");
    let library_file = external_library.write_file("runtime/lua/vim/shared.lua");
    let library_uri = file_path_to_uri(&library_file).unwrap();
    let unsaved_text = "return 'open-buffer'\n";
    let (server, _client) = Connection::memory();
    let context = ServerContext::new(server, ClientCapabilities::default());
    let snapshot = context.snapshot();
    let workspace_folders = vec![WorkspaceFolder::new(workspace.root.clone(), false)];

    {
        let mut workspace_manager = snapshot.workspace_manager().write().await;
        workspace_manager.workspace_folders = workspace_folders.clone();
        workspace_manager.sync_open_file(library_uri.clone(), unsaved_text.to_string());
    }

    let library_emmyrc = Arc::new(emmyrc_from_json(&format!(
        r#"{{
            "workspace": {{
                "library": [{}]
            }}
        }}"#,
        json_string(&to_string(&library_root)),
    )));

    apply_workspace_reload(
        snapshot.clone(),
        workspace_folders.clone(),
        library_emmyrc.clone(),
    )
    .await;
    assert_eq!(
        file_text(&snapshot, &library_uri).await.as_deref(),
        Some(unsaved_text)
    );

    apply_workspace_reload(
        snapshot.clone(),
        workspace_folders.clone(),
        Arc::new(Emmyrc::default()),
    )
    .await;
    assert!(
        snapshot
            .analysis()
            .read()
            .await
            .get_file_id(&library_uri)
            .is_none()
    );

    apply_workspace_reload(snapshot.clone(), workspace_folders, library_emmyrc).await;
    assert_eq!(
        file_text(&snapshot, &library_uri).await.as_deref(),
        Some(unsaved_text)
    );

    snapshot
        .file_diagnostic()
        .cancel_workspace_diagnostic()
        .await;
    context.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn init_analysis_preserves_resolved_schema_files_on_startup() {
    // Create a workspace that resolves a schema through the normal `---@schema` path.
    let workspace = TestWorkspace::new();
    let schema_path = workspace.write_file_with_contents(
        "schemas/config.schema.json",
        r#"{
            "title": "Config",
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            }
        }"#,
    );
    let schema_uri = file_path_to_uri(&schema_path).unwrap();
    workspace.write_file_with_contents(
        "lua/main.lua",
        &format!(
            "---@schema {}\nlocal config = {{}}\nreturn config\n",
            schema_uri.as_str()
        ),
    );

    // Build a normal server context so the test exercises the real startup flow.
    let (server, _client) = Connection::memory();
    let context = ServerContext::new(server, ClientCapabilities::default());
    let snapshot = context.snapshot();

    // The first init should discover the schema and materialize a generated remote file.
    run_init_analysis(&snapshot, workspace.root.clone()).await;
    let first_resolution = resolved_remote_schema(&snapshot).await;

    // Remove the source schema to prove the second init is reusing preserved state.
    fs::remove_file(&schema_path).unwrap();

    // A second init should keep the already-resolved remote schema alive.
    run_init_analysis(&snapshot, workspace.root.clone()).await;
    assert_eq!(resolved_remote_schema(&snapshot).await, first_resolution);

    // Clean up background diagnostic work started by init_analysis.
    snapshot
        .file_diagnostic()
        .cancel_workspace_diagnostic()
        .await;
    context.close().await;
}
