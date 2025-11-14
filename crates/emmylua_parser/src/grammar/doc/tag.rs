use crate::{
    grammar::DocParseResult,
    kind::{LuaSyntaxKind, LuaTokenKind},
    lexer::LuaDocLexerState,
    parser::{CompleteMarker, LuaDocParser, MarkerEventContainer},
    parser_error::LuaParseError,
};

use super::{
    expect_token, if_token_bump, parse_description,
    types::{parse_fun_type, parse_type, parse_type_list, parse_typed_param},
};

pub fn parse_tag(p: &mut LuaDocParser) {
    let level = p.get_mark_level();
    match parse_tag_detail(p) {
        Ok(_) => {}
        Err(error) => {
            p.push_error(error);
            let current_level = p.get_mark_level();
            for _ in 0..(current_level - level) {
                p.push_node_end();
            }
        }
    }
}

pub fn parse_long_tag(p: &mut LuaDocParser) {
    parse_tag(p);
}

fn parse_tag_detail(p: &mut LuaDocParser) -> DocParseResult {
    match p.current_token() {
        // main tag
        LuaTokenKind::TkTagClass | LuaTokenKind::TkTagInterface => parse_tag_class(p),
        LuaTokenKind::TkTagEnum => parse_tag_enum(p),
        LuaTokenKind::TkTagAlias => parse_tag_alias(p),
        LuaTokenKind::TkTagField => parse_tag_field(p),
        LuaTokenKind::TkTagType => parse_tag_type(p),
        LuaTokenKind::TkTagParam => parse_tag_param(p),
        LuaTokenKind::TkTagReturn => parse_tag_return(p),
        LuaTokenKind::TkTagReturnCast => parse_tag_return_cast(p),
        // other tag
        LuaTokenKind::TkTagModule => parse_tag_module(p),
        LuaTokenKind::TkTagSee => parse_tag_see(p),
        LuaTokenKind::TkTagGeneric => parse_tag_generic(p),
        LuaTokenKind::TkTagAs => parse_tag_as(p),
        LuaTokenKind::TkTagOverload => parse_tag_overload(p),
        LuaTokenKind::TkTagCast => parse_tag_cast(p),
        LuaTokenKind::TkTagSource => parse_tag_source(p),
        LuaTokenKind::TkTagDiagnostic => parse_tag_diagnostic(p),
        LuaTokenKind::TkTagVersion => parse_tag_version(p),
        LuaTokenKind::TkTagOperator => parse_tag_operator(p),
        LuaTokenKind::TkTagMapping => parse_tag_mapping(p),
        LuaTokenKind::TkTagNamespace => parse_tag_namespace(p),
        LuaTokenKind::TkTagUsing => parse_tag_using(p),
        LuaTokenKind::TkTagMeta => parse_tag_meta(p),
        LuaTokenKind::TkTagExport => parse_tag_export(p),
        LuaTokenKind::TkLanguage => parse_tag_language(p),
        LuaTokenKind::TkTagAttribute => parse_tag_attribute(p),
        LuaTokenKind::TkDocAttributeUse => parse_tag_attribute_use(p, true),
        LuaTokenKind::TkCallGeneric => parse_tag_call_generic(p),

        // simple tag
        LuaTokenKind::TkTagVisibility => parse_tag_simple(p, LuaSyntaxKind::DocTagVisibility),
        LuaTokenKind::TkTagReadonly => parse_tag_simple(p, LuaSyntaxKind::DocTagReadonly),
        LuaTokenKind::TkTagDeprecated => parse_tag_simple(p, LuaSyntaxKind::DocTagDeprecated),
        LuaTokenKind::TkTagAsync => parse_tag_simple(p, LuaSyntaxKind::DocTagAsync),
        LuaTokenKind::TkTagNodiscard => parse_tag_simple(p, LuaSyntaxKind::DocTagNodiscard),
        LuaTokenKind::TkTagOther => parse_tag_simple(p, LuaSyntaxKind::DocTagOther),
        _ => Ok(CompleteMarker::empty()),
    }
}

