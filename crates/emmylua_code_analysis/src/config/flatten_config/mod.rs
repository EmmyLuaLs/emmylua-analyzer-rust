mod test;

use hashbrown::HashMap;

use serde_json::Value;

use crate::config::Emmyrc;

#[derive(Debug, Clone)]
pub struct FlattenConfigObject {
    config: HashMap<String, Value>,
}

impl FlattenConfigObject {
    pub fn parse(luals_json: Value) -> Self {
        let defaults = serde_json::to_value(Emmyrc::default()).unwrap_or(Value::Null);
        let mut config = HashMap::new();
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
    config: &mut HashMap<String, Value>,
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
    for (k, v) in &config.config {
        let keys: Vec<&str> = k.split('.').collect();
        let mut current = &mut emmyrc;
        for i in 0..keys.len() {
            let key = keys[i];
            if i == keys.len() - 1 {
                current[key] = v.clone();
            } else {
                current = current
                    .as_object_mut()
                    .expect("always an object")
                    .entry(key.to_string())
                    .or_insert(Value::Object(Default::default()));
            }
        }
    }
    emmyrc
}
