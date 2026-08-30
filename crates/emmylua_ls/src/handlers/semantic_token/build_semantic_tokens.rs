//! # build_semantic_tokens — Salsa-based semantic tokens (Stage 1 + Stage 2)
//!
//! Stage 1: recover purely syntactic token classifications (keywords/operators/delimiters/comments/strings/numbers/doc tags).
//! Stage 2: recover names/members/types/declaration modifiers from `SalsaSemanticModel`.

use emmylua_code_analysis::{
    DeclKind, DocumentView, Emmyrc, LuaType, SalsaSemanticModel, SemanticId, SemanticInfo,
    TypeDefKind, TypeScope,
};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaCallArgList, LuaCallExpr, LuaComment, LuaDocFieldKey,
    LuaDocGenericDecl, LuaDocGenericDeclList, LuaDocMappedKey, LuaDocType, LuaExpr,
    LuaGeneralToken, LuaIndexExpr, LuaLiteralToken, LuaNameToken, LuaSyntaxKind, LuaSyntaxNode,
    LuaSyntaxToken, LuaTokenKind, LuaVarExpr,
};
use lsp_types::SemanticToken;
use rowan::{NodeOrToken, TextRange, TextSize};

use crate::context::ClientId;

use super::function_string_highlight::fun_string_highlight;
use super::language_injector::inject_language;
use super::semantic_token_builder::{
    SemanticBuilder, SemanticTokenModifierKind, SemanticTokenTypeKind,
};

pub fn build_semantic_tokens(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    supports_multiline_tokens: bool,
    client_id: ClientId,
    emmyrc: &Emmyrc,
) -> Option<Vec<SemanticToken>> {
    let root = model.chunk()?;
    let mut builder = SemanticBuilder::new(document, supports_multiline_tokens);

    for node_or_token in root.syntax().descendants_with_tokens() {
        match node_or_token {
            NodeOrToken::Node(node) => {
                build_node_semantic_token(model, &mut builder, node, emmyrc);
            }
            NodeOrToken::Token(token) => {
                build_tokens_semantic_token(&mut builder, &token, client_id, emmyrc);
            }
        }
    }

    Some(builder.build())
}