fn parse_tag_simple(p: &mut LuaDocParser, kind: LuaSyntaxKind) -> DocParseResult {
    let m = p.mark(kind);
    p.bump();
    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);

    Ok(m.complete(p))
}

// ---@class <class name>
fn parse_tag_class(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagClass);
    p.bump();
    if p.current_token() == LuaTokenKind::TkLeftParen {
        parse_doc_type_flag(p)?;
    }

    expect_token(p, LuaTokenKind::TkName)?;
    // TODO suffixed
    if p.current_token() == LuaTokenKind::TkLt {
        parse_generic_decl_list(p, true)?;
    }

    if p.current_token() == LuaTokenKind::TkColon {
        p.bump();
        parse_type_list(p)?;
    }

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// (partial, global, local)
fn parse_doc_type_flag(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocTypeFlag);
    p.bump();
    expect_token(p, LuaTokenKind::TkName)?;
    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        expect_token(p, LuaTokenKind::TkName)?;
    }

    expect_token(p, LuaTokenKind::TkRightParen)?;
    Ok(m.complete(p))
}

// <T, R, C: AAA>
pub(super) fn parse_generic_decl_list(
    p: &mut LuaDocParser,
    allow_angle_brackets: bool,
) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocGenericDeclareList);
    if allow_angle_brackets {
        expect_token(p, LuaTokenKind::TkLt)?;
    }
    parse_generic_param(p)?;
    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        parse_generic_param(p)?;
    }
    if allow_angle_brackets {
        expect_token(p, LuaTokenKind::TkGt)?;
    }
    Ok(m.complete(p))
}

// A : type
// A extends type
// A
// A ...
// A ... : type
// A ... extends type
fn parse_generic_param(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocGenericParameter);
    expect_token(p, LuaTokenKind::TkName)?;
    if p.current_token() == LuaTokenKind::TkDots {
        p.bump();
    }
    if matches!(
        p.current_token(),
        LuaTokenKind::TkColon | LuaTokenKind::TkDocExtends
    ) {
        p.bump();
        parse_type(p)?;
    }
    Ok(m.complete(p))
}

// ---@enum A
// ---@enum A : number
fn parse_tag_enum(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagEnum);
    p.bump();
    if p.current_token() == LuaTokenKind::TkLeftParen {
        parse_doc_type_flag(p)?;
    }

    expect_token(p, LuaTokenKind::TkName)?;
    if p.current_token() == LuaTokenKind::TkColon {
        p.bump();
        parse_type(p)?;
    }

    if p.current_token() == LuaTokenKind::TkDocContinueOr {
        parse_enum_field_list(p)?;
    }

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);

    Ok(m.complete(p))
}

fn parse_enum_field_list(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocEnumFieldList);

    while p.current_token() == LuaTokenKind::TkDocContinueOr {
        p.bump();
        parse_enum_field(p)?;
    }
    Ok(m.complete(p))
}

fn parse_enum_field(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocEnumField);
    if matches!(
        p.current_token(),
        LuaTokenKind::TkName | LuaTokenKind::TkString | LuaTokenKind::TkInt
    ) {
        p.bump();
    }

    if p.current_token() == LuaTokenKind::TkDocDetail {
        p.bump();
    }

    Ok(m.complete(p))
}

