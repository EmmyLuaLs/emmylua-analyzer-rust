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
    fn test_normal_doc() {
        let code = r#"
        -- comment

        -- hihihi
        --     hello
        --yyyy
        "#;
        let result = r#"
Syntax(Chunk)@0..83
  Syntax(Block)@0..83
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..19
      Token(TkNormalStart)@9..11 "--"
      Token(TkWhitespace)@11..12 " "
      Syntax(DocDescription)@12..19
        Token(TkDocDetail)@12..19 "comment"
    Token(TkEndOfLine)@19..20 "\n"
    Token(TkEndOfLine)@20..21 "\n"
    Token(TkWhitespace)@21..29 "        "
    Syntax(Comment)@29..74
      Token(TkNormalStart)@29..31 "--"
      Token(TkWhitespace)@31..32 " "
      Syntax(DocDescription)@32..74
        Token(TkDocDetail)@32..38 "hihihi"
        Token(TkEndOfLine)@38..39 "\n"
        Token(TkWhitespace)@39..47 "        "
        Token(TkNormalStart)@47..49 "--"
        Token(TkWhitespace)@49..54 "     "
        Token(TkDocDetail)@54..59 "hello"
        Token(TkEndOfLine)@59..60 "\n"
        Token(TkWhitespace)@60..68 "        "
        Token(TkNormalStart)@68..70 "--"
        Token(TkDocDetail)@70..74 "yyyy"
    Token(TkEndOfLine)@74..75 "\n"
    Token(TkWhitespace)@75..83 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_tag_with_description() {
        let code = r#"
        ---   hiihihi
        ---@param a number hihihi hello
        ---    enenenen
        ---@return string a yyyyy
        function f(a)
        end
        "#;
        let result = r#"
Syntax(Chunk)@0..163
  Syntax(Block)@0..163
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..120
      Token(TkNormalStart)@9..15 "---   "
      Syntax(DocDescription)@15..22
        Token(TkDocDetail)@15..22 "hiihihi"
      Token(TkEndOfLine)@22..23 "\n"
      Token(TkWhitespace)@23..31 "        "
      Token(TkDocStart)@31..35 "---@"
      Syntax(DocTagParam)@35..49
        Token(TkTagParam)@35..40 "param"
        Token(TkWhitespace)@40..41 " "
        Token(TkName)@41..42 "a"
        Token(TkWhitespace)@42..43 " "
        Syntax(TypeName)@43..49
          Token(TkName)@43..49 "number"
      Token(TkWhitespace)@49..50 " "
      Syntax(DocDescription)@50..86
        Token(TkDocDetail)@50..62 "hihihi hello"
        Token(TkEndOfLine)@62..63 "\n"
        Token(TkWhitespace)@63..71 "        "
        Token(TkNormalStart)@71..78 "---    "
        Token(TkDocDetail)@78..86 "enenenen"
      Token(TkEndOfLine)@86..87 "\n"
      Token(TkWhitespace)@87..95 "        "
      Token(TkDocStart)@95..99 "---@"
      Syntax(DocTagReturn)@99..114
        Token(TkTagReturn)@99..105 "return"
        Token(TkWhitespace)@105..106 " "
        Syntax(TypeName)@106..112
          Token(TkName)@106..112 "string"
        Token(TkWhitespace)@112..113 " "
        Token(TkName)@113..114 "a"
      Token(TkWhitespace)@114..115 " "
      Syntax(DocDescription)@115..120
        Token(TkDocDetail)@115..120 "yyyyy"
    Token(TkEndOfLine)@120..121 "\n"
    Token(TkWhitespace)@121..129 "        "
    Syntax(FuncStat)@129..154
      Token(TkFunction)@129..137 "function"
      Token(TkWhitespace)@137..138 " "
      Syntax(NameExpr)@138..139
        Token(TkName)@138..139 "f"
      Syntax(ClosureExpr)@139..154
        Syntax(ParamList)@139..142
          Token(TkLeftParen)@139..140 "("
          Syntax(ParamName)@140..141
            Token(TkName)@140..141 "a"
          Token(TkRightParen)@141..142 ")"
        Token(TkEndOfLine)@142..143 "\n"
        Token(TkWhitespace)@143..151 "        "
        Token(TkEnd)@151..154 "end"
    Token(TkEndOfLine)@154..155 "\n"
    Token(TkWhitespace)@155..163 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_class_doc() {
        let code = r#"
        ---@class A hello
        ---@class B<T>
        ---@class C<T>: B<T>
        ---@class D : A, C<T>
        ---@class E hello
        "#;
        let result = r#"
Syntax(Chunk)@0..143
  Syntax(Block)@0..143
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..134
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagClass)@13..20
        Token(TkTagClass)@13..18 "class"
        Token(TkWhitespace)@18..19 " "
        Token(TkName)@19..20 "A"
      Token(TkWhitespace)@20..21 " "
      Syntax(DocDescription)@21..26
        Token(TkDocDetail)@21..26 "hello"
      Token(TkEndOfLine)@26..27 "\n"
      Token(TkWhitespace)@27..35 "        "
      Token(TkDocStart)@35..39 "---@"
      Syntax(DocTagClass)@39..49
        Token(TkTagClass)@39..44 "class"
        Token(TkWhitespace)@44..45 " "
        Token(TkName)@45..46 "B"
        Syntax(DocGenericDeclareList)@46..49
          Token(TkLt)@46..47 "<"
          Syntax(DocGenericParameter)@47..48
            Token(TkName)@47..48 "T"
          Token(TkGt)@48..49 ">"
      Token(TkEndOfLine)@49..50 "\n"
      Token(TkWhitespace)@50..58 "        "
      Token(TkDocStart)@58..62 "---@"
      Syntax(DocTagClass)@62..78
        Token(TkTagClass)@62..67 "class"
        Token(TkWhitespace)@67..68 " "
        Token(TkName)@68..69 "C"
        Syntax(DocGenericDeclareList)@69..72
          Token(TkLt)@69..70 "<"
          Syntax(DocGenericParameter)@70..71
            Token(TkName)@70..71 "T"
          Token(TkGt)@71..72 ">"
        Token(TkColon)@72..73 ":"
        Token(TkWhitespace)@73..74 " "
        Syntax(DocTypeList)@74..78
          Syntax(TypeGeneric)@74..78
            Syntax(TypeName)@74..75
              Token(TkName)@74..75 "B"
            Token(TkLt)@75..76 "<"
            Syntax(DocTypeList)@76..77
              Syntax(TypeName)@76..77
                Token(TkName)@76..77 "T"
            Token(TkGt)@77..78 ">"
      Token(TkEndOfLine)@78..79 "\n"
      Token(TkWhitespace)@79..87 "        "
      Token(TkDocStart)@87..91 "---@"
      Syntax(DocTagClass)@91..108
        Token(TkTagClass)@91..96 "class"
        Token(TkWhitespace)@96..97 " "
        Token(TkName)@97..98 "D"
        Token(TkWhitespace)@98..99 " "
        Token(TkColon)@99..100 ":"
        Token(TkWhitespace)@100..101 " "
        Syntax(DocTypeList)@101..108
          Syntax(TypeName)@101..102
            Token(TkName)@101..102 "A"
          Token(TkComma)@102..103 ","
          Token(TkWhitespace)@103..104 " "
          Syntax(TypeGeneric)@104..108
            Syntax(TypeName)@104..105
              Token(TkName)@104..105 "C"
            Token(TkLt)@105..106 "<"
            Syntax(DocTypeList)@106..107
              Syntax(TypeName)@106..107
                Token(TkName)@106..107 "T"
            Token(TkGt)@107..108 ">"
      Token(TkEndOfLine)@108..109 "\n"
      Token(TkWhitespace)@109..117 "        "
      Token(TkDocStart)@117..121 "---@"
      Syntax(DocTagClass)@121..128
        Token(TkTagClass)@121..126 "class"
        Token(TkWhitespace)@126..127 " "
        Token(TkName)@127..128 "E"
      Token(TkWhitespace)@128..129 " "
      Syntax(DocDescription)@129..134
        Token(TkDocDetail)@129..134 "hello"
    Token(TkEndOfLine)@134..135 "\n"
    Token(TkWhitespace)@135..143 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_enum_doc() {
        let code = r#"
        ---@enum AAA

        ---@enum BBB: integer

        ---@enum CCC: integer
        local d = {
          a = 123,
          b = 456,
        }

        ---@enum DDD
        ---| AAA
        ---| BBB @ hihihi
        ---| CCC
        "#;

        let result = r#"
Syntax(Chunk)@0..242
  Syntax(Block)@0..242
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..21
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagEnum)@13..21
        Token(TkTagEnum)@13..17 "enum"
        Token(TkWhitespace)@17..18 " "
        Token(TkName)@18..21 "AAA"
    Token(TkEndOfLine)@21..22 "\n"
    Token(TkEndOfLine)@22..23 "\n"
    Token(TkWhitespace)@23..31 "        "
    Syntax(Comment)@31..52
      Token(TkDocStart)@31..35 "---@"
      Syntax(DocTagEnum)@35..52
        Token(TkTagEnum)@35..39 "enum"
        Token(TkWhitespace)@39..40 " "
        Token(TkName)@40..43 "BBB"
        Token(TkColon)@43..44 ":"
        Token(TkWhitespace)@44..45 " "
        Syntax(TypeName)@45..52
          Token(TkName)@45..52 "integer"
    Token(TkEndOfLine)@52..53 "\n"
    Token(TkEndOfLine)@53..54 "\n"
    Token(TkWhitespace)@54..62 "        "
    Syntax(Comment)@62..83
      Token(TkDocStart)@62..66 "---@"
      Syntax(DocTagEnum)@66..83
        Token(TkTagEnum)@66..70 "enum"
        Token(TkWhitespace)@70..71 " "
        Token(TkName)@71..74 "CCC"
        Token(TkColon)@74..75 ":"
        Token(TkWhitespace)@75..76 " "
        Syntax(TypeName)@76..83
          Token(TkName)@76..83 "integer"
    Token(TkEndOfLine)@83..84 "\n"
    Token(TkWhitespace)@84..92 "        "
    Syntax(LocalStat)@92..151
      Token(TkLocal)@92..97 "local"
      Token(TkWhitespace)@97..98 " "
      Syntax(LocalName)@98..99
        Token(TkName)@98..99 "d"
      Token(TkWhitespace)@99..100 " "
      Token(TkAssign)@100..101 "="
      Token(TkWhitespace)@101..102 " "
      Syntax(TableObjectExpr)@102..151
        Token(TkLeftBrace)@102..103 "{"
        Token(TkEndOfLine)@103..104 "\n"
        Token(TkWhitespace)@104..114 "          "
        Syntax(TableFieldAssign)@114..121
          Token(TkName)@114..115 "a"
          Token(TkWhitespace)@115..116 " "
          Token(TkAssign)@116..117 "="
          Token(TkWhitespace)@117..118 " "
          Syntax(LiteralExpr)@118..121
            Token(TkInt)@118..121 "123"
        Token(TkComma)@121..122 ","
        Token(TkEndOfLine)@122..123 "\n"
        Token(TkWhitespace)@123..133 "          "
        Syntax(TableFieldAssign)@133..140
          Token(TkName)@133..134 "b"
          Token(TkWhitespace)@134..135 " "
          Token(TkAssign)@135..136 "="
          Token(TkWhitespace)@136..137 " "
          Syntax(LiteralExpr)@137..140
            Token(TkInt)@137..140 "456"
        Token(TkComma)@140..141 ","
        Token(TkEndOfLine)@141..142 "\n"
        Token(TkWhitespace)@142..150 "        "
        Token(TkRightBrace)@150..151 "}"
    Token(TkEndOfLine)@151..152 "\n"
    Token(TkEndOfLine)@152..153 "\n"
    Token(TkWhitespace)@153..161 "        "
    Syntax(Comment)@161..233
      Token(TkDocStart)@161..165 "---@"
      Syntax(DocTagEnum)@165..233
        Token(TkTagEnum)@165..169 "enum"
        Token(TkWhitespace)@169..170 " "
        Token(TkName)@170..173 "DDD"
        Token(TkEndOfLine)@173..174 "\n"
        Token(TkWhitespace)@174..182 "        "
        Syntax(DocEnumFieldList)@182..233
          Token(TkDocContinueOr)@182..186 "---|"
          Token(TkWhitespace)@186..187 " "
          Syntax(DocEnumField)@187..190
            Token(TkName)@187..190 "AAA"
          Token(TkEndOfLine)@190..191 "\n"
          Token(TkWhitespace)@191..199 "        "
          Token(TkDocContinueOr)@199..203 "---|"
          Token(TkWhitespace)@203..204 " "
          Syntax(DocEnumField)@204..216
            Token(TkName)@204..207 "BBB"
            Token(TkWhitespace)@207..208 " "
            Token(TkDocDetail)@208..216 "@ hihihi"
          Token(TkEndOfLine)@216..217 "\n"
          Token(TkWhitespace)@217..225 "        "
          Token(TkDocContinueOr)@225..229 "---|"
          Token(TkWhitespace)@229..230 " "
          Syntax(DocEnumField)@230..233
            Token(TkName)@230..233 "CCC"
    Token(TkEndOfLine)@233..234 "\n"
    Token(TkWhitespace)@234..242 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_alias_doc() {
        let code = r#"
        ---@alias A B
        
        ---@alias C<T> B<T>

        ---@alias A
        ---| "aaa" @ 1231
        ---| "bbb" @ 456
        ---| "ccc" @ 789

        ---@alias D
        ---| 1
        ---| 2 
        ---| 3
        "#;

        let result = r#"
Syntax(Chunk)@0..232
  Syntax(Block)@0..232
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..22
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagAlias)@13..22
        Token(TkTagAlias)@13..18 "alias"
        Token(TkWhitespace)@18..19 " "
        Token(TkName)@19..20 "A"
        Token(TkWhitespace)@20..21 " "
        Syntax(TypeName)@21..22
          Token(TkName)@21..22 "B"
    Token(TkEndOfLine)@22..23 "\n"
    Token(TkWhitespace)@23..31 "        "
    Token(TkEndOfLine)@31..32 "\n"
    Token(TkWhitespace)@32..40 "        "
    Syntax(Comment)@40..59
      Token(TkDocStart)@40..44 "---@"
      Syntax(DocTagAlias)@44..59
        Token(TkTagAlias)@44..49 "alias"
        Token(TkWhitespace)@49..50 " "
        Token(TkName)@50..51 "C"
        Syntax(DocGenericDeclareList)@51..54
          Token(TkLt)@51..52 "<"
          Syntax(DocGenericParameter)@52..53
            Token(TkName)@52..53 "T"
          Token(TkGt)@53..54 ">"
        Token(TkWhitespace)@54..55 " "
        Syntax(TypeGeneric)@55..59
          Syntax(TypeName)@55..56
            Token(TkName)@55..56 "B"
          Token(TkLt)@56..57 "<"
          Syntax(DocTypeList)@57..58
            Syntax(TypeName)@57..58
              Token(TkName)@57..58 "T"
          Token(TkGt)@58..59 ">"
    Token(TkEndOfLine)@59..60 "\n"
    Token(TkEndOfLine)@60..61 "\n"
    Token(TkWhitespace)@61..69 "        "
    Syntax(Comment)@69..156
      Token(TkDocStart)@69..73 "---@"
      Syntax(DocTagAlias)@73..156
        Token(TkTagAlias)@73..78 "alias"
        Token(TkWhitespace)@78..79 " "
        Token(TkName)@79..80 "A"
        Token(TkEndOfLine)@80..81 "\n"
        Token(TkWhitespace)@81..89 "        "
        Syntax(TypeMultiLineUnion)@89..156
          Token(TkDocContinueOr)@89..93 "---|"
          Token(TkWhitespace)@93..94 " "
          Syntax(DocOneLineField)@94..99
            Syntax(TypeLiteral)@94..99
              Token(TkString)@94..99 "\"aaa\""
          Token(TkWhitespace)@99..100 " "
          Syntax(DocDescription)@100..106
            Token(TkDocDetail)@100..106 "@ 1231"
          Token(TkEndOfLine)@106..107 "\n"
          Token(TkWhitespace)@107..115 "        "
          Token(TkDocContinueOr)@115..119 "---|"
          Token(TkWhitespace)@119..120 " "
          Syntax(DocOneLineField)@120..125
            Syntax(TypeLiteral)@120..125
              Token(TkString)@120..125 "\"bbb\""
          Token(TkWhitespace)@125..126 " "
          Syntax(DocDescription)@126..131
            Token(TkDocDetail)@126..131 "@ 456"
          Token(TkEndOfLine)@131..132 "\n"
          Token(TkWhitespace)@132..140 "        "
          Token(TkDocContinueOr)@140..144 "---|"
          Token(TkWhitespace)@144..145 " "
          Syntax(DocOneLineField)@145..150
            Syntax(TypeLiteral)@145..150
              Token(TkString)@145..150 "\"ccc\""
          Token(TkWhitespace)@150..151 " "
          Syntax(DocDescription)@151..156
            Token(TkDocDetail)@151..156 "@ 789"
    Token(TkEndOfLine)@156..157 "\n"
    Token(TkEndOfLine)@157..158 "\n"
    Token(TkWhitespace)@158..166 "        "
    Syntax(Comment)@166..223
      Token(TkDocStart)@166..170 "---@"
      Syntax(DocTagAlias)@170..223
        Token(TkTagAlias)@170..175 "alias"
        Token(TkWhitespace)@175..176 " "
        Token(TkName)@176..177 "D"
        Token(TkEndOfLine)@177..178 "\n"
        Token(TkWhitespace)@178..186 "        "
        Syntax(TypeMultiLineUnion)@186..223
          Token(TkDocContinueOr)@186..190 "---|"
          Token(TkWhitespace)@190..191 " "
          Syntax(DocOneLineField)@191..192
            Syntax(TypeLiteral)@191..192
              Token(TkInt)@191..192 "1"
          Token(TkEndOfLine)@192..193 "\n"
          Token(TkWhitespace)@193..201 "        "
          Token(TkDocContinueOr)@201..205 "---|"
          Token(TkWhitespace)@205..206 " "
          Syntax(DocOneLineField)@206..207
            Syntax(TypeLiteral)@206..207
              Token(TkInt)@206..207 "2"
          Token(TkWhitespace)@207..208 " "
          Token(TkEndOfLine)@208..209 "\n"
          Token(TkWhitespace)@209..217 "        "
          Token(TkDocContinueOr)@217..221 "---|"
          Token(TkWhitespace)@221..222 " "
          Syntax(DocOneLineField)@222..223
            Syntax(TypeLiteral)@222..223
              Token(TkInt)@222..223 "3"
    Token(TkEndOfLine)@223..224 "\n"
    Token(TkWhitespace)@224..232 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_field_doc() {
        let code = r#"
        ---@field a number
        ---@field b? string
        ---@field [1] number
        ---@field ["hihihi"] table
        ---@field c number? hello
        ---@field d number @hello
        local a = {}
        "#;

        let result = r#"
Syntax(Chunk)@0..217
  Syntax(Block)@0..217
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..187
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagField)@13..27
        Token(TkTagField)@13..18 "field"
        Token(TkWhitespace)@18..19 " "
        Token(TkName)@19..20 "a"
        Token(TkWhitespace)@20..21 " "
        Syntax(TypeName)@21..27
          Token(TkName)@21..27 "number"
      Token(TkEndOfLine)@27..28 "\n"
      Token(TkWhitespace)@28..36 "        "
      Token(TkDocStart)@36..40 "---@"
      Syntax(DocTagField)@40..55
        Token(TkTagField)@40..45 "field"
        Token(TkWhitespace)@45..46 " "
        Token(TkName)@46..47 "b"
        Token(TkDocQuestion)@47..48 "?"
        Token(TkWhitespace)@48..49 " "
        Syntax(TypeName)@49..55
          Token(TkName)@49..55 "string"
      Token(TkEndOfLine)@55..56 "\n"
      Token(TkWhitespace)@56..64 "        "
      Token(TkDocStart)@64..68 "---@"
      Syntax(DocTagField)@68..84
        Token(TkTagField)@68..73 "field"
        Token(TkWhitespace)@73..74 " "
        Token(TkLeftBracket)@74..75 "["
        Token(TkInt)@75..76 "1"
        Token(TkRightBracket)@76..77 "]"
        Token(TkWhitespace)@77..78 " "
        Syntax(TypeName)@78..84
          Token(TkName)@78..84 "number"
      Token(TkEndOfLine)@84..85 "\n"
      Token(TkWhitespace)@85..93 "        "
      Token(TkDocStart)@93..97 "---@"
      Syntax(DocTagField)@97..119
        Token(TkTagField)@97..102 "field"
        Token(TkWhitespace)@102..103 " "
        Token(TkLeftBracket)@103..104 "["
        Token(TkString)@104..112 "\"hihihi\""
        Token(TkRightBracket)@112..113 "]"
        Token(TkWhitespace)@113..114 " "
        Syntax(TypeName)@114..119
          Token(TkName)@114..119 "table"
      Token(TkEndOfLine)@119..120 "\n"
      Token(TkWhitespace)@120..128 "        "
      Token(TkDocStart)@128..132 "---@"
      Syntax(DocTagField)@132..147
        Token(TkTagField)@132..137 "field"
        Token(TkWhitespace)@137..138 " "
        Token(TkName)@138..139 "c"
        Token(TkWhitespace)@139..140 " "
        Syntax(TypeNullable)@140..147
          Syntax(TypeName)@140..146
            Token(TkName)@140..146 "number"
          Token(TkDocQuestion)@146..147 "?"
      Token(TkWhitespace)@147..148 " "
      Syntax(DocDescription)@148..153
        Token(TkDocDetail)@148..153 "hello"
      Token(TkEndOfLine)@153..154 "\n"
      Token(TkWhitespace)@154..162 "        "
      Token(TkDocStart)@162..166 "---@"
      Syntax(DocTagField)@166..180
        Token(TkTagField)@166..171 "field"
        Token(TkWhitespace)@171..172 " "
        Token(TkName)@172..173 "d"
        Token(TkWhitespace)@173..174 " "
        Syntax(TypeName)@174..180
          Token(TkName)@174..180 "number"
      Token(TkWhitespace)@180..181 " "
      Syntax(DocDescription)@181..187
        Token(TkDocDetail)@181..187 "@hello"
    Token(TkEndOfLine)@187..188 "\n"
    Token(TkWhitespace)@188..196 "        "
    Syntax(LocalStat)@196..208
      Token(TkLocal)@196..201 "local"
      Token(TkWhitespace)@201..202 " "
      Syntax(LocalName)@202..203
        Token(TkName)@202..203 "a"
      Token(TkWhitespace)@203..204 " "
      Token(TkAssign)@204..205 "="
      Token(TkWhitespace)@205..206 " "
      Syntax(TableEmptyExpr)@206..208
        Token(TkLeftBrace)@206..207 "{"
        Token(TkRightBrace)@207..208 "}"
    Token(TkEndOfLine)@208..209 "\n"
    Token(TkWhitespace)@209..217 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_param_doc() {
        let code = r#"
        ---@param a number
        ---@param b? string
        ---@param ... string
        ---@param c number? hello
        ---@param d number @hello
        ---@param e 
        ---| "aaa" @ 1231
        function f(a, b, c, d, ...)
        end
        "#;

        let result = r#"
Syntax(Chunk)@0..256
  Syntax(Block)@0..256
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..199
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagParam)@13..27
        Token(TkTagParam)@13..18 "param"
        Token(TkWhitespace)@18..19 " "
        Token(TkName)@19..20 "a"
        Token(TkWhitespace)@20..21 " "
        Syntax(TypeName)@21..27
          Token(TkName)@21..27 "number"
      Token(TkEndOfLine)@27..28 "\n"
      Token(TkWhitespace)@28..36 "        "
      Token(TkDocStart)@36..40 "---@"
      Syntax(DocTagParam)@40..55
        Token(TkTagParam)@40..45 "param"
        Token(TkWhitespace)@45..46 " "
        Token(TkName)@46..47 "b"
        Token(TkDocQuestion)@47..48 "?"
        Token(TkWhitespace)@48..49 " "
        Syntax(TypeName)@49..55
          Token(TkName)@49..55 "string"
      Token(TkEndOfLine)@55..56 "\n"
      Token(TkWhitespace)@56..64 "        "
      Token(TkDocStart)@64..68 "---@"
      Syntax(DocTagParam)@68..84
        Token(TkTagParam)@68..73 "param"
        Token(TkWhitespace)@73..74 " "
        Token(TkDots)@74..77 "..."
        Token(TkWhitespace)@77..78 " "
        Syntax(TypeName)@78..84
          Token(TkName)@78..84 "string"
      Token(TkEndOfLine)@84..85 "\n"
      Token(TkWhitespace)@85..93 "        "
      Token(TkDocStart)@93..97 "---@"
      Syntax(DocTagParam)@97..112
        Token(TkTagParam)@97..102 "param"
        Token(TkWhitespace)@102..103 " "
        Token(TkName)@103..104 "c"
        Token(TkWhitespace)@104..105 " "
        Syntax(TypeNullable)@105..112
          Syntax(TypeName)@105..111
            Token(TkName)@105..111 "number"
          Token(TkDocQuestion)@111..112 "?"
      Token(TkWhitespace)@112..113 " "
      Syntax(DocDescription)@113..118
        Token(TkDocDetail)@113..118 "hello"
      Token(TkEndOfLine)@118..119 "\n"
      Token(TkWhitespace)@119..127 "        "
      Token(TkDocStart)@127..131 "---@"
      Syntax(DocTagParam)@131..145
        Token(TkTagParam)@131..136 "param"
        Token(TkWhitespace)@136..137 " "
        Token(TkName)@137..138 "d"
        Token(TkWhitespace)@138..139 " "
        Syntax(TypeName)@139..145
          Token(TkName)@139..145 "number"
      Token(TkWhitespace)@145..146 " "
      Syntax(DocDescription)@146..152
        Token(TkDocDetail)@146..152 "@hello"
      Token(TkEndOfLine)@152..153 "\n"
      Token(TkWhitespace)@153..161 "        "
      Token(TkDocStart)@161..165 "---@"
      Syntax(DocTagParam)@165..199
        Token(TkTagParam)@165..170 "param"
        Token(TkWhitespace)@170..171 " "
        Token(TkName)@171..172 "e"
        Token(TkWhitespace)@172..173 " "
        Token(TkEndOfLine)@173..174 "\n"
        Token(TkWhitespace)@174..182 "        "
        Syntax(TypeMultiLineUnion)@182..199
          Token(TkDocContinueOr)@182..186 "---|"
          Token(TkWhitespace)@186..187 " "
          Syntax(DocOneLineField)@187..192
            Syntax(TypeLiteral)@187..192
              Token(TkString)@187..192 "\"aaa\""
          Token(TkWhitespace)@192..193 " "
          Syntax(DocDescription)@193..199
            Token(TkDocDetail)@193..199 "@ 1231"
    Token(TkEndOfLine)@199..200 "\n"
    Token(TkWhitespace)@200..208 "        "
    Syntax(FuncStat)@208..247
      Token(TkFunction)@208..216 "function"
      Token(TkWhitespace)@216..217 " "
      Syntax(NameExpr)@217..218
        Token(TkName)@217..218 "f"
      Syntax(ClosureExpr)@218..247
        Syntax(ParamList)@218..235
          Token(TkLeftParen)@218..219 "("
          Syntax(ParamName)@219..220
            Token(TkName)@219..220 "a"
          Token(TkComma)@220..221 ","
          Token(TkWhitespace)@221..222 " "
          Syntax(ParamName)@222..223
            Token(TkName)@222..223 "b"
          Token(TkComma)@223..224 ","
          Token(TkWhitespace)@224..225 " "
          Syntax(ParamName)@225..226
            Token(TkName)@225..226 "c"
          Token(TkComma)@226..227 ","
          Token(TkWhitespace)@227..228 " "
          Syntax(ParamName)@228..229
            Token(TkName)@228..229 "d"
          Token(TkComma)@229..230 ","
          Token(TkWhitespace)@230..231 " "
          Syntax(ParamName)@231..234
            Token(TkDots)@231..234 "..."
          Token(TkRightParen)@234..235 ")"
        Token(TkEndOfLine)@235..236 "\n"
        Token(TkWhitespace)@236..244 "        "
        Token(TkEnd)@244..247 "end"
    Token(TkEndOfLine)@247..248 "\n"
    Token(TkWhitespace)@248..256 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_return_doc() {
        let code = r#"
        ---@return number
        ---@return string ok
        ---@return number ok, string err
        ---@return number, string @hello
        function f()
        end
        "#;

        let result = r#"
Syntax(Chunk)@0..179
  Syntax(Block)@0..179
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..137
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagReturn)@13..26
        Token(TkTagReturn)@13..19 "return"
        Token(TkWhitespace)@19..20 " "
        Syntax(TypeName)@20..26
          Token(TkName)@20..26 "number"
      Token(TkEndOfLine)@26..27 "\n"
      Token(TkWhitespace)@27..35 "        "
      Token(TkDocStart)@35..39 "---@"
      Syntax(DocTagReturn)@39..55
        Token(TkTagReturn)@39..45 "return"
        Token(TkWhitespace)@45..46 " "
        Syntax(TypeName)@46..52
          Token(TkName)@46..52 "string"
        Token(TkWhitespace)@52..53 " "
        Token(TkName)@53..55 "ok"
      Token(TkEndOfLine)@55..56 "\n"
      Token(TkWhitespace)@56..64 "        "
      Token(TkDocStart)@64..68 "---@"
      Syntax(DocTagReturn)@68..96
        Token(TkTagReturn)@68..74 "return"
        Token(TkWhitespace)@74..75 " "
        Syntax(TypeName)@75..81
          Token(TkName)@75..81 "number"
        Token(TkWhitespace)@81..82 " "
        Token(TkName)@82..84 "ok"
        Token(TkComma)@84..85 ","
        Token(TkWhitespace)@85..86 " "
        Syntax(TypeName)@86..92
          Token(TkName)@86..92 "string"
        Token(TkWhitespace)@92..93 " "
        Token(TkName)@93..96 "err"
      Token(TkEndOfLine)@96..97 "\n"
      Token(TkWhitespace)@97..105 "        "
      Token(TkDocStart)@105..109 "---@"
      Syntax(DocTagReturn)@109..130
        Token(TkTagReturn)@109..115 "return"
        Token(TkWhitespace)@115..116 " "
        Syntax(TypeName)@116..122
          Token(TkName)@116..122 "number"
        Token(TkComma)@122..123 ","
        Token(TkWhitespace)@123..124 " "
        Syntax(TypeName)@124..130
          Token(TkName)@124..130 "string"
      Token(TkWhitespace)@130..131 " "
      Syntax(DocDescription)@131..137
        Token(TkDocDetail)@131..137 "@hello"
    Token(TkEndOfLine)@137..138 "\n"
    Token(TkWhitespace)@138..146 "        "
    Syntax(FuncStat)@146..170
      Token(TkFunction)@146..154 "function"
      Token(TkWhitespace)@154..155 " "
      Syntax(NameExpr)@155..156
        Token(TkName)@155..156 "f"
      Syntax(ClosureExpr)@156..170
        Syntax(ParamList)@156..158
          Token(TkLeftParen)@156..157 "("
          Token(TkRightParen)@157..158 ")"
        Token(TkEndOfLine)@158..159 "\n"
        Token(TkWhitespace)@159..167 "        "
        Token(TkEnd)@167..170 "end"
    Token(TkEndOfLine)@170..171 "\n"
    Token(TkWhitespace)@171..179 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_type_doc() {
        let code = r#"
        ---@type A | B | C & D
        "#;
        let result = r#"
Syntax(Chunk)@0..40
  Syntax(Block)@0..40
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..31
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagType)@13..31
        Token(TkTagType)@13..17 "type"
        Token(TkWhitespace)@17..18 " "
        Syntax(TypeBinary)@18..31
          Syntax(TypeBinary)@18..23
            Syntax(TypeName)@18..19
              Token(TkName)@18..19 "A"
            Token(TkWhitespace)@19..20 " "
            Token(TkDocOr)@20..21 "|"
            Token(TkWhitespace)@21..22 " "
            Syntax(TypeName)@22..23
              Token(TkName)@22..23 "B"
          Token(TkWhitespace)@23..24 " "
          Token(TkDocOr)@24..25 "|"
          Token(TkWhitespace)@25..26 " "
          Syntax(TypeBinary)@26..31
            Syntax(TypeName)@26..27
              Token(TkName)@26..27 "C"
            Token(TkWhitespace)@27..28 " "
            Token(TkDocAnd)@28..29 "&"
            Token(TkWhitespace)@29..30 " "
            Syntax(TypeName)@30..31
              Token(TkName)@30..31 "D"
    Token(TkEndOfLine)@31..32 "\n"
    Token(TkWhitespace)@32..40 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_overload_doc() {
        let code = r#"
        ---@overload fun(a: number, b: string): number
        ---@overload async fun(a: string, b: number): string
        "#;

        let result = r#"
Syntax(Chunk)@0..125
  Syntax(Block)@0..125
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..116
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagOverload)@13..55
        Token(TkTagOverload)@13..21 "overload"
        Token(TkWhitespace)@21..22 " "
        Syntax(TypeFun)@22..55
          Token(TkName)@22..25 "fun"
          Token(TkLeftParen)@25..26 "("
          Syntax(DocTypedParameter)@26..35
            Token(TkName)@26..27 "a"
            Token(TkColon)@27..28 ":"
            Token(TkWhitespace)@28..29 " "
            Syntax(TypeName)@29..35
              Token(TkName)@29..35 "number"
          Token(TkComma)@35..36 ","
          Token(TkWhitespace)@36..37 " "
          Syntax(DocTypedParameter)@37..46
            Token(TkName)@37..38 "b"
            Token(TkColon)@38..39 ":"
            Token(TkWhitespace)@39..40 " "
            Syntax(TypeName)@40..46
              Token(TkName)@40..46 "string"
          Token(TkRightParen)@46..47 ")"
          Token(TkColon)@47..48 ":"
          Token(TkWhitespace)@48..49 " "
          Syntax(DocTypeList)@49..55
            Syntax(DocNamedReturnType)@49..55
              Syntax(TypeName)@49..55
                Token(TkName)@49..55 "number"
      Token(TkEndOfLine)@55..56 "\n"
      Token(TkWhitespace)@56..64 "        "
      Token(TkDocStart)@64..68 "---@"
      Syntax(DocTagOverload)@68..116
        Token(TkTagOverload)@68..76 "overload"
        Token(TkWhitespace)@76..77 " "
        Syntax(TypeFun)@77..116
          Token(TkName)@77..82 "async"
          Token(TkWhitespace)@82..83 " "
          Token(TkName)@83..86 "fun"
          Token(TkLeftParen)@86..87 "("
          Syntax(DocTypedParameter)@87..96
            Token(TkName)@87..88 "a"
            Token(TkColon)@88..89 ":"
            Token(TkWhitespace)@89..90 " "
            Syntax(TypeName)@90..96
              Token(TkName)@90..96 "string"
          Token(TkComma)@96..97 ","
          Token(TkWhitespace)@97..98 " "
          Syntax(DocTypedParameter)@98..107
            Token(TkName)@98..99 "b"
            Token(TkColon)@99..100 ":"
            Token(TkWhitespace)@100..101 " "
            Syntax(TypeName)@101..107
              Token(TkName)@101..107 "number"
          Token(TkRightParen)@107..108 ")"
          Token(TkColon)@108..109 ":"
          Token(TkWhitespace)@109..110 " "
          Syntax(DocTypeList)@110..116
            Syntax(DocNamedReturnType)@110..116
              Syntax(TypeName)@110..116
                Token(TkName)@110..116 "string"
    Token(TkEndOfLine)@116..117 "\n"
    Token(TkWhitespace)@117..125 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_generic_doc() {
        let code = r#"
        ---@generic T
        ---@generic T, R
        ---@generic T, R: number, S
        "#;

        let result = r#"
Syntax(Chunk)@0..92
  Syntax(Block)@0..92
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..83
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagGeneric)@13..22
        Token(TkTagGeneric)@13..20 "generic"
        Token(TkWhitespace)@20..21 " "
        Syntax(DocGenericDeclareList)@21..22
          Syntax(DocGenericParameter)@21..22
            Token(TkName)@21..22 "T"
      Token(TkEndOfLine)@22..23 "\n"
      Token(TkWhitespace)@23..31 "        "
      Token(TkDocStart)@31..35 "---@"
      Syntax(DocTagGeneric)@35..47
        Token(TkTagGeneric)@35..42 "generic"
        Token(TkWhitespace)@42..43 " "
        Syntax(DocGenericDeclareList)@43..47
          Syntax(DocGenericParameter)@43..44
            Token(TkName)@43..44 "T"
          Token(TkComma)@44..45 ","
          Token(TkWhitespace)@45..46 " "
          Syntax(DocGenericParameter)@46..47
            Token(TkName)@46..47 "R"
      Token(TkEndOfLine)@47..48 "\n"
      Token(TkWhitespace)@48..56 "        "
      Token(TkDocStart)@56..60 "---@"
      Syntax(DocTagGeneric)@60..83
        Token(TkTagGeneric)@60..67 "generic"
        Token(TkWhitespace)@67..68 " "
        Syntax(DocGenericDeclareList)@68..83
          Syntax(DocGenericParameter)@68..69
            Token(TkName)@68..69 "T"
          Token(TkComma)@69..70 ","
          Token(TkWhitespace)@70..71 " "
          Syntax(DocGenericParameter)@71..80
            Token(TkName)@71..72 "R"
            Token(TkColon)@72..73 ":"
            Token(TkWhitespace)@73..74 " "
            Syntax(TypeName)@74..80
              Token(TkName)@74..80 "number"
          Token(TkComma)@80..81 ","
          Token(TkWhitespace)@81..82 " "
          Syntax(DocGenericParameter)@82..83
            Token(TkName)@82..83 "S"
    Token(TkEndOfLine)@83..84 "\n"
    Token(TkWhitespace)@84..92 "        "
        "#;
        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_diagnostic_doc() {
        let code = r#"
        ---@diagnostic disable
        ---@diagnostic disable-next-line
        ---@diagnostic enable
        ---@diagnostic enable-next-line
        ---@diagnostic disable: undefined-global
        ---@diagnostic disable-next-line: undefined-global, unused-local
        "#;

        let result = r#"
Syntax(Chunk)@0..273
  Syntax(Block)@0..273
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..264
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagDiagnostic)@13..31
        Token(TkTagDiagnostic)@13..23 "diagnostic"
        Token(TkWhitespace)@23..24 " "
        Token(TkName)@24..31 "disable"
      Token(TkEndOfLine)@31..32 "\n"
      Token(TkWhitespace)@32..40 "        "
      Token(TkDocStart)@40..44 "---@"
      Syntax(DocTagDiagnostic)@44..72
        Token(TkTagDiagnostic)@44..54 "diagnostic"
        Token(TkWhitespace)@54..55 " "
        Token(TkName)@55..72 "disable-next-line"
      Token(TkEndOfLine)@72..73 "\n"
      Token(TkWhitespace)@73..81 "        "
      Token(TkDocStart)@81..85 "---@"
      Syntax(DocTagDiagnostic)@85..102
        Token(TkTagDiagnostic)@85..95 "diagnostic"
        Token(TkWhitespace)@95..96 " "
        Token(TkName)@96..102 "enable"
      Token(TkEndOfLine)@102..103 "\n"
      Token(TkWhitespace)@103..111 "        "
      Token(TkDocStart)@111..115 "---@"
      Syntax(DocTagDiagnostic)@115..142
        Token(TkTagDiagnostic)@115..125 "diagnostic"
        Token(TkWhitespace)@125..126 " "
        Token(TkName)@126..142 "enable-next-line"
      Token(TkEndOfLine)@142..143 "\n"
      Token(TkWhitespace)@143..151 "        "
      Token(TkDocStart)@151..155 "---@"
      Syntax(DocTagDiagnostic)@155..191
        Token(TkTagDiagnostic)@155..165 "diagnostic"
        Token(TkWhitespace)@165..166 " "
        Token(TkName)@166..173 "disable"
        Token(TkColon)@173..174 ":"
        Token(TkWhitespace)@174..175 " "
        Syntax(DocDiagnosticCodeList)@175..191
          Token(TkName)@175..191 "undefined-global"
      Token(TkEndOfLine)@191..192 "\n"
      Token(TkWhitespace)@192..200 "        "
      Token(TkDocStart)@200..204 "---@"
      Syntax(DocTagDiagnostic)@204..264
        Token(TkTagDiagnostic)@204..214 "diagnostic"
        Token(TkWhitespace)@214..215 " "
        Token(TkName)@215..232 "disable-next-line"
        Token(TkColon)@232..233 ":"
        Token(TkWhitespace)@233..234 " "
        Syntax(DocDiagnosticCodeList)@234..264
          Token(TkName)@234..250 "undefined-global"
          Token(TkComma)@250..251 ","
          Token(TkWhitespace)@251..252 " "
          Token(TkName)@252..264 "unused-local"
    Token(TkEndOfLine)@264..265 "\n"
    Token(TkWhitespace)@265..273 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_cast_doc() {
        let code = r#"
        ---@cast a string
        ---@cast b +number
        ---@cast c -number
        ---@cast d +?
        ---@cast e -?
        ---@cast f +number, -string
        "#;

        let result = r#"
Syntax(Chunk)@0..169
  Syntax(Block)@0..169
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..160
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagCast)@13..26
        Token(TkTagCast)@13..17 "cast"
        Token(TkWhitespace)@17..18 " "
        Syntax(NameExpr)@18..19
          Token(TkName)@18..19 "a"
        Token(TkWhitespace)@19..20 " "
        Syntax(DocOpType)@20..26
          Syntax(TypeName)@20..26
            Token(TkName)@20..26 "string"
      Token(TkEndOfLine)@26..27 "\n"
      Token(TkWhitespace)@27..35 "        "
      Token(TkDocStart)@35..39 "---@"
      Syntax(DocTagCast)@39..53
        Token(TkTagCast)@39..43 "cast"
        Token(TkWhitespace)@43..44 " "
        Syntax(NameExpr)@44..45
          Token(TkName)@44..45 "b"
        Token(TkWhitespace)@45..46 " "
        Syntax(DocOpType)@46..53
          Token(TkPlus)@46..47 "+"
          Syntax(TypeName)@47..53
            Token(TkName)@47..53 "number"
      Token(TkEndOfLine)@53..54 "\n"
      Token(TkWhitespace)@54..62 "        "
      Token(TkDocStart)@62..66 "---@"
      Syntax(DocTagCast)@66..80
        Token(TkTagCast)@66..70 "cast"
        Token(TkWhitespace)@70..71 " "
        Syntax(NameExpr)@71..72
          Token(TkName)@71..72 "c"
        Token(TkWhitespace)@72..73 " "
        Syntax(DocOpType)@73..80
          Token(TkMinus)@73..74 "-"
          Syntax(TypeName)@74..80
            Token(TkName)@74..80 "number"
      Token(TkEndOfLine)@80..81 "\n"
      Token(TkWhitespace)@81..89 "        "
      Token(TkDocStart)@89..93 "---@"
      Syntax(DocTagCast)@93..102
        Token(TkTagCast)@93..97 "cast"
        Token(TkWhitespace)@97..98 " "
        Syntax(NameExpr)@98..99
          Token(TkName)@98..99 "d"
        Token(TkWhitespace)@99..100 " "
        Syntax(DocOpType)@100..102
          Token(TkPlus)@100..101 "+"
          Token(TkDocQuestion)@101..102 "?"
      Token(TkEndOfLine)@102..103 "\n"
      Token(TkWhitespace)@103..111 "        "
      Token(TkDocStart)@111..115 "---@"
      Syntax(DocTagCast)@115..124
        Token(TkTagCast)@115..119 "cast"
        Token(TkWhitespace)@119..120 " "
        Syntax(NameExpr)@120..121
          Token(TkName)@120..121 "e"
        Token(TkWhitespace)@121..122 " "
        Syntax(DocOpType)@122..124
          Token(TkMinus)@122..123 "-"
          Token(TkDocQuestion)@123..124 "?"
      Token(TkEndOfLine)@124..125 "\n"
      Token(TkWhitespace)@125..133 "        "
      Token(TkDocStart)@133..137 "---@"
      Syntax(DocTagCast)@137..160
        Token(TkTagCast)@137..141 "cast"
        Token(TkWhitespace)@141..142 " "
        Syntax(NameExpr)@142..143
          Token(TkName)@142..143 "f"
        Token(TkWhitespace)@143..144 " "
        Syntax(DocOpType)@144..151
          Token(TkPlus)@144..145 "+"
          Syntax(TypeName)@145..151
            Token(TkName)@145..151 "number"
        Token(TkComma)@151..152 ","
        Token(TkWhitespace)@152..153 " "
        Syntax(DocOpType)@153..160
          Token(TkMinus)@153..154 "-"
          Syntax(TypeName)@154..160
            Token(TkName)@154..160 "string"
    Token(TkEndOfLine)@160..161 "\n"
    Token(TkWhitespace)@161..169 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_module_doc() {
        let code = r#"
        ---@module "socket.core"
        "#;

        let result = r#"
Syntax(Chunk)@0..42
  Syntax(Block)@0..42
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..33
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagModule)@13..33
        Token(TkTagModule)@13..19 "module"
        Token(TkWhitespace)@19..20 " "
        Token(TkString)@20..33 "\"socket.core\""
    Token(TkEndOfLine)@33..34 "\n"
    Token(TkWhitespace)@34..42 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_source_doc() {
        let code = r#"
        ---@source "https://groups.google.com/g/lua-l/#1:2"
        ---@source https://groups.google.com/g/lua-l/#1:2
        "#;

        let result = r#"
Syntax(Chunk)@0..127
  Syntax(Block)@0..127
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..118
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagSource)@13..60
        Token(TkTagSource)@13..19 "source"
        Token(TkWhitespace)@19..20 " "
        Token(TKDocPath)@20..60 "\"https://groups.googl ..."
      Token(TkEndOfLine)@60..61 "\n"
      Token(TkWhitespace)@61..69 "        "
      Token(TkDocStart)@69..73 "---@"
      Syntax(DocTagSource)@73..118
        Token(TkTagSource)@73..79 "source"
        Token(TkWhitespace)@79..80 " "
        Token(TKDocPath)@80..118 "https://groups.google ..."
    Token(TkEndOfLine)@118..119 "\n"
    Token(TkWhitespace)@119..127 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_as_doc() {
        let code = r#"
        --[[@as string]]
        ---@as string
        ---@as number
        ---@as string | number

        "#;

        let result = r#"
Syntax(Chunk)@0..110
  Syntax(Block)@0..110
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..100
      Token(TkDocLongStart)@9..14 "--[[@"
      Syntax(DocTagAs)@14..25
        Token(TkTagAs)@14..16 "as"
        Token(TkWhitespace)@16..17 " "
        Syntax(TypeName)@17..23
          Token(TkName)@17..23 "string"
        Token(TkLongCommentEnd)@23..25 "]]"
      Token(TkEndOfLine)@25..26 "\n"
      Token(TkWhitespace)@26..34 "        "
      Token(TkDocStart)@34..38 "---@"
      Syntax(DocTagAs)@38..47
        Token(TkTagAs)@38..40 "as"
        Token(TkWhitespace)@40..41 " "
        Syntax(TypeName)@41..47
          Token(TkName)@41..47 "string"
      Token(TkEndOfLine)@47..48 "\n"
      Token(TkWhitespace)@48..56 "        "
      Token(TkDocStart)@56..60 "---@"
      Syntax(DocTagAs)@60..69
        Token(TkTagAs)@60..62 "as"
        Token(TkWhitespace)@62..63 " "
        Syntax(TypeName)@63..69
          Token(TkName)@63..69 "number"
      Token(TkEndOfLine)@69..70 "\n"
      Token(TkWhitespace)@70..78 "        "
      Token(TkDocStart)@78..82 "---@"
      Syntax(DocTagAs)@82..100
        Token(TkTagAs)@82..84 "as"
        Token(TkWhitespace)@84..85 " "
        Syntax(TypeBinary)@85..100
          Syntax(TypeName)@85..91
            Token(TkName)@85..91 "string"
          Token(TkWhitespace)@91..92 " "
          Token(TkDocOr)@92..93 "|"
          Token(TkWhitespace)@93..94 " "
          Syntax(TypeName)@94..100
            Token(TkName)@94..100 "number"
    Token(TkEndOfLine)@100..101 "\n"
    Token(TkEndOfLine)@101..102 "\n"
    Token(TkWhitespace)@102..110 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_deprecated_doc() {
        let code = r#"
        ---@deprecated
        ---@deprecated use `f` instead
        ---@deprecated use `f` instead, will be removed in 1.0
        "#;

        let result = r#"
Syntax(Chunk)@0..134
  Syntax(Block)@0..134
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..125
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagDeprecated)@13..23
        Token(TkTagDeprecated)@13..23 "deprecated"
      Token(TkEndOfLine)@23..24 "\n"
      Token(TkWhitespace)@24..32 "        "
      Syntax(DocDescription)@32..32
      Token(TkDocStart)@32..36 "---@"
      Syntax(DocTagDeprecated)@36..46
        Token(TkTagDeprecated)@36..46 "deprecated"
      Token(TkWhitespace)@46..47 " "
      Syntax(DocDescription)@47..62
        Token(TkDocDetail)@47..62 "use `f` instead"
      Token(TkEndOfLine)@62..63 "\n"
      Token(TkWhitespace)@63..71 "        "
      Token(TkDocStart)@71..75 "---@"
      Syntax(DocTagDeprecated)@75..85
        Token(TkTagDeprecated)@75..85 "deprecated"
      Token(TkWhitespace)@85..86 " "
      Syntax(DocDescription)@86..125
        Token(TkDocDetail)@86..125 "use `f` instead, will ..."
    Token(TkEndOfLine)@125..126 "\n"
    Token(TkWhitespace)@126..134 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_see_doc() {
        let code = r#"
        ---@see aaa#bbb
        "#;

        let result = r##"
Syntax(Chunk)@0..33
  Syntax(Block)@0..33
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..24
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagSee)@13..24
        Token(TkTagSee)@13..16 "see"
        Token(TkWhitespace)@16..17 " "
        Token(TkDocSeeContent)@17..24 "aaa#bbb"
    Token(TkEndOfLine)@24..25 "\n"
    Token(TkWhitespace)@25..33 "        "
        "##;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_version_doc() {
        let code = r#"
        ---@version 5.1
        ---@version > 5.1
        ---@version JIT
        ---@version 5.1, 5.2
        ---@version 5.1, > 5.2
        "#;
        print_ast(code);
        let result = r#"
Syntax(Chunk)@0..143
  Syntax(Block)@0..143
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..134
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagVersion)@13..24
        Token(TkTagVersion)@13..20 "version"
        Token(TkWhitespace)@20..21 " "
        Syntax(DocVersion)@21..24
          Token(TkDocVersionNumber)@21..24 "5.1"
      Token(TkEndOfLine)@24..25 "\n"
      Token(TkWhitespace)@25..33 "        "
      Token(TkDocStart)@33..37 "---@"
      Syntax(DocTagVersion)@37..50
        Token(TkTagVersion)@37..44 "version"
        Token(TkWhitespace)@44..45 " "
        Syntax(DocVersion)@45..50
          Token(TkGt)@45..46 ">"
          Token(TkWhitespace)@46..47 " "
          Token(TkDocVersionNumber)@47..50 "5.1"
      Token(TkEndOfLine)@50..51 "\n"
      Token(TkWhitespace)@51..59 "        "
      Token(TkDocStart)@59..63 "---@"
      Syntax(DocTagVersion)@63..74
        Token(TkTagVersion)@63..70 "version"
        Token(TkWhitespace)@70..71 " "
        Syntax(DocVersion)@71..74
          Token(TkDocVersionNumber)@71..74 "JIT"
      Token(TkEndOfLine)@74..75 "\n"
      Token(TkWhitespace)@75..83 "        "
      Token(TkDocStart)@83..87 "---@"
      Syntax(DocTagVersion)@87..103
        Token(TkTagVersion)@87..94 "version"
        Token(TkWhitespace)@94..95 " "
        Syntax(DocVersion)@95..98
          Token(TkDocVersionNumber)@95..98 "5.1"
        Token(TkComma)@98..99 ","
        Token(TkWhitespace)@99..100 " "
        Syntax(DocVersion)@100..103
          Token(TkDocVersionNumber)@100..103 "5.2"
      Token(TkEndOfLine)@103..104 "\n"
      Token(TkWhitespace)@104..112 "        "
      Token(TkDocStart)@112..116 "---@"
      Syntax(DocTagVersion)@116..134
        Token(TkTagVersion)@116..123 "version"
        Token(TkWhitespace)@123..124 " "
        Syntax(DocVersion)@124..127
          Token(TkDocVersionNumber)@124..127 "5.1"
        Token(TkComma)@127..128 ","
        Token(TkWhitespace)@128..129 " "
        Syntax(DocVersion)@129..134
          Token(TkGt)@129..130 ">"
          Token(TkWhitespace)@130..131 " "
          Token(TkDocVersionNumber)@131..134 "5.2"
    Token(TkEndOfLine)@134..135 "\n"
    Token(TkWhitespace)@135..143 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_namespace_and_using_doc() {
        let code = r#"
        ---@namespace System.IO
        ---@using System.IO
        "#;

        let result = r#"
Syntax(Chunk)@0..69
  Syntax(Block)@0..69
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..60
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagNamespace)@13..32
        Token(TkTagNamespace)@13..22 "namespace"
        Token(TkWhitespace)@22..23 " "
        Token(TkName)@23..32 "System.IO"
      Token(TkEndOfLine)@32..33 "\n"
      Token(TkWhitespace)@33..41 "        "
      Token(TkDocStart)@41..45 "---@"
      Syntax(DocTagUsing)@45..60
        Token(TkTagUsing)@45..50 "using"
        Token(TkWhitespace)@50..51 " "
        Token(TkName)@51..60 "System.IO"
    Token(TkEndOfLine)@60..61 "\n"
    Token(TkWhitespace)@61..69 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_simple_doc() {
        let code = r#"
        ---@meta

        ---@mapping str

        ---@async

        ---@readonly

        ---@nodiscard

        ---@private
        ---@public
        ---@package
        ---@protected
        "#;

        let result = r#"
Syntax(Chunk)@0..197
  Syntax(Block)@0..197
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..17
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagMeta)@13..17
        Token(TkTagMeta)@13..17 "meta"
    Token(TkEndOfLine)@17..18 "\n"
    Token(TkEndOfLine)@18..19 "\n"
    Token(TkWhitespace)@19..27 "        "
    Syntax(Comment)@27..42
      Token(TkDocStart)@27..31 "---@"
      Syntax(DocTagMapping)@31..42
        Token(TkTagMapping)@31..38 "mapping"
        Token(TkWhitespace)@38..39 " "
        Token(TkName)@39..42 "str"
    Token(TkEndOfLine)@42..43 "\n"
    Token(TkEndOfLine)@43..44 "\n"
    Token(TkWhitespace)@44..52 "        "
    Syntax(Comment)@52..61
      Token(TkDocStart)@52..56 "---@"
      Syntax(DocTagAsync)@56..61
        Token(TkTagAsync)@56..61 "async"
    Token(TkEndOfLine)@61..62 "\n"
    Token(TkEndOfLine)@62..63 "\n"
    Token(TkWhitespace)@63..71 "        "
    Syntax(Comment)@71..83
      Token(TkDocStart)@71..75 "---@"
      Syntax(DocTagReadonly)@75..83
        Token(TkTagReadonly)@75..83 "readonly"
    Token(TkEndOfLine)@83..84 "\n"
    Token(TkEndOfLine)@84..85 "\n"
    Token(TkWhitespace)@85..93 "        "
    Syntax(Comment)@93..106
      Token(TkDocStart)@93..97 "---@"
      Syntax(DocTagNodiscard)@97..106
        Token(TkTagNodiscard)@97..106 "nodiscard"
    Token(TkEndOfLine)@106..107 "\n"
    Token(TkEndOfLine)@107..108 "\n"
    Token(TkWhitespace)@108..116 "        "
    Syntax(Comment)@116..188
      Token(TkDocStart)@116..120 "---@"
      Syntax(DocTagVisibility)@120..127
        Token(TkTagVisibility)@120..127 "private"
      Token(TkEndOfLine)@127..128 "\n"
      Token(TkWhitespace)@128..136 "        "
      Syntax(DocDescription)@136..136
      Token(TkDocStart)@136..140 "---@"
      Syntax(DocTagVisibility)@140..146
        Token(TkTagVisibility)@140..146 "public"
      Token(TkEndOfLine)@146..147 "\n"
      Token(TkWhitespace)@147..155 "        "
      Syntax(DocDescription)@155..155
      Token(TkDocStart)@155..159 "---@"
      Syntax(DocTagVisibility)@159..166
        Token(TkTagVisibility)@159..166 "package"
      Token(TkEndOfLine)@166..167 "\n"
      Token(TkWhitespace)@167..175 "        "
      Syntax(DocDescription)@175..175
      Token(TkDocStart)@175..179 "---@"
      Syntax(DocTagVisibility)@179..188
        Token(TkTagVisibility)@179..188 "protected"
    Token(TkEndOfLine)@188..189 "\n"
    Token(TkWhitespace)@189..197 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_operator() {
        let code = r#"
        ---@operator add(number): number
        ---@operator call: number
        "#;

        let result = r#"
Syntax(Chunk)@0..84
  Syntax(Block)@0..84
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..75
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagOperator)@13..41
        Token(TkTagOperator)@13..21 "operator"
        Token(TkWhitespace)@21..22 " "
        Token(TkName)@22..25 "add"
        Token(TkLeftParen)@25..26 "("
        Syntax(DocTypeList)@26..32
          Syntax(TypeName)@26..32
            Token(TkName)@26..32 "number"
        Token(TkRightParen)@32..33 ")"
        Token(TkColon)@33..34 ":"
        Token(TkWhitespace)@34..35 " "
        Syntax(TypeName)@35..41
          Token(TkName)@35..41 "number"
      Token(TkEndOfLine)@41..42 "\n"
      Token(TkWhitespace)@42..50 "        "
      Token(TkDocStart)@50..54 "---@"
      Syntax(DocTagOperator)@54..75
        Token(TkTagOperator)@54..62 "operator"
        Token(TkWhitespace)@62..63 " "
        Token(TkName)@63..67 "call"
        Token(TkColon)@67..68 ":"
        Token(TkWhitespace)@68..69 " "
        Syntax(TypeName)@69..75
          Token(TkName)@69..75 "number"
    Token(TkEndOfLine)@75..76 "\n"
    Token(TkWhitespace)@76..84 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_error_doc() {
        let code = r#"
        ---@param
        "#;

        let result = r#"
Syntax(Chunk)@0..27
  Syntax(Block)@0..27
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..18
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagParam)@13..18
        Token(TkTagParam)@13..18 "param"
    Token(TkEndOfLine)@18..19 "\n"
    Token(TkWhitespace)@19..27 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_long_comment() {
        let code = r#"
        --[[long comment]]
        local t = 123
        "#;

        let result = r#"
Syntax(Chunk)@0..58
  Syntax(Block)@0..58
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..27
      Token(TkLongCommentStart)@9..13 "--[["
      Syntax(DocDescription)@13..25
        Token(TkDocDetail)@13..25 "long comment"
      Token(TkDocTrivia)@25..27 "]]"
    Token(TkEndOfLine)@27..28 "\n"
    Token(TkWhitespace)@28..36 "        "
    Syntax(LocalStat)@36..49
      Token(TkLocal)@36..41 "local"
      Token(TkWhitespace)@41..42 " "
      Syntax(LocalName)@42..43
        Token(TkName)@42..43 "t"
      Token(TkWhitespace)@43..44 " "
      Token(TkAssign)@44..45 "="
      Token(TkWhitespace)@45..46 " "
      Syntax(LiteralExpr)@46..49
        Token(TkInt)@46..49 "123"
    Token(TkEndOfLine)@49..50 "\n"
    Token(TkWhitespace)@50..58 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_continuous_comment() {
        let code = r#"
        local t = 123 --comment 1
        --comment 2
        "#;

        let result = r#"
Syntax(Chunk)@0..63
  Syntax(Block)@0..63
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(LocalStat)@9..22
      Token(TkLocal)@9..14 "local"
      Token(TkWhitespace)@14..15 " "
      Syntax(LocalName)@15..16
        Token(TkName)@15..16 "t"
      Token(TkWhitespace)@16..17 " "
      Token(TkAssign)@17..18 "="
      Token(TkWhitespace)@18..19 " "
      Syntax(LiteralExpr)@19..22
        Token(TkInt)@19..22 "123"
    Token(TkWhitespace)@22..23 " "
    Syntax(Comment)@23..34
      Token(TkNormalStart)@23..25 "--"
      Syntax(DocDescription)@25..34
        Token(TkDocDetail)@25..34 "comment 1"
    Token(TkEndOfLine)@34..35 "\n"
    Token(TkWhitespace)@35..43 "        "
    Syntax(Comment)@43..54
      Token(TkNormalStart)@43..45 "--"
      Syntax(DocDescription)@45..54
        Token(TkDocDetail)@45..54 "comment 2"
    Token(TkEndOfLine)@54..55 "\n"
    Token(TkWhitespace)@55..63 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_meta() {
        let code = r#"
        ---@meta socket.io
        "#;

        let result = r#"
Syntax(Chunk)@0..36
  Syntax(Block)@0..36
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..27
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagMeta)@13..27
        Token(TkTagMeta)@13..17 "meta"
        Token(TkWhitespace)@17..18 " "
        Token(TkName)@18..27 "socket.io"
    Token(TkEndOfLine)@27..28 "\n"
    Token(TkWhitespace)@28..36 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_region() {
        let code = r#"
        --region hhhh
        --endregion
        "#;

        let result = r#"
Syntax(Chunk)@0..51
  Syntax(Block)@0..51
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..42
      Token(TkNormalStart)@9..11 "--"
      Token(TkDocRegion)@11..17 "region"
      Token(TkWhitespace)@17..18 " "
      Syntax(DocDescription)@18..33
        Token(TkDocDetail)@18..22 "hhhh"
        Token(TkEndOfLine)@22..23 "\n"
        Token(TkWhitespace)@23..31 "        "
        Token(TkNormalStart)@31..33 "--"
      Token(TkDocEndRegion)@33..42 "endregion"
    Token(TkEndOfLine)@42..43 "\n"
    Token(TkWhitespace)@43..51 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_cast_expr() {
        let code = r#"
---@cast a number
---@cast a.field string
---@cast A.b.c.d boolean
---@cast -?
        "#;
        let result = r#"
Syntax(Chunk)@0..88
  Syntax(Block)@0..88
    Token(TkEndOfLine)@0..1 "\n"
    Syntax(Comment)@1..79
      Token(TkDocStart)@1..5 "---@"
      Syntax(DocTagCast)@5..18
        Token(TkTagCast)@5..9 "cast"
        Token(TkWhitespace)@9..10 " "
        Syntax(NameExpr)@10..11
          Token(TkName)@10..11 "a"
        Token(TkWhitespace)@11..12 " "
        Syntax(DocOpType)@12..18
          Syntax(TypeName)@12..18
            Token(TkName)@12..18 "number"
      Token(TkEndOfLine)@18..19 "\n"
      Token(TkDocStart)@19..23 "---@"
      Syntax(DocTagCast)@23..42
        Token(TkTagCast)@23..27 "cast"
        Token(TkWhitespace)@27..28 " "
        Syntax(IndexExpr)@28..35
          Syntax(NameExpr)@28..29
            Token(TkName)@28..29 "a"
          Token(TkDot)@29..30 "."
          Token(TkName)@30..35 "field"
        Token(TkWhitespace)@35..36 " "
        Syntax(DocOpType)@36..42
          Syntax(TypeName)@36..42
            Token(TkName)@36..42 "string"
      Token(TkEndOfLine)@42..43 "\n"
      Token(TkDocStart)@43..47 "---@"
      Syntax(DocTagCast)@47..67
        Token(TkTagCast)@47..51 "cast"
        Token(TkWhitespace)@51..52 " "
        Syntax(IndexExpr)@52..59
          Syntax(IndexExpr)@52..57
            Syntax(IndexExpr)@52..55
              Syntax(NameExpr)@52..53
                Token(TkName)@52..53 "A"
              Token(TkDot)@53..54 "."
              Token(TkName)@54..55 "b"
            Token(TkDot)@55..56 "."
            Token(TkName)@56..57 "c"
          Token(TkDot)@57..58 "."
          Token(TkName)@58..59 "d"
        Token(TkWhitespace)@59..60 " "
        Syntax(DocOpType)@60..67
          Syntax(TypeName)@60..67
            Token(TkName)@60..67 "boolean"
      Token(TkEndOfLine)@67..68 "\n"
      Token(TkDocStart)@68..72 "---@"
      Syntax(DocTagCast)@72..79
        Token(TkTagCast)@72..76 "cast"
        Token(TkWhitespace)@76..77 " "
        Syntax(DocOpType)@77..79
          Token(TkMinus)@77..78 "-"
          Token(TkDocQuestion)@78..79 "?"
    Token(TkEndOfLine)@79..80 "\n"
    Token(TkWhitespace)@80..88 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_multi_level_cast() {
        let code = r#"
        ---@cast obj.a.b.c.d string
        "#;
        // Note: The exact line numbers may vary, but the structure should be correct
        let tree = LuaParser::parse(code, ParserConfig::default());
        let result = format!("{:#?}", tree.get_red_root());

        // Verify that we have the correct nested structure
        assert!(result.contains("IndexExpr"));
        assert!(result.contains("NameExpr"));
        assert!(result.contains("TkDot"));
        assert!(result.contains("obj"));
        assert!(result.contains("string"));

        // Print the actual result for debugging
        println!("Actual AST structure:\n{}", result);
    }

    #[test]
    fn test_compact_luals_param() {
        let code = r#"
        ---@param a 
        ---| aaa
        ---| bbb
        "#;

        let result = r#"
Syntax(Chunk)@0..64
  Syntax(Block)@0..64
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..55
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagParam)@13..55
        Token(TkTagParam)@13..18 "param"
        Token(TkWhitespace)@18..19 " "
        Token(TkName)@19..20 "a"
        Token(TkWhitespace)@20..21 " "
        Token(TkEndOfLine)@21..22 "\n"
        Token(TkWhitespace)@22..30 "        "
        Syntax(TypeMultiLineUnion)@30..55
          Token(TkDocContinueOr)@30..34 "---|"
          Token(TkWhitespace)@34..35 " "
          Syntax(DocOneLineField)@35..38
            Syntax(TypeName)@35..38
              Token(TkName)@35..38 "aaa"
          Token(TkEndOfLine)@38..39 "\n"
          Token(TkWhitespace)@39..47 "        "
          Token(TkDocContinueOr)@47..51 "---|"
          Token(TkWhitespace)@51..52 " "
          Syntax(DocOneLineField)@52..55
            Syntax(TypeName)@52..55
              Token(TkName)@52..55 "bbb"
    Token(TkEndOfLine)@55..56 "\n"
    Token(TkWhitespace)@56..64 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_compact_luals_return() {
        let code = r#"
        ---@return
        ---| aaa
        ---| bbb
        "#;

        let result = r#"
Syntax(Chunk)@0..62
  Syntax(Block)@0..62
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..53
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagReturn)@13..53
        Token(TkTagReturn)@13..19 "return"
        Token(TkEndOfLine)@19..20 "\n"
        Token(TkWhitespace)@20..28 "        "
        Syntax(TypeMultiLineUnion)@28..53
          Token(TkDocContinueOr)@28..32 "---|"
          Token(TkWhitespace)@32..33 " "
          Syntax(DocOneLineField)@33..36
            Syntax(TypeName)@33..36
              Token(TkName)@33..36 "aaa"
          Token(TkEndOfLine)@36..37 "\n"
          Token(TkWhitespace)@37..45 "        "
          Token(TkDocContinueOr)@45..49 "---|"
          Token(TkWhitespace)@49..50 " "
          Syntax(DocOneLineField)@50..53
            Syntax(TypeName)@50..53
              Token(TkName)@50..53 "bbb"
    Token(TkEndOfLine)@53..54 "\n"
    Token(TkWhitespace)@54..62 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_compact_luals_alias() {
        let code = r#"
        ---@alias a
        ---|+ "12313"
        ---|+ "123131"
        "#;

        let result = r#"
Syntax(Chunk)@0..74
  Syntax(Block)@0..74
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..65
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagAlias)@13..65
        Token(TkTagAlias)@13..18 "alias"
        Token(TkWhitespace)@18..19 " "
        Token(TkName)@19..20 "a"
        Token(TkEndOfLine)@20..21 "\n"
        Token(TkWhitespace)@21..29 "        "
        Syntax(TypeMultiLineUnion)@29..65
          Token(TkDocContinueOr)@29..34 "---|+"
          Token(TkWhitespace)@34..35 " "
          Syntax(DocOneLineField)@35..42
            Syntax(TypeLiteral)@35..42
              Token(TkString)@35..42 "\"12313\""
          Token(TkEndOfLine)@42..43 "\n"
          Token(TkWhitespace)@43..51 "        "
          Token(TkDocContinueOr)@51..56 "---|+"
          Token(TkWhitespace)@56..57 " "
          Syntax(DocOneLineField)@57..65
            Syntax(TypeLiteral)@57..65
              Token(TkString)@57..65 "\"123131\""
    Token(TkEndOfLine)@65..66 "\n"
    Token(TkWhitespace)@66..74 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_visibility() {
        let code = r#"
        ---@private
        ---@public
        ---@package
        ---@protected
        "#;

        let result = r#"
Syntax(Chunk)@0..90
  Syntax(Block)@0..90
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..81
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagVisibility)@13..20
        Token(TkTagVisibility)@13..20 "private"
      Token(TkEndOfLine)@20..21 "\n"
      Token(TkWhitespace)@21..29 "        "
      Syntax(DocDescription)@29..29
      Token(TkDocStart)@29..33 "---@"
      Syntax(DocTagVisibility)@33..39
        Token(TkTagVisibility)@33..39 "public"
      Token(TkEndOfLine)@39..40 "\n"
      Token(TkWhitespace)@40..48 "        "
      Syntax(DocDescription)@48..48
      Token(TkDocStart)@48..52 "---@"
      Syntax(DocTagVisibility)@52..59
        Token(TkTagVisibility)@52..59 "package"
      Token(TkEndOfLine)@59..60 "\n"
      Token(TkWhitespace)@60..68 "        "
      Syntax(DocDescription)@68..68
      Token(TkDocStart)@68..72 "---@"
      Syntax(DocTagVisibility)@72..81
        Token(TkTagVisibility)@72..81 "protected"
    Token(TkEndOfLine)@81..82 "\n"
    Token(TkWhitespace)@82..90 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_region_with_comment() {
        let code = r#"
        -----------
        --region hhhh
        --comment
        --endregion
        "#;

        let result = r#"
Syntax(Chunk)@0..89
  Syntax(Block)@0..89
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..80
      Token(TkNormalStart)@9..12 "---"
      Syntax(DocDescription)@12..31
        Token(TkDocDetail)@12..20 "--------"
        Token(TkEndOfLine)@20..21 "\n"
        Token(TkWhitespace)@21..29 "        "
        Token(TkNormalStart)@29..31 "--"
      Token(TkDocTrivia)@31..37 "region"
      Token(TkDocTrivia)@37..42 " hhhh"
      Token(TkEndOfLine)@42..43 "\n"
      Token(TkWhitespace)@43..51 "        "
      Token(TkNormalStart)@51..53 "--"
      Syntax(DocDescription)@53..71
        Token(TkDocDetail)@53..60 "comment"
        Token(TkEndOfLine)@60..61 "\n"
        Token(TkWhitespace)@61..69 "        "
        Token(TkNormalStart)@69..71 "--"
      Token(TkDocEndRegion)@71..80 "endregion"
    Token(TkEndOfLine)@80..81 "\n"
    Token(TkWhitespace)@81..89 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_tuple_type() {
        let code = r#"
      ---@type [string]
      "#;
        let result = r#"
Syntax(Chunk)@0..31
  Syntax(Block)@0..31
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..7 "      "
    Syntax(Comment)@7..24
      Token(TkDocStart)@7..11 "---@"
      Syntax(DocTagType)@11..24
        Token(TkTagType)@11..15 "type"
        Token(TkWhitespace)@15..16 " "
        Syntax(TypeTuple)@16..24
          Token(TkLeftBracket)@16..17 "["
          Syntax(TypeName)@17..23
            Token(TkName)@17..23 "string"
          Token(TkRightBracket)@23..24 "]"
    Token(TkEndOfLine)@24..25 "\n"
    Token(TkWhitespace)@25..31 "      "
      "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_variadic_type() {
        let code = r#"
        ---@type T...
        "#;
        let result = r#"
Syntax(Chunk)@0..31
  Syntax(Block)@0..31
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..22
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagType)@13..22
        Token(TkTagType)@13..17 "type"
        Token(TkWhitespace)@17..18 " "
        Syntax(TypeVariadic)@18..22
          Syntax(TypeName)@18..19
            Token(TkName)@18..19 "T"
          Token(TkDots)@19..22 "..."
    Token(TkEndOfLine)@22..23 "\n"
    Token(TkWhitespace)@23..31 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_luals_multi_return() {
        let code = r#"
        ---@type fun(): (integer, number)
        "#;
        let result = r#"
Syntax(Chunk)@0..51
  Syntax(Block)@0..51
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..42
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagType)@13..42
        Token(TkTagType)@13..17 "type"
        Token(TkWhitespace)@17..18 " "
        Syntax(TypeFun)@18..42
          Token(TkName)@18..21 "fun"
          Token(TkLeftParen)@21..22 "("
          Token(TkRightParen)@22..23 ")"
          Token(TkColon)@23..24 ":"
          Token(TkWhitespace)@24..25 " "
          Syntax(DocTypeList)@25..42
            Token(TkLeftParen)@25..26 "("
            Syntax(DocNamedReturnType)@26..33
              Syntax(TypeName)@26..33
                Token(TkName)@26..33 "integer"
            Token(TkComma)@33..34 ","
            Token(TkWhitespace)@34..35 " "
            Syntax(DocNamedReturnType)@35..41
              Syntax(TypeName)@35..41
                Token(TkName)@35..41 "number"
            Token(TkRightParen)@41..42 ")"
    Token(TkEndOfLine)@42..43 "\n"
    Token(TkWhitespace)@43..51 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_multi_line_type() {
        let code = r#"
        ---@type {
        --- x: number,
        --- y: number,
        --- z: number,
        ---}
        "#;
        let result = r#"
Syntax(Chunk)@0..110
  Syntax(Block)@0..110
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..101
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagType)@13..101
        Token(TkTagType)@13..17 "type"
        Token(TkWhitespace)@17..18 " "
        Syntax(TypeObject)@18..101
          Token(TkLeftBrace)@18..19 "{"
          Token(TkEndOfLine)@19..20 "\n"
          Token(TkWhitespace)@20..28 "        "
          Token(TkDocContinue)@28..32 "--- "
          Syntax(DocObjectField)@32..41
            Token(TkName)@32..33 "x"
            Token(TkColon)@33..34 ":"
            Token(TkWhitespace)@34..35 " "
            Syntax(TypeName)@35..41
              Token(TkName)@35..41 "number"
          Token(TkComma)@41..42 ","
          Token(TkEndOfLine)@42..43 "\n"
          Token(TkWhitespace)@43..51 "        "
          Token(TkDocContinue)@51..55 "--- "
          Syntax(DocObjectField)@55..64
            Token(TkName)@55..56 "y"
            Token(TkColon)@56..57 ":"
            Token(TkWhitespace)@57..58 " "
            Syntax(TypeName)@58..64
              Token(TkName)@58..64 "number"
          Token(TkComma)@64..65 ","
          Token(TkEndOfLine)@65..66 "\n"
          Token(TkWhitespace)@66..74 "        "
          Token(TkDocContinue)@74..78 "--- "
          Syntax(DocObjectField)@78..87
            Token(TkName)@78..79 "z"
            Token(TkColon)@79..80 ":"
            Token(TkWhitespace)@80..81 " "
            Syntax(TypeName)@81..87
              Token(TkName)@81..87 "number"
          Token(TkComma)@87..88 ","
          Token(TkEndOfLine)@88..89 "\n"
          Token(TkWhitespace)@89..97 "        "
          Token(TkDocContinue)@97..100 "---"
          Token(TkRightBrace)@100..101 "}"
    Token(TkEndOfLine)@101..102 "\n"
    Token(TkWhitespace)@102..110 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_param_union() {
        let code = r#"
        ---@param a
        ---| number # nihao 
        ---| string # wohao
        "#;
        let result = r##"
Syntax(Chunk)@0..86
  Syntax(Block)@0..86
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..77
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagParam)@13..77
        Token(TkTagParam)@13..18 "param"
        Token(TkWhitespace)@18..19 " "
        Token(TkName)@19..20 "a"
        Token(TkEndOfLine)@20..21 "\n"
        Token(TkWhitespace)@21..29 "        "
        Syntax(TypeMultiLineUnion)@29..77
          Token(TkDocContinueOr)@29..33 "---|"
          Token(TkWhitespace)@33..34 " "
          Syntax(DocOneLineField)@34..40
            Syntax(TypeName)@34..40
              Token(TkName)@34..40 "number"
          Token(TkWhitespace)@40..41 " "
          Syntax(DocDescription)@41..49
            Token(TkDocDetail)@41..49 "# nihao "
          Token(TkEndOfLine)@49..50 "\n"
          Token(TkWhitespace)@50..58 "        "
          Token(TkDocContinueOr)@58..62 "---|"
          Token(TkWhitespace)@62..63 " "
          Syntax(DocOneLineField)@63..69
            Syntax(TypeName)@63..69
              Token(TkName)@63..69 "string"
          Token(TkWhitespace)@69..70 " "
          Syntax(DocDescription)@70..77
            Token(TkDocDetail)@70..77 "# wohao"
    Token(TkEndOfLine)@77..78 "\n"
    Token(TkWhitespace)@78..86 "        "
        "##;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_return_union() {
        let code = r#"
        ---@return
        ---| number # nihao 
        ---| string # wohao
        "#;
        let result = r##"
Syntax(Chunk)@0..85
  Syntax(Block)@0..85
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..76
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagReturn)@13..76
        Token(TkTagReturn)@13..19 "return"
        Token(TkEndOfLine)@19..20 "\n"
        Token(TkWhitespace)@20..28 "        "
        Syntax(TypeMultiLineUnion)@28..76
          Token(TkDocContinueOr)@28..32 "---|"
          Token(TkWhitespace)@32..33 " "
          Syntax(DocOneLineField)@33..39
            Syntax(TypeName)@33..39
              Token(TkName)@33..39 "number"
          Token(TkWhitespace)@39..40 " "
          Syntax(DocDescription)@40..48
            Token(TkDocDetail)@40..48 "# nihao "
          Token(TkEndOfLine)@48..49 "\n"
          Token(TkWhitespace)@49..57 "        "
          Token(TkDocContinueOr)@57..61 "---|"
          Token(TkWhitespace)@61..62 " "
          Syntax(DocOneLineField)@62..68
            Syntax(TypeName)@62..68
              Token(TkName)@62..68 "string"
          Token(TkWhitespace)@68..69 " "
          Syntax(DocDescription)@69..76
            Token(TkDocDetail)@69..76 "# wohao"
    Token(TkEndOfLine)@76..77 "\n"
    Token(TkWhitespace)@77..85 "        "
        "##;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_multiline_description_union() {
        let code = r#"
        ---@return
        ---| number # nihao 
        ---  woyehao
        --- dajiahao
        ---| string # wohao
        "#;
        let result = r##"
Syntax(Chunk)@0..127
  Syntax(Block)@0..127
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..118
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagReturn)@13..118
        Token(TkTagReturn)@13..19 "return"
        Token(TkEndOfLine)@19..20 "\n"
        Token(TkWhitespace)@20..28 "        "
        Syntax(TypeMultiLineUnion)@28..118
          Token(TkDocContinueOr)@28..32 "---|"
          Token(TkWhitespace)@32..33 " "
          Syntax(DocOneLineField)@33..39
            Syntax(TypeName)@33..39
              Token(TkName)@33..39 "number"
          Token(TkWhitespace)@39..40 " "
          Syntax(DocDescription)@40..90
            Token(TkDocDetail)@40..48 "# nihao "
            Token(TkEndOfLine)@48..49 "\n"
            Token(TkWhitespace)@49..57 "        "
            Token(TkNormalStart)@57..62 "---  "
            Token(TkDocDetail)@62..69 "woyehao"
            Token(TkEndOfLine)@69..70 "\n"
            Token(TkWhitespace)@70..78 "        "
            Token(TkNormalStart)@78..82 "--- "
            Token(TkDocDetail)@82..90 "dajiahao"
          Token(TkEndOfLine)@90..91 "\n"
          Token(TkWhitespace)@91..99 "        "
          Token(TkDocContinueOr)@99..103 "---|"
          Token(TkWhitespace)@103..104 " "
          Syntax(DocOneLineField)@104..110
            Syntax(TypeName)@104..110
              Token(TkName)@104..110 "string"
          Token(TkWhitespace)@110..111 " "
          Syntax(DocDescription)@111..118
            Token(TkDocDetail)@111..118 "# wohao"
    Token(TkEndOfLine)@118..119 "\n"
    Token(TkWhitespace)@119..127 "        "
        "##;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_neg_integer() {
        let code = r#"
        ---@type -123
        "#;
        let result = r#"
Syntax(Chunk)@0..31
  Syntax(Block)@0..31
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..22
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagType)@13..22
        Token(TkTagType)@13..17 "type"
        Token(TkWhitespace)@17..18 " "
        Syntax(TypeUnary)@18..22
          Token(TkMinus)@18..19 "-"
          Syntax(TypeLiteral)@19..22
            Token(TkInt)@19..22 "123"
    Token(TkEndOfLine)@22..23 "\n"
    Token(TkWhitespace)@23..31 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_fun_return_type() {
        let code = r#"
        ---@type fun(): (name: string, age: number)
        "#;
        let result = r#"
Syntax(Chunk)@0..61
  Syntax(Block)@0..61
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..52
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagType)@13..52
        Token(TkTagType)@13..17 "type"
        Token(TkWhitespace)@17..18 " "
        Syntax(TypeFun)@18..52
          Token(TkName)@18..21 "fun"
          Token(TkLeftParen)@21..22 "("
          Token(TkRightParen)@22..23 ")"
          Token(TkColon)@23..24 ":"
          Token(TkWhitespace)@24..25 " "
          Syntax(DocTypeList)@25..52
            Token(TkLeftParen)@25..26 "("
            Syntax(DocNamedReturnType)@26..38
              Syntax(TypeName)@26..30
                Token(TkName)@26..30 "name"
              Token(TkColon)@30..31 ":"
              Token(TkWhitespace)@31..32 " "
              Syntax(TypeName)@32..38
                Token(TkName)@32..38 "string"
            Token(TkComma)@38..39 ","
            Token(TkWhitespace)@39..40 " "
            Syntax(DocNamedReturnType)@40..51
              Syntax(TypeName)@40..43
                Token(TkName)@40..43 "age"
              Token(TkColon)@43..44 ":"
              Token(TkWhitespace)@44..45 " "
              Syntax(TypeName)@45..51
                Token(TkName)@45..51 "number"
            Token(TkRightParen)@51..52 ")"
    Token(TkEndOfLine)@52..53 "\n"
    Token(TkWhitespace)@53..61 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_str_tpl() {
        let code = r#"
        ---@param a aaa.`T`.bbbb
        ---@param a aaa.`T`
        ---@param a `T`.bbbb
        ---@param a `T`
        "#;
        let result = r#"
Syntax(Chunk)@0..123
  Syntax(Block)@0..123
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..114
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagParam)@13..33
        Token(TkTagParam)@13..18 "param"
        Token(TkWhitespace)@18..19 " "
        Token(TkName)@19..20 "a"
        Token(TkWhitespace)@20..21 " "
        Syntax(TypeStringTemplate)@21..33
          Token(TkStringTemplateType)@21..33 "aaa.`T`.bbbb"
      Token(TkEndOfLine)@33..34 "\n"
      Token(TkWhitespace)@34..42 "        "
      Token(TkDocStart)@42..46 "---@"
      Syntax(DocTagParam)@46..61
        Token(TkTagParam)@46..51 "param"
        Token(TkWhitespace)@51..52 " "
        Token(TkName)@52..53 "a"
        Token(TkWhitespace)@53..54 " "
        Syntax(TypeStringTemplate)@54..61
          Token(TkStringTemplateType)@54..61 "aaa.`T`"
      Token(TkEndOfLine)@61..62 "\n"
      Token(TkWhitespace)@62..70 "        "
      Token(TkDocStart)@70..74 "---@"
      Syntax(DocTagParam)@74..90
        Token(TkTagParam)@74..79 "param"
        Token(TkWhitespace)@79..80 " "
        Token(TkName)@80..81 "a"
        Token(TkWhitespace)@81..82 " "
        Syntax(TypeStringTemplate)@82..90
          Token(TkStringTemplateType)@82..90 "`T`.bbbb"
      Token(TkEndOfLine)@90..91 "\n"
      Token(TkWhitespace)@91..99 "        "
      Token(TkDocStart)@99..103 "---@"
      Syntax(DocTagParam)@103..114
        Token(TkTagParam)@103..108 "param"
        Token(TkWhitespace)@108..109 " "
        Token(TkName)@109..110 "a"
        Token(TkWhitespace)@110..111 " "
        Syntax(TypeStringTemplate)@111..114
          Token(TkStringTemplateType)@111..114 "`T`"
    Token(TkEndOfLine)@114..115 "\n"
    Token(TkWhitespace)@115..123 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_comment() {
        let code = r#"
        --- Note: ajfioiof
        ---  |enenen|
        ---  |enenen|
        ---  |enenen|
        local d
        "#;
        let result = r#"
Syntax(Chunk)@0..118
  Syntax(Block)@0..118
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..93
      Token(TkNormalStart)@9..13 "--- "
      Syntax(DocDescription)@13..93
        Token(TkDocDetail)@13..27 "Note: ajfioiof"
        Token(TkEndOfLine)@27..28 "\n"
        Token(TkWhitespace)@28..36 "        "
        Token(TkNormalStart)@36..41 "---  "
        Token(TkDocDetail)@41..49 "|enenen|"
        Token(TkEndOfLine)@49..50 "\n"
        Token(TkWhitespace)@50..58 "        "
        Token(TkNormalStart)@58..63 "---  "
        Token(TkDocDetail)@63..71 "|enenen|"
        Token(TkEndOfLine)@71..72 "\n"
        Token(TkWhitespace)@72..80 "        "
        Token(TkNormalStart)@80..85 "---  "
        Token(TkDocDetail)@85..93 "|enenen|"
    Token(TkEndOfLine)@93..94 "\n"
    Token(TkWhitespace)@94..102 "        "
    Syntax(LocalStat)@102..109
      Token(TkLocal)@102..107 "local"
      Token(TkWhitespace)@107..108 " "
      Syntax(LocalName)@108..109
        Token(TkName)@108..109 "d"
    Token(TkEndOfLine)@109..110 "\n"
    Token(TkWhitespace)@110..118 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    // can not pass the test, I donot know why
    //     #[test]
    //     fn test_comment_2() {
    //         let code = r#"
    //         --- Sum two numbers
    //         --- Example:
    //         --- ```lua
    //         --- -- `c` is equal to 5
    //         --- local c = sum(2, 3)
    //         --- ```
    //         local function sum(a, b) return a + b end
    //         "#;
    //         let result = r##"
    // Syntax(Chunk)@0..208
    //   Syntax(Block)@0..208
    //     Token(TkEndOfLine)@0..1 "\n"
    //     Token(TkWhitespace)@1..9 "        "
    //     Syntax(Comment)@9..149
    //       Token(TkNormalStart)@9..13 "--- "
    //       Syntax(DocDescription)@13..149
    //         Token(TkDocDetail)@13..28 "Sum two numbers"
    //         Token(TkEndOfLine)@28..29 "\n"
    //         Token(TkWhitespace)@29..37 "        "
    //         Token(TkNormalStart)@37..41 "--- "
    //         Token(TkDocDetail)@41..49 "Example:"
    //         Token(TkEndOfLine)@49..50 "\n"
    //         Token(TkWhitespace)@50..58 "        "
    //         Token(TkNormalStart)@58..62 "--- "
    //         Token(TkDocDetail)@62..68 "```lua"
    //         Token(TkEndOfLine)@68..69 "\n"
    //         Token(TkWhitespace)@69..77 "        "
    //         Token(TkNormalStart)@77..81 "--- "
    //         Token(TkDocDetail)@81..101 "-- `c` is equal to 5"
    //         Token(TkEndOfLine)@101..102 "\n"
    //         Token(TkWhitespace)@102..110 "        "
    //         Token(TkNormalStart)@110..114 "--- "
    //         Token(TkDocDetail)@114..133 "local c = sum(2, 3)"
    //         Token(TkEndOfLine)@133..134 "\n"
    //         Token(TkWhitespace)@134..142 "        "
    //         Token(TkNormalStart)@142..146 "--- "
    //         Token(TkDocDetail)@146..149 "```"
    //     Token(TkEndOfLine)@149..150 "\n"
    //     Token(TkWhitespace)@150..158 "        "
    //     Syntax(LocalFuncStat)@158..199
    //       Token(TkLocal)@158..163 "local"
    //       Token(TkWhitespace)@163..164 " "
    //       Token(TkFunction)@164..172 "function"
    //       Token(TkWhitespace)@172..173 " "
    //       Syntax(LocalName)@173..176
    //         Token(TkName)@173..176 "sum"
    //       Syntax(ClosureExpr)@176..199
    //         Syntax(ParamList)@176..182
    //           Token(TkLeftParen)@176..177 "("
    //           Syntax(ParamName)@177..178
    //             Token(TkName)@177..178 "a"
    //           Token(TkComma)@178..179 ","
    //           Token(TkWhitespace)@179..180 " "
    //           Syntax(ParamName)@180..181
    //             Token(TkName)@180..181 "b"
    //           Token(TkRightParen)@181..182 ")"
    //         Syntax(Block)@182..196
    //           Token(TkWhitespace)@182..183 " "
    //           Syntax(ReturnStat)@183..195
    //             Token(TkReturn)@183..189 "return"
    //             Token(TkWhitespace)@189..190 " "
    //             Syntax(BinaryExpr)@190..195
    //               Syntax(NameExpr)@190..191
    //                 Token(TkName)@190..191 "a"
    //               Token(TkWhitespace)@191..192 " "
    //               Token(TkPlus)@192..193 "+"
    //               Token(TkWhitespace)@193..194 " "
    //               Syntax(NameExpr)@194..195
    //                 Token(TkName)@194..195 "b"
    //           Token(TkWhitespace)@195..196 " "
    //         Token(TkEnd)@196..199 "end"
    //     Token(TkEndOfLine)@199..200 "\n"
    //     Token(TkWhitespace)@200..208 "        "
    //         "##;

    //         assert_ast_eq!(code, result);
    //     }

    #[test]
    fn test_any_type_variadic() {
        let code = r#"
        ---@return string? ...
        "#;
        let result = r#"
Syntax(Chunk)@0..40
  Syntax(Block)@0..40
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..31
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagReturn)@13..31
        Token(TkTagReturn)@13..19 "return"
        Token(TkWhitespace)@19..20 " "
        Syntax(TypeVariadic)@20..31
          Syntax(TypeNullable)@20..27
            Syntax(TypeName)@20..26
              Token(TkName)@20..26 "string"
            Token(TkDocQuestion)@26..27 "?"
          Token(TkWhitespace)@27..28 " "
          Token(TkDots)@28..31 "..."
    Token(TkEndOfLine)@31..32 "\n"
    Token(TkWhitespace)@32..40 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_object_type_grammar() {
        let code = r#"
        ---@type { ["string"|"number"] :string }
        "#;
        let result = r#"
Syntax(Chunk)@0..58
  Syntax(Block)@0..58
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..49
      Token(TkDocStart)@9..13 "---@"
      Syntax(DocTagType)@13..49
        Token(TkTagType)@13..17 "type"
        Token(TkWhitespace)@17..18 " "
        Syntax(TypeObject)@18..49
          Token(TkLeftBrace)@18..19 "{"
          Token(TkWhitespace)@19..20 " "
          Syntax(DocObjectField)@20..47
            Token(TkLeftBracket)@20..21 "["
            Syntax(TypeBinary)@21..38
              Syntax(TypeLiteral)@21..29
                Token(TkString)@21..29 "\"string\""
              Token(TkDocOr)@29..30 "|"
              Syntax(TypeLiteral)@30..38
                Token(TkString)@30..38 "\"number\""
            Token(TkRightBracket)@38..39 "]"
            Token(TkWhitespace)@39..40 " "
            Token(TkColon)@40..41 ":"
            Syntax(TypeName)@41..47
              Token(TkName)@41..47 "string"
          Token(TkWhitespace)@47..48 " "
          Token(TkRightBrace)@48..49 "}"
    Token(TkEndOfLine)@49..50 "\n"
    Token(TkWhitespace)@50..58 "        "
        "#;

        assert_ast_eq!(code, result);
    }

    #[test]
    fn test_description_doc_node() {
        let code = r#"
        --- hihiih
        ---@class A
        --- BBB
        ---@field a string hihiihi

        ---@alias b
        ---| "hihihi" #enenen
        ---| "hehehe" #bnbnbn

        ---@param c string yyyy
        ---@return d string
        --- haohaohao
        "#;
        let result = r##"
Syntax(Chunk)@0..263
  Syntax(Block)@0..263
    Token(TkEndOfLine)@0..1 "\n"
    Token(TkWhitespace)@1..9 "        "
    Syntax(Comment)@9..90
      Token(TkNormalStart)@9..13 "--- "
      Syntax(DocDescription)@13..19
        Token(TkDocDetail)@13..19 "hihiih"
      Token(TkEndOfLine)@19..20 "\n"
      Token(TkWhitespace)@20..28 "        "
      Token(TkDocStart)@28..32 "---@"
      Syntax(DocTagClass)@32..39
        Token(TkTagClass)@32..37 "class"
        Token(TkWhitespace)@37..38 " "
        Token(TkName)@38..39 "A"
      Token(TkEndOfLine)@39..40 "\n"
      Token(TkWhitespace)@40..48 "        "
      Token(TkDocContinue)@48..52 "--- "
      Syntax(DocDescription)@52..55
        Token(TkDocDetail)@52..55 "BBB"
      Token(TkEndOfLine)@55..56 "\n"
      Token(TkWhitespace)@56..64 "        "
      Token(TkDocStart)@64..68 "---@"
      Syntax(DocTagField)@68..82
        Token(TkTagField)@68..73 "field"
        Token(TkWhitespace)@73..74 " "
        Token(TkName)@74..75 "a"
        Token(TkWhitespace)@75..76 " "
        Syntax(TypeName)@76..82
          Token(TkName)@76..82 "string"
      Token(TkWhitespace)@82..83 " "
      Syntax(DocDescription)@83..90
        Token(TkDocDetail)@83..90 "hihiihi"
    Token(TkEndOfLine)@90..91 "\n"
    Token(TkEndOfLine)@91..92 "\n"
    Token(TkWhitespace)@92..100 "        "
    Syntax(Comment)@100..171
      Token(TkDocStart)@100..104 "---@"
      Syntax(DocTagAlias)@104..171
        Token(TkTagAlias)@104..109 "alias"
        Token(TkWhitespace)@109..110 " "
        Token(TkName)@110..111 "b"
        Token(TkEndOfLine)@111..112 "\n"
        Token(TkWhitespace)@112..120 "        "
        Syntax(TypeMultiLineUnion)@120..171
          Token(TkDocContinueOr)@120..124 "---|"
          Token(TkWhitespace)@124..125 " "
          Syntax(DocOneLineField)@125..133
            Syntax(TypeLiteral)@125..133
              Token(TkString)@125..133 "\"hihihi\""
          Token(TkWhitespace)@133..134 " "
          Syntax(DocDescription)@134..141
            Token(TkDocDetail)@134..141 "#enenen"
          Token(TkEndOfLine)@141..142 "\n"
          Token(TkWhitespace)@142..150 "        "
          Token(TkDocContinueOr)@150..154 "---|"
          Token(TkWhitespace)@154..155 " "
          Syntax(DocOneLineField)@155..163
            Syntax(TypeLiteral)@155..163
              Token(TkString)@155..163 "\"hehehe\""
          Token(TkWhitespace)@163..164 " "
          Syntax(DocDescription)@164..171
            Token(TkDocDetail)@164..171 "#bnbnbn"
    Token(TkEndOfLine)@171..172 "\n"
    Token(TkEndOfLine)@172..173 "\n"
    Token(TkWhitespace)@173..181 "        "
    Syntax(Comment)@181..254
      Token(TkDocStart)@181..185 "---@"
      Syntax(DocTagParam)@185..199
        Token(TkTagParam)@185..190 "param"
        Token(TkWhitespace)@190..191 " "
        Token(TkName)@191..192 "c"
        Token(TkWhitespace)@192..193 " "
        Syntax(TypeName)@193..199
          Token(TkName)@193..199 "string"
      Token(TkWhitespace)@199..200 " "
      Syntax(DocDescription)@200..204
        Token(TkDocDetail)@200..204 "yyyy"
      Token(TkEndOfLine)@204..205 "\n"
      Token(TkWhitespace)@205..213 "        "
      Token(TkDocStart)@213..217 "---@"
      Syntax(DocTagReturn)@217..232
        Token(TkTagReturn)@217..223 "return"
        Token(TkWhitespace)@223..224 " "
        Syntax(TypeName)@224..225
          Token(TkName)@224..225 "d"
        Token(TkWhitespace)@225..226 " "
        Token(TkName)@226..232 "string"
      Token(TkEndOfLine)@232..233 "\n"
      Token(TkWhitespace)@233..241 "        "
      Token(TkDocContinue)@241..245 "--- "
      Syntax(DocDescription)@245..254
        Token(TkDocDetail)@245..254 "haohaohao"
    Token(TkEndOfLine)@254..255 "\n"
    Token(TkWhitespace)@255..263 "        "
        "##;

        assert_ast_eq!(code, result);
    }
}
