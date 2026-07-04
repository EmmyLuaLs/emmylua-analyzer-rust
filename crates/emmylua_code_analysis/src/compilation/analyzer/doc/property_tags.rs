use crate::{
    AsyncState, LuaDeclId, LuaMemberId, LuaNoDiscard, LuaSemanticDeclId, LuaSignatureId,
    PropertyDeclFeature, compilation::analyzer::doc::tags::report_orphan_tag,
};

use super::{
    DocAnalyzer,
    tags::{find_owner_closure_or_report, get_owner_id, get_owner_id_or_report},
};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaDocDescriptionOwner, LuaDocTag, LuaDocTagAsync, LuaDocTagDeprecated,
    LuaDocTagField, LuaDocTagNodiscard, LuaDocTagReadonly, LuaDocTagSource, LuaDocTagVersion,
    LuaDocTagVisibility, LuaExpr, LuaKind, LuaSyntaxKind, LuaTokenKind,
};

pub fn analyze_visibility(
    analyzer: &mut DocAnalyzer,
    visibility: LuaDocTagVisibility,
) -> Option<()> {
    let Some(owner) = analyzer.comment.get_owner() else {
        report_orphan_tag(analyzer, &visibility);
        return None;
    };
    let owner_id = match owner {
        LuaAst::LuaReturnStat(return_stat) => {
            let expr = return_stat.child::<LuaExpr>()?;
            match expr {
                // 返回变量不能附加可见性
                // LuaExpr::NameExpr(name_expr) => {
                //     let name = name_expr.get_name_text()?;
                //     let tree = analyzer
                //         .db
                //         .get_decl_index()
                //         .get_decl_tree(&analyzer.file_id)?;
                //     let decl = tree.find_local_decl(&name, name_expr.get_position())?;

                //     Some(LuaSemanticDeclId::LuaDecl(decl.get_id()))
                // }
                LuaExpr::ClosureExpr(closure) => Some(LuaSemanticDeclId::Signature(
                    LuaSignatureId::from_closure(analyzer.file_id, &closure),
                )),
                LuaExpr::TableExpr(table_expr) => Some(LuaSemanticDeclId::LuaDecl(LuaDeclId::new(
                    analyzer.file_id,
                    table_expr.get_position(),
                ))),
                _ => None,
            }?
        }
        _ => get_owner_id_or_report(analyzer, &visibility)?,
    };

    let visibility_kind = visibility.get_visibility_token()?.get_visibility()?;

    analyzer
        .type_context
        .db
        .get_property_index_mut()
        .add_visibility(analyzer.file_id, owner_id, visibility_kind);

    Some(())
}

pub fn analyze_source(analyzer: &mut DocAnalyzer, source: LuaDocTagSource) -> Option<()> {
    let path = source.get_path_token()?.get_path().to_string();
    let owner_id = get_owner_id_or_report(analyzer, &source)?;

    analyzer
        .type_context
        .db
        .get_property_index_mut()
        .add_source(analyzer.file_id, owner_id, path);

    Some(())
}

pub fn analyze_nodiscard(analyzer: &mut DocAnalyzer, nodiscard: LuaDocTagNodiscard) -> Option<()> {
    let closure = find_owner_closure_or_report(analyzer, &nodiscard)?;
    let signature_id = LuaSignatureId::from_closure(analyzer.file_id, &closure);
    let signature = analyzer
        .type_context
        .db
        .get_signature_index_mut()
        .get_mut(&signature_id)?;

    let message = if let Some(desc) = nodiscard.get_description() {
        let message_text = desc.get_description_text().to_string();
        if message_text.is_empty() {
            None
        } else {
            Some(message_text)
        }
    } else {
        None
    };

    signature.nodiscard = match message {
        Some(message) => Some(LuaNoDiscard::NoDiscardWithMessage(Box::new(message))),
        None => Some(LuaNoDiscard::NoDiscard),
    };

    Some(())
}