// ---@alias A string
// ---@alias A<T> keyof T
fn parse_tag_alias(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagAlias);
    p.bump();
    expect_token(p, LuaTokenKind::TkName)?;
    if p.current_token() == LuaTokenKind::TkLt {
        parse_generic_decl_list(p, true)?;
    }

    if_token_bump(p, LuaTokenKind::TkDocDetail);

    parse_type(p)?;

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@module "aaa.bbb.ccc" force variable be "aaa.bbb.ccc"
fn parse_tag_module(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagModule);
    p.bump();

    expect_token(p, LuaTokenKind::TkString)?;

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@field aaa string
// ---@field aaa? number
// ---@field [string] number
// ---@field [1] number
fn parse_tag_field(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::FieldStart);
    let m = p.mark(LuaSyntaxKind::DocTagField);
    p.bump();
    if p.current_token() == LuaTokenKind::TkLeftParen {
        parse_doc_type_flag(p)?;
    }

    p.set_lexer_state(LuaDocLexerState::Normal);
    if_token_bump(p, LuaTokenKind::TkDocVisibility);
    match p.current_token() {
        LuaTokenKind::TkName => p.bump(),
        LuaTokenKind::TkLeftBracket => {
            p.bump();
            if p.current_token() == LuaTokenKind::TkInt
                || p.current_token() == LuaTokenKind::TkString
            {
                p.bump();
            } else {
                parse_type(p)?;
            }
            expect_token(p, LuaTokenKind::TkRightBracket)?;
        }
        _ => {
            return Err(LuaParseError::doc_error_from(
                &t!(
                    "expect field name or '[', but get %{current}",
                    current = p.current_token()
                ),
                p.current_token_range(),
            ));
        }
    }
    if_token_bump(p, LuaTokenKind::TkDocQuestion);
    parse_type(p)?;

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@type string
// ---@type number, string
fn parse_tag_type(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagType);
    p.bump();
    parse_type(p)?;
    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        parse_type(p)?;
    }

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@param a number
// ---@param a? number
// ---@param ... string
fn parse_tag_param(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagParam);
    p.bump();
    if matches!(
        p.current_token(),
        LuaTokenKind::TkName | LuaTokenKind::TkDots
    ) {
        p.bump();
    } else {
        return Err(LuaParseError::doc_error_from(
            &t!(
                "expect param name or '...', but get %{current}",
                current = p.current_token()
            ),
            p.current_token_range(),
        ));
    }

    if_token_bump(p, LuaTokenKind::TkDocQuestion);

    parse_type(p)?;

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@return number
// ---@return number, string
// ---@return number <name> , this just compact luals
fn parse_tag_return(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagReturn);
    p.bump();

    parse_type(p)?;

    if_token_bump(p, LuaTokenKind::TkName);

    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        parse_type(p)?;
        if_token_bump(p, LuaTokenKind::TkName);
    }

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@return_cast <param name> <type>
// ---@return_cast <param name> <true_type> else <false_type>
fn parse_tag_return_cast(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagReturnCast);
    p.bump();
    expect_token(p, LuaTokenKind::TkName)?;

    parse_op_type(p)?;

    // Allow optional second type after 'else' for false condition
    if p.current_token() == LuaTokenKind::TkDocElse {
        p.bump();
        parse_op_type(p)?;
    }

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@generic T
// ---@generic T, R
// ---@generic T, R : number
fn parse_tag_generic(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagGeneric);
    p.bump();

    parse_generic_decl_list(p, false)?;

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@see <name>
// ---@see <name>#<name>
// ---@see <any content>
fn parse_tag_see(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::See);
    let m = p.mark(LuaSyntaxKind::DocTagSee);
    p.bump();
    expect_token(p, LuaTokenKind::TkDocSeeContent)?;
    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@as number
// --[[@as number]]
fn parse_tag_as(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagAs);
    p.bump();
    parse_type(p)?;

    if_token_bump(p, LuaTokenKind::TkLongCommentEnd);
    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@overload fun(a: number): string
// ---@overload async fun(a: number): string
fn parse_tag_overload(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagOverload);
    p.bump();
    parse_fun_type(p)?;
    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@cast a number
// ---@cast a +string
// ---@cast a -string
// ---@cast a +?
// ---@cast a +string, -number
fn parse_tag_cast(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::CastExpr);
    let m = p.mark(LuaSyntaxKind::DocTagCast);
    p.bump();

    if p.current_token() == LuaTokenKind::TkName {
        match parse_cast_expr(p) {
            Ok(_) => {}
            Err(e) => {
                return Err(e);
            }
        }
    }

    // 切换回正常状态
    parse_op_type(p)?;
    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        parse_op_type(p)?;
    }

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

fn parse_cast_expr(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::NameExpr);
    p.bump();
    let mut cm = m.complete(p);
    // 处理多级字段访问
    while p.current_token() == LuaTokenKind::TkDot {
        let index_m = cm.precede(p, LuaSyntaxKind::IndexExpr);
        p.bump();
        if p.current_token() == LuaTokenKind::TkName {
            p.bump();
        } else {
            // 找不到也不报错
        }
        cm = index_m.complete(p);
    }

    Ok(cm)
}