fn build_tokens_semantic_token(
    builder: &mut SemanticBuilder,
    token: &LuaSyntaxToken,
    client_id: ClientId,
    emmyrc: &Emmyrc,
) {
    match token.kind().into() {
        LuaTokenKind::TkLongString | LuaTokenKind::TkString => {
            if !builder.is_special_string_range(&token.text_range()) {
                builder.push(token, SemanticTokenTypeKind::String);
            }
        }
        LuaTokenKind::TkBreak
        | LuaTokenKind::TkDo
        | LuaTokenKind::TkElse
        | LuaTokenKind::TkElseIf
        | LuaTokenKind::TkEnd
        | LuaTokenKind::TkFor
        | LuaTokenKind::TkFunction
        | LuaTokenKind::TkGoto
        | LuaTokenKind::TkIf
        | LuaTokenKind::TkIn
        | LuaTokenKind::TkRepeat
        | LuaTokenKind::TkReturn
        | LuaTokenKind::TkThen
        | LuaTokenKind::TkUntil
        | LuaTokenKind::TkWhile
        | LuaTokenKind::TkConst
        | LuaTokenKind::TkContinue
        | LuaTokenKind::TkGlobal => {
            builder.push(token, SemanticTokenTypeKind::Keyword);
        }
        LuaTokenKind::TkLogicalOr | LuaTokenKind::TkLogicalAnd | LuaTokenKind::TkToggle => {
            builder.push(token, SemanticTokenTypeKind::Operator);
        }
        LuaTokenKind::TkAnd | LuaTokenKind::TkOr | LuaTokenKind::TkNot => {
            builder.push_with_modifier(
                token,
                SemanticTokenTypeKind::Keyword,
                SemanticTokenModifierKind::OPERATOR_LOGICAL,
            );
        }
        LuaTokenKind::TkLocal => {
            if !client_id.is_vscode() {
                builder.push(token, SemanticTokenTypeKind::Keyword);
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
        | LuaTokenKind::TkAssign
        | LuaTokenKind::TkTernary
        | LuaTokenKind::TkSafeNavigation
        | LuaTokenKind::TkShrArithmetic
        | LuaTokenKind::TkNilCoalescing => {
            builder.push(token, SemanticTokenTypeKind::Operator);
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
                builder.push(token, SemanticTokenTypeKind::Operator);
            }
        }
        LuaTokenKind::TkColon => {
            if let Some(parent) = token.parent()
                && parent.kind() != LuaSyntaxKind::IndexExpr.into()
            {
                builder.push(token, SemanticTokenTypeKind::Operator);
            }
        }
        LuaTokenKind::TkLeftBracket | LuaTokenKind::TkRightBracket => {
            if let Some(parent) = token.parent()
                && matches!(
                    parent.kind().into(),
                    LuaSyntaxKind::TableFieldAssign | LuaSyntaxKind::IndexExpr
                )
            {
                builder.push(token, SemanticTokenTypeKind::Delimiter);
            } else {
                builder.push(token, SemanticTokenTypeKind::Operator);
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
                builder.push(token, SemanticTokenTypeKind::Delimiter);
            } else {
                builder.push(token, SemanticTokenTypeKind::Operator);
            }
        }
        LuaTokenKind::TkTrue | LuaTokenKind::TkFalse | LuaTokenKind::TkNil => {
            if is_doc_type_literal_token(token) {
                builder.push(token, SemanticTokenTypeKind::Type);
            } else {
                builder.push_with_modifier(
                    token,
                    SemanticTokenTypeKind::Keyword,
                    SemanticTokenModifierKind::READONLY,
                );
            }
        }
        LuaTokenKind::TkComplex | LuaTokenKind::TkInt | LuaTokenKind::TkFloat => {
            builder.push(token, SemanticTokenTypeKind::Number);
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
        | LuaTokenKind::TkTagReturnOverload
        | LuaTokenKind::TkLanguage
        | LuaTokenKind::TKTagSchema => {
            builder.push_with_modifier(
                token,
                SemanticTokenTypeKind::Keyword,
                SemanticTokenModifierKind::DOCUMENTATION,
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
                SemanticTokenTypeKind::Keyword,
                SemanticTokenModifierKind::DOCUMENTATION,
            );
        }
        LuaTokenKind::TkNormalStart | LuaTokenKind::TKNonStdComment => {
            builder.push(token, SemanticTokenTypeKind::Comment);
        }
        LuaTokenKind::TkDocDetail => {
            let rendering_description = token
                .parent()
                .is_some_and(|parent| parent.kind() == LuaSyntaxKind::DocDescription.into());
            let description_parsing_is_enabled = emmyrc.semantic_tokens.render_documentation_markup;
            if !(rendering_description && description_parsing_is_enabled) {
                builder.push(token, SemanticTokenTypeKind::Comment);
            }
        }
        LuaTokenKind::TkDocQuestion | LuaTokenKind::TkDocOr | LuaTokenKind::TkDocAnd => {
            builder.push_with_modifier(
                token,
                SemanticTokenTypeKind::Operator,
                SemanticTokenModifierKind::DOCUMENTATION,
            );
        }
        LuaTokenKind::TkDocVisibility | LuaTokenKind::TkTagVisibility => {
            builder.push_with_modifier(
                token,
                SemanticTokenTypeKind::Keyword,
                SemanticTokenModifierKind::MODIFICATION | SemanticTokenModifierKind::DOCUMENTATION,
            );
        }
        LuaTokenKind::TkDocVersionNumber => {
            builder.push_with_modifier(
                token,
                SemanticTokenTypeKind::Number,
                SemanticTokenModifierKind::DOCUMENTATION,
            );
        }
        LuaTokenKind::TkStringTemplateType => {
            builder.push_with_modifier(
                token,
                SemanticTokenTypeKind::String,
                SemanticTokenModifierKind::DOCUMENTATION,
            );
        }
        LuaTokenKind::TkDocMatch => {
            builder.push_with_modifier(
                token,
                SemanticTokenTypeKind::Keyword,
                SemanticTokenModifierKind::DOCUMENTATION,
            );
        }
        LuaTokenKind::TKDocPath | LuaTokenKind::TkDocSeeContent => {
            builder.push_with_modifier(
                token,
                SemanticTokenTypeKind::String,
                SemanticTokenModifierKind::DOCUMENTATION,
            );
        }
        LuaTokenKind::TkDocRegion | LuaTokenKind::TkDocEndRegion => {
            builder.push(token, SemanticTokenTypeKind::Comment);
        }
        LuaTokenKind::TkDocStart | LuaTokenKind::TkDocContinue | LuaTokenKind::TkDocContinueOr => {
            render_doc_at(builder, token);
        }
        ch if ch.is_assign_op() => {
            builder.push(token, SemanticTokenTypeKind::Operator);
        }
        _ => {}
    }
}

fn render_doc_at(builder: &mut SemanticBuilder, token: &LuaSyntaxToken) {
    let text = token.text();
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
        TextRange::at(token.text_range().start(), TextSize::new(start as u32)),
        SemanticTokenTypeKind::Comment,
        None,
    );

    builder.push_at_range(
        TextRange::at(
            token.text_range().start() + TextSize::new(start as u32),
            TextSize::new(len as u32),
        ),
        SemanticTokenTypeKind::Keyword,
        Some(SemanticTokenModifierKind::DOCUMENTATION),
    );
}

fn is_doc_type_literal_token(token: &LuaSyntaxToken) -> bool {
    token
        .parent_ancestors()
        .any(|node| LuaDocType::cast(node).is_some())
}

fn build_node_semantic_token(
    model: &SalsaSemanticModel<'_>,
    builder: &mut SemanticBuilder,
    node: LuaSyntaxNode,
    emmyrc: &Emmyrc,
) -> Option<()> {
    let _ = emmyrc;
    match LuaAst::cast(node)? {
        LuaAst::LuaDocTagClass(doc_class) => {
            if let Some(name) = doc_class.get_name_token() {
                builder.push_with_modifier(
                    name.syntax(),
                    SemanticTokenTypeKind::Class,
                    SemanticTokenModifierKind::DECLARATION,
                );
            }
            if let Some(attribs) = doc_class.get_type_flag() {
                for token in attribs.tokens::<LuaGeneralToken>() {
                    builder.push(token.syntax(), SemanticTokenTypeKind::Decorator);
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
                SemanticTokenTypeKind::Enum,
                SemanticTokenModifierKind::DECLARATION,
            );
            if let Some(attribs) = doc_enum.get_type_flag() {
                for token in attribs.tokens::<LuaGeneralToken>() {
                    builder.push(token.syntax(), SemanticTokenTypeKind::Decorator);
                }
            }
        }
        LuaAst::LuaDocTagAlias(doc_alias) => {
            let name = doc_alias.get_name_token()?;
            builder.push_with_modifier(
                name.syntax(),
                SemanticTokenTypeKind::Type,
                SemanticTokenModifierKind::DECLARATION,
            );
            if let Some(generic_decl_list) = doc_alias.get_generic_decl_list() {
                render_type_parameter_list(builder, &generic_decl_list);
            }
            if let Some(alias_type) = doc_alias.get_type() {
                for mapped_key in alias_type
                    .syntax()
                    .descendants()
                    .filter_map(LuaDocMappedKey::cast)
                {
                    if let Some(type_decl) = mapped_key.child::<LuaDocGenericDecl>() {
                        render_type_parameter(builder, &type_decl);
                    }
                }
            }
        }
        LuaAst::LuaDocTagField(doc_field) => {
            if let Some(LuaDocFieldKey::Name(name)) = doc_field.get_field_key() {
                builder.push_with_modifier(
                    name.syntax(),
                    SemanticTokenTypeKind::Property,
                    SemanticTokenModifierKind::DECLARATION,
                );
            }
        }
        LuaAst::LuaDocTagParam(doc_param) => {
            let name = doc_param.get_name_token()?;
            builder.push_with_modifier(
                name.syntax(),
                SemanticTokenTypeKind::Parameter,
                SemanticTokenModifierKind::DECLARATION,
            );
        }
        LuaAst::LuaDocTagReturn(doc_return) => {
            for (_, name) in doc_return.get_info_list() {
                if let Some(name) = name {
                    builder.push(name.syntax(), SemanticTokenTypeKind::Variable);
                }
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
                SemanticTokenTypeKind::Namespace,
                SemanticTokenModifierKind::DECLARATION,
            );
        }
        LuaAst::LuaDocTagUsing(doc_using) => {
            let name = doc_using.get_name_token()?;
            builder.push(name.syntax(), SemanticTokenTypeKind::Namespace);
        }
        LuaAst::LuaDocTagLanguage(language) => {
            let name = language.get_name_token()?;
            builder.push(name.syntax(), SemanticTokenTypeKind::String);
            let language_text = name.get_name_text();
            if let Some(comment) = language.ancestors::<LuaComment>().next() {
                inject_language(builder, language_text, comment);
            }
        }
        LuaAst::LuaParamName(param_name) => {
            let name_token = param_name.get_name_token()?;
            if builder.contains_token(name_token.syntax()) {
                return Some(());
            }
            handle_name_node(model, builder, param_name.syntax(), name_token.syntax());
        }
        LuaAst::LuaLocalName(local_name) => {
            let name_token = local_name.get_name_token()?;
            if builder.contains_token(name_token.syntax()) {
                return Some(());
            }
            handle_name_node(model, builder, local_name.syntax(), name_token.syntax());
        }
        LuaAst::LuaNameExpr(name_expr) => {
            let name_token = name_expr.get_name_token()?;
            if builder.contains_token(name_token.syntax()) {
                return Some(());
            }
            handle_name_node(model, builder, name_expr.syntax(), name_token.syntax());
        }
        LuaAst::LuaForRangeStat(for_range_stat) => {
            for name in for_range_stat.get_var_name_list() {
                builder.push_with_modifier(
                    name.syntax(),
                    SemanticTokenTypeKind::Variable,
                    SemanticTokenModifierKind::DECLARATION,
                );
            }
        }
        LuaAst::LuaForStat(for_stat) => {
            let name = for_stat.get_var_name()?;
            builder.push_with_modifier(
                name.syntax(),
                SemanticTokenTypeKind::Variable,
                SemanticTokenModifierKind::DECLARATION,
            );
        }
        LuaAst::LuaLocalFuncStat(local_func_stat) => {
            let name = local_func_stat.get_local_name()?.get_name_token()?;
            builder.push_with_modifier(
                name.syntax(),
                SemanticTokenTypeKind::Function,
                SemanticTokenModifierKind::DECLARATION,
            );
        }
        LuaAst::LuaFuncStat(func_stat) => {
            let func_name = func_stat.get_func_name()?;
            match func_name {
                LuaVarExpr::NameExpr(name_expr) => {
                    let name = name_expr.get_name_token()?;
                    builder.push_with_modifier(
                        name.syntax(),
                        SemanticTokenTypeKind::Function,
                        SemanticTokenModifierKind::DECLARATION,
                    );
                }
                LuaVarExpr::IndexExpr(index_expr) => {
                    let name = index_expr.get_index_name_token()?;
                    builder.push_with_modifier(
                        &name,
                        SemanticTokenTypeKind::Method,
                        SemanticTokenModifierKind::DECLARATION,
                    );
                }
            }
        }
        LuaAst::LuaTableField(table_field) => {
            if let Some(emmylua_parser::LuaIndexKey::Name(key)) = table_field.get_field_key() {
                builder.push_with_modifier(
                    key.syntax(),
                    SemanticTokenTypeKind::Property,
                    SemanticTokenModifierKind::DECLARATION,
                );
            }
        }
        LuaAst::LuaIndexExpr(index_expr) => {
            if let Some(name_token) = index_expr.get_index_name_token()
                && name_token.kind() == LuaTokenKind::TkName.into()
            {
                if builder.contains_token(&name_token) {
                    return Some(());
                }
                handle_name_node(model, builder, index_expr.syntax(), &name_token);
            }
        }
        LuaAst::LuaDocTagAttributeUse(tag_use) => {
            if let Some(token) = tag_use.token_by_kind(LuaTokenKind::TkDocAttributeUse) {
                builder.push(token.syntax(), SemanticTokenTypeKind::Keyword);
            }
            if let Some(token) = tag_use.syntax().last_token() {
                builder.push(&token, SemanticTokenTypeKind::Keyword);
            }
            for attribute_use in tag_use.get_attribute_uses() {
                if let Some(token) = attribute_use.get_type()?.get_name_token() {
                    builder.push_with_modifier(
                        token.syntax(),
                        SemanticTokenTypeKind::Decorator,
                        SemanticTokenModifierKind::DECLARATION
                            | SemanticTokenModifierKind::DEFAULT_LIBRARY,
                    );
                }
            }
        }
        LuaAst::LuaDocInferType(infer_type) => {
            if let Some(gen_decl) = infer_type.get_generic_decl() {
                render_type_parameter(builder, &gen_decl);
            }
            if let Some(name) = infer_type.token::<LuaNameToken>() {
                if name.get_name_text() == "infer" {
                    builder.push(name.syntax(), SemanticTokenTypeKind::Comment);
                }
            }
        }
        LuaAst::LuaLiteralExpr(literal_expr) => {
            let call_expr = literal_expr
                .get_parent::<LuaCallArgList>()?
                .get_parent::<LuaCallExpr>()?;
            let literal_token = literal_expr.get_literal()?;
            if let LuaLiteralToken::String(string_token) = literal_token
                && !builder.is_special_string_range(&string_token.get_range())
            {
                fun_string_highlight(builder, model, call_expr, &string_token);
            }
        }
        LuaAst::LuaDocNameType(name_type) => {
            if let Some(name) = name_type.get_name_token() {
                let modifiers = if is_primitive_type_name(name.get_name_text()) {
                    SemanticTokenModifierKind::DEFAULT_LIBRARY
                } else {
                    SemanticTokenModifierKind::empty()
                };
                builder.push_with_modifier(name.syntax(), SemanticTokenTypeKind::Type, modifiers);
            }
        }
        _ => {}
    }
    Some(())
}

fn handle_name_node(
    model: &SalsaSemanticModel<'_>,
    builder: &mut SemanticBuilder,
    node: &LuaSyntaxNode,
    name_token: &LuaSyntaxToken,
) -> Option<()> {
    let name_text = name_token.text();

    if name_text == "self" {
        builder.push_with_modifier(
            name_token,
            SemanticTokenTypeKind::Variable,
            SemanticTokenModifierKind::DEFINITION,
        );
        return Some(());
    }

    // Member access on a require module: `m.foo`'s `foo` is rendered as a method.
    if let Some(index_expr) = LuaIndexExpr::cast(node.clone()) {
        if let Some(prefix) = index_expr.get_prefix_expr()
            && let LuaExpr::NameExpr(prefix_name) = prefix
            && let Some(prefix_token) = prefix_name.get_name_token()
            && is_require_alias_name(model, prefix_token.syntax())
        {
            builder.push(name_token, SemanticTokenTypeKind::Method);
            return Some(());
        }
    }

    // require alias: `local m = require("mod")` is treated as a module namespace.
    if is_require_alias_name(model, name_token) {
        if LuaIndexExpr::cast(node.clone()).is_some() {
            // Member name of `m.foo`: require module members are rendered as methods.
            builder.push(name_token, SemanticTokenTypeKind::Method);
            return Some(());
        }
        let is_index_prefix = node.parent().and_then(LuaIndexExpr::cast).is_some();
        if is_index_prefix {
            builder.push(name_token, SemanticTokenTypeKind::Namespace);
        } else {
            builder.push_with_modifier(
                name_token,
                SemanticTokenTypeKind::Class,
                SemanticTokenModifierKind::READONLY,
            );
        }
        return Some(());
    }

    // When no declaration is found, mark common Lua built-in globals as default library.
    if model.find_decl(NodeOrToken::Node(node.clone())).is_none() && is_builtin_global(name_text) {
        builder.push_with_modifier(
            name_token,
            SemanticTokenTypeKind::Function,
            SemanticTokenModifierKind::DEFAULT_LIBRARY | SemanticTokenModifierKind::READONLY,
        );
        return Some(());
    }

    if let Some(info) = model.semantic_info(NodeOrToken::Token(name_token.clone())) {
        if matches!(info.typ, LuaType::ModuleRef(_)) {
            // A require alias's module prefix is shown as namespace in member access (`m.foo`);
            // when it appears alone, it is shown as class/readonly.
            let is_index_prefix = node.parent().and_then(LuaIndexExpr::cast).is_some();
            if is_index_prefix {
                builder.push(name_token, SemanticTokenTypeKind::Namespace);
            } else {
                builder.push_with_modifier(
                    name_token,
                    SemanticTokenTypeKind::Class,
                    SemanticTokenModifierKind::READONLY,
                );
            }
            return Some(());
        }
        let (token_type, modifiers) = classify_semantic_info(model, &info, name_token);
        builder.push_with_modifier(name_token, token_type, modifiers);
        return Some(());
    }

    builder.push(name_token, default_identifier_token_type(name_text));
    Some(())
}

fn classify_semantic_info(
    model: &SalsaSemanticModel<'_>,
    info: &SemanticInfo,
    name_token: &LuaSyntaxToken,
) -> (SemanticTokenTypeKind, SemanticTokenModifierKind) {
    let mut modifiers = SemanticTokenModifierKind::empty();

    let token_type = match &info.decl {
        Some(SemanticId::Decl(key)) => {
            let Some(facts) = model.file_facts_of(key.file_id) else {
                return (default_identifier_token_type(name_token.text()), modifiers);
            };
            let Some(decl) = facts.decl_by_id(&info.decl.clone().unwrap()) else {
                return (default_identifier_token_type(name_token.text()), modifiers);
            };

            if decl.name_range == name_token.text_range() {
                modifiers |= SemanticTokenModifierKind::DECLARATION;
            }
            if decl.readonly {
                modifiers |= SemanticTokenModifierKind::READONLY;
            }
            if decl.deprecated {
                modifiers |= SemanticTokenModifierKind::DEPRECATED;
            }

            match decl.kind {
                DeclKind::Param => SemanticTokenTypeKind::Parameter,
                DeclKind::Global => {
                    if decl.name_range == name_token.text_range() {
                        modifiers |= SemanticTokenModifierKind::STATIC;
                    }
                    if info.typ.is_function() {
                        SemanticTokenTypeKind::Function
                    } else {
                        SemanticTokenTypeKind::Variable
                    }
                }
                DeclKind::Local { .. } => {
                    if info.typ.is_function() {
                        SemanticTokenTypeKind::Function
                    } else {
                        SemanticTokenTypeKind::Variable
                    }
                }
            }
        }
        Some(SemanticId::Member(key)) => {
            let Some(facts) = model.file_facts_of(key.file_id) else {
                return (SemanticTokenTypeKind::Property, modifiers);
            };
            let Some(member) = facts.member_by_id(&info.decl.clone().unwrap()) else {
                return (SemanticTokenTypeKind::Property, modifiers);
            };

            if name_token.text_range() == key.key_range {
                modifiers |= SemanticTokenModifierKind::DECLARATION;
            }
            if member.readonly {
                modifiers |= SemanticTokenModifierKind::READONLY;
            }
            if member.deprecated {
                modifiers |= SemanticTokenModifierKind::DEPRECATED;
            }

            // Standard library function members like `table.insert` / `string.rep` are defined with a dot,
            // so `is_method` is false, but the semantic token should still be colored as function/method.
            if member.is_method || info.typ.is_function() {
                SemanticTokenTypeKind::Method
            } else {
                SemanticTokenTypeKind::Property
            }
        }
        Some(SemanticId::TypeDef(key)) => {
            let file_id = match key.scope {
                TypeScope::File(file_id) => Some(file_id),
                _ => None,
            };
            let facts = file_id
                .and_then(|fid| model.file_facts_of(fid))
                .or_else(|| model.file_facts());
            let Some(facts) = facts else {
                return (SemanticTokenTypeKind::Type, modifiers);
            };
            let Some(def) = facts.type_def_by_id(&info.decl.clone().unwrap()) else {
                return (SemanticTokenTypeKind::Type, modifiers);
            };
            if def.deprecated {
                modifiers |= SemanticTokenModifierKind::DEPRECATED;
            }
            match def.kind {
                TypeDefKind::Class => SemanticTokenTypeKind::Class,
                TypeDefKind::Enum => SemanticTokenTypeKind::Enum,
                TypeDefKind::Alias => SemanticTokenTypeKind::Type,
            }
        }
        _ => default_identifier_token_type(name_token.text()),
    };

    (token_type, modifiers)
}

fn default_identifier_token_type(name_text: &str) -> SemanticTokenTypeKind {
    if name_text.chars().next().is_some_and(|c| c.is_uppercase()) {
        SemanticTokenTypeKind::Class
    } else {
        SemanticTokenTypeKind::Variable
    }
}

fn is_builtin_global(name_text: &str) -> bool {
    matches!(
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
}

fn is_require_alias_name(model: &SalsaSemanticModel<'_>, name_token: &LuaSyntaxToken) -> bool {
    let decl = model
        .resolve_name(name_token.text_range().start())
        .or_else(|| model.decl_by_offset(name_token.text_range().start()));
    let Some(decl) = decl else {
        return false;
    };
    let SemanticId::Decl(ref key) = decl else {
        return false;
    };
    let Some(facts) = model.file_facts_of(key.file_id) else {
        return false;
    };
    let Some(decl_info) = facts.decl_by_id(&decl) else {
        return false;
    };
    let Some(syntax) = decl_info.value_expr_syntax else {
        return false;
    };
    let Some(tree) = model.syntax_tree_of(key.file_id) else {
        return false;
    };
    let Some(node) = syntax.to_node_from_root(&tree.get_red_root()) else {
        return false;
    };
    let Some(call) = LuaCallExpr::cast(node) else {
        return false;
    };
    let Some(prefix) = call.get_prefix_expr() else {
        return false;
    };
    let LuaExpr::NameExpr(name) = prefix else {
        return false;
    };
    name.get_name_text().as_deref() == Some("require")
}

fn is_primitive_type_name(name: &str) -> bool {
    matches!(
        name,
        "string"
            | "integer"
            | "number"
            | "boolean"
            | "table"
            | "function"
            | "thread"
            | "userdata"
            | "nil"
            | "any"
            | "unknown"
            | "never"
            | "self"
            | "true"
            | "false"
    )
}

fn render_type_parameter_list(builder: &mut SemanticBuilder, generic_list: &LuaDocGenericDeclList) {
    for generic in generic_list.get_generic_decl() {
        render_type_parameter(builder, &generic);
    }
}

fn render_type_parameter(builder: &mut SemanticBuilder, type_decl: &LuaDocGenericDecl) {
    if let Some(name) = type_decl.get_name_token() {
        let modifiers = if is_primitive_type_name(name.get_name_text()) {
            SemanticTokenModifierKind::DECLARATION | SemanticTokenModifierKind::DEFAULT_LIBRARY
        } else {
            SemanticTokenModifierKind::DECLARATION
        };
        builder.push_with_modifier(name.syntax(), SemanticTokenTypeKind::Type, modifiers);
    }
}
