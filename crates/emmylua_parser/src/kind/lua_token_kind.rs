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

    TkWhitespace,   // whitespace
    TkEndOfLine,    // end of line
    TkPlus,         // +
    TkMinus,        // -
    TkMul,          // *
    TkDiv,          // /
    TkIDiv,         // //
    TkDot,          // .
    TkConcat,       // ..
    TkDots,         // ...
    TkComma,        // ,
    TkAssign,       // =
    TkEq,           // ==
    TkGe,           // >=
    TkLe,           // <=
    TkNe,           // ~=
    TkShl,          // <<
    TkShr,          // >>
    TkLt,           // <
    TkGt,           // >
    TkMod,          // %
    TkPow,          // ^
    TkLen,          // #
    TkBitAnd,       // &
    TkBitOr,        // |
    TkBitXor,       // ~
    TkColon,        // :
    TkDbColon,      // ::
    TkSemicolon,    // ;
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

    TkDocOr,              // |
    TkDocAnd,             // &
    TkDocKeyOf,           // keyof
    TkDocExtends,         // extends
    TkDocAs,              // as
    TkDocIn,              // in
    TkDocInfer,           // infer
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
}

impl fmt::Display for LuaTokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
