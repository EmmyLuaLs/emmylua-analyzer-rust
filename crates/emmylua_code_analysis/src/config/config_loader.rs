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
        let merge_config =
            config_jsons
                .into_iter()
                .fold(Value::Object(Default::default()), |mut acc, item| {
                    merge_values(&mut acc, item);
                    acc
                });
        let flatten_config = FlattenConfigObject::parse(merge_config.clone());
        flatten_config.to_emmyrc()
    }
}

pub fn load_configs(config_files: Vec<PathBuf>, partial_emmyrcs: Option<Vec<Value>>) -> Emmyrc {
    let emmyrc_json_value = load_configs_raw(config_files, partial_emmyrcs);
    serde_json::from_value(emmyrc_json_value.clone()).unwrap_or_else(|err| {
        log::error!("Failed to parse config: error: {:?}", err);
        parse_valid_settings(emmyrc_json_value)
    })
}

/// Keeps every setting that parses on its own, so one invalid value only drops that value.
fn parse_valid_settings(config: Value) -> Emmyrc {
    let mut valid = Value::Object(Default::default());
    if let Value::Object(sections) = config {
        for (section, value) in sections {
            keep_valid_setting(&mut valid, &mut vec![section], value);
        }
    }
    serde_json::from_value(valid).unwrap_or_default()
}

/// Adds `value` at `path` if the config still parses, otherwise keeps each of its entries that does.
fn keep_valid_setting(valid: &mut Value, path: &mut Vec<String>, value: Value) {
    let mut candidate = valid.clone();
    let slot = path
        .iter()
        .fold(&mut candidate, |node, key| &mut node[key.as_str()]);
    *slot = value.clone();
    let Err(err) = serde_json::from_value::<Emmyrc>(candidate.clone()) else {
        *valid = candidate;
        return;
    };
    match value {
        Value::Object(entries) if !entries.is_empty() => {
            for (key, value) in entries {
                path.push(key);
                keep_valid_setting(valid, path, value);
                path.pop();
            }
        }
        _ => log::error!(
            "Ignoring invalid config setting `{}`: {err}",
            path.join(".")
        ),
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
    use super::load_configs;
    use crate::{DiagnosticCode, DiagnosticSeveritySetting, EmmyrcLuaVersion};

    #[test]
    fn test_invalid_setting_keeps_the_rest_of_the_config() {
        let config = serde_json::json!({
            "diagnostics": {
                "severity": { "need-check-nil": "info", "undefined-global": "hint" },
                "disable": ["unused"]
            },
            "runtime": { "version": "LuaJIT" }
        });
        let emmyrc = load_configs(Vec::new(), Some(vec![config]));
        let severity = &emmyrc.diagnostics.severity;
        assert!(!severity.contains_key(&DiagnosticCode::NeedCheckNil));
        assert!(matches!(
            severity.get(&DiagnosticCode::UndefinedGlobal),
            Some(DiagnosticSeveritySetting::Hint)
        ));
        assert!(emmyrc.diagnostics.disable.contains(&DiagnosticCode::Unused));
        assert!(matches!(emmyrc.runtime.version, EmmyrcLuaVersion::LuaJIT));
    }
}
