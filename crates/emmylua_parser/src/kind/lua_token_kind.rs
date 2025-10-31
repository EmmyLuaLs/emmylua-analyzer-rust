use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum LuaTokenKind {
    None,
    // KeyWord
    TkAnd,
    TkBreak,
    TkDo,
    TkElse,
    TkElseIf,
    TkEnd,
    TkFalse,
    TkFor,
    TkFunction,
    TkGoto,
    TkIf,
    TkIn,
    TkLocal,
    TkNil,
    TkNot,
    TkOr,
    TkRepeat,
    TkReturn,
    TkThen,
    TkTrue,
    TkUntil,
    TkWhile,
    TkGlobal, // global *

    TkWhitespace, // whitespace
    TkEndOfLine,  // end of line
    TkPlus,       // +
    TkMinus,      // -
    TkMul,        // *
    TkDiv,        // /
    TkIDiv,       // //
    TkDot,        // .
    TkConcat,     // ..
    TkDots,       // ...
    TkComma,      // ,
    TkAssign,     // =
    TkEq,         // ==
    TkGe,         // >=
    TkLe,         // <=
    TkNe,         // ~=
    TkShl,        // <<
    TkShr,        // >>
    TkLt,         // <
    TkGt,         // >
    TkMod,        // %
    TkPow,        // ^
    TkLen,        // #
    TkBitAnd,     // &
    TkBitOr,      // |
    TkBitXor,     // ~
    TkColon,      // :
    TkDbColon,    // ::
    TkSemicolon,  // ;

    // Non-standard assignment operators
    TkPlusAssign,        // +=
    TkMinusAssign,       // -=
    TkStarAssign,        // *=
    TkSlashAssign,       // /=
    TkPercentAssign,     // %=
    TkCaretAssign,       // ^=
    TkDoubleSlashAssign, // //=
    TkPipeAssign,        // |=
    TkAmpAssign,         // &=
    TkShiftLeftAssign,   // <<=
    TkShiftRightAssign,  // >>=

    TkLeftBracket,  // [
    TkRightBracket, // ]
    TkLeftParen,    // (
    TkRightParen,   // )
    TkLeftBrace,    // {
    TkRightBrace,   // }
    TkComplex,      // complex
    TkInt,          // int
    TkFloat,        // float

    TkName,         // name
    TkString,       // string
    TkLongString,   // long string
    TkShortComment, // short comment
    TkLongComment,  // long comment
    TkShebang,      // shebang
    TkEof,          // eof

    TkUnknown, // unknown

    // doc
    TkNormalStart,      // -- or ---
    TkLongCommentStart, // --[[
    TkDocLongStart,     // --[[@
    TkDocStart,         // ---@
    TKDocTriviaStart,   // --------------
    TkDocTrivia,        // other can not parsed
    TkLongCommentEnd,   // ]] or ]===]
    TKNonStdComment,    // // comment, non-standard lua comment

    // tag
    TkTagClass,     // class
    TkTagEnum,      // enum
    TkTagInterface, // interface
    TkTagAlias,     // alias
    TkTagModule,    // module

    TkTagField,      // field
    TkTagType,       // type
    TkTagParam,      // param
    TkTagReturn,     // return
    TkTagOverload,   // overload
    TkTagGeneric,    // generic
    TkTagSee,        // see
    TkTagDeprecated, // deprecated
    TkTagAsync,      // async
    TkTagCast,       // cast
    TkTagOther,      // other
    TkTagVisibility, // public private protected package
    TkTagReadonly,   // readonly
    TkTagDiagnostic, // diagnostic
    TkTagMeta,       // meta
    TkTagVersion,    // version
    TkTagAs,         // as
    TkTagNodiscard,  // nodiscard
    TkTagOperator,   // operator
    TkTagMapping,    // mapping
    TkTagNamespace,  // namespace
    TkTagUsing,      // using
    TkTagSource,     // source
    TkTagReturnCast, // return cast
    TkTagExport,     // export
    TkLanguage,      // language
    TkTagAttribute,  // attribute
    TkCallGeneric,   // call generic. function_name--[[@<type>]](...)

    TkDocOr,              // |
    TkDocAnd,             // &
    TkDocKeyOf,           // keyof
    TkDocExtends,         // extends
    TkDocNew,             // new
    TkDocAs,              // as
    TkDocIn,              // in
    TkDocInfer,           // infer
    TkDocElse,            // else (for return_cast)
    TkDocContinue,        // ---
    TkDocContinueOr,      // ---| or ---|+  or ---|>
    TkDocDetail,          // a description
    TkDocQuestion,        // '?'
    TkDocVisibility,      // public private protected package
    TkDocReadonly,        // readonly
    TkAt,                 // '@', invalid lua token, but for postfix completion
    TkDocVersionNumber,   // version number
    TkStringTemplateType, // type template
    TkDocMatch,           // =
    TKDocPath,            // path
    TkDocRegion,          // region
    TkDocEndRegion,       // endregion
    TkDocSeeContent,      // see content
    TkDocAttributeUse,    // '@[', used for attribute usage
}

impl fmt::Display for LuaTokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl LuaTokenKind {
    pub fn is_keyword(self) -> bool {
        matches!(
            self,
            LuaTokenKind::TkAnd
                | LuaTokenKind::TkBreak
                | LuaTokenKind::TkDo
                | LuaTokenKind::TkElse
                | LuaTokenKind::TkElseIf
                | LuaTokenKind::TkEnd
                | LuaTokenKind::TkFalse
                | LuaTokenKind::TkFor
                | LuaTokenKind::TkFunction
                | LuaTokenKind::TkGoto
                | LuaTokenKind::TkIf
                | LuaTokenKind::TkIn
                | LuaTokenKind::TkLocal
                | LuaTokenKind::TkNil
                | LuaTokenKind::TkNot
                | LuaTokenKind::TkOr
                | LuaTokenKind::TkRepeat
                | LuaTokenKind::TkReturn
                | LuaTokenKind::TkThen
                | LuaTokenKind::TkTrue
                | LuaTokenKind::TkUntil
                | LuaTokenKind::TkWhile
        )
    }

    pub fn is_assign_op(self) -> bool {
        matches!(
            self,
            LuaTokenKind::TkAssign
                | LuaTokenKind::TkPlusAssign
                | LuaTokenKind::TkMinusAssign
                | LuaTokenKind::TkStarAssign
                | LuaTokenKind::TkSlashAssign
                | LuaTokenKind::TkPercentAssign
                | LuaTokenKind::TkCaretAssign
                | LuaTokenKind::TkDoubleSlashAssign
                | LuaTokenKind::TkPipeAssign
                | LuaTokenKind::TkAmpAssign
                | LuaTokenKind::TkShiftLeftAssign
                | LuaTokenKind::TkShiftRightAssign
        )
    }
}
