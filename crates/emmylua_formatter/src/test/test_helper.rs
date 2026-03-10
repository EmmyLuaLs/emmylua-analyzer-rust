#[macro_export]
macro_rules! assert_format_with_config {
    ($input:expr, $expected:expr, $config:expr) => {{
        let input = $input.trim_start_matches('\n');
        let expected = $expected.trim_start_matches('\n');
        let result = $crate::reformat_lua_code(input, &$config);
        if result != expected {
            let result_lines: Vec<&str> = result.lines().collect();
            let expected_lines: Vec<&str> = expected.lines().collect();
            let max_lines = result_lines.len().max(expected_lines.len());

            let mut diff = String::new();
            diff.push_str("=== Formatting mismatch ===\n");
            diff.push_str(&format!("Input:\n{:?}\n\n", input));
            diff.push_str(&format!(
                "Expected ({} lines):\n{:?}\n\n",
                expected_lines.len(),
                expected
            ));
            diff.push_str(&format!(
                "Got ({} lines):\n{:?}\n\n",
                result_lines.len(),
                &result
            ));

            diff.push_str("Line diff:\n");
            for i in 0..max_lines {
                let exp = expected_lines.get(i).unwrap_or(&"<missing>");
                let got = result_lines.get(i).unwrap_or(&"<missing>");
                if exp != got {
                    diff.push_str(&format!("  line {}: DIFFER\n", i + 1));
                    diff.push_str(&format!("    expected: {:?}\n", exp));
                    diff.push_str(&format!("    got:      {:?}\n", got));
                }
            }

            panic!("{}", diff);
        }
    }};
}

#[macro_export]
macro_rules! assert_format {
    ($input:expr, $expected:expr) => {{
        let config = $crate::config::LuaFormatConfig::default();
        $crate::assert_format_with_config!($input, $expected, config)
    }};
}
