#[cfg(test)]
mod tests {
    use crate::text::Reader;
    use crate::{
        LuaNonStdSymbol,
        lexer::{LexerConfig, LuaLexer},
        parser_error::LuaParseError,
    };

    #[test]
    fn test_all_lua_token() {
        let text = r#"#! /usr/bin/env lua
        local a = 1
        local b = 2.0
        local c = 0x3F
        local d = 0b1010
        local e = 1.2e3
        local f = 1.2e-3
        local g = 0x1.2p3
        local h = 0x1.2p-3
        local i = "string"
        local j = 'string'
        local k = [[long string]]
        local l = true
        local m = false
        local n = nil
        local o = function() end
        local p = {}
        local q = {1, 2, 3}
        local r = {a = 1, b = 2}
        local s = a + b
        local t = a - b
        local u = a * b
        local v = a / b
        local w = a // b
        local x = a % b
        local y = a ^ b
        local z = -a
        local aa = not a
        local ab = a == b
        local ac = a ~= b
        local ad = a < b
        local ae = a <= b
        local af = a > b
        local ag = a >= b
        local ah = a and b
        local ai = a or b
        local aj = a .. b
        local ak = #a
        local al = a[b]
        local am = a.b
        local an = a:b()
        local ao = a()
        local ap = a[1]
        local aq = a[1][2]
        local ar = a[1].b
        local as = a[1]:b()
        local at = a.b[1]
        local au = a.b:c()
        local av = a.b[1].c
        local aw = a.b[1]:c()
        a = 123
        do local a = 1 end
        while a do local a = 1 end
        repeat local a = 1 until a
        if a then local a = 1 end
        if a then local a = 1 elseif b then local a = 1 else local a = 1 end
        for a = 1, 10 do local a = 1 end
        for a, b in pairs({1, 2, 3}) do local a = 1 end
        for a, b in ipairs({1, 2, 3}) do local a = 1 end
        for a, b in next, {1, 2, 3} do local a = 1 end
        for a, b in pairs({1, 2, 3}) do break end
        for a, b in pairs({1, 2, 3}) do goto label end
        for a, b in pairs({1, 2, 3}) do return end
        ::label:: do end
        goto label
        return
        break
        function a() end
        function a.b() end
        function a:b() end
        function a.b.c() end

        "#;
        let config = LexerConfig::default();
        let mut errors: Vec<LuaParseError> = Vec::new();
        let mut lexer = LuaLexer::new(Reader::new(text), config, Some(&mut errors));
        let tokens = lexer.tokenize();
        // for token in &tokens {
        //     println!("{:?}", token);
        // }

        let test_str = tokens
            .iter()
            .map(|x| format!("{:?}", x))
            .collect::<Vec<String>>()
            .join("\n");
        let expected = r#"
LuaTokenData { kind: TkShebang, range: SourceRange { start_offset: 0, length: 19 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 19, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 20, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 28, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 33, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 34, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 35, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 36, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 37, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 38, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 39, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 40, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 48, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 53, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 54, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 55, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 56, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 57, length: 1 } }
LuaTokenData { kind: TkFloat, range: SourceRange { start_offset: 58, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 61, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 62, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 70, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 75, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 76, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 77, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 78, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 79, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 80, length: 4 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 84, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 85, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 93, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 98, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 99, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 100, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 101, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 102, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 103, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 104, length: 5 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 109, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 110, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 118, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 123, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 124, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 125, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 126, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 127, length: 1 } }
LuaTokenData { kind: TkFloat, range: SourceRange { start_offset: 128, length: 5 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 133, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 134, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 142, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 147, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 148, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 149, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 150, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 151, length: 1 } }
LuaTokenData { kind: TkFloat, range: SourceRange { start_offset: 152, length: 6 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 158, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 159, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 167, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 172, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 173, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 174, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 175, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 176, length: 1 } }
LuaTokenData { kind: TkFloat, range: SourceRange { start_offset: 177, length: 7 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 184, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 185, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 193, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 198, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 199, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 200, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 201, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 202, length: 1 } }
LuaTokenData { kind: TkFloat, range: SourceRange { start_offset: 203, length: 8 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 211, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 212, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 220, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 225, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 226, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 227, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 228, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 229, length: 1 } }
LuaTokenData { kind: TkString, range: SourceRange { start_offset: 230, length: 8 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 238, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 239, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 247, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 252, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 253, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 254, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 255, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 256, length: 1 } }
LuaTokenData { kind: TkString, range: SourceRange { start_offset: 257, length: 8 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 265, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 266, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 274, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 279, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 280, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 281, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 282, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 283, length: 1 } }
LuaTokenData { kind: TkLongString, range: SourceRange { start_offset: 284, length: 15 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 299, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 300, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 308, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 313, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 314, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 315, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 316, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 317, length: 1 } }
LuaTokenData { kind: TkTrue, range: SourceRange { start_offset: 318, length: 4 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 322, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 323, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 331, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 336, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 337, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 338, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 339, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 340, length: 1 } }
LuaTokenData { kind: TkFalse, range: SourceRange { start_offset: 341, length: 5 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 346, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 347, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 355, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 360, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 361, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 362, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 363, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 364, length: 1 } }
LuaTokenData { kind: TkNil, range: SourceRange { start_offset: 365, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 368, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 369, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 377, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 382, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 383, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 384, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 385, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 386, length: 1 } }
LuaTokenData { kind: TkFunction, range: SourceRange { start_offset: 387, length: 8 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 395, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 396, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 397, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 398, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 401, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 402, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 410, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 415, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 416, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 417, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 418, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 419, length: 1 } }
LuaTokenData { kind: TkLeftBrace, range: SourceRange { start_offset: 420, length: 1 } }
LuaTokenData { kind: TkRightBrace, range: SourceRange { start_offset: 421, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 422, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 423, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 431, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 436, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 437, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 438, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 439, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 440, length: 1 } }
LuaTokenData { kind: TkLeftBrace, range: SourceRange { start_offset: 441, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 442, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 443, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 444, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 445, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 446, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 447, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 448, length: 1 } }
LuaTokenData { kind: TkRightBrace, range: SourceRange { start_offset: 449, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 450, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 451, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 459, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 464, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 465, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 466, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 467, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 468, length: 1 } }
LuaTokenData { kind: TkLeftBrace, range: SourceRange { start_offset: 469, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 470, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 471, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 472, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 473, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 474, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 475, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 476, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 477, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 478, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 479, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 480, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 481, length: 1 } }
LuaTokenData { kind: TkRightBrace, range: SourceRange { start_offset: 482, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 483, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 484, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 492, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 497, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 498, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 499, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 500, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 501, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 502, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 503, length: 1 } }
LuaTokenData { kind: TkPlus, range: SourceRange { start_offset: 504, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 505, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 506, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 507, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 508, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 516, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 521, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 522, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 523, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 524, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 525, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 526, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 527, length: 1 } }
LuaTokenData { kind: TkMinus, range: SourceRange { start_offset: 528, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 529, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 530, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 531, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 532, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 540, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 545, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 546, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 547, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 548, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 549, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 550, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 551, length: 1 } }
LuaTokenData { kind: TkMul, range: SourceRange { start_offset: 552, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 553, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 554, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 555, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 556, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 564, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 569, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 570, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 571, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 572, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 573, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 574, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 575, length: 1 } }
LuaTokenData { kind: TkDiv, range: SourceRange { start_offset: 576, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 577, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 578, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 579, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 580, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 588, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 593, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 594, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 595, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 596, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 597, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 598, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 599, length: 1 } }
LuaTokenData { kind: TkIDiv, range: SourceRange { start_offset: 600, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 602, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 603, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 604, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 605, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 613, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 618, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 619, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 620, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 621, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 622, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 623, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 624, length: 1 } }
LuaTokenData { kind: TkMod, range: SourceRange { start_offset: 625, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 626, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 627, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 628, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 629, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 637, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 642, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 643, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 644, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 645, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 646, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 647, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 648, length: 1 } }
LuaTokenData { kind: TkPow, range: SourceRange { start_offset: 649, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 650, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 651, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 652, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 653, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 661, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 666, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 667, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 668, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 669, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 670, length: 1 } }
LuaTokenData { kind: TkMinus, range: SourceRange { start_offset: 671, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 672, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 673, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 674, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 682, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 687, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 688, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 690, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 691, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 692, length: 1 } }
LuaTokenData { kind: TkNot, range: SourceRange { start_offset: 693, length: 3 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 696, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 697, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 698, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 699, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 707, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 712, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 713, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 715, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 716, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 717, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 718, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 719, length: 1 } }
LuaTokenData { kind: TkEq, range: SourceRange { start_offset: 720, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 722, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 723, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 724, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 725, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 733, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 738, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 739, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 741, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 742, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 743, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 744, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 745, length: 1 } }
LuaTokenData { kind: TkNe, range: SourceRange { start_offset: 746, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 748, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 749, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 750, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 751, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 759, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 764, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 765, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 767, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 768, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 769, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 770, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 771, length: 1 } }
LuaTokenData { kind: TkLt, range: SourceRange { start_offset: 772, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 773, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 774, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 775, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 776, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 784, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 789, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 790, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 792, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 793, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 794, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 795, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 796, length: 1 } }
LuaTokenData { kind: TkLe, range: SourceRange { start_offset: 797, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 799, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 800, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 801, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 802, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 810, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 815, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 816, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 818, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 819, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 820, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 821, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 822, length: 1 } }
LuaTokenData { kind: TkGt, range: SourceRange { start_offset: 823, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 824, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 825, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 826, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 827, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 835, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 840, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 841, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 843, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 844, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 845, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 846, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 847, length: 1 } }
LuaTokenData { kind: TkGe, range: SourceRange { start_offset: 848, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 850, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 851, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 852, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 853, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 861, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 866, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 867, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 869, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 870, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 871, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 872, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 873, length: 1 } }
LuaTokenData { kind: TkAnd, range: SourceRange { start_offset: 874, length: 3 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 877, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 878, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 879, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 880, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 888, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 893, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 894, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 896, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 897, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 898, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 899, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 900, length: 1 } }
LuaTokenData { kind: TkOr, range: SourceRange { start_offset: 901, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 903, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 904, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 905, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 906, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 914, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 919, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 920, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 922, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 923, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 924, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 925, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 926, length: 1 } }
LuaTokenData { kind: TkConcat, range: SourceRange { start_offset: 927, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 929, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 930, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 931, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 932, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 940, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 945, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 946, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 948, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 949, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 950, length: 1 } }
LuaTokenData { kind: TkLen, range: SourceRange { start_offset: 951, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 952, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 953, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 954, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 962, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 967, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 968, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 970, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 971, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 972, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 973, length: 1 } }
LuaTokenData { kind: TkLeftBracket, range: SourceRange { start_offset: 974, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 975, length: 1 } }
LuaTokenData { kind: TkRightBracket, range: SourceRange { start_offset: 976, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 977, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 978, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 986, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 991, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 992, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 994, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 995, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 996, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 997, length: 1 } }
LuaTokenData { kind: TkDot, range: SourceRange { start_offset: 998, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 999, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1000, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1001, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1009, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1014, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1015, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1017, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1018, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1019, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1020, length: 1 } }
LuaTokenData { kind: TkColon, range: SourceRange { start_offset: 1021, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1022, length: 1 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 1023, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 1024, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1025, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1026, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1034, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1039, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1040, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1042, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1043, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1044, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1045, length: 1 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 1046, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 1047, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1048, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1049, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1057, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1062, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1063, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1065, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1066, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1067, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1068, length: 1 } }
LuaTokenData { kind: TkLeftBracket, range: SourceRange { start_offset: 1069, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1070, length: 1 } }
LuaTokenData { kind: TkRightBracket, range: SourceRange { start_offset: 1071, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1072, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1073, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1081, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1086, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1087, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1089, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1090, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1091, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1092, length: 1 } }
LuaTokenData { kind: TkLeftBracket, range: SourceRange { start_offset: 1093, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1094, length: 1 } }
LuaTokenData { kind: TkRightBracket, range: SourceRange { start_offset: 1095, length: 1 } }
LuaTokenData { kind: TkLeftBracket, range: SourceRange { start_offset: 1096, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1097, length: 1 } }
LuaTokenData { kind: TkRightBracket, range: SourceRange { start_offset: 1098, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1099, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1100, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1108, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1113, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1114, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1116, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1117, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1118, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1119, length: 1 } }
LuaTokenData { kind: TkLeftBracket, range: SourceRange { start_offset: 1120, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1121, length: 1 } }
LuaTokenData { kind: TkRightBracket, range: SourceRange { start_offset: 1122, length: 1 } }
LuaTokenData { kind: TkDot, range: SourceRange { start_offset: 1123, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1124, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1125, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1126, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1134, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1139, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1140, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1142, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1143, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1144, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1145, length: 1 } }
LuaTokenData { kind: TkLeftBracket, range: SourceRange { start_offset: 1146, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1147, length: 1 } }
LuaTokenData { kind: TkRightBracket, range: SourceRange { start_offset: 1148, length: 1 } }
LuaTokenData { kind: TkColon, range: SourceRange { start_offset: 1149, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1150, length: 1 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 1151, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 1152, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1153, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1154, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1162, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1167, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1168, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1170, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1171, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1172, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1173, length: 1 } }
LuaTokenData { kind: TkDot, range: SourceRange { start_offset: 1174, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1175, length: 1 } }
LuaTokenData { kind: TkLeftBracket, range: SourceRange { start_offset: 1176, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1177, length: 1 } }
LuaTokenData { kind: TkRightBracket, range: SourceRange { start_offset: 1178, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1179, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1180, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1188, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1193, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1194, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1196, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1197, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1198, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1199, length: 1 } }
LuaTokenData { kind: TkDot, range: SourceRange { start_offset: 1200, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1201, length: 1 } }
LuaTokenData { kind: TkColon, range: SourceRange { start_offset: 1202, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1203, length: 1 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 1204, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 1205, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1206, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1207, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1215, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1220, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1221, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1223, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1224, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1225, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1226, length: 1 } }
LuaTokenData { kind: TkDot, range: SourceRange { start_offset: 1227, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1228, length: 1 } }
LuaTokenData { kind: TkLeftBracket, range: SourceRange { start_offset: 1229, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1230, length: 1 } }
LuaTokenData { kind: TkRightBracket, range: SourceRange { start_offset: 1231, length: 1 } }
LuaTokenData { kind: TkDot, range: SourceRange { start_offset: 1232, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1233, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1234, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1235, length: 8 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1243, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1248, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1249, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1251, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1252, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1253, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1254, length: 1 } }
LuaTokenData { kind: TkDot, range: SourceRange { start_offset: 1255, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1256, length: 1 } }
LuaTokenData { kind: TkLeftBracket, range: SourceRange { start_offset: 1257, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1258, length: 1 } }
LuaTokenData { kind: TkRightBracket, range: SourceRange { start_offset: 1259, length: 1 } }
LuaTokenData { kind: TkColon, range: SourceRange { start_offset: 1260, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1261, length: 1 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 1262, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 1263, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1264, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1265, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1273, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1274, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1275, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1276, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1277, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1280, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1281, length: 8 } }
LuaTokenData { kind: TkDo, range: SourceRange { start_offset: 1289, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1291, length: 1 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1292, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1297, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1298, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1299, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1300, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1301, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1302, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1303, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1304, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1307, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1308, length: 8 } }
LuaTokenData { kind: TkWhile, range: SourceRange { start_offset: 1316, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1321, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1322, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1323, length: 1 } }
LuaTokenData { kind: TkDo, range: SourceRange { start_offset: 1324, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1326, length: 1 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1327, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1332, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1333, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1334, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1335, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1336, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1337, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1338, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1339, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1342, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1343, length: 8 } }
LuaTokenData { kind: TkRepeat, range: SourceRange { start_offset: 1351, length: 6 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1357, length: 1 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1358, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1363, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1364, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1365, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1366, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1367, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1368, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1369, length: 1 } }
LuaTokenData { kind: TkUntil, range: SourceRange { start_offset: 1370, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1375, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1376, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1377, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1378, length: 8 } }
LuaTokenData { kind: TkIf, range: SourceRange { start_offset: 1386, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1388, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1389, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1390, length: 1 } }
LuaTokenData { kind: TkThen, range: SourceRange { start_offset: 1391, length: 4 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1395, length: 1 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1396, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1401, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1402, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1403, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1404, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1405, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1406, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1407, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1408, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1411, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1412, length: 8 } }
LuaTokenData { kind: TkIf, range: SourceRange { start_offset: 1420, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1422, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1423, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1424, length: 1 } }
LuaTokenData { kind: TkThen, range: SourceRange { start_offset: 1425, length: 4 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1429, length: 1 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1430, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1435, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1436, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1437, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1438, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1439, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1440, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1441, length: 1 } }
LuaTokenData { kind: TkElseIf, range: SourceRange { start_offset: 1442, length: 6 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1448, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1449, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1450, length: 1 } }
LuaTokenData { kind: TkThen, range: SourceRange { start_offset: 1451, length: 4 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1455, length: 1 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1456, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1461, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1462, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1463, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1464, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1465, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1466, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1467, length: 1 } }
LuaTokenData { kind: TkElse, range: SourceRange { start_offset: 1468, length: 4 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1472, length: 1 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1473, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1478, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1479, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1480, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1481, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1482, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1483, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1484, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1485, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1488, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1489, length: 8 } }
LuaTokenData { kind: TkFor, range: SourceRange { start_offset: 1497, length: 3 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1500, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1501, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1502, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1503, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1504, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1505, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1506, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1507, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1508, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1510, length: 1 } }
LuaTokenData { kind: TkDo, range: SourceRange { start_offset: 1511, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1513, length: 1 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1514, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1519, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1520, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1521, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1522, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1523, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1524, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1525, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1526, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1529, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1530, length: 8 } }
LuaTokenData { kind: TkFor, range: SourceRange { start_offset: 1538, length: 3 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1541, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1542, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1543, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1544, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1545, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1546, length: 1 } }
LuaTokenData { kind: TkIn, range: SourceRange { start_offset: 1547, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1549, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1550, length: 5 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 1555, length: 1 } }
LuaTokenData { kind: TkLeftBrace, range: SourceRange { start_offset: 1556, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1557, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1558, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1559, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1560, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1561, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1562, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1563, length: 1 } }
LuaTokenData { kind: TkRightBrace, range: SourceRange { start_offset: 1564, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 1565, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1566, length: 1 } }
LuaTokenData { kind: TkDo, range: SourceRange { start_offset: 1567, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1569, length: 1 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1570, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1575, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1576, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1577, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1578, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1579, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1580, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1581, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1582, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1585, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1586, length: 8 } }
LuaTokenData { kind: TkFor, range: SourceRange { start_offset: 1594, length: 3 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1597, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1598, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1599, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1600, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1601, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1602, length: 1 } }
LuaTokenData { kind: TkIn, range: SourceRange { start_offset: 1603, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1605, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1606, length: 6 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 1612, length: 1 } }
LuaTokenData { kind: TkLeftBrace, range: SourceRange { start_offset: 1613, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1614, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1615, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1616, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1617, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1618, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1619, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1620, length: 1 } }
LuaTokenData { kind: TkRightBrace, range: SourceRange { start_offset: 1621, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 1622, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1623, length: 1 } }
LuaTokenData { kind: TkDo, range: SourceRange { start_offset: 1624, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1626, length: 1 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1627, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1632, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1633, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1634, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1635, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1636, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1637, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1638, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1639, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1642, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1643, length: 8 } }
LuaTokenData { kind: TkFor, range: SourceRange { start_offset: 1651, length: 3 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1654, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1655, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1656, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1657, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1658, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1659, length: 1 } }
LuaTokenData { kind: TkIn, range: SourceRange { start_offset: 1660, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1662, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1663, length: 4 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1667, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1668, length: 1 } }
LuaTokenData { kind: TkLeftBrace, range: SourceRange { start_offset: 1669, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1670, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1671, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1672, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1673, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1674, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1675, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1676, length: 1 } }
LuaTokenData { kind: TkRightBrace, range: SourceRange { start_offset: 1677, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1678, length: 1 } }
LuaTokenData { kind: TkDo, range: SourceRange { start_offset: 1679, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1681, length: 1 } }
LuaTokenData { kind: TkLocal, range: SourceRange { start_offset: 1682, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1687, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1688, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1689, length: 1 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 1690, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1691, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1692, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1693, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1694, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1697, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1698, length: 8 } }
LuaTokenData { kind: TkFor, range: SourceRange { start_offset: 1706, length: 3 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1709, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1710, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1711, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1712, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1713, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1714, length: 1 } }
LuaTokenData { kind: TkIn, range: SourceRange { start_offset: 1715, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1717, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1718, length: 5 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 1723, length: 1 } }
LuaTokenData { kind: TkLeftBrace, range: SourceRange { start_offset: 1724, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1725, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1726, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1727, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1728, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1729, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1730, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1731, length: 1 } }
LuaTokenData { kind: TkRightBrace, range: SourceRange { start_offset: 1732, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 1733, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1734, length: 1 } }
LuaTokenData { kind: TkDo, range: SourceRange { start_offset: 1735, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1737, length: 1 } }
LuaTokenData { kind: TkBreak, range: SourceRange { start_offset: 1738, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1743, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1744, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1747, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1748, length: 8 } }
LuaTokenData { kind: TkFor, range: SourceRange { start_offset: 1756, length: 3 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1759, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1760, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1761, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1762, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1763, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1764, length: 1 } }
LuaTokenData { kind: TkIn, range: SourceRange { start_offset: 1765, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1767, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1768, length: 5 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 1773, length: 1 } }
LuaTokenData { kind: TkLeftBrace, range: SourceRange { start_offset: 1774, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1775, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1776, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1777, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1778, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1779, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1780, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1781, length: 1 } }
LuaTokenData { kind: TkRightBrace, range: SourceRange { start_offset: 1782, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 1783, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1784, length: 1 } }
LuaTokenData { kind: TkDo, range: SourceRange { start_offset: 1785, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1787, length: 1 } }
LuaTokenData { kind: TkGoto, range: SourceRange { start_offset: 1788, length: 4 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1792, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1793, length: 5 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1798, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1799, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1802, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1803, length: 8 } }
LuaTokenData { kind: TkFor, range: SourceRange { start_offset: 1811, length: 3 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1814, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1815, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1816, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1817, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1818, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1819, length: 1 } }
LuaTokenData { kind: TkIn, range: SourceRange { start_offset: 1820, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1822, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1823, length: 5 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 1828, length: 1 } }
LuaTokenData { kind: TkLeftBrace, range: SourceRange { start_offset: 1829, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1830, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1831, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1832, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1833, length: 1 } }
LuaTokenData { kind: TkComma, range: SourceRange { start_offset: 1834, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1835, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 1836, length: 1 } }
LuaTokenData { kind: TkRightBrace, range: SourceRange { start_offset: 1837, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 1838, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1839, length: 1 } }
LuaTokenData { kind: TkDo, range: SourceRange { start_offset: 1840, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1842, length: 1 } }
LuaTokenData { kind: TkReturn, range: SourceRange { start_offset: 1843, length: 6 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1849, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1850, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1853, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1854, length: 8 } }
LuaTokenData { kind: TkDbColon, range: SourceRange { start_offset: 1862, length: 2 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1864, length: 5 } }
LuaTokenData { kind: TkDbColon, range: SourceRange { start_offset: 1869, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1871, length: 1 } }
LuaTokenData { kind: TkDo, range: SourceRange { start_offset: 1872, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1874, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1875, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1878, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1879, length: 8 } }
LuaTokenData { kind: TkGoto, range: SourceRange { start_offset: 1887, length: 4 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1891, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1892, length: 5 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1897, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1898, length: 8 } }
LuaTokenData { kind: TkReturn, range: SourceRange { start_offset: 1906, length: 6 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1912, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1913, length: 8 } }
LuaTokenData { kind: TkBreak, range: SourceRange { start_offset: 1921, length: 5 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1926, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1927, length: 8 } }
LuaTokenData { kind: TkFunction, range: SourceRange { start_offset: 1935, length: 8 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1943, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1944, length: 1 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 1945, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 1946, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1947, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1948, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1951, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1952, length: 8 } }
LuaTokenData { kind: TkFunction, range: SourceRange { start_offset: 1960, length: 8 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1968, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1969, length: 1 } }
LuaTokenData { kind: TkDot, range: SourceRange { start_offset: 1970, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1971, length: 1 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 1972, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 1973, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1974, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 1975, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 1978, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1979, length: 8 } }
LuaTokenData { kind: TkFunction, range: SourceRange { start_offset: 1987, length: 8 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 1995, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1996, length: 1 } }
LuaTokenData { kind: TkColon, range: SourceRange { start_offset: 1997, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 1998, length: 1 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 1999, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 2000, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 2001, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 2002, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 2005, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 2006, length: 8 } }
LuaTokenData { kind: TkFunction, range: SourceRange { start_offset: 2014, length: 8 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 2022, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 2023, length: 1 } }
LuaTokenData { kind: TkDot, range: SourceRange { start_offset: 2024, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 2025, length: 1 } }
LuaTokenData { kind: TkDot, range: SourceRange { start_offset: 2026, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 2027, length: 1 } }
LuaTokenData { kind: TkLeftParen, range: SourceRange { start_offset: 2028, length: 1 } }
LuaTokenData { kind: TkRightParen, range: SourceRange { start_offset: 2029, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 2030, length: 1 } }
LuaTokenData { kind: TkEnd, range: SourceRange { start_offset: 2031, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 2034, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 2035, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 2036, length: 8 } }
        "#;

        assert_eq!(expected.trim(), test_str);
    }

