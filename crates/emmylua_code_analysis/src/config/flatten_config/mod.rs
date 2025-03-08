mod test;

use std::collections::HashMap;

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct FlattenConfigObject {
    config: HashMap<String, Value>,
}

impl FlattenConfigObject {
    pub fn parse(luals_json: Value) -> Self {
        let mut config = HashMap::new();
        flatten_object("", &luals_json, &mut config);
        Self { config }
    }

    pub fn to_emmyrc(&self) -> Value {
        to_emmyrc_json(self)
    }
}

fn flatten_object(prefix: &str, val: &Value, config: &mut HashMap<String, Value>) {
    match val {
        Value::Object(map) => {
            for (k, v) in map.iter() {
                let new_key = if prefix.is_empty() {
                    k.to_owned()
                } else {
                    format!("{}.{}", prefix, k)
                };
                flatten_object(&new_key, v, config);
            }
        }
        _ => {
            config.insert(prefix.to_string(), val.clone());
        }
    }
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
                    .unwrap()
                    .entry(key.to_string())
                    .or_insert(Value::Object(Default::default()));
            }
        }
    }
    emmyrc
}
