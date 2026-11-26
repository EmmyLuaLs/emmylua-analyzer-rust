#[cfg(test)]
mod test {
    use crate::config::Emmyrc;
    use crate::config::flatten_config::FlattenConfigObject;

    #[test]
    fn test_parse() {
        let luals_json = serde_json::json!({
            "config": {
                "runtime": {
                    "version": "Lua 5.1"
                },
                "completion": {
                    "enable": true
                },
                "diagnostics.enable": true
            }
        });
        let config = FlattenConfigObject::parse(luals_json);
        let emmyrc_json = config.to_emmyrc();
        let json_str = serde_json::to_string_pretty(&emmyrc_json).unwrap();
        let expected = r#"{
  "config": {
    "completion": {
      "enable": true
    },
    "diagnostics": {
      "enable": true
    },
    "runtime": {
      "version": "Lua 5.1"
    }
  }
}"#;
        assert_eq!(json_str, expected);
    }

    #[test]
    fn test_map_setting_keys_keep_their_dots() {
        for luals_json in [
            serde_json::json!({ "runtime.special": { "a.b": "require" } }),
            serde_json::json!({ "runtime": { "special": { "a.b": "require" } } }),
        ] {
            let emmyrc_json = FlattenConfigObject::parse(luals_json).to_emmyrc();
            let emmyrc: Emmyrc = serde_json::from_value(emmyrc_json).expect("valid config");
            assert!(emmyrc.runtime.special.contains_key("a.b"));
        }
    }
}
