#[cfg(test)]
mod tests {
    use crate::{parser::ParserConfig, LuaParser};

    macro_rules! assert_ast_eq {
        ($lua_code:expr, $expected:expr) => {
            let tree = LuaParser::parse($lua_code, ParserConfig::default());
            let result = format!("{:#?}", tree.get_red_root()).trim().to_string();
            let expected = $expected.trim().to_string();
            assert_eq!(result, expected);
        };
    }

    #[allow(unused)]
    fn print_ast(lua_code: &str) {
        let tree = LuaParser::parse(lua_code, ParserConfig::default());
        println!("{:#?}", tree.get_red_root());
    }

    #[test]
    fn test_full_lua_syntax() {
        let code = r#"
            -- This is a comment
            local a = 10
            local b = "string"
            local c = { key = "value", 1, 2, 3 }

            function foo(x, y)
                if x > y then
                    return x
                else
                    return y
                end
            end

            for i = 1, 10 do
                print(i)
            end

            while a > 0 do
                a = a - 1
            end

            repeat
                a = a + 1
            until a == 10

            local mt = {
                __index = function(table, key)
                    return "default"
                end
            }

            setmetatable(c, mt)

            local d = c.key
            local e = c[1]
        "#;

        let result = r#"
Syntax(Chunk)@0..770
  Syntax(Block)@0..770
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..13 "            "
    Syntax(Comment)@13..33
      Token(TkNormalStart)@13..15 "--"
      Token(TkWhitespace)@15..16 " "
      Syntax(DocDescription)@16..33
        Token(TkDocDetail)@16..33 "This is a comment"
    Token(TkEndOfLine)@33..34 "\n"
    Token(TkWhitespace)@34..46 "            "
    Syntax(LocalStat)@46..58
      Token(TkLocal)@46..51 "local"
      Token(TkWhitespace)@51..52 " "
      Syntax(LocalName)@52..53
        Token(TkName)@52..53 "a"
      Token(TkWhitespace)@53..54 " "
      Token(TkAssign)@54..55 "="
      Token(TkWhitespace)@55..56 " "
      Syntax(LiteralExpr)@56..58
        Token(TkInt)@56..58 "10"
    Token(TkEndOfLine)@58..59 "\n"
    Token(TkWhitespace)@59..71 "            "
    Syntax(LocalStat)@71..89
      Token(TkLocal)@71..76 "local"
      Token(TkWhitespace)@76..77 " "
      Syntax(LocalName)@77..78
        Token(TkName)@77..78 "b"
      Token(TkWhitespace)@78..79 " "
      Token(TkAssign)@79..80 "="
      Token(TkWhitespace)@80..81 " "
      Syntax(LiteralExpr)@81..89
        Token(TkString)@81..89 "\"string\""
    Token(TkEndOfLine)@89..90 "\n"
    Token(TkWhitespace)@90..102 "            "
    Syntax(LocalStat)@102..138
      Token(TkLocal)@102..107 "local"
      Token(TkWhitespace)@107..108 " "
      Syntax(LocalName)@108..109
        Token(TkName)@108..109 "c"
      Token(TkWhitespace)@109..110 " "
      Token(TkAssign)@110..111 "="
      Token(TkWhitespace)@111..112 " "
      Syntax(TableObjectExpr)@112..138
        Token(TkLeftBrace)@112..113 "{"
        Token(TkWhitespace)@113..114 " "
        Syntax(TableFieldAssign)@114..127
          Token(TkName)@114..117 "key"
          Token(TkWhitespace)@117..118 " "
          Token(TkAssign)@118..119 "="
          Token(TkWhitespace)@119..120 " "
          Syntax(LiteralExpr)@120..127
            Token(TkString)@120..127 "\"value\""
        Token(TkComma)@127..128 ","
        Token(TkWhitespace)@128..129 " "
        Syntax(TableFieldValue)@129..130
          Syntax(LiteralExpr)@129..130
            Token(TkInt)@129..130 "1"
        Token(TkComma)@130..131 ","
        Token(TkWhitespace)@131..132 " "
        Syntax(TableFieldValue)@132..133
          Syntax(LiteralExpr)@132..133
            Token(TkInt)@132..133 "2"
        Token(TkComma)@133..134 ","
        Token(TkWhitespace)@134..135 " "
        Syntax(TableFieldValue)@135..136
          Syntax(LiteralExpr)@135..136
            Token(TkInt)@135..136 "3"
        Token(TkWhitespace)@136..137 " "
        Token(TkRightBrace)@137..138 "}"
    Token(TkEndOfLine)@138..139 "\n"
    Token(TkEndOfLine)@139..140 "\n"
    Token(TkWhitespace)@140..152 "            "
    Syntax(FuncStat)@152..315
      Token(TkFunction)@152..160 "function"
      Token(TkWhitespace)@160..161 " "
      Syntax(NameExpr)@161..164
        Token(TkName)@161..164 "foo"
      Syntax(ClosureExpr)@164..315
        Syntax(ParamList)@164..170
          Token(TkLeftParen)@164..165 "("
          Syntax(ParamName)@165..166
            Token(TkName)@165..166 "x"
          Token(TkComma)@166..167 ","
          Token(TkWhitespace)@167..168 " "
          Syntax(ParamName)@168..169
            Token(TkName)@168..169 "y"
          Token(TkRightParen)@169..170 ")"
        Syntax(Block)@170..312
          Token(TkEndOfLine)@170..171 "\n"
          Token(TkWhitespace)@171..187 "                "
          Syntax(IfStat)@187..299
            Token(TkIf)@187..189 "if"
            Token(TkWhitespace)@189..190 " "
            Syntax(BinaryExpr)@190..195
              Syntax(NameExpr)@190..191
                Token(TkName)@190..191 "x"
              Token(TkWhitespace)@191..192 " "
              Token(TkGt)@192..193 ">"
              Token(TkWhitespace)@193..194 " "
              Syntax(NameExpr)@194..195
                Token(TkName)@194..195 "y"
            Token(TkWhitespace)@195..196 " "
            Token(TkThen)@196..200 "then"
            Syntax(Block)@200..246
              Token(TkEndOfLine)@200..201 "\n"
              Token(TkWhitespace)@201..221 "                    "
              Syntax(ReturnStat)@221..229
                Token(TkReturn)@221..227 "return"
                Token(TkWhitespace)@227..228 " "
                Syntax(NameExpr)@228..229
                  Token(TkName)@228..229 "x"
              Token(TkEndOfLine)@229..230 "\n"
              Token(TkWhitespace)@230..246 "                "
            Syntax(ElseClauseStat)@246..296
              Token(TkElse)@246..250 "else"
              Syntax(Block)@250..296
                Token(TkEndOfLine)@250..251 "\n"
                Token(TkWhitespace)@251..271 "                    "
                Syntax(ReturnStat)@271..279
                  Token(TkReturn)@271..277 "return"
                  Token(TkWhitespace)@277..278 " "
                  Syntax(NameExpr)@278..279
                    Token(TkName)@278..279 "y"
                Token(TkEndOfLine)@279..280 "\n"
                Token(TkWhitespace)@280..296 "                "
            Token(TkEnd)@296..299 "end"
          Token(TkEndOfLine)@299..300 "\n"
          Token(TkWhitespace)@300..312 "            "
        Token(TkEnd)@312..315 "end"
    Token(TkEndOfLine)@315..316 "\n"
    Token(TkEndOfLine)@316..317 "\n"
    Token(TkWhitespace)@317..329 "            "
    Syntax(ForStat)@329..386
      Token(TkFor)@329..332 "for"
      Token(TkWhitespace)@332..333 " "
      Token(TkName)@333..334 "i"
      Token(TkWhitespace)@334..335 " "
      Token(TkAssign)@335..336 "="
      Token(TkWhitespace)@336..337 " "
      Syntax(LiteralExpr)@337..338
        Token(TkInt)@337..338 "1"
      Token(TkComma)@338..339 ","
      Token(TkWhitespace)@339..340 " "
      Syntax(LiteralExpr)@340..342
        Token(TkInt)@340..342 "10"
      Token(TkWhitespace)@342..343 " "
      Token(TkDo)@343..345 "do"
      Syntax(Block)@345..383
        Token(TkEndOfLine)@345..346 "\n"
        Token(TkWhitespace)@346..362 "                "
        Syntax(CallExprStat)@362..370
          Syntax(CallExpr)@362..370
            Syntax(NameExpr)@362..367
              Token(TkName)@362..367 "print"
            Syntax(CallArgList)@367..370
              Token(TkLeftParen)@367..368 "("
              Syntax(NameExpr)@368..369
                Token(TkName)@368..369 "i"
              Token(TkRightParen)@369..370 ")"
        Token(TkEndOfLine)@370..371 "\n"
        Token(TkWhitespace)@371..383 "            "
      Token(TkEnd)@383..386 "end"
    Token(TkEndOfLine)@386..387 "\n"
    Token(TkEndOfLine)@387..388 "\n"
    Token(TkWhitespace)@388..400 "            "
    Syntax(WhileStat)@400..456
      Token(TkWhile)@400..405 "while"
      Token(TkWhitespace)@405..406 " "
      Syntax(BinaryExpr)@406..411
        Syntax(NameExpr)@406..407
          Token(TkName)@406..407 "a"
        Token(TkWhitespace)@407..408 " "
        Token(TkGt)@408..409 ">"
        Token(TkWhitespace)@409..410 " "
        Syntax(LiteralExpr)@410..411
          Token(TkInt)@410..411 "0"
      Token(TkWhitespace)@411..412 " "
      Token(TkDo)@412..414 "do"
      Syntax(Block)@414..453
        Token(TkEndOfLine)@414..415 "\n"
        Token(TkWhitespace)@415..431 "                "
        Syntax(AssignStat)@431..440
          Syntax(NameExpr)@431..432
            Token(TkName)@431..432 "a"
          Token(TkWhitespace)@432..433 " "
          Token(TkAssign)@433..434 "="
          Token(TkWhitespace)@434..435 " "
          Syntax(BinaryExpr)@435..440
            Syntax(NameExpr)@435..436
              Token(TkName)@435..436 "a"
            Token(TkWhitespace)@436..437 " "
            Token(TkMinus)@437..438 "-"
            Token(TkWhitespace)@438..439 " "
            Syntax(LiteralExpr)@439..440
              Token(TkInt)@439..440 "1"
        Token(TkEndOfLine)@440..441 "\n"
        Token(TkWhitespace)@441..453 "            "
      Token(TkEnd)@453..456 "end"
    Token(TkEndOfLine)@456..457 "\n"
    Token(TkEndOfLine)@457..458 "\n"
    Token(TkWhitespace)@458..470 "            "
    Syntax(RepeatStat)@470..528
      Token(TkRepeat)@470..476 "repeat"
      Syntax(Block)@476..515
        Token(TkEndOfLine)@476..477 "\n"
        Token(TkWhitespace)@477..493 "                "
        Syntax(AssignStat)@493..502
          Syntax(NameExpr)@493..494
            Token(TkName)@493..494 "a"
          Token(TkWhitespace)@494..495 " "
          Token(TkAssign)@495..496 "="
          Token(TkWhitespace)@496..497 " "
          Syntax(BinaryExpr)@497..502
            Syntax(NameExpr)@497..498
              Token(TkName)@497..498 "a"
            Token(TkWhitespace)@498..499 " "
            Token(TkPlus)@499..500 "+"
            Token(TkWhitespace)@500..501 " "
            Syntax(LiteralExpr)@501..502
              Token(TkInt)@501..502 "1"
        Token(TkEndOfLine)@502..503 "\n"
        Token(TkWhitespace)@503..515 "            "
      Token(TkUntil)@515..520 "until"
      Token(TkWhitespace)@520..521 " "
      Syntax(BinaryExpr)@521..528
        Syntax(NameExpr)@521..522
          Token(TkName)@521..522 "a"
        Token(TkWhitespace)@522..523 " "
        Token(TkEq)@523..525 "=="
        Token(TkWhitespace)@525..526 " "
        Syntax(LiteralExpr)@526..528
          Token(TkInt)@526..528 "10"
    Token(TkEndOfLine)@528..529 "\n"
    Token(TkEndOfLine)@529..530 "\n"
    Token(TkWhitespace)@530..542 "            "
    Syntax(LocalStat)@542..672
      Token(TkLocal)@542..547 "local"
      Token(TkWhitespace)@547..548 " "
      Syntax(LocalName)@548..550
        Token(TkName)@548..550 "mt"
      Token(TkWhitespace)@550..551 " "
      Token(TkAssign)@551..552 "="
      Token(TkWhitespace)@552..553 " "
      Syntax(TableObjectExpr)@553..672
        Token(TkLeftBrace)@553..554 "{"
        Token(TkEndOfLine)@554..555 "\n"
        Token(TkWhitespace)@555..571 "                "
        Syntax(TableFieldAssign)@571..658
          Token(TkName)@571..578 "__index"
          Token(TkWhitespace)@578..579 " "
          Token(TkAssign)@579..580 "="
          Token(TkWhitespace)@580..581 " "
          Syntax(ClosureExpr)@581..658
            Token(TkFunction)@581..589 "function"
            Syntax(ParamList)@589..601
              Token(TkLeftParen)@589..590 "("
              Syntax(ParamName)@590..595
                Token(TkName)@590..595 "table"
              Token(TkComma)@595..596 ","
              Token(TkWhitespace)@596..597 " "
              Syntax(ParamName)@597..600
                Token(TkName)@597..600 "key"
              Token(TkRightParen)@600..601 ")"
            Syntax(Block)@601..655
              Token(TkEndOfLine)@601..602 "\n"
              Token(TkWhitespace)@602..622 "                    "
              Syntax(ReturnStat)@622..638
                Token(TkReturn)@622..628 "return"
                Token(TkWhitespace)@628..629 " "
                Syntax(LiteralExpr)@629..638
                  Token(TkString)@629..638 "\"default\""
              Token(TkEndOfLine)@638..639 "\n"
              Token(TkWhitespace)@639..655 "                "
            Token(TkEnd)@655..658 "end"
        Token(TkEndOfLine)@658..659 "\n"
        Token(TkWhitespace)@659..671 "            "
        Token(TkRightBrace)@671..672 "}"
    Token(TkEndOfLine)@672..673 "\n"
    Token(TkEndOfLine)@673..674 "\n"
    Token(TkWhitespace)@674..686 "            "
    Syntax(CallExprStat)@686..705
      Syntax(CallExpr)@686..705
        Syntax(NameExpr)@686..698
          Token(TkName)@686..698 "setmetatable"
        Syntax(CallArgList)@698..705
          Token(TkLeftParen)@698..699 "("
          Syntax(NameExpr)@699..700
            Token(TkName)@699..700 "c"
          Token(TkComma)@700..701 ","
          Token(TkWhitespace)@701..702 " "
          Syntax(NameExpr)@702..704
            Token(TkName)@702..704 "mt"
          Token(TkRightParen)@704..705 ")"
    Token(TkEndOfLine)@705..706 "\n"
    Token(TkEndOfLine)@706..707 "\n"
    Token(TkWhitespace)@707..719 "            "
    Syntax(LocalStat)@719..734
      Token(TkLocal)@719..724 "local"
      Token(TkWhitespace)@724..725 " "
      Syntax(LocalName)@725..726
        Token(TkName)@725..726 "d"
      Token(TkWhitespace)@726..727 " "
      Token(TkAssign)@727..728 "="
      Token(TkWhitespace)@728..729 " "
      Syntax(IndexExpr)@729..734
        Syntax(NameExpr)@729..730
          Token(TkName)@729..730 "c"
        Token(TkDot)@730..731 "."
        Token(TkName)@731..734 "key"
    Token(TkEndOfLine)@734..735 "\n"
    Token(TkWhitespace)@735..747 "            "
    Syntax(LocalStat)@747..761
      Token(TkLocal)@747..752 "local"
      Token(TkWhitespace)@752..753 " "
      Syntax(LocalName)@753..754
        Token(TkName)@753..754 "e"
      Token(TkWhitespace)@754..755 " "
      Token(TkAssign)@755..756 "="
      Token(TkWhitespace)@756..757 " "
      Syntax(IndexExpr)@757..761
        Syntax(NameExpr)@757..758
          Token(TkName)@757..758 "c"
        Token(TkLeftBracket)@758..759 "["
        Syntax(LiteralExpr)@759..760
          Token(TkInt)@759..760 "1"
        Token(TkRightBracket)@760..761 "]"
    Token(TkEndOfLine)@761..762 "\n"
    Token(TkWhitespace)@762..770 "        "
"#;
        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_expr() {
        let code = r#"
        local a = 1 + 2 + 3 + 4
        "#;

        let result = r#"
Syntax(Chunk)@0..41
  Syntax(Block)@0..41
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(LocalStat)@9..32
      Token(TkLocal)@9..14 "local"
      Token(TkWhitespace)@14..15 " "
      Syntax(LocalName)@15..16
        Token(TkName)@15..16 "a"
      Token(TkWhitespace)@16..17 " "
      Token(TkAssign)@17..18 "="
      Token(TkWhitespace)@18..19 " "
      Syntax(BinaryExpr)@19..32
        Syntax(BinaryExpr)@19..28
          Syntax(BinaryExpr)@19..24
            Syntax(LiteralExpr)@19..20
              Token(TkInt)@19..20 "1"
            Token(TkWhitespace)@20..21 " "
            Token(TkPlus)@21..22 "+"
            Token(TkWhitespace)@22..23 " "
            Syntax(LiteralExpr)@23..24
              Token(TkInt)@23..24 "2"
          Token(TkWhitespace)@24..25 " "
          Token(TkPlus)@25..26 "+"
          Token(TkWhitespace)@26..27 " "
          Syntax(LiteralExpr)@27..28
            Token(TkInt)@27..28 "3"
        Token(TkWhitespace)@28..29 " "
        Token(TkPlus)@29..30 "+"
        Token(TkWhitespace)@30..31 " "
        Syntax(LiteralExpr)@31..32
          Token(TkInt)@31..32 "4"
    Token(TkEndOfLine)@32..33 "\n"
    Token(TkWhitespace)@33..41 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_assign_stat() {
        let code = r#"
        a = 1
        b, c = 2, 3
        d, e = 4
        f, g = 5, 6, 7
        "#;

        let result = r#"
Syntax(Chunk)@0..83
  Syntax(Block)@0..83
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(AssignStat)@9..14
      Syntax(NameExpr)@9..10
        Token(TkName)@9..10 "a"
      Token(TkWhitespace)@10..11 " "
      Token(TkAssign)@11..12 "="
      Token(TkWhitespace)@12..13 " "
      Syntax(LiteralExpr)@13..14
        Token(TkInt)@13..14 "1"
    Token(TkEndOfLine)@14..15 "\n"
    Token(TkWhitespace)@15..23 "        "
    Syntax(AssignStat)@23..34
      Syntax(NameExpr)@23..24
        Token(TkName)@23..24 "b"
      Token(TkComma)@24..25 ","
      Token(TkWhitespace)@25..26 " "
      Syntax(NameExpr)@26..27
        Token(TkName)@26..27 "c"
      Token(TkWhitespace)@27..28 " "
      Token(TkAssign)@28..29 "="
      Token(TkWhitespace)@29..30 " "
      Syntax(LiteralExpr)@30..31
        Token(TkInt)@30..31 "2"
      Token(TkComma)@31..32 ","
      Token(TkWhitespace)@32..33 " "
      Syntax(LiteralExpr)@33..34
        Token(TkInt)@33..34 "3"
    Token(TkEndOfLine)@34..35 "\n"
    Token(TkWhitespace)@35..43 "        "
    Syntax(AssignStat)@43..51
      Syntax(NameExpr)@43..44
        Token(TkName)@43..44 "d"
      Token(TkComma)@44..45 ","
      Token(TkWhitespace)@45..46 " "
      Syntax(NameExpr)@46..47
        Token(TkName)@46..47 "e"
      Token(TkWhitespace)@47..48 " "
      Token(TkAssign)@48..49 "="
      Token(TkWhitespace)@49..50 " "
      Syntax(LiteralExpr)@50..51
        Token(TkInt)@50..51 "4"
    Token(TkEndOfLine)@51..52 "\n"
    Token(TkWhitespace)@52..60 "        "
    Syntax(AssignStat)@60..74
      Syntax(NameExpr)@60..61
        Token(TkName)@60..61 "f"
      Token(TkComma)@61..62 ","
      Token(TkWhitespace)@62..63 " "
      Syntax(NameExpr)@63..64
        Token(TkName)@63..64 "g"
      Token(TkWhitespace)@64..65 " "
      Token(TkAssign)@65..66 "="
      Token(TkWhitespace)@66..67 " "
      Syntax(LiteralExpr)@67..68
        Token(TkInt)@67..68 "5"
      Token(TkComma)@68..69 ","
      Token(TkWhitespace)@69..70 " "
      Syntax(LiteralExpr)@70..71
        Token(TkInt)@70..71 "6"
      Token(TkComma)@71..72 ","
      Token(TkWhitespace)@72..73 " "
      Syntax(LiteralExpr)@73..74
        Token(TkInt)@73..74 "7"
    Token(TkEndOfLine)@74..75 "\n"
    Token(TkWhitespace)@75..83 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_index_expr() {
        let code = r#"
        local t = a.b[c]["1123"]
        "#;

        let result = r#"
Syntax(Chunk)@0..42
  Syntax(Block)@0..42
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(LocalStat)@9..33
      Token(TkLocal)@9..14 "local"
      Token(TkWhitespace)@14..15 " "
      Syntax(LocalName)@15..16
        Token(TkName)@15..16 "t"
      Token(TkWhitespace)@16..17 " "
      Token(TkAssign)@17..18 "="
      Token(TkWhitespace)@18..19 " "
      Syntax(IndexExpr)@19..33
        Syntax(IndexExpr)@19..25
          Syntax(IndexExpr)@19..22
            Syntax(NameExpr)@19..20
              Token(TkName)@19..20 "a"
            Token(TkDot)@20..21 "."
            Token(TkName)@21..22 "b"
          Token(TkLeftBracket)@22..23 "["
          Syntax(NameExpr)@23..24
            Token(TkName)@23..24 "c"
          Token(TkRightBracket)@24..25 "]"
        Token(TkLeftBracket)@25..26 "["
        Syntax(LiteralExpr)@26..32
          Token(TkString)@26..32 "\"1123\""
        Token(TkRightBracket)@32..33 "]"
    Token(TkEndOfLine)@33..34 "\n"
    Token(TkWhitespace)@34..42 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_call_expr() {
        let code = r#"
        local a = foo(1, 2, 3)
        local c = aaa.bbbb:cccc()
        require "aaaa.bbbb"
        call {
            a = 1,
            b = 2,
            c = 3
        }
        "#;

        let result = r#"
Syntax(Chunk)@0..183
  Syntax(Block)@0..183
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(LocalStat)@9..31
      Token(TkLocal)@9..14 "local"
      Token(TkWhitespace)@14..15 " "
      Syntax(LocalName)@15..16
        Token(TkName)@15..16 "a"
      Token(TkWhitespace)@16..17 " "
      Token(TkAssign)@17..18 "="
      Token(TkWhitespace)@18..19 " "
      Syntax(CallExpr)@19..31
        Syntax(NameExpr)@19..22
          Token(TkName)@19..22 "foo"
        Syntax(CallArgList)@22..31
          Token(TkLeftParen)@22..23 "("
          Syntax(LiteralExpr)@23..24
            Token(TkInt)@23..24 "1"
          Token(TkComma)@24..25 ","
          Token(TkWhitespace)@25..26 " "
          Syntax(LiteralExpr)@26..27
            Token(TkInt)@26..27 "2"
          Token(TkComma)@27..28 ","
          Token(TkWhitespace)@28..29 " "
          Syntax(LiteralExpr)@29..30
            Token(TkInt)@29..30 "3"
          Token(TkRightParen)@30..31 ")"
    Token(TkEndOfLine)@31..32 "\n"
    Token(TkWhitespace)@32..40 "        "
    Syntax(LocalStat)@40..65
      Token(TkLocal)@40..45 "local"
      Token(TkWhitespace)@45..46 " "
      Syntax(LocalName)@46..47
        Token(TkName)@46..47 "c"
      Token(TkWhitespace)@47..48 " "
      Token(TkAssign)@48..49 "="
      Token(TkWhitespace)@49..50 " "
      Syntax(CallExpr)@50..65
        Syntax(IndexExpr)@50..63
          Syntax(IndexExpr)@50..58
            Syntax(NameExpr)@50..53
              Token(TkName)@50..53 "aaa"
            Token(TkDot)@53..54 "."
            Token(TkName)@54..58 "bbbb"
          Token(TkColon)@58..59 ":"
          Token(TkName)@59..63 "cccc"
        Syntax(CallArgList)@63..65
          Token(TkLeftParen)@63..64 "("
          Token(TkRightParen)@64..65 ")"
    Token(TkEndOfLine)@65..66 "\n"
    Token(TkWhitespace)@66..74 "        "
    Syntax(CallExprStat)@74..93
      Syntax(CallExpr)@74..93
        Syntax(NameExpr)@74..81
          Token(TkName)@74..81 "require"
        Token(TkWhitespace)@81..82 " "
        Syntax(CallArgList)@82..93
          Syntax(LiteralExpr)@82..93
            Token(TkString)@82..93 "\"aaaa.bbbb\""
    Token(TkEndOfLine)@93..94 "\n"
    Token(TkWhitespace)@94..102 "        "
    Syntax(CallExprStat)@102..174
      Syntax(CallExpr)@102..174
        Syntax(NameExpr)@102..106
          Token(TkName)@102..106 "call"
        Token(TkWhitespace)@106..107 " "
        Syntax(CallArgList)@107..174
          Syntax(TableObjectExpr)@107..174
            Token(TkLeftBrace)@107..108 "{"
            Token(TkEndOfLine)@108..109 "\n"
            Token(TkWhitespace)@109..121 "            "
            Syntax(TableFieldAssign)@121..126
              Token(TkName)@121..122 "a"
              Token(TkWhitespace)@122..123 " "
              Token(TkAssign)@123..124 "="
              Token(TkWhitespace)@124..125 " "
              Syntax(LiteralExpr)@125..126
                Token(TkInt)@125..126 "1"
            Token(TkComma)@126..127 ","
            Token(TkEndOfLine)@127..128 "\n"
            Token(TkWhitespace)@128..140 "            "
            Syntax(TableFieldAssign)@140..145
              Token(TkName)@140..141 "b"
              Token(TkWhitespace)@141..142 " "
              Token(TkAssign)@142..143 "="
              Token(TkWhitespace)@143..144 " "
              Syntax(LiteralExpr)@144..145
                Token(TkInt)@144..145 "2"
            Token(TkComma)@145..146 ","
            Token(TkEndOfLine)@146..147 "\n"
            Token(TkWhitespace)@147..159 "            "
            Syntax(TableFieldAssign)@159..164
              Token(TkName)@159..160 "c"
              Token(TkWhitespace)@160..161 " "
              Token(TkAssign)@161..162 "="
              Token(TkWhitespace)@162..163 " "
              Syntax(LiteralExpr)@163..164
                Token(TkInt)@163..164 "3"
            Token(TkEndOfLine)@164..165 "\n"
            Token(TkWhitespace)@165..173 "        "
            Token(TkRightBrace)@173..174 "}"
    Token(TkEndOfLine)@174..175 "\n"
    Token(TkWhitespace)@175..183 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_table_expr() {
        let code = r#"
        local t = {
            a = 1,
            ["aa"] = 2,
            [1] = 3
        }
        local d = {
            1,
            2,
            3
        }
        local c = {}
        local d = { a = 1, 1 }
        "#;
        print_ast(code);
        let result = r#"
Syntax(Chunk)@0..228
  Syntax(Block)@0..228
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(LocalStat)@9..93
      Token(TkLocal)@9..14 "local"
      Token(TkWhitespace)@14..15 " "
      Syntax(LocalName)@15..16
        Token(TkName)@15..16 "t"
      Token(TkWhitespace)@16..17 " "
      Token(TkAssign)@17..18 "="
      Token(TkWhitespace)@18..19 " "
      Syntax(TableObjectExpr)@19..93
        Token(TkLeftBrace)@19..20 "{"
        Token(TkEndOfLine)@20..21 "\n"
        Token(TkWhitespace)@21..33 "            "
        Syntax(TableFieldAssign)@33..38
          Token(TkName)@33..34 "a"
          Token(TkWhitespace)@34..35 " "
          Token(TkAssign)@35..36 "="
          Token(TkWhitespace)@36..37 " "
          Syntax(LiteralExpr)@37..38
            Token(TkInt)@37..38 "1"
        Token(TkComma)@38..39 ","
        Token(TkEndOfLine)@39..40 "\n"
        Token(TkWhitespace)@40..52 "            "
        Syntax(TableFieldAssign)@52..62
          Token(TkLeftBracket)@52..53 "["
          Syntax(LiteralExpr)@53..57
            Token(TkString)@53..57 "\"aa\""
          Token(TkRightBracket)@57..58 "]"
          Token(TkWhitespace)@58..59 " "
          Token(TkAssign)@59..60 "="
          Token(TkWhitespace)@60..61 " "
          Syntax(LiteralExpr)@61..62
            Token(TkInt)@61..62 "2"
        Token(TkComma)@62..63 ","
        Token(TkEndOfLine)@63..64 "\n"
        Token(TkWhitespace)@64..76 "            "
        Syntax(TableFieldAssign)@76..83
          Token(TkLeftBracket)@76..77 "["
          Syntax(LiteralExpr)@77..78
            Token(TkInt)@77..78 "1"
          Token(TkRightBracket)@78..79 "]"
          Token(TkWhitespace)@79..80 " "
          Token(TkAssign)@80..81 "="
          Token(TkWhitespace)@81..82 " "
          Syntax(LiteralExpr)@82..83
            Token(TkInt)@82..83 "3"
        Token(TkEndOfLine)@83..84 "\n"
        Token(TkWhitespace)@84..92 "        "
        Token(TkRightBrace)@92..93 "}"
    Token(TkEndOfLine)@93..94 "\n"
    Token(TkWhitespace)@94..102 "        "
    Syntax(LocalStat)@102..167
      Token(TkLocal)@102..107 "local"
      Token(TkWhitespace)@107..108 " "
      Syntax(LocalName)@108..109
        Token(TkName)@108..109 "d"
      Token(TkWhitespace)@109..110 " "
      Token(TkAssign)@110..111 "="
      Token(TkWhitespace)@111..112 " "
      Syntax(TableArrayExpr)@112..167
        Token(TkLeftBrace)@112..113 "{"
        Token(TkEndOfLine)@113..114 "\n"
        Token(TkWhitespace)@114..126 "            "
        Syntax(TableFieldValue)@126..127
          Syntax(LiteralExpr)@126..127
            Token(TkInt)@126..127 "1"
        Token(TkComma)@127..128 ","
        Token(TkEndOfLine)@128..129 "\n"
        Token(TkWhitespace)@129..141 "            "
        Syntax(TableFieldValue)@141..142
          Syntax(LiteralExpr)@141..142
            Token(TkInt)@141..142 "2"
        Token(TkComma)@142..143 ","
        Token(TkEndOfLine)@143..144 "\n"
        Token(TkWhitespace)@144..156 "            "
        Syntax(TableFieldValue)@156..157
          Syntax(LiteralExpr)@156..157
            Token(TkInt)@156..157 "3"
        Token(TkEndOfLine)@157..158 "\n"
        Token(TkWhitespace)@158..166 "        "
        Token(TkRightBrace)@166..167 "}"
    Token(TkEndOfLine)@167..168 "\n"
    Token(TkWhitespace)@168..176 "        "
    Syntax(LocalStat)@176..188
      Token(TkLocal)@176..181 "local"
      Token(TkWhitespace)@181..182 " "
      Syntax(LocalName)@182..183
        Token(TkName)@182..183 "c"
      Token(TkWhitespace)@183..184 " "
      Token(TkAssign)@184..185 "="
      Token(TkWhitespace)@185..186 " "
      Syntax(TableEmptyExpr)@186..188
        Token(TkLeftBrace)@186..187 "{"
        Token(TkRightBrace)@187..188 "}"
    Token(TkEndOfLine)@188..189 "\n"
    Token(TkWhitespace)@189..197 "        "
    Syntax(LocalStat)@197..219
      Token(TkLocal)@197..202 "local"
      Token(TkWhitespace)@202..203 " "
      Syntax(LocalName)@203..204
        Token(TkName)@203..204 "d"
      Token(TkWhitespace)@204..205 " "
      Token(TkAssign)@205..206 "="
      Token(TkWhitespace)@206..207 " "
      Syntax(TableObjectExpr)@207..219
        Token(TkLeftBrace)@207..208 "{"
        Token(TkWhitespace)@208..209 " "
        Syntax(TableFieldAssign)@209..214
          Token(TkName)@209..210 "a"
          Token(TkWhitespace)@210..211 " "
          Token(TkAssign)@211..212 "="
          Token(TkWhitespace)@212..213 " "
          Syntax(LiteralExpr)@213..214
            Token(TkInt)@213..214 "1"
        Token(TkComma)@214..215 ","
        Token(TkWhitespace)@215..216 " "
        Syntax(TableFieldValue)@216..217
          Syntax(LiteralExpr)@216..217
            Token(TkInt)@216..217 "1"
        Token(TkWhitespace)@217..218 " "
        Token(TkRightBrace)@218..219 "}"
    Token(TkEndOfLine)@219..220 "\n"
    Token(TkWhitespace)@220..228 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_if_stat() {
        let code = r#"
        if a > 0 then
            return a
        elseif a < 0 then
            return -a
        else
            return 0
        end
        "#;

        let result = r#"
Syntax(Chunk)@0..146
  Syntax(Block)@0..146
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(IfStat)@9..137
      Token(TkIf)@9..11 "if"
      Token(TkWhitespace)@11..12 " "
      Syntax(BinaryExpr)@12..17
        Syntax(NameExpr)@12..13
          Token(TkName)@12..13 "a"
        Token(TkWhitespace)@13..14 " "
        Token(TkGt)@14..15 ">"
        Token(TkWhitespace)@15..16 " "
        Syntax(LiteralExpr)@16..17
          Token(TkInt)@16..17 "0"
      Token(TkWhitespace)@17..18 " "
      Token(TkThen)@18..22 "then"
      Syntax(Block)@22..52
        Token(TkEndOfLine)@22..23 "\n"
        Token(TkWhitespace)@23..35 "            "
        Syntax(ReturnStat)@35..43
          Token(TkReturn)@35..41 "return"
          Token(TkWhitespace)@41..42 " "
          Syntax(NameExpr)@42..43
            Token(TkName)@42..43 "a"
        Token(TkEndOfLine)@43..44 "\n"
        Token(TkWhitespace)@44..52 "        "
      Syntax(ElseIfClauseStat)@52..100
        Token(TkElseIf)@52..58 "elseif"
        Token(TkWhitespace)@58..59 " "
        Syntax(BinaryExpr)@59..64
          Syntax(NameExpr)@59..60
            Token(TkName)@59..60 "a"
          Token(TkWhitespace)@60..61 " "
          Token(TkLt)@61..62 "<"
          Token(TkWhitespace)@62..63 " "
          Syntax(LiteralExpr)@63..64
            Token(TkInt)@63..64 "0"
        Token(TkWhitespace)@64..65 " "
        Token(TkThen)@65..69 "then"
        Syntax(Block)@69..100
          Token(TkEndOfLine)@69..70 "\n"
          Token(TkWhitespace)@70..82 "            "
          Syntax(ReturnStat)@82..91
            Token(TkReturn)@82..88 "return"
            Token(TkWhitespace)@88..89 " "
            Syntax(UnaryExpr)@89..91
              Token(TkMinus)@89..90 "-"
              Syntax(NameExpr)@90..91
                Token(TkName)@90..91 "a"
          Token(TkEndOfLine)@91..92 "\n"
          Token(TkWhitespace)@92..100 "        "
      Syntax(ElseClauseStat)@100..134
        Token(TkElse)@100..104 "else"
        Syntax(Block)@104..134
          Token(TkEndOfLine)@104..105 "\n"
          Token(TkWhitespace)@105..117 "            "
          Syntax(ReturnStat)@117..125
            Token(TkReturn)@117..123 "return"
            Token(TkWhitespace)@123..124 " "
            Syntax(LiteralExpr)@124..125
              Token(TkInt)@124..125 "0"
          Token(TkEndOfLine)@125..126 "\n"
          Token(TkWhitespace)@126..134 "        "
      Token(TkEnd)@134..137 "end"
    Token(TkEndOfLine)@137..138 "\n"
    Token(TkWhitespace)@138..146 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_local_stat() {
        let code = r#"
        local a<const>, b<close> = 123, {}
        "#;

        let result = r#"
Syntax(Chunk)@0..52
  Syntax(Block)@0..52
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(LocalStat)@9..43
      Token(TkLocal)@9..14 "local"
      Token(TkWhitespace)@14..15 " "
      Syntax(LocalName)@15..23
        Token(TkName)@15..16 "a"
        Syntax(Attribute)@16..23
          Token(TkLt)@16..17 "<"
          Token(TkName)@17..22 "const"
          Token(TkGt)@22..23 ">"
      Token(TkComma)@23..24 ","
      Token(TkWhitespace)@24..25 " "
      Syntax(LocalName)@25..33
        Token(TkName)@25..26 "b"
        Syntax(Attribute)@26..33
          Token(TkLt)@26..27 "<"
          Token(TkName)@27..32 "close"
          Token(TkGt)@32..33 ">"
      Token(TkWhitespace)@33..34 " "
      Token(TkAssign)@34..35 "="
      Token(TkWhitespace)@35..36 " "
      Syntax(LiteralExpr)@36..39
        Token(TkInt)@36..39 "123"
      Token(TkComma)@39..40 ","
      Token(TkWhitespace)@40..41 " "
      Syntax(TableEmptyExpr)@41..43
        Token(TkLeftBrace)@41..42 "{"
        Token(TkRightBrace)@42..43 "}"
    Token(TkEndOfLine)@43..44 "\n"
    Token(TkWhitespace)@44..52 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_func_stat() {
        let code = r#"
        function foo(a, b)
            return a + b
        end
        function t.foo(a, b)
            return a + b
        end
        function t:foo(a, b)
            return a + b
        end
        "#;

        let result = r#"
Syntax(Chunk)@0..205
  Syntax(Block)@0..205
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(FuncStat)@9..64
      Token(TkFunction)@9..17 "function"
      Token(TkWhitespace)@17..18 " "
      Syntax(NameExpr)@18..21
        Token(TkName)@18..21 "foo"
      Syntax(ClosureExpr)@21..64
        Syntax(ParamList)@21..27
          Token(TkLeftParen)@21..22 "("
          Syntax(ParamName)@22..23
            Token(TkName)@22..23 "a"
          Token(TkComma)@23..24 ","
          Token(TkWhitespace)@24..25 " "
          Syntax(ParamName)@25..26
            Token(TkName)@25..26 "b"
          Token(TkRightParen)@26..27 ")"
        Syntax(Block)@27..61
          Token(TkEndOfLine)@27..28 "\n"
          Token(TkWhitespace)@28..40 "            "
          Syntax(ReturnStat)@40..52
            Token(TkReturn)@40..46 "return"
            Token(TkWhitespace)@46..47 " "
            Syntax(BinaryExpr)@47..52
              Syntax(NameExpr)@47..48
                Token(TkName)@47..48 "a"
              Token(TkWhitespace)@48..49 " "
              Token(TkPlus)@49..50 "+"
              Token(TkWhitespace)@50..51 " "
              Syntax(NameExpr)@51..52
                Token(TkName)@51..52 "b"
          Token(TkEndOfLine)@52..53 "\n"
          Token(TkWhitespace)@53..61 "        "
        Token(TkEnd)@61..64 "end"
    Token(TkEndOfLine)@64..65 "\n"
    Token(TkWhitespace)@65..73 "        "
    Syntax(FuncStat)@73..130
      Token(TkFunction)@73..81 "function"
      Token(TkWhitespace)@81..82 " "
      Syntax(IndexExpr)@82..87
        Syntax(NameExpr)@82..83
          Token(TkName)@82..83 "t"
        Token(TkDot)@83..84 "."
        Token(TkName)@84..87 "foo"
      Syntax(ClosureExpr)@87..130
        Syntax(ParamList)@87..93
          Token(TkLeftParen)@87..88 "("
          Syntax(ParamName)@88..89
            Token(TkName)@88..89 "a"
          Token(TkComma)@89..90 ","
          Token(TkWhitespace)@90..91 " "
          Syntax(ParamName)@91..92
            Token(TkName)@91..92 "b"
          Token(TkRightParen)@92..93 ")"
        Syntax(Block)@93..127
          Token(TkEndOfLine)@93..94 "\n"
          Token(TkWhitespace)@94..106 "            "
          Syntax(ReturnStat)@106..118
            Token(TkReturn)@106..112 "return"
            Token(TkWhitespace)@112..113 " "
            Syntax(BinaryExpr)@113..118
              Syntax(NameExpr)@113..114
                Token(TkName)@113..114 "a"
              Token(TkWhitespace)@114..115 " "
              Token(TkPlus)@115..116 "+"
              Token(TkWhitespace)@116..117 " "
              Syntax(NameExpr)@117..118
                Token(TkName)@117..118 "b"
          Token(TkEndOfLine)@118..119 "\n"
          Token(TkWhitespace)@119..127 "        "
        Token(TkEnd)@127..130 "end"
    Token(TkEndOfLine)@130..131 "\n"
    Token(TkWhitespace)@131..139 "        "
    Syntax(FuncStat)@139..196
      Token(TkFunction)@139..147 "function"
      Token(TkWhitespace)@147..148 " "
      Syntax(IndexExpr)@148..153
        Syntax(NameExpr)@148..149
          Token(TkName)@148..149 "t"
        Token(TkColon)@149..150 ":"
        Token(TkName)@150..153 "foo"
      Syntax(ClosureExpr)@153..196
        Syntax(ParamList)@153..159
          Token(TkLeftParen)@153..154 "("
          Syntax(ParamName)@154..155
            Token(TkName)@154..155 "a"
          Token(TkComma)@155..156 ","
          Token(TkWhitespace)@156..157 " "
          Syntax(ParamName)@157..158
            Token(TkName)@157..158 "b"
          Token(TkRightParen)@158..159 ")"
        Syntax(Block)@159..193
          Token(TkEndOfLine)@159..160 "\n"
          Token(TkWhitespace)@160..172 "            "
          Syntax(ReturnStat)@172..184
            Token(TkReturn)@172..178 "return"
            Token(TkWhitespace)@178..179 " "
            Syntax(BinaryExpr)@179..184
              Syntax(NameExpr)@179..180
                Token(TkName)@179..180 "a"
              Token(TkWhitespace)@180..181 " "
              Token(TkPlus)@181..182 "+"
              Token(TkWhitespace)@182..183 " "
              Syntax(NameExpr)@183..184
                Token(TkName)@183..184 "b"
          Token(TkEndOfLine)@184..185 "\n"
          Token(TkWhitespace)@185..193 "        "
        Token(TkEnd)@193..196 "end"
    Token(TkEndOfLine)@196..197 "\n"
    Token(TkWhitespace)@197..205 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_error_for_completion() {
        let code = "a():";
        let result = r#"
Syntax(Chunk)@0..4
  Syntax(Block)@0..4
    Syntax(AssignStat)@0..4
      Syntax(IndexExpr)@0..4
        Syntax(CallExpr)@0..3
          Syntax(NameExpr)@0..1
            Token(TkName)@0..1 "a"
          Syntax(CallArgList)@1..3
            Token(TkLeftParen)@1..2 "("
            Token(TkRightParen)@2..3 ")"
        Token(TkColon)@3..4 ":"
        "#;

        assert_ast_eq!(code, result);
    }
}
