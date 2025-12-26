use super::{
    SEMANTIC_TOKEN_MODIFIERS, SEMANTIC_TOKEN_TYPES, semantic_token_builder::SemanticBuilder,
};
use crate::handlers::semantic_token::function_string_highlight::fun_string_highlight;
use crate::handlers::semantic_token::semantic_token_builder::CustomSemanticTokenType;
use crate::util::parse_desc;
use crate::{context::ClientId, handlers::semantic_token::language_injector::inject_language};
use emmylua_code_analysis::{
    Emmyrc, LuaDecl, LuaDeclExtra, LuaMemberId, LuaMemberOwner, LuaSemanticDeclId, LuaType,
    LuaTypeDeclId, SemanticDeclLevel, SemanticModel, WorkspaceId, check_export_visibility,
    parse_require_module_info,
};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaCallArgList, LuaCallExpr, LuaComment, LuaDocFieldKey,
    LuaDocGenericDecl, LuaDocGenericDeclList, LuaDocObjectFieldKey, LuaDocType, LuaExpr,
    LuaGeneralToken, LuaKind, LuaLiteralToken, LuaNameToken, LuaSyntaxKind, LuaSyntaxNode,
    LuaSyntaxToken, LuaTokenKind, LuaVarExpr,
};
use emmylua_parser_desc::{CodeBlockHighlightKind, DescItem, DescItemKind};
use lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType};
use rowan::{NodeOrToken, TextRange, TextSize};

pub fn build_semantic_tokens(
    semantic_model: &SemanticModel,
    support_muliline_token: bool,
    client_id: ClientId,
    emmyrc: &Emmyrc,
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
                build_node_semantic_token(semantic_model, &mut builder, node, emmyrc);
            }
            NodeOrToken::Token(token) => {
                build_tokens_semantic_token(
                    semantic_model,
                    &mut builder,
                    &token,
                    client_id,
                    emmyrc,
                );
            }
        }
    }

    Some(builder.build())
}

