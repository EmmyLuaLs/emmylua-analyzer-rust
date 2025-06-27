mod doc;
mod lua;
mod test;
mod token;

#[allow(unused)]
pub use doc::*;
#[allow(unused)]
pub use lua::*;
#[allow(unused)]
pub use token::*;

use crate::kind::LuaSyntaxKind;

use super::{traits::LuaAstNode, LuaSyntaxNode};

#[allow(unused)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LuaAst {
    LuaChunk(LuaChunk),
    LuaBlock(LuaBlock),
    // stats
    LuaAssignStat(LuaAssignStat),
    LuaLocalStat(LuaLocalStat),
    LuaCallExprStat(LuaCallExprStat),
    LuaLabelStat(LuaLabelStat),
    LuaBreakStat(LuaBreakStat),
    LuaGotoStat(LuaGotoStat),
    LuaDoStat(LuaDoStat),
    LuaWhileStat(LuaWhileStat),
    LuaRepeatStat(LuaRepeatStat),
    LuaIfStat(LuaIfStat),
    LuaForStat(LuaForStat),
    LuaForRangeStat(LuaForRangeStat),
    LuaFuncStat(LuaFuncStat),
    LuaLocalFuncStat(LuaLocalFuncStat),
    LuaReturnStat(LuaReturnStat),

    // exprs
    LuaNameExpr(LuaNameExpr),
    LuaIndexExpr(LuaIndexExpr),
    LuaTableExpr(LuaTableExpr),
    LuaBinaryExpr(LuaBinaryExpr),
    LuaUnaryExpr(LuaUnaryExpr),
    LuaParenExpr(LuaParenExpr),
    LuaCallExpr(LuaCallExpr),
    LuaLiteralExpr(LuaLiteralExpr),
    LuaClosureExpr(LuaClosureExpr),

    // other lua struct
    LuaTableField(LuaTableField),
    LuaParamList(LuaParamList),
    LuaParamName(LuaParamName),
    LuaCallArgList(LuaCallArgList),
    LuaLocalName(LuaLocalName),
    LuaLocalAttribute(LuaLocalAttribute),
    LuaElseIfClauseStat(LuaElseIfClauseStat),
    LuaElseClauseStat(LuaElseClauseStat),

    // comment
    LuaComment(LuaComment),
    // doc tag
    LuaDocTagClass(LuaDocTagClass),
    LuaDocTagEnum(LuaDocTagEnum),
    LuaDocTagAlias(LuaDocTagAlias),
    LuaDocTagType(LuaDocTagType),
    LuaDocTagParam(LuaDocTagParam),
    LuaDocTagReturn(LuaDocTagReturn),
    LuaDocTagOverload(LuaDocTagOverload),
    LuaDocTagField(LuaDocTagField),
    LuaDocTagModule(LuaDocTagModule),
    LuaDocTagSee(LuaDocTagSee),
    LuaDocTagDiagnostic(LuaDocTagDiagnostic),
    LuaDocTagDeprecated(LuaDocTagDeprecated),
    LuaDocTagVersion(LuaDocTagVersion),
    LuaDocTagCast(LuaDocTagCast),
    LuaDocTagSource(LuaDocTagSource),
    LuaDocTagOther(LuaDocTagOther),
    LuaDocTagNamespace(LuaDocTagNamespace),
    LuaDocTagUsing(LuaDocTagUsing),
    LuaDocTagMeta(LuaDocTagMeta),
    LuaDocTagNodiscard(LuaDocTagNodiscard),
    LuaDocTagReadonly(LuaDocTagReadonly),
    LuaDocTagOperator(LuaDocTagOperator),
    LuaDocTagGeneric(LuaDocTagGeneric),
    LuaDocTagAsync(LuaDocTagAsync),
    LuaDocTagAs(LuaDocTagAs),
    LuaDocTagReturnCast(LuaDocTagReturnCast),
    LuaDocTagExport(LuaDocTagExport),

    // doc type
    LuaDocNameType(LuaDocNameType),
    LuaDocArrayType(LuaDocArrayType),
    LuaDocFuncType(LuaDocFuncType),
    LuaDocObjectType(LuaDocObjectType),
    LuaDocBinaryType(LuaDocBinaryType),
    LuaDocUnaryType(LuaDocUnaryType),
    LuaDocConditionalType(LuaDocConditionalType),
    LuaDocTupleType(LuaDocTupleType),
    LuaDocLiteralType(LuaDocLiteralType),
    LuaDocVariadicType(LuaDocVariadicType),
    LuaDocNullableType(LuaDocNullableType),
    LuaDocGenericType(LuaDocGenericType),
    LuaDocStrTplType(LuaDocStrTplType),
    LuaDocMultiLineUnionType(LuaDocMultiLineUnionType),
    // other structure do not need enum here
}

