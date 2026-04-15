use std::{collections::HashSet, path::PathBuf};

use serde_json::Value;

use crate::{config::lua_loader::load_lua_config, read_file_with_encoding};

use super::{Emmyrc, flatten_config::FlattenConfigObject};

/// Load a config file into an Emmyrc-shaped JSON value without path preprocessing.
///
/// This keeps file-relative paths exactly as written in the source config. It is
/// intended for callers that need to inspect or edit the raw config shape before
/// writing it back to disk.
pub fn load_config_json_unprocessed(config_file: PathBuf) -> Value {
    let Some(config_value) = load_config_file_value(&config_file) else {
        log::info!("No valid config file found.");
        return Value::Object(Default::default());
    };

    FlattenConfigObject::parse(config_value).to_emmyrc()
}

/// Load config files into a final [`Emmyrc`].
///
/// Unlike [`load_config_json_unprocessed`], this is the semantic runtime loader.
/// Each file config is flattened and preprocessed relative to the directory that
/// declared it before configs are merged. That has to happen pre-merge because
/// relative settings such as `library`, `packages`, and per-entry ignores lose
/// their originating config path once they have been folded into one JSON value.
pub fn load_configs(config_files: Vec<PathBuf>, partial_emmyrcs: Option<Vec<Value>>) -> Emmyrc {
    let mut merged_emmyrc = None;

    for config_file in config_files {
        let Some(config_value) = load_config_file_value(&config_file) else {
            continue;
        };

        let mut config_emmyrc = FlattenConfigObject::parse(config_value).to_emmyrc();
        if let Some(config_root) = config_file.parent() {
            // Relative config paths only make sense while we still know which
            // config file they came from.
            if let Ok(mut emmyrc) = serde_json::from_value::<Emmyrc>(config_emmyrc.clone()) {
                let original_emmyrc = serde_json::to_value(&emmyrc).unwrap();
                emmyrc.pre_process_emmyrc(config_root);
                // Write back only the leaves changed by preprocessing. Replacing
                // the whole config would materialize defaults and change merge
                // semantics for sparse per-file configs.
                apply_changed_values(
                    &mut config_emmyrc,
                    &original_emmyrc,
                    serde_json::to_value(emmyrc).unwrap(),
                );
            }
        }

        if let Err(err) = serde_json::from_value::<Emmyrc>(config_emmyrc.clone()) {
            log::error!(
                "Failed to parse config file as emmyrc {:?}: {:?}",
                config_file,
                err
            );
            continue;
        }

        if let Some(merged_emmyrc) = merged_emmyrc.as_mut() {
            merge_values(merged_emmyrc, config_emmyrc);
        } else {
            merged_emmyrc = Some(config_emmyrc);
        }
    }

    if let Some(partial_emmyrcs) = partial_emmyrcs {
        // Partial configs are late overlays from the client, so they win over
        // file-backed config. They are not file-relative; callers preprocess
        // the merged runtime config against the active workspace root later.
        for partial_emmyrc in partial_emmyrcs {
            let partial_emmyrc = FlattenConfigObject::parse(partial_emmyrc).to_emmyrc();
            if let Some(merged_emmyrc) = merged_emmyrc.as_mut() {
                merge_values(merged_emmyrc, partial_emmyrc);
            } else {
                merged_emmyrc = Some(partial_emmyrc);
            }
        }
    }

    let Some(merged_emmyrc) = merged_emmyrc else {
        log::info!("No valid config file found.");
        return Emmyrc::default();
    };

    serde_json::from_value(merged_emmyrc).unwrap_or_else(|err| {
        log::error!("Failed to parse config: error: {:?}", err);
        Emmyrc::default()
    })
}

fn load_config_file_value(config_file: &PathBuf) -> Option<Value> {
    log::info!("Loading config file: {:?}", config_file);
    let config_content = match read_file_with_encoding(config_file, "utf-8") {
        Some(content) => content,
        None => {
            log::error!(
                "Failed to read config file: {:?}, error: File not found or unreadable",
                config_file
            );
            return None;
        }
    };

    if config_file.extension().and_then(|s| s.to_str()) == Some("lua") {
        match load_lua_config(&config_content) {
            Ok(value) => Some(value),
            Err(err) => {
                log::error!(
                    "Failed to parse lua config file: {:?}, error: {:?}",
                    config_file,
                    err
                );
                None
            }
        }
    } else {
        match serde_json::from_str(&config_content) {
            Ok(json) => Some(json),
            Err(err) => {
                log::error!(
                    "Failed to parse config file: {:?}, error: {:?}",
                    config_file,
                    err
                );
                None
            }
        }
    }
}

fn apply_changed_values(target: &mut Value, before: &Value, after: Value) {
    match (target, before, after) {
        (Value::Object(target_map), Value::Object(before_map), Value::Object(after_map)) => {
            for (key, after_value) in after_map {
                let Some(before_value) = before_map.get(&key) else {
                    continue;
                };

                if before_value == &after_value {
                    continue;
                }

                if let Some(target_value) = target_map.get_mut(&key) {
                    apply_changed_values(target_value, before_value, after_value);
                } else {
                    target_map.insert(key, after_value);
                }
            }
        }
        (target_slot, before_value, after_value) => {
            if before_value != &after_value {
                *target_slot = after_value;
            }
        }
    }
}

