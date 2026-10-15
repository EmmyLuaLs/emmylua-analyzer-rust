use std::{collections::HashSet, path::PathBuf};

use serde_json::Value;

use crate::{config::lua_loader::load_lua_config, read_file_with_encoding};

use super::{Emmyrc, flatten_config::FlattenConfigObject};

pub fn load_configs_raw(config_files: Vec<PathBuf>, partial_emmyrcs: Option<Vec<Value>>) -> Value {
    let mut config_jsons = Vec::new();

    for config_file in config_files {
        log::info!("Loading config file: {:?}", config_file);
        let config_content = match read_file_with_encoding(&config_file, "utf-8") {
            Some(content) => content,
            None => {
                log::error!(
                    "Failed to read config file: {:?}, error: File not found or unreadable",
                    config_file
                );
                continue;
            }
        };

        let config_value = if config_file.extension().and_then(|s| s.to_str()) == Some("lua") {
            match load_lua_config(&config_content) {
                Ok(value) => value,
                Err(e) => {
                    log::error!(
                        "Failed to parse lua config file: {:?}, error: {:?}",
                        config_file,
                        e
                    );
                    continue;
                }
            }
        } else {
            match serde_json::from_str(&config_content) {
                Ok(json) => json,
                Err(e) => {
                    log::error!(
                        "Failed to parse config file: {:?}, error: {:?}",
                        config_file,
                        e
                    );
                    continue;
                }
            }
        };

        config_jsons.push(config_value);
    }

    if let Some(partial_emmyrcs) = partial_emmyrcs {
        for partial_emmyrc in partial_emmyrcs {
            config_jsons.push(partial_emmyrc);
        }
    }

    if config_jsons.is_empty() {
        log::info!("No valid config file found.");
        Value::Object(Default::default())
    } else if config_jsons.len() == 1 {
        let first_config = config_jsons.into_iter().next().unwrap_or_else(|| {
            log::error!("No valid config file found.");
            Value::Object(Default::default())
        });

        let flatten_config = FlattenConfigObject::parse(first_config);
        flatten_config.to_emmyrc()
    } else {
        config_jsons
            .into_iter()
            // Normalize each layer before merging so dotted and nested keys
            // share the same precedence and array-append behavior.
            .map(|item| FlattenConfigObject::parse(item).to_emmyrc())
            .fold(Value::Object(Default::default()), |mut acc, item| {
                merge_values(&mut acc, item);
                acc
            })
    }
}

