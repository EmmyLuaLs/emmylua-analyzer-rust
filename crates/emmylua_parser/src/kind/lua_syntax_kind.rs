#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum LuaSyntaxKind {
    None,
    // source
    Chunk,

    // block
    Block,

    // statements
    EmptyStat,
    LocalStat,
    LocalFuncStat,
    IfStat,
    ElseIfClauseStat,
    ElseClauseStat,
    WhileStat,
    DoStat,
    ForStat,
    ForRangeStat,
    RepeatStat,
    FuncStat,
    LabelStat,
    BreakStat,
    ReturnStat,
    GotoStat,
    CallExprStat,
    AssignStat,
    UnknownStat,

    // expressions
    ParenExpr,
    LiteralExpr,
    ClosureExpr,
    UnaryExpr,
    BinaryExpr,
    TableArrayExpr,  // { a, b, c}
    TableObjectExpr, // { a = 1, b = 2, c = 3}
    TableEmptyExpr,  // {}
    CallExpr,
    IndexExpr,
    NameExpr,

    // other
    LocalName,
    ParamName,
    ParamList,
    CallArgList,
    TableFieldAssign,
    TableFieldValue,
    Attribute,

    // comment
    Comment,

    // doc tag
    DocTagClass,
    DocTagEnum,
    DocTagInterface,
    DocTagAlias,
    DocTagField,
    DocTagType,
    DocTagParam,
    DocTagReturn,
    DocTagGeneric,
    DocTagSee,
    DocTagDeprecated,
    DocTagCast,
    DocTagOverload,
    DocTagAsync,
    DocTagVisibility,
    DocTagMeta,
    DocTagOther,
    DocTagDiagnostic,
    DocTagVersion,
    DocTagAs,
    DocTagNodiscard,
    DocTagOperator,
    DocTagModule,
    DocTagMapping,
    DocTagNamespace,
    DocTagUsing,
    DocTagSource,
    DocTagReadonly,

    // doc Type
    TypeArray,          // baseType []
    TypeUnary,          // keyof type
    TypeBinary,         // aType | bType, aType & bType, aType extends bType, aType in bType
    TypeConditional,    // <conditionType> and <trueType> or <falseType>
    TypeFun,            // fun(<paramList>): returnType
    TypeGeneric,        // name<typeList>
    TypeTuple,          // [typeList]
    TypeObject, // { a: aType, b: bType } or { [1]: aType, [2]: bType } or { a: aType, b: bType, [number]: string }
    TypeLiteral, // "string" or <integer> or true or false
    TypeName,   // name
    TypeVariadic, // type...
    TypeNullable, // <Type>?
    TypeStringTemplate, // prefixName.`T`
    TypeMultiLineUnion, // | simple type # description

    // follow donot support now
    TypeMatch,
    TypeIndexAccess, // type[keyType]
    TypeMapped,      // { [p in KeyType]+? : ValueType }

    // doc other
    DocObjectField,
    DocContinueOrField,
    // doc parameter
    DocTypedParameter,
    DocGenericParameter,
    DocGenericDeclareList,
    DocDiagnosticNameList,
    DocTypeList,
    DocAttribute,
    DocOpType,             // +<type>, -<type>, +?
    DocMappedKeys,         // [p in KeyType]?
    DocEnumFieldList,      // ---| <EnumField>
    DocEnumField, // <string> # description or <integer> # description or <name> # description
    DocOneLineField, // <type> # description
    DocDiagnosticCodeList, // unused-local, undefined-global ...
    // start with '#' or '@'
    DocDescription,

    // [<|>] [<framework>] <version>, <version> can be '5.1', '5.2', '5.3', '5.4', 'JIT', <framework> can be 'openresty'
    DocVersion,
}
