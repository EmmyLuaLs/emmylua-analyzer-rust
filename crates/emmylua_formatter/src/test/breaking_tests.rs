#[cfg(test)]
mod tests {
    use crate::{
        assert_format_with_config,
        config::{LayoutConfig, LuaFormatConfig},
    };

    #[test]
    fn test_long_binary_expr_breaking() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 80,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"local result = very_long_variable_name_aaa + another_long_variable_name_bbb + yet_another_variable_name_ccc + final_variable_name_ddd
"#,
            r#"
local result = very_long_variable_name_aaa + another_long_variable_name_bbb
    + yet_another_variable_name_ccc + final_variable_name_ddd
"#,
            config
        );
    }

    #[test]
    fn test_long_call_args_breaking() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 60,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"some_function(very_long_argument_one, very_long_argument_two, very_long_argument_three, very_long_argument_four)
"#,
            r#"
some_function(
    very_long_argument_one, very_long_argument_two,
    very_long_argument_three, very_long_argument_four
)
"#,
            config
        );
    }

    #[test]
    fn test_long_table_breaking() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 60,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"local t = { first_key = 1, second_key = 2, third_key = 3, fourth_key = 4, fifth_key = 5 }
"#,
            r#"
local t = {
    first_key = 1,
    second_key = 2,
    third_key = 3,
    fourth_key = 4,
    fifth_key = 5
}
"#,
            config
        );
    }

    #[test]
    fn test_multiline_table_input_reflows_in_auto_mode_when_width_allows() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 120,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"local t = {
    a = 1,
    b = 2,
}
"#,
            r#"local t = { a = 1, b = 2 }
"#,
            config
        );
    }

    #[test]
    fn test_table_with_nested_values_stays_inline_when_width_allows() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 120,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"local t = { user = { name = "a", age = 1 }, enabled = true }
"#,
            r#"local t = { user = { name = "a", age = 1 }, enabled = true }
"#,
            config
        );
    }

    #[test]
    fn test_binary_chain_operand_per_line_fixes_nested_call_breaking() {
        // Without the flag, the `and` chain does not fit, and the formatter
        // falls back to breaking inside the nested `getStorageValue` call
        // argument list instead of breaking at the chain's own operators.
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 120,
                prefer_binary_chain_operand_per_line: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"if player:getStorageValue(Storage.ExplorerSociety.TheIceMusic) >= 62 and player:getStorageValue(Storage.ExplorerSociety.QuestLine) >= 62 and player:removeItem(5022, 1) then
	player:getPosition():sendMagicEffect(CONST_ME_TELEPORT)
	player:teleportTo(carvingTP.position)
end
"#,
            r#"
if player:getStorageValue(Storage.ExplorerSociety.TheIceMusic) >= 62
    and player:getStorageValue(Storage.ExplorerSociety.QuestLine) >= 62
    and player:removeItem(5022, 1) then
    player:getPosition():sendMagicEffect(CONST_ME_TELEPORT)
    player:teleportTo(carvingTP.position)
end
"#,
            config
        );
    }

    #[test]
    fn test_binary_chain_operand_per_line_disabled_keeps_legacy_behavior() {
        // Regression guard: with the flag off (the default), behavior is
        // unchanged from before this option existed, including the
        // less-than-ideal nested-call break.
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 120,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"if player:getStorageValue(Storage.ExplorerSociety.TheIceMusic) >= 62 and player:getStorageValue(Storage.ExplorerSociety.QuestLine) >= 62 and player:removeItem(5022, 1) then
	player:getPosition():sendMagicEffect(CONST_ME_TELEPORT)
	player:teleportTo(carvingTP.position)
end
"#,
            r#"
if player:getStorageValue(Storage.ExplorerSociety.TheIceMusic) >= 62 and player:getStorageValue(
        Storage.ExplorerSociety.QuestLine
    )
        >= 62 and player:removeItem(5022, 1) then
    player:getPosition():sendMagicEffect(CONST_ME_TELEPORT)
    player:teleportTo(carvingTP.position)
end
"#,
            config
        );
    }

    #[test]
    fn test_binary_chain_operand_per_line_keeps_short_chain_flat() {
        // A chain that already fits on one line stays flat even with the
        // option enabled; the new candidate only wins when it is needed.
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 120,
                prefer_binary_chain_operand_per_line: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"if a and b and c then
	work()
end
"#,
            r#"
if a and b and c then
    work()
end
"#,
            config
        );
    }

    #[test]
    fn test_binary_chain_operand_per_line_does_not_override_shorter_fill_layout() {
        // When the existing fill/packed layout already fits without
        // overflowing max_line_width in fewer lines than one-operand-per-line
        // would need, the scorer keeps preferring the more compact layout.
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 100,
                prefer_binary_chain_operand_per_line: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"if get_value(namespace_alpha_beta) >= threshold_value and get_value(namespace_gamma_delta) >= threshold_value and remove_item(item_id_number, item_count) then
	do_something()
end
"#,
            r#"
if get_value(namespace_alpha_beta) >= threshold_value and get_value(namespace_gamma_delta)
        >= threshold_value and remove_item(item_id_number, item_count) then
    do_something()
end
"#,
            config
        );
    }

    #[test]
    fn test_binary_chain_operand_per_line_works_for_or_operator() {
        let config = LuaFormatConfig {
            layout: LayoutConfig {
                max_line_width: 120,
                prefer_binary_chain_operand_per_line: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_format_with_config!(
            r#"if player:getStorageValue(Storage.QuestSystem.FirstPartCompleted) == 1 or player:getStorageValue(Storage.QuestSystem.SecondPartCompleted) == 1 or player:getStorageValue(Storage.QuestSystem.ThirdPartCompleted) == 1 then
	work()
end
"#,
            r#"
if player:getStorageValue(Storage.QuestSystem.FirstPartCompleted) == 1
    or player:getStorageValue(Storage.QuestSystem.SecondPartCompleted) == 1
    or player:getStorageValue(Storage.QuestSystem.ThirdPartCompleted) == 1 then
    work()
end
"#,
            config
        );
    }
}