pub fn load_configs(config_files: Vec<PathBuf>, partial_emmyrcs: Option<Vec<Value>>) -> Emmyrc {
    let emmyrc_json_value = load_configs_raw(config_files, partial_emmyrcs);
    serde_json::from_value(emmyrc_json_value).unwrap_or_else(|err| {
        log::error!("Failed to parse config: error: {:?}", err);
        Emmyrc::default()
    })
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
    use super::{load_configs, load_configs_raw};
    use crate::config::EmmyrcLuaVersion;
    use serde_json::{Value, json};

    fn merge(layers: Vec<Value>) -> Value {
        load_configs_raw(Vec::new(), Some(layers))
    }

    #[test]
    fn mixed_key_notation_preserves_array_layers() {
        let dotted = json!({"workspace.ignoreDir": ["vendor"]});
        let nested = json!({"workspace": {"ignoreDir": ["managed-mod"]}});
        assert_eq!(
            merge(vec![dotted.clone(), nested.clone()])["workspace"]["ignoreDir"],
            json!(["vendor", "managed-mod"])
        );
        assert_eq!(
            merge(vec![nested, dotted])["workspace"]["ignoreDir"],
            json!(["managed-mod", "vendor"])
        );
    }

    #[test]
    fn mixed_key_notation_preserves_scalar_precedence() {
        assert_eq!(
            merge(vec![
                json!({"runtime.version": "Lua5.1"}),
                json!({"runtime": {"version": "Lua5.2"}}),
            ])["runtime"]["version"],
            "Lua5.2"
        );
        assert_eq!(
            merge(vec![
                json!({"runtime": {"version": "Lua5.2"}}),
                json!({"runtime.version": "Lua5.3"}),
            ])["runtime"]["version"],
            "Lua5.3"
        );
    }

    #[test]
    fn normalization_retains_other_settings_and_array_entry_objects() {
        let library = json!({"path": "library", "ignoreDir": ["test"]});
        let result = merge(vec![
            json!({"workspace.library": [library.clone()], "diagnostics.disable": ["unused"]}),
            json!({"workspace": {"ignoreDir": ["managed-mod"]}}),
        ]);
        assert_eq!(result["workspace"]["library"], json!([library]));
        assert_eq!(result["diagnostics"]["disable"], json!(["unused"]));
        assert_eq!(result["workspace"]["ignoreDir"], json!(["managed-mod"]));
    }

    #[test]
    fn same_notation_retains_existing_merge_behavior() {
        // Only duplicates within the appended overlay are removed, not matches in the base.
        assert_eq!(
            merge(vec![
                json!({"workspace": {"ignoreDir": ["first"]}}),
                json!({"workspace": {"ignoreDir": ["second", "second"]}}),
            ])["workspace"]["ignoreDir"],
            json!(["first", "second"])
        );
        assert_eq!(
            merge(vec![
                json!({"workspace": {"ignoreDir": ["first"]}}),
                json!({"workspace": {"ignoreDir": ["first", "second", "second"]}}),
            ])["workspace"]["ignoreDir"],
            json!(["first", "first", "second"])
        );
        assert_eq!(merge(vec![]), json!({}));
        assert_eq!(
            merge(vec![json!({"runtime.version": "Lua5.2"})]),
            json!({"runtime": {"version": "Lua5.2"}})
        );
    }

    #[test]
    fn empty_and_sparse_layers_preserve_values_and_array_order() {
        assert_eq!(
            merge(vec![json!({"diagnostics.enable": false}), json!({})]),
            json!({"diagnostics": {"enable": false}})
        );
        assert_eq!(
            merge(vec![json!({"runtime.version": "Lua5.2"}), json!({})]),
            json!({"runtime": {"version": "Lua5.2"}})
        );
        assert_eq!(
            merge(vec![
                json!({"runtime.version": "Lua5.2", "workspace.ignoreDir": ["first"]}),
                json!({}),
                json!({"workspace": {"ignoreDir": ["second"]}}),
                json!({}),
                json!({"workspace.ignoreDir": ["third"], "diagnostics.enable": false}),
            ]),
            json!({
                "runtime": {"version": "Lua5.2"},
                "workspace": {"ignoreDir": ["first", "second", "third"]},
                "diagnostics": {"enable": false},
            })
        );
    }

    #[test]
    fn normalization_is_sparse_and_preserves_leaf_values() {
        assert_eq!(merge(vec![json!({})]), json!({}));
        let entries = json!([{"path": "library", "custom.key": {"enabled": false}}, null]);
        let result = merge(vec![json!({
            "workspace.library": entries.clone(),
            "extension": {"enabled": false, "count": 0, "value": null, "list": []},
        })]);
        assert_eq!(
            result,
            json!({
                "workspace": {"library": entries},
                "extension": {"enabled": false, "count": 0, "value": null, "list": []},
            })
        );
    }

    #[test]
    fn single_layer_mixed_notation_keeps_existing_duplicate_key_precedence() {
        let layer = json!({
            "runtime.version": "Lua5.2",
            "workspace.ignoreDir": ["dotted"],
            "workspace": {"ignoreDir": ["nested"], "library": ["library"]},
        });
        // Traversal visits the nested workspace object first. The explicit
        // dotted key then replaces the same flattened path within this layer.
        let expected = json!({
            "runtime": {"version": "Lua5.2"},
            "workspace": {"ignoreDir": ["dotted"], "library": ["library"]},
        });
        assert_eq!(merge(vec![layer.clone()]), expected);
        assert_eq!(merge(vec![layer, json!({})]), expected);
    }

    #[test]
    fn dotted_child_survives_an_invalid_parent_repaired_by_a_later_layer() {
        let result = merge(vec![
            json!({"workspace": false, "workspace.ignoreDir": ["vendor"]}),
            json!({"workspace": {"library": ["lib"]}}),
        ]);
        assert_eq!(
            result,
            json!({"workspace": {"ignoreDir": ["vendor"], "library": ["lib"]}})
        );
    }

    #[test]
    fn mixed_layers_keep_unknown_keys_and_duplicates_across_layers() {
        let result = merge(vec![
            json!({"unknown.foo": 1, "workspace.ignoreDir": ["same"]}),
            json!({"unknown": {"bar": 2}, "workspace": {"ignoreDir": ["same", "new", "new"]}}),
        ]);
        assert_eq!(result["unknown"], json!({"foo": 1, "bar": 2}));
        assert_eq!(
            result["workspace"]["ignoreDir"],
            json!(["same", "same", "new"])
        );
    }

    #[test]
    fn later_valid_scalar_overrides_invalid_type_before_deserialization() {
        let layers = vec![
            json!({"runtime.version": false}),
            json!({"runtime": {"version": "Lua5.2"}}),
        ];
        assert_eq!(
            merge(layers.clone()),
            json!({"runtime": {"version": "Lua5.2"}})
        );
        assert_eq!(
            load_configs(Vec::new(), Some(layers)).runtime.version,
            EmmyrcLuaVersion::Lua52
        );
    }

    #[test]
    fn file_layers_and_partial_settings_share_normalization_and_precedence() {
        struct TestDirectory(std::path::PathBuf);
        impl Drop for TestDirectory {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        let directory = std::env::temp_dir().join(format!(
            "emmylua-config-layers-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir(&directory).unwrap();
        let _cleanup = TestDirectory(directory.clone());
        let base = directory.join("base.json");
        let empty = directory.join("empty.json");
        let overlay = directory.join("overlay.json");
        std::fs::write(
            &base,
            r#"{"runtime.version":"Lua5.1","workspace.ignoreDir":["base"]}"#,
        )
        .unwrap();
        std::fs::write(&empty, "{}").unwrap();
        std::fs::write(
            &overlay,
            r#"{"runtime":{"version":"Lua5.2"},"workspace":{"ignoreDir":["overlay"]}}"#,
        )
        .unwrap();
        let result = load_configs_raw(
            vec![base, empty, overlay],
            Some(vec![
                json!({"runtime.version": "Lua5.3", "workspace.ignoreDir": ["partial"]}),
            ]),
        );
        assert_eq!(
            result,
            json!({
                "runtime": {"version": "Lua5.3"},
                "workspace": {"ignoreDir": ["base", "overlay", "partial"]},
            })
        );
    }
}
