use emmylua_code_analysis::{LuaDeclExtra, LuaPropertyOwnerId, LuaType, SemanticModel};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaDocFieldKey, LuaDocObjectFieldKey, LuaExpr, LuaNameToken,
    LuaSyntaxNode, LuaSyntaxToken, LuaTokenKind, LuaVarExpr,
};
use lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType};
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
                build_node_semantic_token(semantic_model, &mut builder, node, client_id);
            }
            NodeOrToken::Token(token) => {
                build_tokens_semantic_token(semantic_model, &mut builder, token, client_id);
            }
        }
    }

    Some(builder.build())
}

#[allow(unused)]
fn build_tokens_semantic_token(
    semantic_model: &SemanticModel,
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
            builder.push_with_modifier(
                token,
                SemanticTokenType::KEYWORD,
                SemanticTokenModifier::DOCUMENTATION,
            );
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
        LuaTokenKind::TkDocVisibility | LuaTokenKind::TkTagVisibility => {
            builder.push_with_modifier(
                token,
                SemanticTokenType::KEYWORD,
                SemanticTokenModifier::MODIFICATION,
            );
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
        LuaTokenKind::TkDocStart => {
            let range = token.text_range();
            // find '@'
            let text = token.text();
            let mut start = 0;
            for (i, c) in text.char_indices() {
                if c == '@' {
                    start = i;
                    break;
                }
            }
            let position = u32::from(range.start()) + start as u32;
            builder.push_at_position(
                position.into(),
                1,
                SemanticTokenType::KEYWORD,
                SemanticTokenModifier::DOCUMENTATION,
            );
        }
        _ => {}
    }
}