fn merge_values(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match base_map.get_mut(&key) {
                    Some(base_value) => {
                        merge_values(base_value, overlay_value);
                    }
                    None => {
                        base_map.insert(key, overlay_value);
                    }
                }
            }
        }
        (Value::Array(base_array), Value::Array(overlay_array)) => {
            let mut seen = HashSet::new();
            base_array.extend(
                overlay_array
                    .into_iter()
                    .filter(|item| seen.insert(item.clone())),
            );
        }
        (base_slot, overlay_value) => {
            *base_slot = overlay_value;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{Emmyrc, load_config_json_unprocessed, load_configs};

    static TEST_CONFIG_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestConfigRoot {
        root: PathBuf,
    }

    impl TestConfigRoot {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let counter = TEST_CONFIG_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "emmylua-config-loader-{}-{}-{}",
                std::process::id(),
                unique,
                counter,
            ));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn write_file(&self, relative_path: &str, contents: &str) -> PathBuf {
            let path = self.root.join(relative_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, contents).unwrap();
            path
        }

        fn path(&self, relative_path: &str) -> PathBuf {
            self.root.join(relative_path)
        }
    }

    impl Drop for TestConfigRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn to_string(path: &Path) -> String {
        path.to_string_lossy().to_string()
    }

    #[test]
    fn merged_configs_should_resolve_relative_workspace_paths_from_declaring_config_file() {
        let workspace = TestConfigRoot::new();
        let shared_config = workspace.write_file(
            "shared/config/.luarc.json",
            r#"{
                "workspace": {
                    "library": ["../shared-lib"]
                }
            }"#,
        );
        let workspace_config = workspace.write_file(
            "project/.luarc.json",
            r#"{
                "workspace": {
                    "library": ["./vendor"]
                }
            }"#,
        );
        let expected = vec![
            to_string(&workspace.path("shared/shared-lib")),
            to_string(&workspace.path("project/vendor")),
        ];

        let emmyrc = load_configs(vec![shared_config, workspace_config], None);

        let actual = emmyrc
            .workspace
            .library
            .iter()
            .map(|item| item.get_path().clone())
            .collect::<Vec<_>>();

        assert_eq!(actual, expected);
    }

    #[test]
    fn path_preprocessing_preserves_unrelated_workspace_fields() {
        let workspace = TestConfigRoot::new();
        let config = workspace.write_file(
            "project/.luarc.json",
            r#"{
                "workspace": {
                    "library": ["./vendor"],
                    "moduleMap": [
                        {
                            "pattern": "^(.*)$",
                            "replace": "$1"
                        }
                    ]
                }
            }"#,
        );

        let emmyrc = load_configs(vec![config], None);
        let actual = emmyrc
            .workspace
            .library
            .iter()
            .map(|item| item.get_path().clone())
            .collect::<Vec<_>>();

        assert_eq!(actual, vec![to_string(&workspace.path("project/vendor"))]);
        assert_eq!(emmyrc.workspace.module_map.len(), 1);
        assert_eq!(emmyrc.workspace.module_map[0].pattern, "^(.*)$");
    }

    #[test]
    fn raw_config_loader_keeps_relative_workspace_paths_as_authored() {
        let workspace = TestConfigRoot::new();
        let config = workspace.write_file(
            "project/.luarc.json",
            r#"{
                "workspace": {
                    "library": ["../shared-lib"]
                }
            }"#,
        );

        let emmyrc_value = load_config_json_unprocessed(config);
        let emmyrc: Emmyrc = serde_json::from_value(emmyrc_value).unwrap();

        let actual = emmyrc
            .workspace
            .library
            .iter()
            .map(|item| item.get_path().clone())
            .collect::<Vec<_>>();

        assert_eq!(actual, vec!["../shared-lib".to_string()]);
    }

    #[test]
    fn invalid_config_does_not_override_earlier_valid_settings() {
        let workspace = TestConfigRoot::new();
        let valid_config = workspace.write_file(
            "project/.luarc.json",
            r#"{
                "diagnostics": {
                    "enable": false
                }
            }"#,
        );
        let invalid_config = workspace.write_file(
            "project/invalid.luarc.json",
            r#"{
                "diagnostics": {
                    "enable": "oops"
                }
            }"#,
        );

        let emmyrc = load_configs(vec![valid_config, invalid_config], None);

        assert!(!emmyrc.diagnostics.enable);
    }

    #[test]
    fn dotted_partial_configs_are_flattened_before_merge() {
        let emmyrc = load_configs(
            vec![],
            Some(vec![serde_json::json!({
                "diagnostics.enable": false
            })]),
        );

        assert!(!emmyrc.diagnostics.enable);
    }

    #[test]
    fn later_valid_configs_do_not_restore_defaults() {
        let workspace = TestConfigRoot::new();
        let first_config = workspace.write_file(
            "project/first.luarc.json",
            r#"{
                "diagnostics": {
                    "enable": false
                }
            }"#,
        );
        let second_config = workspace.write_file(
            "project/second.luarc.json",
            r#"{
                "strict": {
                    "requirePath": true
                }
            }"#,
        );

        let emmyrc = load_configs(vec![first_config, second_config], None);

        assert!(!emmyrc.diagnostics.enable);
        assert!(emmyrc.strict.require_path);
    }

    #[test]
    fn later_workspace_path_config_does_not_restore_workspace_defaults() {
        let workspace = TestConfigRoot::new();
        let first_config = workspace.write_file(
            "project/first.luarc.json",
            r#"{
                "workspace": {
                    "enableReindex": true
                }
            }"#,
        );
        let second_config = workspace.write_file(
            "project/second.luarc.json",
            r#"{
                "workspace": {
                    "library": ["./vendor"]
                }
            }"#,
        );

        let emmyrc = load_configs(vec![first_config, second_config], None);

        assert!(emmyrc.workspace.enable_reindex);
        assert_eq!(
            emmyrc.workspace.library[0].get_path(),
            &to_string(&workspace.path("project/vendor"))
        );
    }
}