// +<type>, -<type>, +?, <type>
fn parse_op_type(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocOpType);
    if p.current_token() == LuaTokenKind::TkPlus || p.current_token() == LuaTokenKind::TkMinus {
        p.bump();
        if p.current_token() == LuaTokenKind::TkDocQuestion {
            p.bump();
        } else {
            parse_type(p)?;
        }
    } else {
        parse_type(p)?;
    }

    Ok(m.complete(p))
}

// ---@source <path>
// ---@source "<path>"
fn parse_tag_source(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Source);

    let m = p.mark(LuaSyntaxKind::DocTagSource);
    p.bump();
    expect_token(p, LuaTokenKind::TKDocPath)?;

    Ok(m.complete(p))
}

// ---@diagnostic <action>: <diagnostic-code>, ...
fn parse_tag_diagnostic(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagDiagnostic);
    p.bump();
    expect_token(p, LuaTokenKind::TkName)?;
    if p.current_token() == LuaTokenKind::TkColon {
        p.bump();
        parse_diagnostic_code_list(p)?;
    }

    Ok(m.complete(p))
}

fn parse_diagnostic_code_list(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocDiagnosticCodeList);
    expect_token(p, LuaTokenKind::TkName)?;
    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        expect_token(p, LuaTokenKind::TkName)?;
    }
    Ok(m.complete(p))
}

// ---@version Lua 5.1
// ---@version Lua JIT
// ---@version 5.1, JIT
// ---@version > Lua 5.1, Lua JIT
// ---@version > 5.1, 5.2, 5.3
fn parse_tag_version(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Version);
    let m = p.mark(LuaSyntaxKind::DocTagVersion);
    p.bump();
    parse_version(p)?;
    while p.current_token() == LuaTokenKind::TkComma {
        p.bump();
        parse_version(p)?;
    }
    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// 5.1
// JIT
// > 5.1
// < 5.4
// > Lua 5.1
fn parse_version(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocVersion);
    if matches!(p.current_token(), LuaTokenKind::TkLt | LuaTokenKind::TkGt) {
        p.bump();
    }

    if p.current_token() == LuaTokenKind::TkName {
        p.bump();
    }

    expect_token(p, LuaTokenKind::TkDocVersionNumber)?;
    Ok(m.complete(p))
}

// ---@operator add(number): number
// ---@operator call: number
fn parse_tag_operator(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagOperator);
    p.bump();
    expect_token(p, LuaTokenKind::TkName)?;
    if p.current_token() == LuaTokenKind::TkLeftParen {
        p.bump();
        parse_type_list(p)?;
        expect_token(p, LuaTokenKind::TkRightParen)?;
    }

    if p.current_token() == LuaTokenKind::TkColon {
        p.bump();
        parse_type(p)?;
    }

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@mapping <new name>
fn parse_tag_mapping(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagMapping);
    p.bump();
    expect_token(p, LuaTokenKind::TkName)?;
    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@namespace path
// ---@namespace System.Net
fn parse_tag_namespace(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagNamespace);
    p.bump();
    expect_token(p, LuaTokenKind::TkName)?;
    Ok(m.complete(p))
}

// ---@using path
fn parse_tag_using(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagUsing);
    p.bump();
    expect_token(p, LuaTokenKind::TkName)?;
    Ok(m.complete(p))
}

fn parse_tag_meta(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagMeta);
    p.bump();
    if_token_bump(p, LuaTokenKind::TkName);
    Ok(m.complete(p))
}

fn parse_tag_export(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagExport);
    p.bump();
    // @export 可以有可选的参数，如 @export namespace 或 @export global
    if p.current_token() == LuaTokenKind::TkName {
        p.bump();
    }
    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