fn build_tokens_semantic_token(
    _semantic_model: &SemanticModel,
    builder: &mut SemanticBuilder,
    token: &LuaSyntaxToken,
    client_id: ClientId,
    emmyrc: &Emmyrc,
) {
    match token.kind().into() {
        LuaTokenKind::TkLongString | LuaTokenKind::TkString => {
            if !builder.is_special_string_range(&token.text_range()) {
                builder.push(token, SemanticTokenType::STRING);
            }
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
        | LuaTokenKind::TkWhile
        | LuaTokenKind::TkGlobal => {
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
        | LuaTokenKind::TkAssign => {
            builder.push(token, SemanticTokenType::OPERATOR);
        }
        LuaTokenKind::TkLeftBrace | LuaTokenKind::TkRightBrace => {
            if let Some(parent) = token.parent()
                && !matches!(
                    parent.kind().into(),
                    LuaSyntaxKind::TableArrayExpr
                        | LuaSyntaxKind::TableEmptyExpr
                        | LuaSyntaxKind::TableObjectExpr
                )
            {
                builder.push(token, SemanticTokenType::OPERATOR);
            }
        }
        LuaTokenKind::TkColon => {
            if let Some(parent) = token.parent()
                && parent.kind() != LuaSyntaxKind::IndexExpr.into()
            {
                builder.push(token, SemanticTokenType::OPERATOR);
            }
        }
        // delimiter
        LuaTokenKind::TkLeftBracket | LuaTokenKind::TkRightBracket => {
            if let Some(parent) = token.parent()
                && matches!(
                    parent.kind().into(),
                    LuaSyntaxKind::TableFieldAssign | LuaSyntaxKind::IndexExpr
                )
            {
                builder.push(token, CustomSemanticTokenType::DELIMITER);
            } else {
                builder.push(token, SemanticTokenType::OPERATOR);
            }
        }
        LuaTokenKind::TkLeftParen | LuaTokenKind::TkRightParen => {
            if let Some(parent) = token.parent()
                && matches!(
                    parent.kind().into(),
                    LuaSyntaxKind::ParamList
                        | LuaSyntaxKind::CallArgList
                        | LuaSyntaxKind::ParenExpr
                )
            {
                builder.push(token, CustomSemanticTokenType::DELIMITER);
            } else {
                builder.push(token, SemanticTokenType::OPERATOR);
            }
        }
        LuaTokenKind::TkTrue | LuaTokenKind::TkFalse | LuaTokenKind::TkNil => {
            builder.push_with_modifier(
                token,
                SemanticTokenType::KEYWORD,
                SemanticTokenModifier::READONLY,
            );
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
        | LuaTokenKind::TkTagSource
        | LuaTokenKind::TkTagReturnCast
        | LuaTokenKind::TkTagExport
        | LuaTokenKind::TkLanguage
        | LuaTokenKind::TkTagAttribute => {
            builder.push_with_modifier(
                token,
                SemanticTokenType::KEYWORD,
                SemanticTokenModifier::DOCUMENTATION,
            );
        }
        LuaTokenKind::TkDocKeyOf
        | LuaTokenKind::TkDocExtends
        | LuaTokenKind::TkDocNew
        | LuaTokenKind::TkDocAs
        | LuaTokenKind::TkDocIn
        | LuaTokenKind::TkDocInfer
        | LuaTokenKind::TkDocReadonly => {
            builder.push_with_modifier(
                token,
                SemanticTokenType::KEYWORD,
                SemanticTokenModifier::DOCUMENTATION,
            );
        }
        LuaTokenKind::TkNormalStart | LuaTokenKind::TKNonStdComment => {
            builder.push(token, SemanticTokenType::COMMENT);
        }
        LuaTokenKind::TkDocDetail => {
            // We're rendering a description. If description parsing is enabled,
            // this token will be handled by the corresponding description parser.
            let rendering_description = token
                .parent()
                .is_some_and(|parent| parent.kind() == LuaSyntaxKind::DocDescription.into());
            let description_parsing_is_enabled = emmyrc.semantic_tokens.render_documentation_markup;

            if !(rendering_description && description_parsing_is_enabled) {
                builder.push(token, SemanticTokenType::COMMENT);
            }
        }
        LuaTokenKind::TkDocQuestion | LuaTokenKind::TkDocOr | LuaTokenKind::TkDocAnd => {
            builder.push_with_modifier(
                token,
                SemanticTokenType::OPERATOR,
                SemanticTokenModifier::DOCUMENTATION,
            );
        }
        LuaTokenKind::TkDocVisibility | LuaTokenKind::TkTagVisibility => {
            builder.push_with_modifiers(
                token,
                SemanticTokenType::KEYWORD,
                &[
                    SemanticTokenModifier::MODIFICATION,
                    SemanticTokenModifier::DOCUMENTATION,
                ],
            );
        }
        LuaTokenKind::TkDocVersionNumber => {
            builder.push_with_modifier(
                token,
                SemanticTokenType::NUMBER,
                SemanticTokenModifier::DOCUMENTATION,
            );
        }
        LuaTokenKind::TkStringTemplateType => {
            builder.push_with_modifier(
                token,
                SemanticTokenType::STRING,
                SemanticTokenModifier::DOCUMENTATION,
            );
        }
        LuaTokenKind::TkDocMatch => {
            builder.push_with_modifier(
                token,
                SemanticTokenType::KEYWORD,
                SemanticTokenModifier::DOCUMENTATION,
            );
        }
        LuaTokenKind::TKDocPath | LuaTokenKind::TkDocSeeContent => {
            builder.push_with_modifier(
                token,
                SemanticTokenType::STRING,
                SemanticTokenModifier::DOCUMENTATION,
            );
        }
        LuaTokenKind::TkDocRegion | LuaTokenKind::TkDocEndRegion => {
            builder.push(token, SemanticTokenType::COMMENT);
        }
        LuaTokenKind::TkDocStart | LuaTokenKind::TkDocContinue | LuaTokenKind::TkDocContinueOr => {
            render_doc_at(builder, token)
        }
        _ => {}
    }
}

fn build_node_semantic_token(
    semantic_model: &SemanticModel,
    builder: &mut SemanticBuilder,
    node: LuaSyntaxNode,
    emmyrc: &Emmyrc,
) -> Option<()> {
    match LuaAst::cast(node)? {
        LuaAst::LuaDocTagClass(doc_class) => {
            if let Some(name) = doc_class.get_name_token() {
                builder.push_with_modifier(
                    name.syntax(),
                    SemanticTokenType::CLASS,
                    SemanticTokenModifier::DECLARATION,
                );
            }
            if let Some(attribs) = doc_class.get_type_flag() {
                for token in attribs.tokens::<LuaGeneralToken>() {
                    builder.push(token.syntax(), SemanticTokenType::DECORATOR);
                }
            }
            if let Some(generic_list) = doc_class.get_generic_decl() {
                render_type_parameter_list(builder, &generic_list);
            }
        }
        LuaAst::LuaDocTagEnum(doc_enum) => {
            let name = doc_enum.get_name_token()?;
            builder.push_with_modifier(
                name.syntax(),
                SemanticTokenType::ENUM,
                SemanticTokenModifier::DECLARATION,
            );
            if let Some(attribs) = doc_enum.get_type_flag() {
                for token in attribs.tokens::<LuaGeneralToken>() {
                    builder.push(token.syntax(), SemanticTokenType::DECORATOR);
                }
            }
        }
        LuaAst::LuaDocTagAlias(doc_alias) => {
            let name = doc_alias.get_name_token()?;
            builder.push_with_modifier(
                name.syntax(),
                SemanticTokenType::TYPE,
                SemanticTokenModifier::DECLARATION,
            );
            if let Some(generic_decl_list) = doc_alias.get_generic_decl_list() {
                render_type_parameter_list(builder, &generic_decl_list);
            }
        }
        LuaAst::LuaDocTagField(doc_field) => {
            if let Some(LuaDocFieldKey::Name(name)) = doc_field.get_field_key() {
                builder.push_with_modifier(
                    name.syntax(),
                    SemanticTokenType::PROPERTY,
                    SemanticTokenModifier::DECLARATION,
                );
            }
        }
        LuaAst::LuaDocTagDiagnostic(doc_diagnostic) => {
            let name = doc_diagnostic.get_action_token()?;
            builder.push(name.syntax(), SemanticTokenType::PROPERTY);
            if let Some(code_list) = doc_diagnostic.get_code_list() {
                for code in code_list.get_codes() {
                    builder.push(code.syntax(), SemanticTokenType::REGEXP);
                }
            }
        }
        LuaAst::LuaDocTagParam(doc_param) => {
            let name = doc_param.get_name_token()?;
            builder.push_with_modifier(
                name.syntax(),
                SemanticTokenType::PARAMETER,
                SemanticTokenModifier::DECLARATION,
            );
        }
        LuaAst::LuaDocTagReturn(doc_return) => {
            let type_name_list = doc_return.get_info_list();
            for (_, name) in type_name_list {
                if let Some(name) = name {
                    builder.push(name.syntax(), SemanticTokenType::VARIABLE);
                }
            }
        }
        LuaAst::LuaDocTagCast(doc_cast) => {
            if let Some(target_expr) = doc_cast.get_key_expr() {
                match target_expr {
                    LuaExpr::NameExpr(name_expr) => {
                        builder.push(
                            name_expr.get_name_token()?.syntax(),
                            SemanticTokenType::VARIABLE,
                        );
                    }
                    LuaExpr::IndexExpr(index_expr) => {
                        let position = index_expr.syntax().text_range().start();
                        let len = index_expr.syntax().text_range().len();
                        builder.push_at_position(
                            position,
                            len.into(),
                            SemanticTokenType::VARIABLE,
                            None,
                        );
                    }
                    _ => {}
                }
            }
            if let Some(NodeOrToken::Token(token)) = doc_cast.syntax().prev_sibling_or_token()
                && token.kind() == LuaKind::Token(LuaTokenKind::TkDocLongStart)
            {
                render_doc_at(builder, &token);
            }
        }
        LuaAst::LuaDocTagAs(doc_as) => {
            if let Some(NodeOrToken::Token(token)) = doc_as.syntax().prev_sibling_or_token()
                && token.kind() == LuaKind::Token(LuaTokenKind::TkDocLongStart)
            {
                render_doc_at(builder, &token);
            }
        }
        LuaAst::LuaDocTagGeneric(doc_generic) => {
            let type_parameter_list = doc_generic.get_generic_decl_list()?;
            render_type_parameter_list(builder, &type_parameter_list);
        }
        LuaAst::LuaDocTagNamespace(doc_namespace) => {
            let name = doc_namespace.get_name_token()?;
            builder.push_with_modifier(
                name.syntax(),
                SemanticTokenType::NAMESPACE,
                SemanticTokenModifier::DECLARATION,
            );
        }
        LuaAst::LuaDocTagUsing(doc_using) => {
            let name = doc_using.get_name_token()?;
            builder.push(name.syntax(), SemanticTokenType::NAMESPACE);
        }
        LuaAst::LuaDocTagExport(doc_export) => {
            let name = doc_export.get_name_token()?;
            builder.push_with_modifier(
                name.syntax(),
                SemanticTokenType::NAMESPACE,
                SemanticTokenModifier::MODIFICATION,
            );
        }
        LuaAst::LuaParamName(param_name) => {
            let name_token = param_name.get_name_token()?;
            handle_name_node(semantic_model, builder, param_name.syntax(), &name_token);
        }
        LuaAst::LuaLocalName(local_name) => {
            handle_name_node(
                semantic_model,
                builder,
                local_name.syntax(),
                &local_name.get_name_token()?,
            );
        }
        LuaAst::LuaNameExpr(name_expr) => {
            let name_token = name_expr.get_name_token()?;
            handle_name_node(semantic_model, builder, name_expr.syntax(), &name_token)
                .unwrap_or_else(|| {
                    // 改进：为未知名称提供更好的默认分类
                    let name_text = name_token.get_name_text();
                    if name_text.chars().next().is_some_and(|c| c.is_uppercase()) {
                        // 首字母大写可能是类或常量
                        builder.push(name_token.syntax(), SemanticTokenType::CLASS);
                    } else {
                        builder.push(name_token.syntax(), SemanticTokenType::VARIABLE);
                    }
                });
        }
        LuaAst::LuaForRangeStat(for_range_stat) => {
            for name in for_range_stat.get_var_name_list() {
                builder.push_with_modifier(
                    name.syntax(),
                    SemanticTokenType::VARIABLE,
                    SemanticTokenModifier::DECLARATION,
                );
            }
        }
        LuaAst::LuaForStat(for_stat) => {
            let name = for_stat.get_var_name()?;
            builder.push_with_modifier(
                name.syntax(),
                SemanticTokenType::VARIABLE,
                SemanticTokenModifier::DECLARATION,
            );
        }
        LuaAst::LuaLocalFuncStat(local_func_stat) => {
            let name = local_func_stat.get_local_name()?.get_name_token()?;
            builder.push_with_modifier(
                name.syntax(),
                SemanticTokenType::FUNCTION,
                SemanticTokenModifier::DECLARATION,
            );
        }
        LuaAst::LuaFuncStat(func_stat) => {
            let func_name = func_stat.get_func_name()?;
            match func_name {
                LuaVarExpr::NameExpr(name_expr) => {
                    let name = name_expr.get_name_token()?;
                    builder.push_with_modifier(
                        name.syntax(),
                        SemanticTokenType::FUNCTION,
                        SemanticTokenModifier::DECLARATION,
                    );
                }
                LuaVarExpr::IndexExpr(index_expr) => {
                    let name = index_expr.get_index_name_token()?;
                    builder.push_with_modifier(
                        &name,
                        SemanticTokenType::METHOD,
                        SemanticTokenModifier::DECLARATION,
                    );
                }
            }
        }
        LuaAst::LuaLocalAttribute(local_attribute) => {
            let name = local_attribute.get_name_token()?;
            builder.push(name.syntax(), SemanticTokenType::KEYWORD);
        }
        LuaAst::LuaCallExpr(call_expr) => {
            let prefix = call_expr.get_prefix_expr()?;
            let prefix_type = semantic_model.infer_expr(prefix.clone()).ok();

            match prefix {
                LuaExpr::NameExpr(name_expr) => {
                    let name = name_expr.get_name_token()?;
                    if let Some(prefix_type) = prefix_type {
                        match prefix_type {
                            LuaType::Signature(signature) => {
                                if semantic_model
                                    .get_db()
                                    .get_module_index()
                                    .is_meta_file(&signature.get_file_id())
                                {
                                    builder.push_with_modifier(
                                        name.syntax(),
                                        SemanticTokenType::FUNCTION,
                                        SemanticTokenModifier::DEFAULT_LIBRARY,
                                    );
                                    return Some(());
                                }
                            }
                            _ => {
                                if !prefix_type.is_function() {
                                    return Some(());
                                }
                            }
                        }
                    }

                    builder.push(name.syntax(), SemanticTokenType::FUNCTION);
                }
                LuaExpr::IndexExpr(index_expr) => {
                    let name = index_expr.get_index_name_token()?;
                    // 改进：区分方法调用和属性访问
                    if call_expr.get_args_list().is_some() {
                        builder.push(&name, SemanticTokenType::METHOD);
                    } else {
                        builder.push(&name, SemanticTokenType::FUNCTION);
                    }
                }
                _ => {}
            }
        }
        LuaAst::LuaDocNameType(doc_name_type) => {
            let name = doc_name_type.get_name_token()?;
            let name_text = name.get_name_text();
            if name_text == "self"
                || name_text == "nil"
                || name_text == "boolean"
                || name_text == "number"
                || name_text == "string"
                || name_text == "table"
                || name_text == "function"
                || name_text == "userdata"
                || name_text == "thread"
            {
                // Lua内置类型
                builder.push_with_modifier(
                    name.syntax(),
                    SemanticTokenType::TYPE,
                    SemanticTokenModifier::DEFAULT_LIBRARY,
                );
            } else {
                builder.push(name.syntax(), SemanticTokenType::TYPE);
            }
        }
        LuaAst::LuaDocObjectType(doc_object_type) => {
            let fields = doc_object_type.get_fields();
            for field in fields {
                if let Some(field_key) = field.get_field_key()
                    && let LuaDocObjectFieldKey::Name(name) = &field_key
                {
                    builder.push(name.syntax(), SemanticTokenType::PROPERTY);
                }
            }
        }
        LuaAst::LuaDocFuncType(doc_func_type) => {
            for name_token in doc_func_type.tokens::<LuaNameToken>() {
                match name_token.get_name_text() {
                    "fun" => {
                        builder.push(name_token.syntax(), SemanticTokenType::KEYWORD);
                    }
                    "async" => {
                        builder.push_with_modifier(
                            name_token.syntax(),
                            SemanticTokenType::KEYWORD,
                            SemanticTokenModifier::ASYNC,
                        );
                    }
                    _ => {}
                }
            }

            for param in doc_func_type.get_params() {
                let name = param.get_name_token()?;
                builder.push(name.syntax(), SemanticTokenType::PARAMETER);
            }
        }
        LuaAst::LuaIndexExpr(index_expr) => {
            let name = index_expr.get_name_token()?;
            let semantic_decl = semantic_model
                .find_decl(name.syntax().clone().into(), SemanticDeclLevel::default());
            if let Some(property_owner) = semantic_decl
                && let LuaSemanticDeclId::Member(member_id) = property_owner
            {
                let decl_type = semantic_model.get_type(member_id.into());
                if decl_type.is_function() {
                    builder.push(name.syntax(), SemanticTokenType::METHOD);
                    return Some(());
                }
                if decl_type.is_def() {
                    builder.push_with_modifier(
                        name.syntax(),
                        SemanticTokenType::CLASS,
                        SemanticTokenModifier::READONLY,
                    );
                    return Some(());
                }

                let owner_id = semantic_model
                    .get_db()
                    .get_member_index()
                    .get_current_owner(&member_id);
                if let Some(LuaMemberOwner::Type(type_id)) = owner_id
                    && let Some(type_decl) = semantic_model
                        .get_db()
                        .get_type_index()
                        .get_type_decl(type_id)
                    && type_decl.is_enum()
                {
                    builder.push_with_modifier(
                        name.syntax(),
                        SemanticTokenType::ENUM_MEMBER,
                        SemanticTokenModifier::READONLY,
                    );
                    return Some(());
                }
            }

            // 默认情况：检查是否在调用上下文中
            if index_expr
                .syntax()
                .parent()
                .is_some_and(|p| p.kind() == LuaSyntaxKind::CallExpr.into())
            {
                builder.push(name.syntax(), SemanticTokenType::METHOD);
            } else {
                builder.push(name.syntax(), SemanticTokenType::PROPERTY);
            }
        }
        LuaAst::LuaTableField(table_field) => {
            let owner_id =
                LuaMemberId::new(table_field.get_syntax_id(), semantic_model.get_file_id());
            if let Some(member) = semantic_model
                .get_db()
                .get_member_index()
                .get_member(&owner_id)
            {
                let owner_id = semantic_model
                    .get_db()
                    .get_member_index()
                    .get_current_owner(&member.get_id());
                if let Some(LuaMemberOwner::Type(type_id)) = owner_id
                    && let Some(type_decl) = semantic_model
                        .get_db()
                        .get_type_index()
                        .get_type_decl(type_id)
                    && type_decl.is_enum()
                {
                    if let Some(field_name) = table_field.get_field_key()?.get_name() {
                        builder.push_with_modifier(
                            field_name.syntax(),
                            SemanticTokenType::ENUM_MEMBER,
                            SemanticTokenModifier::DECLARATION,
                        );
                    }
                    return Some(());
                }
            }

            let value_type = semantic_model
                .infer_expr(table_field.get_value_expr()?.clone())
                .ok()?;
            match value_type {
                LuaType::Signature(_) | LuaType::DocFunction(_) => {
                    if let Some(field_name) = table_field.get_field_key()?.get_name() {
                        builder.push_with_modifier(
                            field_name.syntax(),
                            SemanticTokenType::METHOD,
                            SemanticTokenModifier::DECLARATION,
                        );
                    }
                }
                _ => {
                    if let Some(field_name) = table_field.get_field_key()?.get_name() {
                        builder.push_with_modifier(
                            field_name.syntax(),
                            SemanticTokenType::PROPERTY,
                            SemanticTokenModifier::DECLARATION,
                        );
                    }
                }
            }
        }
        LuaAst::LuaDocLiteralType(literal) => {
            if let LuaLiteralToken::Bool(bool_token) = &literal.get_literal()? {
                builder.push_with_modifier(
                    bool_token.syntax(),
                    SemanticTokenType::KEYWORD,
                    SemanticTokenModifier::DOCUMENTATION,
                );
            }
        }
        LuaAst::LuaDocDescription(description) => {
            if !emmyrc.semantic_tokens.render_documentation_markup {
                for token in description.tokens::<LuaGeneralToken>() {
                    if matches!(
                        token.get_token_kind(),
                        LuaTokenKind::TkDocDetail | LuaTokenKind::TkNormalStart
                    ) {
                        builder.push(token.syntax(), SemanticTokenType::COMMENT);
                    }
                }
                return None;
            }
            // 如果文档的开始是 #, 则需要将其渲染为注释而不是文档
            if let Some(start_token) = description.tokens::<LuaGeneralToken>().next() {
                if start_token.get_text().starts_with('#') {
                    builder.push_at_position(
                        start_token.get_range().start(),
                        1,
                        SemanticTokenType::COMMENT,
                        None,
                    );
                }
            }

            let desc_range = description.get_range();
            let document = semantic_model.get_document();
            let text = document.get_text();
            let items = parse_desc(
                semantic_model
                    .get_module()
                    .map(|m| m.workspace_id)
                    .unwrap_or(WorkspaceId::MAIN),
                emmyrc,
                text,
                description,
                None,
            );
            render_desc_ranges(builder, text, items, desc_range);
        }
        LuaAst::LuaDocTagLanguage(language) => {
            let name = language.get_name_token()?;
            builder.push(name.syntax(), SemanticTokenType::STRING);
            let language_text = name.get_name_text();
            let comment = language.ancestors::<LuaComment>().next()?;

            inject_language(builder, language_text, comment);
        }
        LuaAst::LuaLiteralExpr(literal_expr) => {
            let call_expr = literal_expr
                .get_parent::<LuaCallArgList>()?
                .get_parent::<LuaCallExpr>()?;
            let literal_token = literal_expr.get_literal()?;
            if let LuaLiteralToken::String(string_token) = literal_token
                && !builder.is_special_string_range(&string_token.get_range())
            {
                fun_string_highlight(builder, semantic_model, call_expr, &string_token);
            }
        }
        LuaAst::LuaDocTagAttributeUse(tag_use) => {
            // 给 `@[` 染色
            if let Some(token) = tag_use.token_by_kind(LuaTokenKind::TkDocAttributeUse) {
                builder.push(token.syntax(), SemanticTokenType::KEYWORD);
            }
            // `]`染色
            if let Some(token) = tag_use.syntax().last_token() {
                builder.push(&token, SemanticTokenType::KEYWORD);
            }
            // 名称染色
            for attribute_use in tag_use.get_attribute_uses() {
                if let Some(token) = attribute_use.get_type()?.get_name_token() {
                    builder.push_with_modifiers(
                        token.syntax(),
                        SemanticTokenType::DECORATOR,
                        &[
                            SemanticTokenModifier::DECLARATION,
                            SemanticTokenModifier::DEFAULT_LIBRARY,
                        ],
                    );
                }
            }
        }
        LuaAst::LuaDocTagAttribute(tag_attribute) => {
            if let Some(name) = tag_attribute.get_name_token() {
                builder.push_with_modifier(
                    name.syntax(),
                    SemanticTokenType::TYPE,
                    SemanticTokenModifier::DECLARATION,
                );
            }
            if let Some(LuaDocType::Attribute(attribute)) = tag_attribute.get_type() {
                for param in attribute.get_params() {
                    if let Some(name) = param.get_name_token() {
                        builder.push(name.syntax(), SemanticTokenType::PARAMETER);
                    }
                }
            }
        }
        LuaAst::LuaDocInferType(infer_type) => {
            // 推断出的泛型定义
            if let Some(gen_decl) = infer_type.get_generic_decl() {
                render_type_parameter(builder, &gen_decl);
            }
            if let Some(name) = infer_type.token::<LuaNameToken>() {
                // 应该单独设置颜色
                if name.get_name_text() == "infer" {
                    builder.push(name.syntax(), SemanticTokenType::COMMENT);
                }
            }
        }
        _ => {}
    }

    Some(())
}

// 处理`local a = class``local a = class.method/field`
fn handle_name_node(
    semantic_model: &SemanticModel,
    builder: &mut SemanticBuilder,
    node: &LuaSyntaxNode,
    name_token: &LuaNameToken,
) -> Option<()> {
    let name_text = name_token.get_name_text();

    if name_text == "self" {
        builder.push_with_modifier(
            name_token.syntax(),
            SemanticTokenType::VARIABLE,
            SemanticTokenModifier::DEFINITION,
        );
        return Some(());
    }

    // 先查找声明，如果找不到声明再检查是否是 Lua 内置全局变量
    let semantic_decl = semantic_model.find_decl(node.clone().into(), SemanticDeclLevel::default());
    if semantic_decl.is_none()
        && matches!(
            name_text,
            "_G" | "_ENV"
                | "_VERSION"
                | "arg"
                | "package"
                | "require"
                | "load"
                | "loadfile"
                | "dofile"
                | "print"
                | "assert"
                | "error"
                | "warn"
                | "type"
                | "getmetatable"
                | "setmetatable"
                | "rawget"
                | "rawset"
                | "rawequal"
                | "rawlen"
                | "next"
                | "pairs"
                | "ipairs"
                | "tostring"
                | "tonumber"
                | "select"
                | "unpack"
                | "pcall"
                | "xpcall"
                | "collectgarbage"
        )
    {
        builder.push_with_modifiers(
            name_token.syntax(),
            SemanticTokenType::FUNCTION,
            &[
                SemanticTokenModifier::DEFAULT_LIBRARY,
                SemanticTokenModifier::READONLY,
            ],
        );
        return Some(());
    }
    let semantic_decl = semantic_decl?;
    match semantic_decl {
        LuaSemanticDeclId::Member(member_id) => {
            let decl_type = semantic_model.get_type(member_id.into());
            if matches!(decl_type, LuaType::Signature(_)) {
                builder.push(name_token.syntax(), SemanticTokenType::FUNCTION);
                return Some(());
            }
        }

        LuaSemanticDeclId::LuaDecl(decl_id) => {
            let decl = semantic_model
                .get_db()
                .get_decl_index()
                .get_decl(&decl_id)?;
            let decl_type = semantic_model.get_type(decl_id.into());

            if let Some(true) = check_require_decl(semantic_model, decl) {
                builder.push_with_modifier(
                    name_token.syntax(),
                    SemanticTokenType::CLASS,
                    SemanticTokenModifier::READONLY,
                );
                return Some(());
            }

            let (token_type, mut modifier) = match decl_type {
                LuaType::Def(_) => (SemanticTokenType::CLASS, None),
                LuaType::Ref(ref_id) => {
                    if let Some(is_require) =
                        check_ref_is_require_def(semantic_model, decl, &ref_id)
                    {
                        if is_require {
                            (
                                SemanticTokenType::CLASS,
                                Some(SemanticTokenModifier::READONLY),
                            )
                        } else {
                            // 改进：根据声明类型选择更准确的token类型
                            let base_type = if decl.is_param() {
                                SemanticTokenType::PARAMETER
                            } else {
                                SemanticTokenType::VARIABLE
                            };
                            (base_type, None)
                        }
                    } else {
                        let base_type = if decl.is_param() {
                            SemanticTokenType::PARAMETER
                        } else {
                            SemanticTokenType::VARIABLE
                        };
                        (base_type, None)
                    }
                }
                LuaType::Signature(signature) => {
                    let is_meta = semantic_model
                        .get_db()
                        .get_module_index()
                        .is_meta_file(&signature.get_file_id());
                    (
                        SemanticTokenType::FUNCTION,
                        is_meta.then_some(SemanticTokenModifier::DEFAULT_LIBRARY),
                    )
                }
                LuaType::DocFunction(_) => (SemanticTokenType::FUNCTION, None),
                LuaType::Union(union) => {
                    if union.into_vec().iter().any(|typ| typ.is_function()) {
                        (SemanticTokenType::FUNCTION, None)
                    } else {
                        let base_type = if decl.is_param() {
                            SemanticTokenType::PARAMETER
                        } else {
                            SemanticTokenType::VARIABLE
                        };
                        (base_type, None)
                    }
                }
                _ => {
                    let token_type = match &decl.extra {
                        LuaDeclExtra::Param {
                            idx, signature_id, ..
                        } => {
                            let signature = semantic_model
                                .get_db()
                                .get_signature_index()
                                .get(signature_id)?;
                            if let Some(param_info) = signature.get_param_info_by_id(*idx) {
                                if param_info.type_ref.is_function() {
                                    SemanticTokenType::FUNCTION
                                } else {
                                    SemanticTokenType::PARAMETER
                                }
                            } else {
                                SemanticTokenType::PARAMETER
                            }
                        }
                        _ => {
                            if decl.is_param() {
                                SemanticTokenType::PARAMETER
                            } else {
                                SemanticTokenType::VARIABLE
                            }
                        }
                    };

                    (token_type, None)
                }
            };

            // 检查是否只读
            if modifier.is_none() {
                let file_id = semantic_model.get_file_id();
                let ref_decl = semantic_model
                    .get_db()
                    .get_reference_index()
                    .get_decl_references(&file_id, &decl_id);
                if let Some(ref_decl) = ref_decl
                    && !ref_decl.mutable
                {
                    modifier = Some(SemanticTokenModifier::READONLY);
                }
            }

            let mut modifiers = vec![];
            if decl.is_global() {
                modifiers.push(SemanticTokenModifier::STATIC);
            }

            // 为声明添加 DECLARATION modifier
            if node.parent().is_some_and(|p| {
                matches!(p.kind(),
                    k if k == LuaSyntaxKind::LocalName.into() ||
                         k == LuaSyntaxKind::ParamName.into() ||
                         k == LuaSyntaxKind::LocalFuncStat.into()
                )
            }) {
                modifiers.push(SemanticTokenModifier::DECLARATION);
            }

            if let Some(modifier) = modifier {
                modifiers.push(modifier);
            }

            if !modifiers.is_empty() {
                builder.push_with_modifiers(name_token.syntax(), token_type, &modifiers);
            } else {
                builder.push(name_token.syntax(), token_type);
            }
            return Some(());
        }

        _ => {}
    }

    // 默认情况：如果不能确定类型，根据名称约定推断
    let default_type = if name_text.chars().next().is_some_and(|c| c.is_uppercase()) {
        SemanticTokenType::CLASS
    } else {
        SemanticTokenType::VARIABLE
    };

    builder.push(name_token.syntax(), default_type);
    Some(())
}

fn render_doc_at(builder: &mut SemanticBuilder, token: &LuaSyntaxToken) {
    let text = token.text();
    // find '@'/'|'
    let mut start = 0;
    let mut len = 0;
    for (i, c) in text.char_indices() {
        if matches!(c, '@' | '|') {
            start = i;
            if c == '|' && text[i + c.len_utf8()..].starts_with(['+', '>']) {
                len = 2;
            } else {
                len = 1;
            }
            break;
        }
    }

    builder.push_at_range(
        &text[..start],
        TextRange::at(token.text_range().start(), TextSize::new(start as u32)),
        SemanticTokenType::COMMENT,
        &[],
    );

    builder.push_at_range(
        &text[start..start + len],
        TextRange::at(
            token.text_range().start() + TextSize::new(start as u32),
            TextSize::new(len as u32),
        ),
        SemanticTokenType::KEYWORD,
        &[SemanticTokenModifier::DOCUMENTATION],
    );
}

fn render_desc_ranges(
    builder: &mut SemanticBuilder,
    text: &str,
    items: Vec<DescItem>,
    desc_range: TextRange,
) {
    let mut pos = desc_range.start();

    for item in items {
        if item.range.start() > pos {
            // Ensure that we override IDE's default comment parsing algorithm.
            let detail_range = TextRange::new(pos, item.range.start());
            builder.push_at_range(
                &text[detail_range],
                detail_range,
                SemanticTokenType::COMMENT,
                &[],
            );
        }
        let token_text = &text[item.range];
        match item.kind {
            DescItemKind::Code | DescItemKind::CodeBlock | DescItemKind::Ref => {
                builder.push_at_range(
                    token_text,
                    item.range,
                    SemanticTokenType::VARIABLE,
                    &[SemanticTokenModifier::DOCUMENTATION],
                );
                pos = item.range.end();
            }
            DescItemKind::Link | DescItemKind::JavadocLink => {
                builder.push_at_range(
                    token_text,
                    item.range,
                    SemanticTokenType::STRING,
                    &[SemanticTokenModifier::DOCUMENTATION],
                );
                pos = item.range.end();
            }
            DescItemKind::Markup | DescItemKind::Arg => {
                builder.push_at_range(
                    token_text,
                    item.range,
                    SemanticTokenType::OPERATOR,
                    &[SemanticTokenModifier::DOCUMENTATION],
                );
                pos = item.range.end();
            }
            DescItemKind::CodeBlockHl(highlight_kind) => {
                let token_type = match highlight_kind {
                    CodeBlockHighlightKind::Keyword => SemanticTokenType::KEYWORD,
                    CodeBlockHighlightKind::String => SemanticTokenType::STRING,
                    CodeBlockHighlightKind::Number => SemanticTokenType::NUMBER,
                    CodeBlockHighlightKind::Comment => SemanticTokenType::COMMENT,
                    CodeBlockHighlightKind::Function => SemanticTokenType::FUNCTION,
                    CodeBlockHighlightKind::Class => SemanticTokenType::CLASS,
                    CodeBlockHighlightKind::Enum => SemanticTokenType::ENUM,
                    CodeBlockHighlightKind::Variable => SemanticTokenType::VARIABLE,
                    CodeBlockHighlightKind::Property => SemanticTokenType::PROPERTY,
                    CodeBlockHighlightKind::Decorator => SemanticTokenType::DECORATOR,
                    CodeBlockHighlightKind::Operators => SemanticTokenType::OPERATOR,
                    _ => continue, // Fallback for other kinds
                };
                builder.push_at_range(token_text, item.range, token_type, &[]);
                pos = item.range.end();
            }
            _ => {}
        }
    }

    if pos < desc_range.end() {
        let detail_range = TextRange::new(pos, desc_range.end());
        builder.push_at_range(
            &text[detail_range],
            detail_range,
            SemanticTokenType::COMMENT,
            &[],
        );
    }
}

// 检查导入语句是否是类定义
fn check_ref_is_require_def(
    semantic_model: &SemanticModel,
    decl: &LuaDecl,
    ref_id: &LuaTypeDeclId,
) -> Option<bool> {
    let module_info = parse_require_module_info(semantic_model, decl)?;
    match &module_info.export_type {
        Some(ty) => match ty {
            LuaType::Def(id) => Some(id == ref_id),
            _ => Some(false),
        },
        None => None,
    }
}

/// 检查是否是导入语句
fn check_require_decl(semantic_model: &SemanticModel, decl: &LuaDecl) -> Option<bool> {
    let module_info = parse_require_module_info(semantic_model, decl)?;
    if check_export_visibility(semantic_model, module_info).unwrap_or(false) {
        return Some(true);
    }
    None
}

fn render_type_parameter_list(
    builder: &mut SemanticBuilder,
    type_parameter_list: &LuaDocGenericDeclList,
) {
    for type_decl in type_parameter_list.get_generic_decl() {
        render_type_parameter(builder, &type_decl);
    }
}

fn render_type_parameter(builder: &mut SemanticBuilder, type_decl: &LuaDocGenericDecl) {
    if let Some(name) = type_decl.get_name_token() {
        builder.push_with_modifier(
            name.syntax(),
            SemanticTokenType::TYPE,
            SemanticTokenModifier::DECLARATION,
        );
    }
}
