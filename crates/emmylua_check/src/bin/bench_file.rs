use std::path::PathBuf;
use std::time::Instant;

use emmylua_code_analysis::{
    EmmyLuaAnalysis, WorkspaceFolder, build_workspace_folders, collect_workspace_files,
    file_path_to_uri, load_configs,
};
use tokio_util::sync::CancellationToken;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: bench_file <workspace-root> <target-lua-file>");
        std::process::exit(2);
    }
    let root = PathBuf::from(&args[1]);
    let target = PathBuf::from(&args[2]);

    let logger = fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!("{}: {}\n", record.level(), message))
        })
        .level(log::LevelFilter::Info)
        .chain(std::io::stderr());
    logger.apply().unwrap();

    let config_files = vec![root.join(".luarc.json"), root.join(".emmyrc.json")]
        .into_iter()
        .filter(|p| p.exists())
        .collect::<Vec<_>>();
    let mut emmyrc = load_configs(config_files, None);
    emmyrc.pre_process_emmyrc(&root);

    let mut analysis = EmmyLuaAnalysis::new();
    analysis.update_config(emmyrc.clone().into());
    analysis.init_std_lib(None);

    let workspace = WorkspaceFolder::new(root.clone(), false);
    let workspace_folders = build_workspace_folders(&[workspace], &emmyrc);
    for w in &workspace_folders {
        if w.is_library {
            analysis.add_library_workspace(w);
        } else {
            analysis.add_main_workspace(w.root.clone());
        }
    }

    let file_infos = collect_workspace_files(&workspace_folders, &analysis.emmyrc, None, None);
    let files = file_infos
        .into_iter()
        .map(|f| (PathBuf::from(f.path), Some(f.content)))
        .collect();
    analysis.update_files_by_path(files);

    let target_uri = file_path_to_uri(&target).expect("target uri");
    let file_id = analysis
        .get_file_id(&target_uri)
        .expect("target file id not found");

    let start = Instant::now();
    let _ = analysis.diagnose_file(file_id, CancellationToken::new());
    eprintln!("TOTAL {:?}", start.elapsed());
}
