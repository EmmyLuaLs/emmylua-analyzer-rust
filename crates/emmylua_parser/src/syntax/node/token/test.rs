#[cfg(test)]
mod tests {
    use crate::{
        kind::{LuaSyntaxKind, LuaTokenKind},
        syntax::node::{float_token_value, int_token_value, string_token_value},
        LuaSyntaxNode, LuaSyntaxToken,
    };

    fn get_token(text: &str, kind: LuaTokenKind) -> LuaSyntaxToken {
        let mut builder = rowan::GreenNodeBuilder::new();
        builder.start_node(LuaSyntaxKind::Chunk.into());
        builder.token(kind.into(), text);
        builder.finish_node();
        let green = builder.finish();
        let root = LuaSyntaxNode::new_root(green);
        root.first_token().unwrap()
    }

    macro_rules! test_token_value {
        ($name:ident, $code:expr, $expected:expr, $kind:expr) => {
            #[test]
            fn $name() {
                let token = &get_token($code, $kind);
                let result = string_token_value(token);
                assert_eq!(result.unwrap(), $expected.to_string());
            }
        };
    }

    test_token_value!(
        test_string_token_value_normal,
        "\"hello\"",
        "hello",
        LuaTokenKind::TkString
    );
    test_token_value!(
        test_string_token_value_normal_2,
        "'hello'",
        "hello",
        LuaTokenKind::TkString
    );
    test_token_value!(
        test_string_token_value_long,
        "[[hello]]",
        "hello",
        LuaTokenKind::TkLongString
    );
    test_token_value!(
        test_string_token_value_escaped_quote,
        "\"he\\\"llo\"",
        "he\"llo",
        LuaTokenKind::TkString
    );
    test_token_value!(
        test_string_token_value_escaped_single_quote,
        "'he\\'llo'",
        "he'llo",
        LuaTokenKind::TkString
    );
    test_token_value!(
        test_string_token_value_multiline,
        "\"hello\nworld\"",
        "hello\nworld",
        LuaTokenKind::TkString
    );
    test_token_value!(
        test_string_token_value_empty,
        "\"\"",
        "",
        LuaTokenKind::TkString
    );
    test_token_value!(
        test_string_token_value_empty_single,
        "''",
        "",
        LuaTokenKind::TkString
    );
    test_token_value!(
        test_string_token_value_long_multiline,
        "[[hello\nworld]]",
        "hello\nworld",
        LuaTokenKind::TkLongString
    );
    test_token_value!(
        test_string_token_value_long_multiline2,
        "[===[\nhello]===]",
        "hello",
        LuaTokenKind::TkLongString
    );
    test_token_value!(
        test_string_token_value_long_multiline3,
        "[===[\r\n\r\nhello]===]",
        "\r\nhello",
        LuaTokenKind::TkLongString
    );
    test_token_value!(
        test_string_token_value_hex,
        "\"\\x68\\x65\\x6c\\x6c\\x6f\"",
        "hello",
        LuaTokenKind::TkString
    );
    test_token_value!(
        test_string_token_value_utf8,
        "\"\\u{1F600}\"",
        "😀",
        LuaTokenKind::TkString
    );
    test_token_value!(
        test_string_token_value_mixed,
        "\"hello\\x20\\u{1F600}\"",
        "hello 😀",
        LuaTokenKind::TkString
    );

    #[test]
    fn test_multi_line() {
        let code = r#"'\
        aaa'"#;
        let expected = r#"
        aaa"#;
        let token = &get_token(code, LuaTokenKind::TkString);
        let result = string_token_value(token);
        assert_eq!(result.unwrap(), expected.to_string());
    }

    #[test]
    fn test_multi_line2() {
        let code = r#"'\z
        aaa'"#;
        let expected = r#"aaa"#;
        let token = &get_token(code, LuaTokenKind::TkString);
        let result = string_token_value(token);
        assert_eq!(result.unwrap(), expected.to_string());
    }

    macro_rules! test_float_token_value {
        ($name:ident, $code:expr, $expected:expr) => {
            #[test]
            fn $name() {
                let token = &get_token($code, LuaTokenKind::TkFloat);
                let result = float_token_value(token);
                assert_eq!(result.unwrap(), $expected);
            }
        };
    }

    test_float_token_value!(test_number_token_value_float1, "123.0", 123.0);
    test_float_token_value!(test_number_token_value_float2, "123.456", 123.456);
    test_float_token_value!(test_number_token_value_float3, "0.123", 0.123);
    test_float_token_value!(test_number_token_value_float4, "1e10", 1e10);
    test_float_token_value!(test_number_token_value_float5, "1.23e-4", 1.23e-4);
    test_float_token_value!(test_number_token_value_float6, "1.23e+4", 1.23e+4);
    test_float_token_value!(
        test_number_token_value_hex_float1,
        "0x1.91eb851eb851fp+1",
        3.14
    );
    test_float_token_value!(test_number_token_value_hex_float2, "0x1.0p+1", 2.0);
    test_float_token_value!(test_number_token_value_hex_float3, "0x1.8p+1", 3.0);
    test_float_token_value!(test_number_token_value_hex_float4, "0x1.0p-1", 0.5);
    test_float_token_value!(test_number_token_value_hex_float5, "0x1.8p-1", 0.75);
    test_float_token_value!(test_number_token_value_hex_float6, "0xABCDE2", 11259362.0);

    macro_rules! test_int_token_value {
        ($name:ident, $code:expr, $expected:expr) => {
            #[test]
            fn $name() {
                let token = &get_token($code, LuaTokenKind::TkInt);
                let result = int_token_value(token);
                assert_eq!(result.unwrap(), $expected);
            }
        };
    }

    test_int_token_value!(test_number_token_value_int1, "123", 123);
    test_int_token_value!(test_number_token_value_int2, "0", 0);
    test_int_token_value!(test_number_token_value_int3, "0x1A", 26);
    test_int_token_value!(test_number_token_value_int4, "0xFF", 255);
    test_int_token_value!(test_number_token_value_int5, "0x10", 16);
    test_int_token_value!(test_number_token_value_int6, "0x7B", 123);
    test_int_token_value!(test_number_token_value_int7, "0x0", 0);
    test_int_token_value!(test_number_token_value_int8, "0x11LL", 17);
    test_int_token_value!(test_number_token_value_int9, "0b10101", 21);
}
