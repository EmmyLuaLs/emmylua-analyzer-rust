use super::*;
use std::sync::Arc;

#[salsa::tracked]
pub(crate) fn tracked_file_doc_type_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaDocTypeIndexSummary> {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    Arc::new(analyze_doc_type_summary(parsed.value))
}

#[salsa::tracked]
pub(crate) fn tracked_file_doc_type_lowered_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaDocTypeLoweredIndex> {
    let doc_types = tracked_file_doc_type_summary(db, file, config);
    Arc::new(build_lowered_doc_type_index(doc_types.as_ref()))
}

#[salsa::tracked]
pub(crate) fn tracked_file_doc_type_lowered_at(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_offset: TextSize,
) -> Option<SalsaDocTypeLoweredNode> {
    let lowered_index = tracked_file_doc_type_lowered_index(db, file, config);
    find_lowered_doc_type_at(lowered_index.as_ref(), syntax_offset)
}

#[salsa::tracked]
pub(crate) fn tracked_file_doc_type_lowered_by_key(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    type_key: SalsaDocTypeNodeKey,
) -> Option<SalsaDocTypeLoweredNode> {
    let lowered_index = tracked_file_doc_type_lowered_index(db, file, config);
    find_lowered_doc_type_by_key(lowered_index.as_ref(), type_key)
}

#[salsa::tracked]
pub(crate) fn tracked_file_doc_type_resolved_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaDocTypeResolvedIndex> {
    let doc_types = tracked_file_doc_type_summary(db, file, config);
    let lowered_types = tracked_file_doc_type_lowered_index(db, file, config);
    Arc::new(build_resolved_doc_type_index(
        doc_types.as_ref(),
        lowered_types.as_ref(),
    ))
}

#[salsa::tracked]
pub(crate) fn tracked_file_doc_type_resolved_at(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_offset: TextSize,
) -> Option<SalsaDocTypeResolvedSummary> {
    let resolved_index = tracked_file_doc_type_resolved_index(db, file, config);
    find_resolved_doc_type_at(resolved_index.as_ref(), syntax_offset)
}

#[salsa::tracked]
pub(crate) fn tracked_file_doc_type_resolved_by_key(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    type_key: SalsaDocTypeNodeKey,
) -> Option<SalsaDocTypeResolvedSummary> {
    let resolved_index = tracked_file_doc_type_resolved_index(db, file, config);
    find_resolved_doc_type_by_key(resolved_index.as_ref(), type_key)
}

#[salsa::tracked]
pub(crate) fn tracked_file_doc_tag_query_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaDocTagQueryIndex> {
    let doc = tracked_file_doc_summary(db, file, config);
    Arc::new(build_doc_tag_query_index(doc.as_ref()))
}

#[salsa::tracked]
pub(crate) fn tracked_file_doc_tag_at(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_offset: TextSize,
) -> Option<SalsaDocTagSummary> {
    let index = tracked_file_doc_tag_query_index(db, file, config);
    find_doc_tag_at_in_index(index.as_ref(), syntax_offset)
}