pub fn analyze_deprecated(analyzer: &mut DocAnalyzer, tag: LuaDocTagDeprecated) -> Option<()> {
    let message = get_deprecated_message(&tag);

    if let Some(field_tag) = find_following_field_tag(&tag) {
        let field_owner_id = LuaSemanticDeclId::Member(LuaMemberId::new(
            field_tag.get_syntax_id(),
            analyzer.file_id,
        ));
        add_deprecated(analyzer, field_owner_id, message)?;
        return Some(());
    }

    let type_owner_id = if let Some(current_type_id) = analyzer.current_type_id.clone() {
        Some(LuaSemanticDeclId::TypeDecl(current_type_id))
    } else {
        let file_id = analyzer.file_id;
        let workspace_id = analyzer.workspace_id;
        let tags = analyzer.comment.get_doc_tags();
        let type_index = analyzer.get_db().get_type_index();

        tags.filter_map(|tag| match tag {
            LuaDocTag::Class(class) => class.get_name_token().and_then(|name_token| {
                type_index
                    .find_type_decl(file_id, name_token.get_name_text(), Some(workspace_id))
                    .filter(|decl| decl.is_class())
                    .map(|decl| LuaSemanticDeclId::TypeDecl(decl.get_id()))
            }),
            LuaDocTag::Alias(alias) => alias.get_name_token().and_then(|name_token| {
                type_index
                    .find_type_decl(file_id, name_token.get_name_text(), Some(workspace_id))
                    .filter(|decl| decl.is_alias())
                    .map(|decl| LuaSemanticDeclId::TypeDecl(decl.get_id()))
            }),
            _ => None,
        })
        .next()
    };

    if let Some(type_owner_id) = type_owner_id {
        add_deprecated(analyzer, type_owner_id, message.clone())?;
        if let Some(owner @ (LuaSemanticDeclId::LuaDecl(_) | LuaSemanticDeclId::Member(_))) =
            get_owner_id(analyzer, None, true)
        {
            add_deprecated(analyzer, owner, message)?;
        }
        return Some(());
    }

    let owner_id = get_owner_id_or_report(analyzer, &tag)?;
    add_deprecated(analyzer, owner_id, message)?;

    Some(())
}

fn get_deprecated_message(tag: &LuaDocTagDeprecated) -> Option<String> {
    let description = tag.get_description()?.get_description_text();
    let message = description.lines().next()?.trim_end();
    if message.is_empty() {
        None
    } else {
        Some(message.to_string())
    }
}

fn find_following_field_tag(tag: &LuaDocTagDeprecated) -> Option<LuaDocTagField> {
    let mut next_sibling = tag.syntax().next_sibling_or_token();
    while let Some(sibling) = next_sibling {
        match sibling.kind() {
            LuaKind::Token(
                LuaTokenKind::TkWhitespace
                | LuaTokenKind::TkEndOfLine
                | LuaTokenKind::TkDocStart
                | LuaTokenKind::TkDocContinue,
            ) => {}
            LuaKind::Syntax(kind) if LuaDocTagField::can_cast(kind) => {
                return LuaDocTagField::cast(sibling.into_node()?);
            }
            LuaKind::Syntax(LuaSyntaxKind::DocDescription) => {}
            _ => return None,
        }

        next_sibling = sibling.next_sibling_or_token();
    }
    None
}

fn add_deprecated(
    analyzer: &mut DocAnalyzer,
    owner_id: LuaSemanticDeclId,
    message: Option<String>,
) -> Option<()> {
    analyzer
        .type_context
        .db
        .get_property_index_mut()
        .add_deprecated(analyzer.file_id, owner_id, message)
}

pub fn analyze_version(analyzer: &mut DocAnalyzer, version: LuaDocTagVersion) -> Option<()> {
    let owner_id = get_owner_id_or_report(analyzer, &version)?;

    let mut version_set = Vec::new();
    for version in version.get_version_list() {
        if let Some(version_condition) = version.get_version_condition() {
            version_set.push(version_condition);
        }
    }

    analyzer
        .type_context
        .db
        .get_property_index_mut()
        .add_version(analyzer.file_id, owner_id, version_set);

    Some(())
}

pub fn analyze_async(analyzer: &mut DocAnalyzer, tag: LuaDocTagAsync) -> Option<()> {
    let closure = find_owner_closure_or_report(analyzer, &tag)?;
    let signature_id = LuaSignatureId::from_closure(analyzer.file_id, &closure);
    let signature = analyzer
        .type_context
        .db
        .get_signature_index_mut()
        .get_mut(&signature_id)?;

    signature.async_state = AsyncState::Async;

    Some(())
}

pub fn analyze_readonly(analyzer: &mut DocAnalyzer, readonly: LuaDocTagReadonly) -> Option<()> {
    let owner_id = get_owner_id_or_report(analyzer, &readonly)?;

    analyzer
        .type_context
        .db
        .get_property_index_mut()
        .add_decl_feature(analyzer.file_id, owner_id, PropertyDeclFeature::ReadOnly);

    Some(())
}