fn build_node_semantic_token(
    semantic_model: &SemanticModel,
    builder: &mut SemanticBuilder,
    node: LuaSyntaxNode,
    _: ClientId,
) -> Option<()> {
    match LuaAst::cast(node)? {
        LuaAst::LuaDocTagClass(doc_class) => {
            let name = doc_class.get_name_token()?;
            builder.push_with_modifier(
                name.syntax().clone(),
                SemanticTokenType::CLASS,
                SemanticTokenModifier::DECLARATION,
            );
            if let Some(attribs) = doc_class.get_attrib() {
                for attrib_token in attribs.get_attrib_tokens() {
                    builder.push(attrib_token.syntax().clone(), SemanticTokenType::MODIFIER);
                }
            }
            if let Some(generic_list) = doc_class.get_generic_decl() {
                for generic_decl in generic_list.get_generic_decl() {
                    if let Some(name) = generic_decl.get_name_token() {
                        builder.push_with_modifier(
                            name.syntax().clone(),
                            SemanticTokenType::CLASS,
                            SemanticTokenModifier::DECLARATION,
                        );
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
                    builder.push_with_modifier(
                        name.syntax().clone(),
                        SemanticTokenType::TYPE,
                        SemanticTokenModifier::DECLARATION,
                    );
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
            handle_name_node(
                semantic_model,
                builder,
                local_name.syntax().clone(),
                local_name.get_name_token()?.syntax().clone(),
            );
        }
        LuaAst::LuaNameExpr(name_expr) => {
            handle_name_node(
                semantic_model,
                builder,
                name_expr.syntax().clone(),
                name_expr.get_name_token()?.syntax().clone(),
            );
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
            let prefix_type = semantic_model.infer_expr(prefix.clone());

            match prefix {
                LuaExpr::NameExpr(name_expr) => {
                    let name = name_expr.get_name_token()?;
                    if let Some(prefix_type) = prefix_type {
                        match prefix_type {
                            LuaType::Signature(signature) => {
                                if semantic_model
                                    .get_db()
                                    .get_meta_file()
                                    .is_meta_file(&signature.get_file_id())
                                {
                                    builder.push_with_modifier(
                                        name.syntax().clone(),
                                        SemanticTokenType::FUNCTION,
                                        SemanticTokenModifier::DEFAULT_LIBRARY,
                                    );
                                    return Some(());
                                }
                            }
                            _ => {}
                        }
                    }
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
            if name.get_name_text() == "self" {
                builder.push_with_modifier(
                    name.syntax().clone(),
                    SemanticTokenType::TYPE,
                    SemanticTokenModifier::READONLY,
                );
            } else {
                builder.push(name.syntax().clone(), SemanticTokenType::TYPE);
            }
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
            for name_token in doc_func_type.tokens::<LuaNameToken>() {
                match name_token.get_name_text() {
                    "fun" => {
                        builder.push(name_token.syntax().clone(), SemanticTokenType::KEYWORD);
                    }
                    "async" => {
                        builder.push_with_modifier(
                            name_token.syntax().clone(),
                            SemanticTokenType::KEYWORD,
                            SemanticTokenModifier::ASYNC,
                        );
                    }
                    _ => {}
                }
            }

            for param in doc_func_type.get_params() {
                let name = param.get_name_token()?;
                builder.push(name.syntax().clone(), SemanticTokenType::PARAMETER);
            }
        }
        LuaAst::LuaIndexExpr(index_expr) => {
            let name = index_expr.get_name_token()?;
            let property_owner =
                semantic_model.get_property_owner_id(name.syntax().clone().into())?;
            if let LuaPropertyOwnerId::Member(member_id) = property_owner {
                let member = semantic_model
                    .get_db()
                    .get_member_index()
                    .get_member(&member_id)?;
                let decl = member.get_decl_type();
                if decl.is_function() {
                    builder.push(name.syntax().clone(), SemanticTokenType::FUNCTION);
                    return Some(());
                }
            }
            builder.push(name.syntax().clone(), SemanticTokenType::PROPERTY);
        }
        LuaAst::LuaTableField(table_field) => {
            let value_type = semantic_model.infer_expr(table_field.get_value_expr()?.clone())?;
            match value_type {
                LuaType::Signature(_) => {
                    builder.push(
                        table_field.get_field_key()?.get_name()?.syntax().clone(),
                        SemanticTokenType::FUNCTION,
                    );
                }
                _ => {}
            }
        }
        _ => {}
    }

    Some(())
}

fn is_class_def(semantic_model: &SemanticModel, node: LuaSyntaxNode) -> Option<()> {
    let property_owner = semantic_model.get_property_owner_id(node.into())?;
    if let LuaPropertyOwnerId::LuaDecl(decl_id) = property_owner {
        let decl = semantic_model
            .get_db()
            .get_decl_index()
            .get_decl(&decl_id)?
            .get_type()?;
        match decl {
            LuaType::Def(_) => Some(()),
            _ => None,
        }
    } else {
        None
    }
}

// 处理`local a = class``local a = class.method/field`
fn handle_name_node(
    semantic_model: &SemanticModel,
    builder: &mut SemanticBuilder,
    node: LuaSyntaxNode,
    name_token: LuaSyntaxToken,
) -> Option<()> {
    if is_class_def(semantic_model, node.clone()).is_some() {
        builder.push(name_token, SemanticTokenType::CLASS);
        return Some(());
    }

    let owner_id = semantic_model.get_property_owner_id(node.into())?;

    match owner_id {
        LuaPropertyOwnerId::Member(member_id) => {
            let member = semantic_model
                .get_db()
                .get_member_index()
                .get_member(&member_id)?;
            if matches!(member.get_decl_type(), LuaType::Signature(_)) {
                builder.push(name_token, SemanticTokenType::FUNCTION);
                return Some(());
            }
        }

        LuaPropertyOwnerId::LuaDecl(decl_id) => {
            let decl = semantic_model
                .get_db()
                .get_decl_index()
                .get_decl(&decl_id)?;

            let (token_type, modifier) = match &decl.extra {
                LuaDeclExtra::Local { decl_type, .. } => match decl_type {
                    Some(LuaType::Signature(_) | LuaType::DocFunction(_)) => {
                        builder.push(name_token, SemanticTokenType::FUNCTION);
                        return Some(());
                    }
                    _ => (SemanticTokenType::VARIABLE, None),
                },

                LuaDeclExtra::Global { decl_type, .. } => match decl_type {
                    Some(LuaType::Signature(signature)) => {
                        let is_meta = semantic_model
                            .get_db()
                            .get_meta_file()
                            .is_meta_file(&signature.get_file_id());
                        (
                            SemanticTokenType::FUNCTION,
                            is_meta.then_some(SemanticTokenModifier::DEFAULT_LIBRARY),
                        )
                    }
                    _ => (SemanticTokenType::VARIABLE, None),
                },

                _ => (SemanticTokenType::VARIABLE, None),
            };

            if let Some(modifier) = modifier {
                builder.push_with_modifier(name_token, token_type, modifier);
            } else {
                builder.push(name_token, token_type);
            }
            return Some(());
        }

        _ => {}
    }

    builder.push(name_token, SemanticTokenType::VARIABLE);
    Some(())
}