#[salsa::tracked]
pub(crate) fn tracked_file_doc_tag_properties(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Vec<SalsaDocTagPropertySummary> {
    tracked_file_doc_tag_query_index(db, file, config)
        .properties
        .clone()
}

pub(crate) fn tracked_file_resolved_doc_tag_diagnostics(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    owner: SalsaDocOwnerSummary,
) -> Option<Vec<crate::SalsaResolvedDocDiagnosticActionSummary>> {
    let query_index = tracked_file_doc_tag_query_index(db, file, config);
    let property = query_index
        .properties
        .iter()
        .find(|property| property.owner == owner)?;
    let text = file.text(db);
    let parsed = parse_chunk(file.file_id(db), &text, &config.config(db));
    Some(collect_resolved_doc_tag_diagnostics_for_property(
        property,
        &parsed.value,
        &text,
    ))
}

#[salsa::tracked]
pub(crate) fn tracked_file_signature_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaSignatureIndexSummary> {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    let doc = tracked_file_doc_summary(db, file, config);
    Arc::new(analyze_signature_summary(parsed.value, doc.as_ref()))
}

#[salsa::tracked]
pub(crate) fn tracked_file_signature_explain_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaSignatureExplainIndex> {
    let signatures = tracked_file_signature_summary(db, file, config);
    let owner_resolves = tracked_file_doc_owner_resolve_index(db, file, config);
    let lowered_types = tracked_file_doc_type_lowered_index(db, file, config);
    let lexical_uses = tracked_file_lexical_use_index(db, file, config);
    let doc = tracked_file_doc_summary(db, file, config);
    let doc_tag_query_index = tracked_file_doc_tag_query_index(db, file, config);
    let tag_properties = tracked_file_doc_tag_properties(db, file, config);
    Arc::new(build_signature_explain_index(
        signatures.as_ref(),
        owner_resolves.as_ref(),
        doc_tag_query_index.as_ref(),
        &tag_properties,
        lowered_types.as_ref(),
        lexical_uses.as_ref(),
        &doc.generics,
        &doc.params,
        &doc.returns,
        &doc.operators,
    ))
}

#[salsa::tracked]
pub(crate) fn tracked_file_signature_explain(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    signature_offset: TextSize,
) -> Option<SalsaSignatureExplainSummary> {
    let index = tracked_file_signature_explain_index(db, file, config);
    find_signature_explain_at(index.as_ref(), signature_offset)
}

#[salsa::tracked]
pub(crate) fn tracked_file_call_explain(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    call_offset: TextSize,
) -> Option<SalsaCallExplainSummary> {
    let index = tracked_file_signature_explain_index(db, file, config);
    find_call_explain_at(index.as_ref(), call_offset)
}

#[salsa::tracked]
pub(crate) fn tracked_file_signature_return_query_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaSignatureReturnQueryIndex> {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    let signatures = tracked_file_signature_summary(db, file, config);
    let signature_explain_index = tracked_file_signature_explain_index(db, file, config);
    let use_sites = tracked_file_use_site_summary(db, file, config);
    let decl_index = tracked_file_decl_type_query_index(db, file, config);
    let member_index = tracked_file_member_type_query_index(db, file, config);
    let assignments = tracked_file_local_assignment_query_index(db, file, config);
    let property_index = tracked_file_property_summary(db, file, config);
    let doc = tracked_file_doc_summary(db, file, config);
    let doc_types = tracked_file_doc_type_summary(db, file, config);
    let doc_tag_query_index = tracked_file_doc_tag_query_index(db, file, config);
    let decl_tree = tracked_file_decl_tree_summary(db, file, config);
    let lowered_types = tracked_file_doc_type_lowered_index(db, file, config);

    Arc::new(build_signature_return_query_index(
        signatures.as_ref(),
        signature_explain_index.as_ref(),
        use_sites.as_ref(),
        decl_index.as_ref(),
        member_index.as_ref(),
        assignments.as_ref(),
        property_index.as_ref(),
        doc.as_ref(),
        doc_types.as_ref(),
        doc_tag_query_index.as_ref(),
        decl_tree.as_ref(),
        &parsed.value,
        lowered_types.as_ref(),
    ))
}

#[salsa::tracked]
pub(crate) fn tracked_file_signature_return_query(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    signature_offset: TextSize,
) -> Option<SalsaSignatureReturnQuerySummary> {
    if let Some(summary) =
        tracked_file_semantic_signature_return_summary(db, file, config, signature_offset)
    {
        return Some(project_signature_return_query_from_semantic_summary(
            &summary,
        ));
    }

    let index = tracked_file_signature_return_query_index(db, file, config);
    find_signature_return_query_at(index.as_ref(), signature_offset)
}

#[salsa::tracked]
pub(crate) fn tracked_file_doc_owner_binding_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaDocOwnerBindingIndexSummary> {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    let decl_tree = tracked_file_decl_tree_summary(db, file, config);
    let members = tracked_file_member_summary(db, file, config);
    let properties = tracked_file_property_summary(db, file, config);
    let signatures = tracked_file_signature_summary(db, file, config);
    Arc::new(analyze_doc_owner_binding_summary(
        decl_tree.as_ref(),
        members.as_ref(),
        properties.as_ref(),
        signatures.as_ref(),
        parsed.value,
    ))
}

#[salsa::tracked]
pub(crate) fn tracked_file_doc_owner_resolve_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaDocOwnerResolveIndex> {
    let owner_bindings = tracked_file_doc_owner_binding_summary(db, file, config);
    Arc::new(build_doc_owner_resolve_index(owner_bindings.as_ref()))
}

pub(crate) fn file_doc_type_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaDocTypeIndexSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_doc_type_summary(db, file, config))
}