    #[test]
    fn test_non_std_tokens() {
        let text = r#"#! /usr/bin/env lua
        // bbbbb
        /*
        afafaf
        */
        a `b`
        a += 1
        a -= 2
        a *= 3
        a /= 4
        a %= 5
        a ^= 6
        a //= 7
        a |= 8
        a &= 9
        a <<= 10
        a >>= 11
        a || b
        a && b
        !a
        a != b
        continue
        "#;

        let mut config = LexerConfig::default();
        config.non_std_symbols.extends(vec![
            LuaNonStdSymbol::DoubleSlash,
            LuaNonStdSymbol::SlashStar,
            LuaNonStdSymbol::Backtick,
            LuaNonStdSymbol::PlusAssign,
            LuaNonStdSymbol::MinusAssign,
            LuaNonStdSymbol::StarAssign,
            LuaNonStdSymbol::SlashAssign,
            LuaNonStdSymbol::PercentAssign,
            LuaNonStdSymbol::CaretAssign,
            LuaNonStdSymbol::DoubleSlashAssign,
            LuaNonStdSymbol::PipeAssign,
            LuaNonStdSymbol::AmpAssign,
            LuaNonStdSymbol::ShiftLeftAssign,
            LuaNonStdSymbol::ShiftRightAssign,
            LuaNonStdSymbol::DoublePipe,
            LuaNonStdSymbol::DoubleAmp,
            LuaNonStdSymbol::Exclamation,
            LuaNonStdSymbol::NotEqual,
            LuaNonStdSymbol::Continue,
        ]);

        let mut errors: Vec<LuaParseError> = Vec::new();
        let mut lexer = LuaLexer::new(Reader::new(text), config, Some(&mut errors));
        let tokens = lexer.tokenize();

        let test_str = tokens
            .iter()
            .map(|x| format!("{:?}", x))
            .collect::<Vec<String>>()
            .join("\n");

        let expected = r#"
LuaTokenData { kind: TkShebang, range: SourceRange { start_offset: 0, length: 19 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 19, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 20, length: 8 } }
LuaTokenData { kind: TkShortComment, range: SourceRange { start_offset: 28, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 30, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 31, length: 5 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 36, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 37, length: 8 } }
LuaTokenData { kind: TkLongComment, range: SourceRange { start_offset: 45, length: 28 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 73, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 74, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 82, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 83, length: 1 } }
LuaTokenData { kind: TkString, range: SourceRange { start_offset: 84, length: 3 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 87, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 88, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 96, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 97, length: 1 } }
LuaTokenData { kind: TkPlusAssign, range: SourceRange { start_offset: 98, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 100, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 101, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 102, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 103, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 111, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 112, length: 1 } }
LuaTokenData { kind: TkMinusAssign, range: SourceRange { start_offset: 113, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 115, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 116, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 117, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 118, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 126, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 127, length: 1 } }
LuaTokenData { kind: TkStarAssign, range: SourceRange { start_offset: 128, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 130, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 131, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 132, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 133, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 141, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 142, length: 1 } }
LuaTokenData { kind: TkSlashAssign, range: SourceRange { start_offset: 143, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 145, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 146, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 147, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 148, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 156, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 157, length: 1 } }
LuaTokenData { kind: TkPercentAssign, range: SourceRange { start_offset: 158, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 160, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 161, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 162, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 163, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 171, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 172, length: 1 } }
LuaTokenData { kind: TkCaretAssign, range: SourceRange { start_offset: 173, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 175, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 176, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 177, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 178, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 186, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 187, length: 1 } }
LuaTokenData { kind: TkShortComment, range: SourceRange { start_offset: 188, length: 2 } }
LuaTokenData { kind: TkAssign, range: SourceRange { start_offset: 190, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 191, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 192, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 193, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 194, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 202, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 203, length: 1 } }
LuaTokenData { kind: TkPipeAssign, range: SourceRange { start_offset: 204, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 206, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 207, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 208, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 209, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 217, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 218, length: 1 } }
LuaTokenData { kind: TkAmpAssign, range: SourceRange { start_offset: 219, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 221, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 222, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 223, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 224, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 232, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 233, length: 1 } }
LuaTokenData { kind: TkShiftLeftAssign, range: SourceRange { start_offset: 234, length: 3 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 237, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 238, length: 2 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 240, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 241, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 249, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 250, length: 1 } }
LuaTokenData { kind: TkShiftRightAssign, range: SourceRange { start_offset: 251, length: 3 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 254, length: 1 } }
LuaTokenData { kind: TkInt, range: SourceRange { start_offset: 255, length: 2 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 257, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 258, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 266, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 267, length: 1 } }
LuaTokenData { kind: TkOr, range: SourceRange { start_offset: 268, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 270, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 271, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 272, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 273, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 281, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 282, length: 1 } }
LuaTokenData { kind: TkAnd, range: SourceRange { start_offset: 283, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 285, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 286, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 287, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 288, length: 8 } }
LuaTokenData { kind: TkNot, range: SourceRange { start_offset: 296, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 297, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 298, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 299, length: 8 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 307, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 308, length: 1 } }
LuaTokenData { kind: TkNe, range: SourceRange { start_offset: 309, length: 2 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 311, length: 1 } }
LuaTokenData { kind: TkName, range: SourceRange { start_offset: 312, length: 1 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 313, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 314, length: 8 } }
LuaTokenData { kind: TkBreak, range: SourceRange { start_offset: 322, length: 8 } }
LuaTokenData { kind: TkEndOfLine, range: SourceRange { start_offset: 330, length: 1 } }
LuaTokenData { kind: TkWhitespace, range: SourceRange { start_offset: 331, length: 8 } }
        "#;

        assert_eq!(expected.trim(), test_str.trim());
    }
}