impl LuaAstNode for LuaAst {
    fn syntax(&self) -> &LuaSyntaxNode {
        match self {
            LuaAst::LuaChunk(node) => node.syntax(),
            LuaAst::LuaBlock(node) => node.syntax(),
            LuaAst::LuaAssignStat(node) => node.syntax(),
            LuaAst::LuaLocalStat(node) => node.syntax(),
            LuaAst::LuaCallExprStat(node) => node.syntax(),
            LuaAst::LuaLabelStat(node) => node.syntax(),
            LuaAst::LuaBreakStat(node) => node.syntax(),
            LuaAst::LuaGotoStat(node) => node.syntax(),
            LuaAst::LuaDoStat(node) => node.syntax(),
            LuaAst::LuaWhileStat(node) => node.syntax(),
            LuaAst::LuaRepeatStat(node) => node.syntax(),
            LuaAst::LuaIfStat(node) => node.syntax(),
            LuaAst::LuaForStat(node) => node.syntax(),
            LuaAst::LuaForRangeStat(node) => node.syntax(),
            LuaAst::LuaFuncStat(node) => node.syntax(),
            LuaAst::LuaLocalFuncStat(node) => node.syntax(),
            LuaAst::LuaReturnStat(node) => node.syntax(),
            LuaAst::LuaNameExpr(node) => node.syntax(),
            LuaAst::LuaIndexExpr(node) => node.syntax(),
            LuaAst::LuaTableExpr(node) => node.syntax(),
            LuaAst::LuaBinaryExpr(node) => node.syntax(),
            LuaAst::LuaUnaryExpr(node) => node.syntax(),
            LuaAst::LuaParenExpr(node) => node.syntax(),
            LuaAst::LuaCallExpr(node) => node.syntax(),
            LuaAst::LuaLiteralExpr(node) => node.syntax(),
            LuaAst::LuaClosureExpr(node) => node.syntax(),
            LuaAst::LuaComment(node) => node.syntax(),
            LuaAst::LuaTableField(node) => node.syntax(),
            LuaAst::LuaParamList(node) => node.syntax(),
            LuaAst::LuaParamName(node) => node.syntax(),
            LuaAst::LuaCallArgList(node) => node.syntax(),
            LuaAst::LuaLocalName(node) => node.syntax(),
            LuaAst::LuaLocalAttribute(node) => node.syntax(),
            LuaAst::LuaElseIfClauseStat(node) => node.syntax(),
            LuaAst::LuaElseClauseStat(node) => node.syntax(),
            LuaAst::LuaDocTagClass(node) => node.syntax(),
            LuaAst::LuaDocTagEnum(node) => node.syntax(),
            LuaAst::LuaDocTagAlias(node) => node.syntax(),
            LuaAst::LuaDocTagType(node) => node.syntax(),
            LuaAst::LuaDocTagParam(node) => node.syntax(),
            LuaAst::LuaDocTagReturn(node) => node.syntax(),
            LuaAst::LuaDocTagOverload(node) => node.syntax(),
            LuaAst::LuaDocTagField(node) => node.syntax(),
            LuaAst::LuaDocTagModule(node) => node.syntax(),
            LuaAst::LuaDocTagSee(node) => node.syntax(),
            LuaAst::LuaDocTagDiagnostic(node) => node.syntax(),
            LuaAst::LuaDocTagDeprecated(node) => node.syntax(),
            LuaAst::LuaDocTagVersion(node) => node.syntax(),
            LuaAst::LuaDocTagCast(node) => node.syntax(),
            LuaAst::LuaDocTagSource(node) => node.syntax(),
            LuaAst::LuaDocTagOther(node) => node.syntax(),
            LuaAst::LuaDocTagNamespace(node) => node.syntax(),
            LuaAst::LuaDocTagUsing(node) => node.syntax(),
            LuaAst::LuaDocTagMeta(node) => node.syntax(),
            LuaAst::LuaDocTagNodiscard(node) => node.syntax(),
            LuaAst::LuaDocTagReadonly(node) => node.syntax(),
            LuaAst::LuaDocTagOperator(node) => node.syntax(),
            LuaAst::LuaDocTagGeneric(node) => node.syntax(),
            LuaAst::LuaDocTagAsync(node) => node.syntax(),
            LuaAst::LuaDocTagAs(node) => node.syntax(),
            LuaAst::LuaDocTagReturnCast(node) => node.syntax(),
            LuaAst::LuaDocTagExport(node) => node.syntax(),
            LuaAst::LuaDocNameType(node) => node.syntax(),
            LuaAst::LuaDocArrayType(node) => node.syntax(),
            LuaAst::LuaDocFuncType(node) => node.syntax(),
            LuaAst::LuaDocObjectType(node) => node.syntax(),
            LuaAst::LuaDocBinaryType(node) => node.syntax(),
            LuaAst::LuaDocUnaryType(node) => node.syntax(),
            LuaAst::LuaDocConditionalType(node) => node.syntax(),
            LuaAst::LuaDocTupleType(node) => node.syntax(),
            LuaAst::LuaDocLiteralType(node) => node.syntax(),
            LuaAst::LuaDocVariadicType(node) => node.syntax(),
            LuaAst::LuaDocNullableType(node) => node.syntax(),
            LuaAst::LuaDocGenericType(node) => node.syntax(),
            LuaAst::LuaDocStrTplType(node) => node.syntax(),
            LuaAst::LuaDocMultiLineUnionType(node) => node.syntax(),
        }
    }