pub(crate) fn file_doc_type_lowered_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaDocTypeLoweredIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_doc_type_lowered_index(db, file, config))
}

pub(crate) fn file_doc_type_lowered_at(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_offset: TextSize,
) -> Option<SalsaDocTypeLoweredNode> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_doc_type_lowered_at(db, file, config, syntax_offset)
}

pub(crate) fn file_doc_type_resolved_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaDocTypeResolvedIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_doc_type_resolved_index(db, file, config))
}

pub(crate) fn file_doc_type_resolved_at(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_offset: TextSize,
) -> Option<SalsaDocTypeResolvedSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_doc_type_resolved_at(db, file, config, syntax_offset)
}

pub(crate) fn file_doc_tags(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Vec<SalsaDocTagSummary>> {
    let doc = file_doc_summary(db, file_id)?;
    Some(doc.tags.clone())
}

pub(crate) fn file_doc_tag_at(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_offset: TextSize,
) -> Option<SalsaDocTagSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_doc_tag_at(db, file, config, syntax_offset)
}

pub(crate) fn file_doc_tags_for_kind(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    kind: SalsaDocTagKindSummary,
) -> Option<Vec<SalsaDocTagSummary>> {
    let index = file_doc_tag_query_index(db, file_id)?;
    Some(collect_doc_tags_for_kind_in_index(&index, &kind))
}

pub(crate) fn file_doc_tags_for_owner(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    owner: SalsaDocOwnerSummary,
) -> Option<Vec<SalsaDocTagSummary>> {
    let index = file_doc_tag_query_index(db, file_id)?;
    Some(collect_doc_tags_for_owner_in_index(&index, &owner))
}

pub(crate) fn file_doc_tag_properties(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Vec<SalsaDocTagPropertySummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_doc_tag_properties(db, file, config))
}

pub(crate) fn file_doc_tag_property(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    owner: SalsaDocOwnerSummary,
) -> Option<SalsaDocTagPropertySummary> {
    let index = file_doc_tag_query_index(db, file_id)?;
    find_doc_tag_property_in_index(&index, &owner)
}

pub(crate) fn file_resolved_doc_tag_diagnostics(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    owner: SalsaDocOwnerSummary,
) -> Option<Vec<crate::SalsaResolvedDocDiagnosticActionSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_resolved_doc_tag_diagnostics(db, file, config, owner)
}

pub(crate) fn file_signature_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaSignatureIndexSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_signature_summary(db, file, config))
}

pub(crate) fn file_signature_explain_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaSignatureExplainIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_signature_explain_index(db, file, config))
}

pub(crate) fn file_signature_explain(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    signature_offset: TextSize,
) -> Option<SalsaSignatureExplainSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_signature_explain(db, file, config, signature_offset)
}

pub(crate) fn file_call_explain(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    call_offset: TextSize,
) -> Option<SalsaCallExplainSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_call_explain(db, file, config, call_offset)
}

pub(crate) fn file_signature_return_query_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaSignatureReturnQueryIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_signature_return_query_index(db, file, config))
}

pub(crate) fn file_signature_return_query(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    signature_offset: TextSize,
) -> Option<SalsaSignatureReturnQuerySummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_signature_return_query(db, file, config, signature_offset)
}

pub(crate) fn file_doc_owner_binding_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaDocOwnerBindingIndexSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_doc_owner_binding_summary(db, file, config))
}

pub(crate) fn file_doc_type_lowered_by_key(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    type_key: SalsaDocTypeNodeKey,
) -> Option<SalsaDocTypeLoweredNode> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_doc_type_lowered_by_key(db, file, config, type_key)
}

pub(crate) fn file_doc_type_resolved_by_key(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    type_key: SalsaDocTypeNodeKey,
) -> Option<SalsaDocTypeResolvedSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_doc_type_resolved_by_key(db, file, config, type_key)
}

pub(crate) fn file_doc_owner_resolve_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaDocOwnerResolveIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_doc_owner_resolve_index(db, file, config))
}
