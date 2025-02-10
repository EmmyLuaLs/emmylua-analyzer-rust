use emmylua_code_analysis::SemanticModel;
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaDocFieldKey, LuaDocObjectFieldKey, LuaDocType, LuaExpr,
    LuaSyntaxNode, LuaSyntaxToken, LuaTokenKind, LuaVarExpr,
};
use lsp_types::{SemanticToken, SemanticTokenType};
use rowan::NodeOrToken;

use crate::context::ClientId;

use super::{
    semantic_token_builder::SemanticBuilder, SEMANTIC_TOKEN_MODIFIERS, SEMANTIC_TOKEN_TYPES,
};

pub fn build_semantic_tokens(
    semantic_model: &mut SemanticModel,
    support_muliline_token: bool,
    client_id: ClientId,
) -> Option<Vec<SemanticToken>> {
    let root = semantic_model.get_root();
    let document = semantic_model.get_document();
    let mut builder = SemanticBuilder::new(
        &document,
        support_muliline_token,
        SEMANTIC_TOKEN_TYPES.to_vec(),
        SEMANTIC_TOKEN_MODIFIERS.to_vec(),
    );

    for node_or_token in root.syntax().descendants_with_tokens() {
        match node_or_token {
            NodeOrToken::Node(node) => {
                build_node_semantic_token(&mut builder, node, client_id);
            }
            NodeOrToken::Token(token) => {
                build_tokens_semantic_token(&mut builder, token, client_id);
            }
        }
    }

    Some(builder.build())
}

fn build_tokens_semantic_token(
    builder: &mut SemanticBuilder,
    token: LuaSyntaxToken,
    client_id: ClientId,
) {
    match token.kind().into() {
        LuaTokenKind::TkLongString | LuaTokenKind::TkString => {
            builder.push(token, SemanticTokenType::STRING);
        }
        LuaTokenKind::TkAnd
        | LuaTokenKind::TkBreak
        | LuaTokenKind::TkDo
        | LuaTokenKind::TkElse
        | LuaTokenKind::TkElseIf
        | LuaTokenKind::TkEnd
        | LuaTokenKind::TkFor
        | LuaTokenKind::TkFunction
        | LuaTokenKind::TkGoto
        | LuaTokenKind::TkIf
        | LuaTokenKind::TkIn
        | LuaTokenKind::TkNot
        | LuaTokenKind::TkOr
        | LuaTokenKind::TkRepeat
        | LuaTokenKind::TkReturn
        | LuaTokenKind::TkThen
        | LuaTokenKind::TkUntil
        | LuaTokenKind::TkWhile => {
            builder.push(token, SemanticTokenType::KEYWORD);
        }
        LuaTokenKind::TkLocal => {
            if !client_id.is_vscode() {
                builder.push(token, SemanticTokenType::KEYWORD);
            }
        }
        LuaTokenKind::TkPlus
        | LuaTokenKind::TkMinus
        | LuaTokenKind::TkMul
        | LuaTokenKind::TkDiv
        | LuaTokenKind::TkIDiv
        | LuaTokenKind::TkDot
        | LuaTokenKind::TkConcat
        | LuaTokenKind::TkEq
        | LuaTokenKind::TkGe
        | LuaTokenKind::TkLe
        | LuaTokenKind::TkNe
        | LuaTokenKind::TkShl
        | LuaTokenKind::TkShr
        | LuaTokenKind::TkLt
        | LuaTokenKind::TkGt
        | LuaTokenKind::TkMod
        | LuaTokenKind::TkPow
        | LuaTokenKind::TkLen
        | LuaTokenKind::TkBitAnd
        | LuaTokenKind::TkBitOr
        | LuaTokenKind::TkBitXor
        | LuaTokenKind::TkLeftBrace
        | LuaTokenKind::TkRightBrace
        | LuaTokenKind::TkLeftBracket
        | LuaTokenKind::TkRightBracket => {
            builder.push(token, SemanticTokenType::OPERATOR);
        }
        LuaTokenKind::TkComplex | LuaTokenKind::TkInt | LuaTokenKind::TkFloat => {
            builder.push(token, SemanticTokenType::NUMBER);
        }
        LuaTokenKind::TkTagClass
        | LuaTokenKind::TkTagEnum
        | LuaTokenKind::TkTagInterface
        | LuaTokenKind::TkTagAlias
        | LuaTokenKind::TkTagModule
        | LuaTokenKind::TkTagField
        | LuaTokenKind::TkTagType
        | LuaTokenKind::TkTagParam
        | LuaTokenKind::TkTagReturn
        | LuaTokenKind::TkTagOverload
        | LuaTokenKind::TkTagGeneric
        | LuaTokenKind::TkTagSee
        | LuaTokenKind::TkTagDeprecated
        | LuaTokenKind::TkTagAsync
        | LuaTokenKind::TkTagCast
        | LuaTokenKind::TkTagOther
        | LuaTokenKind::TkTagVisibility
        | LuaTokenKind::TkTagReadonly
        | LuaTokenKind::TkTagDiagnostic
        | LuaTokenKind::TkTagMeta
        | LuaTokenKind::TkTagVersion
        | LuaTokenKind::TkTagAs
        | LuaTokenKind::TkTagNodiscard
        | LuaTokenKind::TkTagOperator
        | LuaTokenKind::TkTagMapping
        | LuaTokenKind::TkTagNamespace
        | LuaTokenKind::TkTagUsing
        | LuaTokenKind::TkTagSource => {
            if !client_id.is_vscode() {
                builder.push(token, SemanticTokenType::DECORATOR);
            }
        }
        LuaTokenKind::TkDocKeyOf
        | LuaTokenKind::TkDocExtends
        | LuaTokenKind::TkDocAs
        | LuaTokenKind::TkDocIn
        | LuaTokenKind::TkDocInfer => {
            builder.push(token, SemanticTokenType::KEYWORD);
        }
        LuaTokenKind::TkDocDetail => {
            builder.push(token, SemanticTokenType::COMMENT);
        }
        LuaTokenKind::TkDocQuestion => {
            builder.push(token, SemanticTokenType::OPERATOR);
        }
        LuaTokenKind::TkDocVisibility => {
            builder.push(token, SemanticTokenType::DECORATOR);
        }
        LuaTokenKind::TkDocVersionNumber => {
            builder.push(token, SemanticTokenType::NUMBER);
        }
        LuaTokenKind::TkStringTemplateType => {
            builder.push(token, SemanticTokenType::STRING);
        }
        LuaTokenKind::TkDocMatch | LuaTokenKind::TkDocBoolean => {
            builder.push(token, SemanticTokenType::KEYWORD);
        }
        LuaTokenKind::TKDocPath => {
            builder.push(token, SemanticTokenType::STRING);
        }
        LuaTokenKind::TkDocRegion | LuaTokenKind::TkDocEndRegion => {
            builder.push(token, SemanticTokenType::KEYWORD);
        }
        _ => {}
    }
}

