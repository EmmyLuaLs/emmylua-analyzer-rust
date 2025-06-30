use emmylua_code_analysis::{
    check_export_visibility, parse_require_module_info, LuaDecl, LuaDeclExtra, LuaMemberId,
    LuaMemberOwner, LuaSemanticDeclId, LuaType, LuaTypeDeclId, SemanticDeclLevel, SemanticModel,
};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaDocFieldKey, LuaDocObjectFieldKey, LuaExpr,
    LuaGeneralToken, LuaKind, LuaLiteralToken, LuaNameToken, LuaSyntaxNode, LuaSyntaxToken,
    LuaTokenKind, LuaVarExpr,
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
                build_tokens_semantic_token(semantic_model, &mut builder, &token, client_id);
            }
        }
    }

    Some(builder.build())
}

#[allow(unused)]
fn build_tokens_semantic_token(
    semantic_model: &SemanticModel,
    builder: &mut SemanticBuilder,
    token: &LuaSyntaxToken,
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
        | LuaTokenKind::TkTagSource
        | LuaTokenKind::TkTagReturnCast => {
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
        LuaTokenKind::TkDocMatch => {
            builder.push(token, SemanticTokenType::KEYWORD);
        }
        LuaTokenKind::TKDocPath | LuaTokenKind::TkDocSeeContent => {
            builder.push(token, SemanticTokenType::STRING);
        }
        LuaTokenKind::TkDocRegion | LuaTokenKind::TkDocEndRegion => {
            builder.push(token, SemanticTokenType::KEYWORD);
        }
        LuaTokenKind::TkDocStart => {
            render_doc_at(builder, &token);
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
            if let Some(name) = doc_class.get_name_token() {
                builder.push_with_modifier(
                    name.syntax(),
                    SemanticTokenType::CLASS,
                    SemanticTokenModifier::DECLARATION,
                );
            }
            if let Some(attribs) = doc_class.get_attrib() {
                for token in attribs.tokens::<LuaGeneralToken>() {
                    builder.push(token.syntax(), SemanticTokenType::DECORATOR);
                }
            }
            if let Some(generic_list) = doc_class.get_generic_decl() {
                for generic_decl in generic_list.get_generic_decl() {
                    if let Some(name) = generic_decl.get_name_token() {
                        builder.push_with_modifier(
                            name.syntax(),
                            SemanticTokenType::CLASS,
                            SemanticTokenModifier::DECLARATION,
                        );
                    }
                }
            }
        }
        LuaAst::LuaDocTagEnum(doc_enum) => {
            let name = doc_enum.get_name_token()?;
            builder.push(name.syntax(), SemanticTokenType::ENUM);
            if let Some(attribs) = doc_enum.get_attrib() {
                for token in attribs.tokens::<LuaGeneralToken>() {
                    builder.push(token.syntax(), SemanticTokenType::DECORATOR);
                }
            }
        }
        LuaAst::LuaDocTagAlias(doc_alias) => {
            let name = doc_alias.get_name_token()?;
            builder.push(name.syntax(), SemanticTokenType::TYPE);
        }
        LuaAst::LuaDocTagField(doc_field) => {
            if let Some(LuaDocFieldKey::Name(name)) = doc_field.get_field_key() {
                builder.push(name.syntax(), SemanticTokenType::PROPERTY);
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
            builder.push(name.syntax(), SemanticTokenType::PARAMETER);
        }
        LuaAst::LuaDocTagReturn(doc_return) => {
            let type_name_list = doc_return.get_type_and_name_list();
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
                            position.into(),
                            len.into(),
                            SemanticTokenType::VARIABLE,
                            None,
                        );
                    }
                    _ => {}
                }
            }
            if let Some(NodeOrToken::Token(token)) = doc_cast.syntax().prev_sibling_or_token() {
                if token.kind() == LuaKind::Token(LuaTokenKind::TkDocLongStart) {
                    render_doc_at(builder, &token);
                }
            }
        }
        LuaAst::LuaDocTagAs(doc_as) => {
            if let Some(NodeOrToken::Token(token)) = doc_as.syntax().prev_sibling_or_token() {
                if token.kind() == LuaKind::Token(LuaTokenKind::TkDocLongStart) {
                    render_doc_at(builder, &token);
                }
            }
        }
        LuaAst::LuaDocTagGeneric(doc_generic) => {
            let type_parameter_list = doc_generic.get_generic_decl_list()?;
            for type_decl in type_parameter_list.get_generic_decl() {
                if let Some(name) = type_decl.get_name_token() {
                    builder.push_with_modifier(
                        name.syntax(),
                        SemanticTokenType::TYPE,
                        SemanticTokenModifier::DECLARATION,
                    );
                }
            }
        }
        LuaAst::LuaDocTagNamespace(doc_namespace) => {
            let name = doc_namespace.get_name_token()?;
            builder.push(name.syntax(), SemanticTokenType::NAMESPACE);
        }
        LuaAst::LuaDocTagUsing(doc_using) => {
            let name = doc_using.get_name_token()?;
            builder.push(name.syntax(), SemanticTokenType::NAMESPACE);
        }
        LuaAst::LuaDocTagExport(doc_export) => {
            let name = doc_export.get_name_token()?;
            builder.push(name.syntax(), SemanticTokenType::NAMESPACE);
        }
        LuaAst::LuaParamName(param_name) => {
            let name = param_name.get_name_token()?;
            if name.get_name_text() == "self" {
                builder.push_with_modifier(
                    name.syntax(),
                    SemanticTokenType::VARIABLE,
                    SemanticTokenModifier::DEFINITION,
                );
            } else {
                builder.push(name.syntax(), SemanticTokenType::PARAMETER);
            }
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
            handle_name_node(
                semantic_model,
                builder,
                name_expr.syntax(),
                &name_expr.get_name_token()?,
            );
        }
        LuaAst::LuaForRangeStat(for_range_stat) => {
            for name in for_range_stat.get_var_name_list() {
                builder.push(name.syntax(), SemanticTokenType::VARIABLE);
            }
        }
        LuaAst::LuaForStat(for_stat) => {
            let name = for_stat.get_var_name()?;
            builder.push(name.syntax(), SemanticTokenType::VARIABLE);
        }
        LuaAst::LuaLocalFuncStat(local_func_stat) => {
            let name = local_func_stat.get_local_name()?.get_name_token()?;
            builder.push(name.syntax(), SemanticTokenType::FUNCTION);
        }
        LuaAst::LuaFuncStat(func_stat) => {
            let func_name = func_stat.get_func_name()?;
            match func_name {
                LuaVarExpr::NameExpr(name_expr) => {
                    let name = name_expr.get_name_token()?;
                    builder.push(name.syntax(), SemanticTokenType::FUNCTION);
                }
                LuaVarExpr::IndexExpr(index_expr) => {
                    let name = index_expr.get_index_name_token()?;
                    builder.push(&name, SemanticTokenType::FUNCTION);
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
                            _ => {}
                        }
                    }
                    builder.push(name.syntax(), SemanticTokenType::FUNCTION);
                }
                LuaExpr::IndexExpr(index_expr) => {
                    let name = index_expr.get_index_name_token()?;
                    builder.push(&name, SemanticTokenType::FUNCTION);
                }
                _ => {}
            }
        }
        LuaAst::LuaDocNameType(doc_name_type) => {
            let name = doc_name_type.get_name_token()?;
            if name.get_name_text() == "self" {
                builder.push_with_modifier(
                    name.syntax(),
                    SemanticTokenType::TYPE,
                    SemanticTokenModifier::READONLY,
                );
            } else {
                builder.push(name.syntax(), SemanticTokenType::TYPE);
            }
        }
        LuaAst::LuaDocObjectType(doc_object_type) => {
            let fields = doc_object_type.get_fields();
            for field in fields {
                if let Some(field_key) = field.get_field_key() {
                    match &field_key {
                        LuaDocObjectFieldKey::Name(name) => {
                            builder.push(name.syntax(), SemanticTokenType::PROPERTY);
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
            if let Some(property_owner) = semantic_decl {
                if let LuaSemanticDeclId::Member(member_id) = property_owner {
                    let decl_type = semantic_model.get_type(member_id.into());
                    if decl_type.is_function() {
                        builder.push(name.syntax(), SemanticTokenType::FUNCTION);
                        return Some(());
                    }
                    if decl_type.is_def() {
                        builder.push(name.syntax(), SemanticTokenType::CLASS);
                        return Some(());
                    }

                    let owner_id = semantic_model
                        .get_db()
                        .get_member_index()
                        .get_current_owner(&member_id);
                    if let Some(LuaMemberOwner::Type(type_id)) = owner_id {
                        if let Some(type_decl) = semantic_model
                            .get_db()
                            .get_type_index()
                            .get_type_decl(&type_id)
                        {
                            if type_decl.is_enum() {
                                builder.push(name.syntax(), SemanticTokenType::ENUM_MEMBER);
                                return Some(());
                            }
                        }
                    }
                }
            }
            builder.push(name.syntax(), SemanticTokenType::PROPERTY);
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
                if let Some(LuaMemberOwner::Type(type_id)) = owner_id {
                    if let Some(type_decl) = semantic_model
                        .get_db()
                        .get_type_index()
                        .get_type_decl(&type_id)
                    {
                        if type_decl.is_enum() {
                            builder.push(
                                table_field.get_field_key()?.get_name()?.syntax(),
                                SemanticTokenType::ENUM_MEMBER,
                            );
                            return Some(());
                        }
                    }
                }
            }

            let value_type = semantic_model
                .infer_expr(table_field.get_value_expr()?.clone())
                .ok()?;
            match value_type {
                LuaType::Signature(_) | LuaType::DocFunction(_) => {
                    builder.push(
                        table_field.get_field_key()?.get_name()?.syntax(),
                        SemanticTokenType::FUNCTION,
                    );
                }
                _ => {}
            }
        }
        LuaAst::LuaDocLiteralType(literal) => match &literal.get_literal()? {
            LuaLiteralToken::Bool(bool_token) => {
                builder.push_with_modifier(
                    bool_token.syntax(),
                    SemanticTokenType::KEYWORD,
                    SemanticTokenModifier::DOCUMENTATION,
                );
            }
            _ => {}
        },
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
    if name_token.get_name_text() == "self" {
        builder.push_with_modifier(
            name_token.syntax(),
            SemanticTokenType::VARIABLE,
            SemanticTokenModifier::DEFINITION,
        );
        return Some(());
    }

    let semantic_decl =
        semantic_model.find_decl(node.clone().into(), SemanticDeclLevel::default())?;
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
            if let Some(true) = check_require_decl(semantic_model, &decl) {
                builder.push_with_modifier(
                    name_token.syntax(),
                    SemanticTokenType::CLASS,
                    SemanticTokenModifier::READONLY,
                );
                return Some(());
            }

            let (token_type, modifier) = match decl_type {
                LuaType::Def(_) => (SemanticTokenType::CLASS, None),
                LuaType::Ref(ref_id) => {
                    if let Some(is_require) =
                        check_ref_is_require_def(semantic_model, &decl, &ref_id)
                    {
                        if is_require {
                            (
                                SemanticTokenType::CLASS,
                                Some(SemanticTokenModifier::READONLY),
                            )
                        } else {
                            (SemanticTokenType::VARIABLE, None)
                        }
                    } else {
                        (SemanticTokenType::VARIABLE, None)
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
                    if union.get_types().iter().any(|typ| typ.is_function()) {
                        (SemanticTokenType::FUNCTION, None)
                    } else {
                        if decl.is_param() {
                            (SemanticTokenType::PARAMETER, None)
                        } else {
                            (SemanticTokenType::VARIABLE, None)
                        }
                    }
                }
                _ => match &decl.extra {
                    LuaDeclExtra::Param {
                        idx, signature_id, ..
                    } => {
                        let signature = semantic_model
                            .get_db()
                            .get_signature_index()
                            .get(&signature_id)?;
                        if let Some(param_info) = signature.get_param_info_by_id(*idx) {
                            if param_info.type_ref.is_function() {
                                (SemanticTokenType::FUNCTION, None)
                            } else {
                                (SemanticTokenType::PARAMETER, None)
                            }
                        } else {
                            (SemanticTokenType::VARIABLE, None)
                        }
                    }
                    _ => (SemanticTokenType::VARIABLE, None),
                },
            };

            if let Some(modifier) = modifier {
                builder.push_with_modifier(name_token.syntax(), token_type, modifier);
            } else {
                builder.push(name_token.syntax(), token_type);
            }
            return Some(());
        }

        _ => {}
    }

    builder.push(name_token.syntax(), SemanticTokenType::VARIABLE);
    Some(())
}

fn render_doc_at(builder: &mut SemanticBuilder, token: &LuaSyntaxToken) {
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
        Some(SemanticTokenModifier::DOCUMENTATION),
    );
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
    if check_export_visibility(semantic_model, &module_info).unwrap_or(false) {
        return Some(true);
    }
    None
}
