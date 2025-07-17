use crate::{
    LuaDeclId, LuaExport, LuaExportScope, LuaNoDiscard, LuaSemanticDeclId, LuaSignatureId,
};

use super::{
    tags::{find_owner_closure_or_report, get_owner_id_or_report},
    DocAnalyzer,
};
use crate::compilation::analyzer::doc::tags::report_orphan_tag;
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaDocDescriptionOwner, LuaDocTagAsync, LuaDocTagDeprecated,
    LuaDocTagExport, LuaDocTagNodiscard, LuaDocTagSource, LuaDocTagVersion, LuaDocTagVisibility,
    LuaTableExpr,
};

pub fn analyze_visibility(
    analyzer: &mut DocAnalyzer,
    visibility: LuaDocTagVisibility,
) -> Option<()> {
    let visibility_kind = visibility.get_visibility_token()?.get_visibility();
    let owner_id = get_owner_id_or_report(analyzer, &visibility)?;

    analyzer.db.get_property_index_mut().add_visibility(
        analyzer.file_id,
        owner_id,
        visibility_kind,
    );

    Some(())
}

pub fn analyze_source(analyzer: &mut DocAnalyzer, source: LuaDocTagSource) -> Option<()> {
    let path = source.get_path_token()?.get_path().to_string();
    let owner_id = get_owner_id_or_report(analyzer, &source)?;

    analyzer
        .db
        .get_property_index_mut()
        .add_source(analyzer.file_id, owner_id, path);

    Some(())
}

pub fn analyze_nodiscard(analyzer: &mut DocAnalyzer, nodiscard: LuaDocTagNodiscard) -> Option<()> {
    let closure = find_owner_closure_or_report(analyzer, &nodiscard)?;
    let signature_id = LuaSignatureId::from_closure(analyzer.file_id, &closure);
    let signature = analyzer
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
    let message = if let Some(desc) = tag.get_description() {
        Some(desc.get_description_text().to_string())
    } else {
        None
    };
    let owner_id = get_owner_id_or_report(analyzer, &tag)?;

    analyzer
        .db
        .get_property_index_mut()
        .add_deprecated(analyzer.file_id, owner_id, message);

    Some(())
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
        .db
        .get_property_index_mut()
        .add_version(analyzer.file_id, owner_id, version_set);

    Some(())
}

pub fn analyze_async(analyzer: &mut DocAnalyzer, tag: LuaDocTagAsync) -> Option<()> {
    let closure = find_owner_closure_or_report(analyzer, &tag)?;
    let signature_id = LuaSignatureId::from_closure(analyzer.file_id, &closure);
    let signature = analyzer
        .db
        .get_signature_index_mut()
        .get_mut(&signature_id)?;

    signature.is_async = true;

    Some(())
}

pub fn analyze_export(analyzer: &mut DocAnalyzer, tag: LuaDocTagExport) -> Option<()> {
    let Some(owner) = analyzer.comment.get_owner() else {
        report_orphan_tag(analyzer, &tag);
        return None;
    };
    let owner_id = match owner {
        LuaAst::LuaReturnStat(return_stat) => {
            let return_table_expr = return_stat.child::<LuaTableExpr>()?;
            LuaSemanticDeclId::LuaDecl(LuaDeclId::new(
                analyzer.file_id,
                return_table_expr.get_position(),
            ))
        }
        _ => get_owner_id_or_report(analyzer, &tag)?,
    };

    let export_scope = if let Some(scope_text) = tag.get_export_scope() {
        match scope_text.as_str() {
            "namespace" => LuaExportScope::Namespace,
            "global" => LuaExportScope::Global,
            _ => LuaExportScope::Global, // 默认为 global
        }
    } else {
        LuaExportScope::Global // 没有参数时默认为 global
    };

    let export = LuaExport {
        scope: export_scope,
    };

    analyzer
        .db
        .get_property_index_mut()
        .add_export(analyzer.file_id, owner_id, export);

    Some(())
}