    fn can_cast(kind: LuaSyntaxKind) -> bool
    where
        Self: Sized,
    {
        match kind {
            LuaSyntaxKind::Chunk => true,
            LuaSyntaxKind::Block => true,
            LuaSyntaxKind::AssignStat => true,
            LuaSyntaxKind::LocalStat => true,
            LuaSyntaxKind::CallExprStat => true,
            LuaSyntaxKind::LabelStat => true,
            LuaSyntaxKind::BreakStat => true,
            LuaSyntaxKind::GotoStat => true,
            LuaSyntaxKind::DoStat => true,
            LuaSyntaxKind::WhileStat => true,
            LuaSyntaxKind::RepeatStat => true,
            LuaSyntaxKind::IfStat => true,
            LuaSyntaxKind::ForStat => true,
            LuaSyntaxKind::ForRangeStat => true,
            LuaSyntaxKind::FuncStat => true,
            LuaSyntaxKind::LocalFuncStat => true,
            LuaSyntaxKind::ReturnStat => true,
            LuaSyntaxKind::NameExpr => true,
            LuaSyntaxKind::IndexExpr => true,
            LuaSyntaxKind::TableEmptyExpr
            | LuaSyntaxKind::TableArrayExpr
            | LuaSyntaxKind::TableObjectExpr => true,
            LuaSyntaxKind::BinaryExpr => true,
            LuaSyntaxKind::UnaryExpr => true,
            LuaSyntaxKind::ParenExpr => true,
            LuaSyntaxKind::CallExpr
            | LuaSyntaxKind::AssertCallExpr
            | LuaSyntaxKind::ErrorCallExpr
            | LuaSyntaxKind::RequireCallExpr
            | LuaSyntaxKind::TypeCallExpr
            | LuaSyntaxKind::SetmetatableCallExpr => true,
            LuaSyntaxKind::LiteralExpr => true,
            LuaSyntaxKind::ClosureExpr => true,
            LuaSyntaxKind::ParamList => true,
            LuaSyntaxKind::CallArgList => true,
            LuaSyntaxKind::LocalName => true,
            LuaSyntaxKind::TableFieldAssign | LuaSyntaxKind::TableFieldValue => true,
            LuaSyntaxKind::ParamName => true,
            LuaSyntaxKind::Attribute => true,
            LuaSyntaxKind::ElseIfClauseStat => true,
            LuaSyntaxKind::ElseClauseStat => true,
            LuaSyntaxKind::Comment => true,
            LuaSyntaxKind::DocTagClass => true,
            LuaSyntaxKind::DocTagEnum => true,
            LuaSyntaxKind::DocTagAlias => true,
            LuaSyntaxKind::DocTagType => true,
            LuaSyntaxKind::DocTagParam => true,
            LuaSyntaxKind::DocTagReturn => true,
            LuaSyntaxKind::DocTagOverload => true,
            LuaSyntaxKind::DocTagField => true,
            LuaSyntaxKind::DocTagModule => true,
            LuaSyntaxKind::DocTagSee => true,
            LuaSyntaxKind::DocTagDiagnostic => true,
            LuaSyntaxKind::DocTagDeprecated => true,
            LuaSyntaxKind::DocTagVersion => true,
            LuaSyntaxKind::DocTagCast => true,
            LuaSyntaxKind::DocTagSource => true,
            LuaSyntaxKind::DocTagOther => true,
            LuaSyntaxKind::DocTagNamespace => true,
            LuaSyntaxKind::DocTagUsing => true,
            LuaSyntaxKind::DocTagMeta => true,
            LuaSyntaxKind::DocTagNodiscard => true,
            LuaSyntaxKind::DocTagReadonly => true,
            LuaSyntaxKind::DocTagOperator => true,
            LuaSyntaxKind::DocTagGeneric => true,
            LuaSyntaxKind::DocTagAsync => true,
            LuaSyntaxKind::DocTagAs => true,
            LuaSyntaxKind::DocTagReturnCast => true,
            LuaSyntaxKind::DocTagExport => true,
            LuaSyntaxKind::TypeName => true,
            LuaSyntaxKind::TypeArray => true,
            LuaSyntaxKind::TypeFun => true,
            LuaSyntaxKind::TypeObject => true,
            LuaSyntaxKind::TypeBinary => true,
            LuaSyntaxKind::TypeUnary => true,
            LuaSyntaxKind::TypeConditional => true,
            LuaSyntaxKind::TypeTuple => true,
            LuaSyntaxKind::TypeLiteral => true,
            LuaSyntaxKind::TypeVariadic => true,
            LuaSyntaxKind::TypeNullable => true,
            LuaSyntaxKind::TypeGeneric => true,
            LuaSyntaxKind::TypeStringTemplate => true,
            LuaSyntaxKind::TypeMultiLineUnion => true,
            _ => false,
        }
    }

