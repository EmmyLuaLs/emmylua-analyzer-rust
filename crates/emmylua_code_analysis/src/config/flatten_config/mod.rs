mod test;

use std::collections::BTreeMap;

use serde_json::Value;

use crate::config::Emmyrc;

#[derive(Debug, Clone)]
pub struct FlattenConfigObject {
    config: BTreeMap<String, Value>,
}

impl FlattenConfigObject {
    pub fn parse(luals_json: Value) -> Self {
        let defaults = serde_json::to_value(Emmyrc::default()).unwrap_or(Value::Null);
        let mut config = BTreeMap::new();
        flatten_object("", &luals_json, &defaults, &mut config);
        Self { config }
    }

    pub fn to_emmyrc(&self) -> Value {
        to_emmyrc_json(self)
    }
}

fn flatten_object(
    prefix: &str,
    val: &Value,
    defaults: &Value,
    config: &mut BTreeMap<String, Value>,
) {
    match val {
        Value::Object(map) if !is_map_setting(prefix, defaults) => {
            for (k, v) in map.iter() {
                let new_key = if prefix.is_empty() {
                    k.to_owned()
                } else {
                    format!("{}.{}", prefix, k)
                };
                flatten_object(&new_key, v, defaults, config);
            }
        }
        _ => {
            config.insert(prefix.to_string(), val.clone());
        }
    }
}

/// Whether `path` names a map setting such as `runtime.special`. Its keys are data, so they
/// are kept whole instead of being split into nested setting names at each `.`.
fn is_map_setting(path: &str, defaults: &Value) -> bool {
    path.split('.')
        .try_fold(defaults, |node, key| node.get(key))
        .and_then(Value::as_object)
        .is_some_and(|map| map.is_empty())
}

fn to_emmyrc_json(config: &FlattenConfigObject) -> Value {
    let mut emmyrc = Value::Object(Default::default());
    // The sorted paths visit a parent before any dotted children. A more
    // specific setting can therefore replace an invalid scalar parent.
    for (path, value) in &config.config {
        let mut node = &mut emmyrc;
        for key in path.split('.') {
            if !node.is_object() {
                *node = Value::Object(Default::default());
            }
            node = &mut node[key];
        }
        *node = value.clone();
    }
    emmyrc
}