fn parse_tag_language(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagLanguage);
    p.bump();
    expect_token(p, LuaTokenKind::TkName)?;

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// ---@attribute 名称(参数列表)
fn parse_tag_attribute(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagAttribute);
    p.bump();

    // 解析属性名称
    expect_token(p, LuaTokenKind::TkName)?;

    // 解析参数列表
    parse_type_attribute(p)?;

    p.set_lexer_state(LuaDocLexerState::Description);
    parse_description(p);
    Ok(m.complete(p))
}

// (param1: type1, param2: type2, ...)
fn parse_type_attribute(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::TypeAttribute);
    expect_token(p, LuaTokenKind::TkLeftParen)?;

    if p.current_token() != LuaTokenKind::TkRightParen {
        parse_typed_param(p)?;
        while p.current_token() == LuaTokenKind::TkComma {
            p.bump();
            parse_typed_param(p)?;
        }
    }

    expect_token(p, LuaTokenKind::TkRightParen)?;
    Ok(m.complete(p))
}

// ---@[attribute(arg1, arg2, ...)]
// ---@[attribute]
// ---@[attribute1, attribute2, ...]
// ---@generic [attribute] T
pub fn parse_tag_attribute_use(p: &mut LuaDocParser, allow_description: bool) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocTagAttributeUse);
    p.bump(); // consume '['

    while p.current_token() == LuaTokenKind::TkName {
        parse_doc_attribute_use(p)?;
        if p.current_token() != LuaTokenKind::TkComma {
            break;
        }
        p.bump(); // consume comma
    }

    // 期望结束符号 ']'
    expect_token(p, LuaTokenKind::TkRightBracket)?;

    // 属性使用解析完成后, 重置状态
    if allow_description {
        p.set_lexer_state(LuaDocLexerState::Description);
        parse_description(p);
    } else {
        p.set_lexer_state(LuaDocLexerState::Normal);
    }
    Ok(m.complete(p))
}

// attribute
// attribute(arg1, arg2, ...)
fn parse_doc_attribute_use(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocAttributeUse);

    // attribute 被视为类型
    parse_type(p)?;

    // 解析参数列表, 允许没有参数的特性在使用时省略括号
    if p.current_token() == LuaTokenKind::TkLeftParen {
        parse_attribute_arg_list(p)?;
    }

    Ok(m.complete(p))
}

// 解析属性参数列表
fn parse_attribute_arg_list(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::DocAttributeCallArgList);
    p.bump(); // consume '('

    // 解析参数值列表
    if p.current_token() != LuaTokenKind::TkRightParen {
        loop {
            if p.current_token() == LuaTokenKind::TkEof {
                break;
            }
            parse_attribute_arg(p)?;
            if p.current_token() != LuaTokenKind::TkComma {
                break;
            }
            p.bump(); // consume comma
            if p.current_token() == LuaTokenKind::TkRightParen {
                break; // trailing comma
            }
        }
    }

    expect_token(p, LuaTokenKind::TkRightParen)?;
    Ok(m.complete(p))
}

// 解析单个属性参数
fn parse_attribute_arg(p: &mut LuaDocParser) -> DocParseResult {
    let m = p.mark(LuaSyntaxKind::LiteralExpr);

    // TODO: 添加具名参数支持(name: value)
    match p.current_token() {
        LuaTokenKind::TkInt
        | LuaTokenKind::TkFloat
        | LuaTokenKind::TkComplex
        | LuaTokenKind::TkNil
        | LuaTokenKind::TkTrue
        | LuaTokenKind::TkFalse
        | LuaTokenKind::TkDots
        | LuaTokenKind::TkString
        | LuaTokenKind::TkLongString => {
            p.bump();
        }
        _ => {
            return Err(LuaParseError::doc_error_from(
                "Expected attribute argument value",
                p.current_token_range(),
            ));
        }
    };

    Ok(m.complete(p))
}

// function_name--[[@<type>, <type>...]](...args)
fn parse_tag_call_generic(p: &mut LuaDocParser) -> DocParseResult {
    p.set_lexer_state(LuaDocLexerState::Normal);
    let m = p.mark(LuaSyntaxKind::DocTagCallGeneric);
    p.bump();
    parse_type_list(p)?;

    expect_token(p, LuaTokenKind::TkGt)?;

    Ok(m.complete(p))
}
