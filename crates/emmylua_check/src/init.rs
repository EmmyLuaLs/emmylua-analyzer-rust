use emmylua_code_analysis::{
    EmmyLuaAnalysis, WorkspaceFolder, build_workspace_folders, collect_workspace_files,
    load_configs, update_code_style,
};
use fern::Dispatch;
use log::LevelFilter;
use std::path::{Path, PathBuf};

fn root_from_configs(config_paths: &[PathBuf], fallback: &Path) -> PathBuf {
    if config_paths.len() != 1 {
        fallback.to_path_buf()
    } else {
        let config_path = &config_paths[0];
        // Need to convert to canonical path to ensure parent() is not an empty
        // string in the case the path is a relative basename.
        match config_path.canonicalize() {
            Ok(path) => path.parent().unwrap().to_path_buf(),
            Err(err) => {
                log::error!(
                    "Failed to canonicalize config path: \"{:?}\": {}",
                    config_path,
                    err
                );
                fallback.to_path_buf()
            }
        }
    }
}

pub fn setup_logger(verbose: bool) {
    let logger = Dispatch::new()
        .format(move |out, message, record| {
            let (color, reset) = match record.level() {
                log::Level::Error => ("\x1b[31m", "\x1b[0m"), // Red
                log::Level::Warn => ("\x1b[33m", "\x1b[0m"),  // Yellow
                log::Level::Info | log::Level::Debug | log::Level::Trace => ("", ""),
            };
            out.finish(format_args!(
                "{}{}: {}{}",
                color,
                record.level(),
                if verbose {
                    format!("({}) {}", record.target(), message)
                } else {
                    message.to_string()
                },
                reset
            ))
        })
        .level(if verbose {
            LevelFilter::Info
        } else {
            LevelFilter::Warn
        })
        .chain(std::io::stderr());

    if let Err(e) = logger.apply() {
        eprintln!("Failed to apply logger: {:?}", e);
    }
}

pub async fn load_workspace(
    main_path: PathBuf,
    cmd_workspace_folders: Vec<PathBuf>,
    config_paths: Option<Vec<PathBuf>>,
    ignore: Option<Vec<String>>,
) -> Option<EmmyLuaAnalysis> {
    let (config_files, config_root): (Vec<PathBuf>, PathBuf) =
        if let Some(config_paths) = config_paths {
            (
                config_paths.clone(),
                root_from_configs(&config_paths, &main_path),
            )
        } else {
            (
                vec![
                    main_path.join(".luarc.json"),
                    main_path.join(".emmyrc.json"),
                ]
                .into_iter()
                .filter(|path| path.exists())
                .collect(),
                main_path.clone(),
            )
        };

    let mut emmyrc = load_configs(config_files, None);
    log::info!(
        "Pre processing configurations using root: \"{}\"",
        config_root.display()
    );
    emmyrc.pre_process_emmyrc(&config_root);

    let workspace_folders = cmd_workspace_folders
        .iter()
        .map(|path| WorkspaceFolder::new(path.clone(), false))
        .collect::<Vec<WorkspaceFolder>>();
    let mut analysis = EmmyLuaAnalysis::new();
    analysis.update_config(emmyrc.clone().into());
    analysis.init_std_lib(None);

    let workspace_folders = build_workspace_folders(&workspace_folders, &emmyrc);
    for workspace in &workspace_folders {
        if workspace.is_library {
            analysis.add_library_workspace(workspace);
        } else {
            analysis.add_main_workspace(workspace.root.clone());
        }
    }

    let file_infos = collect_workspace_files(&workspace_folders, &analysis.emmyrc, None, ignore);
    let files = file_infos
        .into_iter()
        .filter_map(|file| {
            if file.path.ends_with(".editorconfig") {
                let file_path = PathBuf::from(file.path);
                let parent_dir = file_path
                    .parent()
                    .unwrap()
                    .to_path_buf()
                    .to_string_lossy()
                    .to_string()
                    .replace("\\", "/");
                let file_normalized = file_path.to_string_lossy().to_string().replace("\\", "/");
                update_code_style(&parent_dir, &file_normalized);
                None
            } else {
                Some(file.into_tuple())
            }
        })
        .collect();
    analysis.update_files_by_path(files);

    if analysis.check_schema_update() {
        analysis.update_schema().await;
    }

    Some(analysis)
}
