pub mod cmd_args;
mod init;
mod output;
mod terminal_display;

pub use cmd_args::*;
use output::output_result;
use std::{error::Error, sync::Arc};
use tokio_util::sync::CancellationToken;

use crate::init::setup_logger;

pub async fn run_check(cmd_args: CmdArgs) -> Result<(), Box<dyn Error + Sync + Send>> {
    setup_logger(cmd_args.verbose);

    let cwd = std::env::current_dir()?;
    let workspaces: Vec<_> = cmd_args
        .workspace
        .into_iter()
        .map(|workspace| {
            let path = if workspace.is_absolute() {
                workspace
            } else {
                cwd.join(workspace)
            };
            // Canonicalize to resolve ".." components so the path matches
            // the workspace root registered via add_main_workspace.
            path.canonicalize().unwrap_or(path)
        })
        .collect();
    let main_path = workspaces
        .first()
        .ok_or("Failed to load workspace")?
        .clone();

    let analysis = match init::load_workspace(
        main_path.clone(),
        workspaces.clone(),
        cmd_args.config,
        cmd_args.ignore,
    )
    .await
    {
        Some(analysis) => analysis,
        None => {
            return Err("Failed to load workspace".into());
        }
    };

    let db = &analysis.salsa;
    let need_check_files = db.main_workspace_file_ids();

    let (sender, receiver) = tokio::sync::mpsc::channel(100);
    let analysis = Arc::new(analysis);
    let total_count = need_check_files.len();
    let task_analysis = analysis.clone();
    // Run the files sequentially in one worker. Salsa's database is shared and
    // concurrent diagnose calls on the same database are not safe here; sequential
    // execution is also a more accurate representation of the single-core LS path.
    let sender_for_task = sender.clone();
    tokio::spawn(async move {
        for file_id in need_check_files {
            let cancel_token = CancellationToken::new();
            let diagnostics = task_analysis.diagnose_file(file_id, cancel_token);
            if sender_for_task.send((file_id, diagnostics)).await.is_err() {
                break;
            }
        }
    });
    // Drop the original sender so the receiver can detect when the worker has finished.
    drop(sender);
    let db = &analysis.salsa;

    let exit_code = output_result(
        total_count,
        db,
        main_path,
        receiver,
        cmd_args.output_format,
        cmd_args.output,
        cmd_args.warnings_as_errors,
        cmd_args.severity,
    )
    .await;

    if exit_code != 0 {
        return Err(format!("exit code: {}", exit_code).into());
    }

    eprintln!("Check finished");
    Ok(())
}