fn build_node_semantic_token(
    builder: &mut SemanticBuilder,
    node: LuaSyntaxNode,
    _: ClientId,
) -> Option<()> {
    match LuaAst::cast(node)? {
        LuaAst::LuaDocTagClass(doc_class) => {
            let name = doc_class.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::CLASS);
            if let Some(attribs) = doc_class.get_attrib() {
                for attrib_token in attribs.get_attrib_tokens() {
                    builder.push(attrib_token.syntax().clone(), SemanticTokenType::MODIFIER);
                }
            }
            if let Some(generic_list) = doc_class.get_generic_decl() {
                for generic_decl in generic_list.get_generic_decl() {
                    if let Some(name) = generic_decl.get_name_token() {
                        builder.push(name.syntax().clone(), SemanticTokenType::TYPE_PARAMETER);
                    }
                }
            }
        }
        LuaAst::LuaDocTagEnum(doc_enum) => {
            let name = doc_enum.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::ENUM);
            if let Some(attribs) = doc_enum.get_attrib() {
                for attrib_token in attribs.get_attrib_tokens() {
                    builder.push(attrib_token.syntax().clone(), SemanticTokenType::MODIFIER);
                }
            }
        }
        LuaAst::LuaDocTagAlias(doc_alias) => {
            let name = doc_alias.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::TYPE);
        }
        LuaAst::LuaDocTagField(doc_field) => {
            if let Some(LuaDocFieldKey::Name(name)) = doc_field.get_field_key() {
                builder.push(name.syntax().clone(), SemanticTokenType::PROPERTY);
            }
            if let Some(visiblity_token) = doc_field.get_visibility_token() {
                builder.push(visiblity_token.syntax().clone(), SemanticTokenType::KEYWORD);
            }
        }
        LuaAst::LuaDocTagDiagnostic(doc_diagnostic) => {
            let name = doc_diagnostic.get_action_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::PROPERTY);
            if let Some(code_list) = doc_diagnostic.get_code_list() {
                for code in code_list.get_codes() {
                    builder.push(code.syntax().clone(), SemanticTokenType::REGEXP);
                }
            }
        }
        LuaAst::LuaDocTagParam(doc_param) => {
            let name = doc_param.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::PARAMETER);
        }
        LuaAst::LuaDocTagReturn(doc_return) => {
            let type_name_list = doc_return.get_type_and_name_list();
            for (_, name) in type_name_list {
                if let Some(name) = name {
                    builder.push(name.syntax().clone(), SemanticTokenType::VARIABLE);
                }
            }
        }
        LuaAst::LuaDocTagCast(doc_cast) => {
            let name = doc_cast.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::VARIABLE);
        }
        LuaAst::LuaDocTagGeneric(doc_generic) => {
            let type_parameter_list = doc_generic.get_generic_decl_list()?;
            for type_decl in type_parameter_list.get_generic_decl() {
                if let Some(name) = type_decl.get_name_token() {
                    builder.push(name.syntax().clone(), SemanticTokenType::TYPE_PARAMETER);
                }
            }
        }
        LuaAst::LuaDocTagNamespace(doc_namespace) => {
            let name = doc_namespace.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::NAMESPACE);
        }
        LuaAst::LuaDocTagUsing(doc_using) => {
            let name = doc_using.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::NAMESPACE);
        }
        LuaAst::LuaParamName(param_name) => {
            let name = param_name.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::PARAMETER);
        }
        LuaAst::LuaLocalName(local_name) => {
            let name = local_name.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::VARIABLE);
        }
        LuaAst::LuaNameExpr(name_expr) => {
            let name = name_expr.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::VARIABLE);
        }
        LuaAst::LuaForRangeStat(for_range_stat) => {
            for name in for_range_stat.get_var_name_list() {
                builder.push(name.syntax().clone(), SemanticTokenType::VARIABLE);
            }
        }
        LuaAst::LuaForStat(for_stat) => {
            let name = for_stat.get_var_name()?;
            builder.push(name.syntax().clone(), SemanticTokenType::VARIABLE);
        }
        LuaAst::LuaLocalFuncStat(local_func_stat) => {
            let name = local_func_stat.get_local_name()?.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::FUNCTION);
        }
        LuaAst::LuaFuncStat(func_stat) => {
            let func_name = func_stat.get_func_name()?;
            match func_name {
                LuaVarExpr::NameExpr(name_expr) => {
                    let name = name_expr.get_name_token()?;
                    builder.push(name.syntax().clone(), SemanticTokenType::FUNCTION);
                }
                LuaVarExpr::IndexExpr(index_expr) => {
                    let name = index_expr.get_index_name_token()?;
                    builder.push(name, SemanticTokenType::FUNCTION);
                }
            }
        }
        LuaAst::LuaLocalAttribute(local_attribute) => {
            let name = local_attribute.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::KEYWORD);
        }
        LuaAst::LuaCallExpr(call_expr) => {
            let prefix = call_expr.get_prefix_expr()?;
            match prefix {
                LuaExpr::NameExpr(name_expr) => {
                    let name = name_expr.get_name_token()?;
                    builder.push(name.syntax().clone(), SemanticTokenType::FUNCTION);
                }
                LuaExpr::IndexExpr(index_expr) => {
                    let name = index_expr.get_index_name_token()?;
                    builder.push(name, SemanticTokenType::FUNCTION);
                }
                _ => {}
            }
        }
        LuaAst::LuaDocNameType(doc_name_type) => {
            let name = doc_name_type.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::TYPE);
        }
        LuaAst::LuaDocObjectType(doc_object_type) => {
            let fields = doc_object_type.get_fields();
            for field in fields {
                if let Some(field_key) = field.get_field_key() {
                    match &field_key {
                        LuaDocObjectFieldKey::Name(name) => {
                            builder.push(name.syntax().clone(), SemanticTokenType::PROPERTY);
                        }
                        _ => {}
                    }
                }
            }
        }
        LuaAst::LuaDocFuncType(doc_func_type) => {
            let name = doc_func_type.get_name_token()?;
            builder.push(name.syntax().clone(), SemanticTokenType::KEYWORD);
            for param in doc_func_type.get_params().into_iter() {
                if let Some(name) = param.get_name_token() {
                    builder.push(name.syntax().clone(), SemanticTokenType::PARAMETER);

                    // 类型为`self`时需要特殊处理
                    if let Some(param_type) = param.get_type() {
                        match param_type {
                            LuaDocType::Name(name_type) => {
                                if let Some(name) = name_type.get_name_token() {
                                    if name.get_name_text() == "self" {
                                        builder.push(
                                            name.syntax().clone(),
                                            SemanticTokenType::KEYWORD,
                                        );
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        _ => {}
    }

    Some(())
}