    fn cast(syntax: LuaSyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        match syntax.kind().into() {
            LuaSyntaxKind::Chunk => LuaChunk::cast(syntax).map(LuaAst::LuaChunk),
            LuaSyntaxKind::Block => LuaBlock::cast(syntax).map(LuaAst::LuaBlock),
            LuaSyntaxKind::AssignStat => LuaAssignStat::cast(syntax).map(LuaAst::LuaAssignStat),
            LuaSyntaxKind::LocalStat => LuaLocalStat::cast(syntax).map(LuaAst::LuaLocalStat),
            LuaSyntaxKind::CallExprStat => {
                LuaCallExprStat::cast(syntax).map(LuaAst::LuaCallExprStat)
            }
            LuaSyntaxKind::LabelStat => LuaLabelStat::cast(syntax).map(LuaAst::LuaLabelStat),
            LuaSyntaxKind::BreakStat => LuaBreakStat::cast(syntax).map(LuaAst::LuaBreakStat),
            LuaSyntaxKind::GotoStat => LuaGotoStat::cast(syntax).map(LuaAst::LuaGotoStat),
            LuaSyntaxKind::DoStat => LuaDoStat::cast(syntax).map(LuaAst::LuaDoStat),
            LuaSyntaxKind::WhileStat => LuaWhileStat::cast(syntax).map(LuaAst::LuaWhileStat),
            LuaSyntaxKind::RepeatStat => LuaRepeatStat::cast(syntax).map(LuaAst::LuaRepeatStat),
            LuaSyntaxKind::IfStat => LuaIfStat::cast(syntax).map(LuaAst::LuaIfStat),
            LuaSyntaxKind::ForStat => LuaForStat::cast(syntax).map(LuaAst::LuaForStat),
            LuaSyntaxKind::ForRangeStat => {
                LuaForRangeStat::cast(syntax).map(LuaAst::LuaForRangeStat)
            }
            LuaSyntaxKind::FuncStat => LuaFuncStat::cast(syntax).map(LuaAst::LuaFuncStat),
            LuaSyntaxKind::LocalFuncStat => {
                LuaLocalFuncStat::cast(syntax).map(LuaAst::LuaLocalFuncStat)
            }
            LuaSyntaxKind::ReturnStat => LuaReturnStat::cast(syntax).map(LuaAst::LuaReturnStat),
            LuaSyntaxKind::NameExpr => LuaNameExpr::cast(syntax).map(LuaAst::LuaNameExpr),
            LuaSyntaxKind::IndexExpr => LuaIndexExpr::cast(syntax).map(LuaAst::LuaIndexExpr),
            LuaSyntaxKind::TableEmptyExpr
            | LuaSyntaxKind::TableArrayExpr
            | LuaSyntaxKind::TableObjectExpr => {
                LuaTableExpr::cast(syntax).map(LuaAst::LuaTableExpr)
            }
            LuaSyntaxKind::BinaryExpr => LuaBinaryExpr::cast(syntax).map(LuaAst::LuaBinaryExpr),
            LuaSyntaxKind::UnaryExpr => LuaUnaryExpr::cast(syntax).map(LuaAst::LuaUnaryExpr),
            LuaSyntaxKind::ParenExpr => LuaParenExpr::cast(syntax).map(LuaAst::LuaParenExpr),
            LuaSyntaxKind::CallExpr
            | LuaSyntaxKind::AssertCallExpr
            | LuaSyntaxKind::ErrorCallExpr
            | LuaSyntaxKind::RequireCallExpr
            | LuaSyntaxKind::TypeCallExpr
            | LuaSyntaxKind::SetmetatableCallExpr => {
                LuaCallExpr::cast(syntax).map(LuaAst::LuaCallExpr)
            }
            LuaSyntaxKind::LiteralExpr => LuaLiteralExpr::cast(syntax).map(LuaAst::LuaLiteralExpr),
            LuaSyntaxKind::ClosureExpr => LuaClosureExpr::cast(syntax).map(LuaAst::LuaClosureExpr),
            LuaSyntaxKind::Comment => LuaComment::cast(syntax).map(LuaAst::LuaComment),
            LuaSyntaxKind::TableFieldAssign | LuaSyntaxKind::TableFieldValue => {
                LuaTableField::cast(syntax).map(LuaAst::LuaTableField)
            }
            LuaSyntaxKind::ParamList => LuaParamList::cast(syntax).map(LuaAst::LuaParamList),
            LuaSyntaxKind::ParamName => LuaParamName::cast(syntax).map(LuaAst::LuaParamName),
            LuaSyntaxKind::CallArgList => LuaCallArgList::cast(syntax).map(LuaAst::LuaCallArgList),
            LuaSyntaxKind::LocalName => LuaLocalName::cast(syntax).map(LuaAst::LuaLocalName),
            LuaSyntaxKind::Attribute => {
                LuaLocalAttribute::cast(syntax).map(LuaAst::LuaLocalAttribute)
            }
            LuaSyntaxKind::ElseIfClauseStat => {
                LuaElseIfClauseStat::cast(syntax).map(LuaAst::LuaElseIfClauseStat)
            }
            LuaSyntaxKind::ElseClauseStat => {
                LuaElseClauseStat::cast(syntax).map(LuaAst::LuaElseClauseStat)
            }
            LuaSyntaxKind::DocTagClass => LuaDocTagClass::cast(syntax).map(LuaAst::LuaDocTagClass),
            LuaSyntaxKind::DocTagEnum => LuaDocTagEnum::cast(syntax).map(LuaAst::LuaDocTagEnum),
            LuaSyntaxKind::DocTagAlias => LuaDocTagAlias::cast(syntax).map(LuaAst::LuaDocTagAlias),
            LuaSyntaxKind::DocTagType => LuaDocTagType::cast(syntax).map(LuaAst::LuaDocTagType),
            LuaSyntaxKind::DocTagParam => LuaDocTagParam::cast(syntax).map(LuaAst::LuaDocTagParam),
            LuaSyntaxKind::DocTagReturn => {
                LuaDocTagReturn::cast(syntax).map(LuaAst::LuaDocTagReturn)
            }
            LuaSyntaxKind::DocTagOverload => {
                LuaDocTagOverload::cast(syntax).map(LuaAst::LuaDocTagOverload)
            }
            LuaSyntaxKind::DocTagField => LuaDocTagField::cast(syntax).map(LuaAst::LuaDocTagField),
            LuaSyntaxKind::DocTagModule => {
                LuaDocTagModule::cast(syntax).map(LuaAst::LuaDocTagModule)
            }
            LuaSyntaxKind::DocTagSee => LuaDocTagSee::cast(syntax).map(LuaAst::LuaDocTagSee),
            LuaSyntaxKind::DocTagDiagnostic => {
                LuaDocTagDiagnostic::cast(syntax).map(LuaAst::LuaDocTagDiagnostic)
            }
            LuaSyntaxKind::DocTagDeprecated => {
                LuaDocTagDeprecated::cast(syntax).map(LuaAst::LuaDocTagDeprecated)
            }
            LuaSyntaxKind::DocTagVersion => {
                LuaDocTagVersion::cast(syntax).map(LuaAst::LuaDocTagVersion)
            }
            LuaSyntaxKind::DocTagCast => LuaDocTagCast::cast(syntax).map(LuaAst::LuaDocTagCast),
            LuaSyntaxKind::DocTagSource => {
                LuaDocTagSource::cast(syntax).map(LuaAst::LuaDocTagSource)
            }
            LuaSyntaxKind::DocTagOther => LuaDocTagOther::cast(syntax).map(LuaAst::LuaDocTagOther),
            LuaSyntaxKind::DocTagNamespace => {
                LuaDocTagNamespace::cast(syntax).map(LuaAst::LuaDocTagNamespace)
            }
            LuaSyntaxKind::DocTagUsing => LuaDocTagUsing::cast(syntax).map(LuaAst::LuaDocTagUsing),
            LuaSyntaxKind::DocTagMeta => LuaDocTagMeta::cast(syntax).map(LuaAst::LuaDocTagMeta),
            LuaSyntaxKind::DocTagNodiscard => {
                LuaDocTagNodiscard::cast(syntax).map(LuaAst::LuaDocTagNodiscard)
            }
            LuaSyntaxKind::DocTagReadonly => {
                LuaDocTagReadonly::cast(syntax).map(LuaAst::LuaDocTagReadonly)
            }
            LuaSyntaxKind::DocTagOperator => {
                LuaDocTagOperator::cast(syntax).map(LuaAst::LuaDocTagOperator)
            }
            LuaSyntaxKind::DocTagGeneric => {
                LuaDocTagGeneric::cast(syntax).map(LuaAst::LuaDocTagGeneric)
            }
            LuaSyntaxKind::DocTagAsync => LuaDocTagAsync::cast(syntax).map(LuaAst::LuaDocTagAsync),
            LuaSyntaxKind::DocTagAs => LuaDocTagAs::cast(syntax).map(LuaAst::LuaDocTagAs),
            LuaSyntaxKind::DocTagReturnCast => {
                LuaDocTagReturnCast::cast(syntax).map(LuaAst::LuaDocTagReturnCast)
            }
            LuaSyntaxKind::DocTagExport => {
                LuaDocTagExport::cast(syntax).map(LuaAst::LuaDocTagExport)
            }
            LuaSyntaxKind::TypeName => LuaDocNameType::cast(syntax).map(LuaAst::LuaDocNameType),
            LuaSyntaxKind::TypeArray => LuaDocArrayType::cast(syntax).map(LuaAst::LuaDocArrayType),
            LuaSyntaxKind::TypeFun => LuaDocFuncType::cast(syntax).map(LuaAst::LuaDocFuncType),
            LuaSyntaxKind::TypeObject => {
                LuaDocObjectType::cast(syntax).map(LuaAst::LuaDocObjectType)
            }
            LuaSyntaxKind::TypeBinary => {
                LuaDocBinaryType::cast(syntax).map(LuaAst::LuaDocBinaryType)
            }
            LuaSyntaxKind::TypeUnary => LuaDocUnaryType::cast(syntax).map(LuaAst::LuaDocUnaryType),
            LuaSyntaxKind::TypeConditional => {
                LuaDocConditionalType::cast(syntax).map(LuaAst::LuaDocConditionalType)
            }
            LuaSyntaxKind::TypeTuple => LuaDocTupleType::cast(syntax).map(LuaAst::LuaDocTupleType),
            LuaSyntaxKind::TypeLiteral => {
                LuaDocLiteralType::cast(syntax).map(LuaAst::LuaDocLiteralType)
            }
            LuaSyntaxKind::TypeVariadic => {
                LuaDocVariadicType::cast(syntax).map(LuaAst::LuaDocVariadicType)
            }
            LuaSyntaxKind::TypeNullable => {
                LuaDocNullableType::cast(syntax).map(LuaAst::LuaDocNullableType)
            }
            LuaSyntaxKind::TypeGeneric => {
                LuaDocGenericType::cast(syntax).map(LuaAst::LuaDocGenericType)
            }
            LuaSyntaxKind::TypeStringTemplate => {
                LuaDocStrTplType::cast(syntax).map(LuaAst::LuaDocStrTplType)
            }
            LuaSyntaxKind::TypeMultiLineUnion => {
                LuaDocMultiLineUnionType::cast(syntax).map(LuaAst::LuaDocMultiLineUnionType)
            }
            _ => None,
        }
    }
}
