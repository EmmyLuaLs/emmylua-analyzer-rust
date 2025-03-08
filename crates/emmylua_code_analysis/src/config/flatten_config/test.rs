#[cfg(test)]
mod test {
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
}
