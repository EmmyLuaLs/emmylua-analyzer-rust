use super::*;
use std::sync::Arc;

#[salsa::tracked]
pub(crate) fn tracked_file_use_site_summary(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaUseSiteIndexSummary> {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    let decl_tree = tracked_file_decl_tree_summary(db, file, config);
    Arc::new(analyze_use_site_summary(&decl_tree, parsed.value))
}

#[salsa::tracked]
pub(crate) fn tracked_file_lexical_name_resolution(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_offset: TextSize,
) -> Option<SalsaNameUseSummary> {
    let use_sites = tracked_file_use_site_summary(db, file, config);
    find_name_use_at(use_sites.as_ref(), syntax_offset)
}

#[salsa::tracked]
pub(crate) fn tracked_file_lexical_name_resolution_by_syntax_id(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaNameUseSummary> {
    let lexical_uses = tracked_file_lexical_use_index(db, file, config);
    find_name_use_by_syntax_id(lexical_uses.as_ref(), syntax_id)
}

#[salsa::tracked]
pub(crate) fn tracked_file_lexical_use_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaLexicalUseIndex> {
    let use_sites = tracked_file_use_site_summary(db, file, config);
    Arc::new(build_lexical_use_index(use_sites.as_ref()))
}

#[salsa::tracked]
pub(crate) fn tracked_file_lexical_use(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_offset: TextSize,
) -> Option<SalsaLexicalUseSummary> {
    let lexical_uses = tracked_file_lexical_use_index(db, file, config);
    find_lexical_use_at(lexical_uses.as_ref(), syntax_offset)
}

#[salsa::tracked]
pub(crate) fn tracked_file_lexical_member_resolution(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_offset: TextSize,
) -> Option<SalsaMemberUseSummary> {
    let use_sites = tracked_file_use_site_summary(db, file, config);
    find_member_use_at(use_sites.as_ref(), syntax_offset)
}

#[salsa::tracked]
pub(crate) fn tracked_file_lexical_member_resolution_by_syntax_id(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaMemberUseSummary> {
    let lexical_uses = tracked_file_lexical_use_index(db, file, config);
    find_member_use_by_syntax_id(lexical_uses.as_ref(), syntax_id)
}

#[salsa::tracked]
pub(crate) fn tracked_file_lexical_call_use(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_offset: TextSize,
) -> Option<SalsaCallUseSummary> {
    let lexical_uses = tracked_file_lexical_use_index(db, file, config);
    find_call_use_in_index(lexical_uses.as_ref(), syntax_offset)
}

#[salsa::tracked]
pub(crate) fn tracked_file_lexical_call_use_by_syntax_id(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaCallUseSummary> {
    let lexical_uses = tracked_file_lexical_use_index(db, file, config);
    find_call_use_by_syntax_id(lexical_uses.as_ref(), syntax_id)
}

#[salsa::tracked]
pub(crate) fn tracked_file_decl_type_query_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaDeclTypeQueryIndex> {
    let decl_tree = tracked_file_decl_tree_summary(db, file, config);
    let doc = tracked_file_doc_summary(db, file, config);
    let signatures = tracked_file_signature_summary(db, file, config);
    let owner_resolves = tracked_file_doc_owner_resolve_index(db, file, config);
    Arc::new(build_decl_type_query_index(
        decl_tree.as_ref(),
        doc.as_ref(),
        signatures.as_ref(),
        owner_resolves.as_ref(),
    ))
}

#[salsa::tracked]
pub(crate) fn tracked_file_decl_type_info(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    decl_id: SalsaDeclId,
) -> Option<SalsaDeclTypeInfoSummary> {
    let index = tracked_file_decl_type_query_index(db, file, config);
    find_decl_type_info(index.as_ref(), decl_id)
}

#[salsa::tracked]
pub(crate) fn tracked_file_name_type_info(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_offset: TextSize,
) -> Option<SalsaNameTypeInfoSummary> {
    let name_use = tracked_file_lexical_name_resolution(db, file, config, syntax_offset)?;
    let index = tracked_file_decl_type_query_index(db, file, config);
    Some(find_name_type_info(index.as_ref(), &name_use))
}

#[salsa::tracked]
pub(crate) fn tracked_file_local_assignment_query_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaLocalAssignmentQueryIndex> {
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    let decl_tree = tracked_file_decl_tree_summary(db, file, config);
    Arc::new(build_local_assignment_query_index(
        decl_tree.as_ref(),
        parsed.value,
    ))
}

#[salsa::tracked]
pub(crate) fn tracked_file_global_type_query_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaGlobalTypeQueryIndex> {
    let globals = tracked_file_global_summary(db, file, config);
    let decl_types = tracked_file_decl_type_query_index(db, file, config);
    Arc::new(build_global_type_query_index(
        globals.as_ref(),
        decl_types.as_ref(),
    ))
}

#[salsa::tracked]
pub(crate) fn tracked_file_global_type_info(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    name: SmolStr,
) -> Option<SalsaGlobalTypeInfoSummary> {
    let index = tracked_file_global_type_query_index(db, file, config);
    find_global_type_info(index.as_ref(), &name)
}

#[salsa::tracked]
pub(crate) fn tracked_file_global_name_type_info(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_offset: TextSize,
) -> Option<SalsaGlobalTypeInfoSummary> {
    let name_use = tracked_file_lexical_name_resolution(db, file, config, syntax_offset)?;
    let index = tracked_file_global_type_query_index(db, file, config);
    find_global_name_type_info(index.as_ref(), &name_use)
}

#[salsa::tracked]
pub(crate) fn tracked_file_member_type_query_index(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
) -> Arc<SalsaMemberTypeQueryIndex> {
    let members = tracked_file_member_summary(db, file, config);
    let properties = tracked_file_property_summary(db, file, config);
    let signatures = tracked_file_signature_summary(db, file, config);
    let doc = tracked_file_doc_summary(db, file, config);
    let owner_resolves = tracked_file_doc_owner_resolve_index(db, file, config);
    Arc::new(build_member_type_query_index(
        members.as_ref(),
        properties.as_ref(),
        signatures.as_ref(),
        doc.as_ref(),
        owner_resolves.as_ref(),
    ))
}

#[salsa::tracked]
pub(crate) fn tracked_file_member_type_info(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    member_target: SalsaMemberTargetId,
) -> Option<SalsaMemberTypeInfoSummary> {
    let index = tracked_file_member_type_query_index(db, file, config);
    find_member_type_info(index.as_ref(), &member_target)
}

#[salsa::tracked]
pub(crate) fn tracked_file_member_use_type_info(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_offset: TextSize,
) -> Option<SalsaMemberTypeInfoSummary> {
    let member_use = tracked_file_lexical_member_resolution(db, file, config, syntax_offset)?;
    let index = tracked_file_member_type_query_index(db, file, config);
    find_member_use_type_info(index.as_ref(), &member_use)
}

#[salsa::tracked]
pub(crate) fn tracked_file_member_type_at_program_point(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_offset: TextSize,
    program_point_offset: TextSize,
) -> Option<SalsaProgramPointMemberTypeInfoSummary> {
    let member_use = tracked_file_lexical_member_resolution(db, file, config, syntax_offset)?;
    let member_index = tracked_file_member_type_query_index(db, file, config);
    let property_index = tracked_file_property_summary(db, file, config);
    let decl_index = tracked_file_decl_type_query_index(db, file, config);
    let doc = tracked_file_doc_summary(db, file, config);
    let doc_types = tracked_file_doc_type_summary(db, file, config);
    let doc_tags = tracked_file_doc_tag_query_index(db, file, config);
    let assignments = tracked_file_local_assignment_query_index(db, file, config);
    let lowered_types = tracked_file_doc_type_lowered_index(db, file, config);
    let signature_explain_index = tracked_file_signature_explain_index(db, file, config);
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    let chunk = parsed.value.clone();
    let decl_tree = tracked_file_decl_tree_summary(db, file, config);
    Some(find_member_type_at_program_point(
        member_index.as_ref(),
        property_index.as_ref(),
        decl_index.as_ref(),
        doc.as_ref(),
        doc_types.as_ref(),
        &member_use,
        program_point_offset,
        assignments.as_ref(),
        decl_tree.as_ref(),
        &chunk,
        lowered_types.as_ref(),
        signature_explain_index.as_ref(),
        doc_tags.as_ref(),
    ))
}

#[salsa::tracked]
pub(crate) fn tracked_file_name_type_at_program_point(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    syntax_offset: TextSize,
    program_point_offset: TextSize,
) -> Option<SalsaProgramPointTypeInfoSummary> {
    let name_use = tracked_file_lexical_name_resolution(db, file, config, syntax_offset)?;
    let index = tracked_file_decl_type_query_index(db, file, config);
    let assignments = tracked_file_local_assignment_query_index(db, file, config);
    let properties = tracked_file_property_summary(db, file, config);
    let doc_types = tracked_file_doc_type_summary(db, file, config);
    let doc_tags = tracked_file_doc_tag_query_index(db, file, config);
    let lowered_types = tracked_file_doc_type_lowered_index(db, file, config);
    let signature_explain_index = tracked_file_signature_explain_index(db, file, config);
    let parsed = parse_chunk(file.file_id(db), &file.text(db), &config.config(db));
    let chunk = parsed.value.clone();
    let decl_tree = tracked_file_decl_tree_summary(db, file, config);
    let active_narrows = match name_use.resolution {
        SalsaNameUseResolutionSummary::LocalDecl(decl_id) => {
            collect_active_type_narrows(&decl_tree, chunk.clone(), decl_id, program_point_offset)
        }
        SalsaNameUseResolutionSummary::Global => Vec::new(),
    };
    Some(find_name_type_at_program_point(
        index.as_ref(),
        &name_use,
        program_point_offset,
        assignments.as_ref(),
        properties.as_ref(),
        doc_types.as_ref(),
        signature_explain_index.as_ref(),
        doc_tags.as_ref(),
        decl_tree.as_ref(),
        &chunk,
        lowered_types.as_ref(),
        &active_narrows,
    ))
}

#[salsa::tracked]
pub(crate) fn tracked_file_lexical_decl_references(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    decl_id: SalsaDeclId,
) -> Vec<SalsaNameUseSummary> {
    let lexical_uses = tracked_file_lexical_use_index(db, file, config);
    collect_decl_references_in_index(lexical_uses.as_ref(), decl_id)
}

#[salsa::tracked]
pub(crate) fn tracked_file_lexical_call_references_for_name(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    callee_name: SmolStr,
) -> Vec<SalsaCallUseSummary> {
    let lexical_uses = tracked_file_lexical_use_index(db, file, config);
    collect_call_references_for_name_in_index(lexical_uses.as_ref(), callee_name.as_str())
}

#[salsa::tracked]
pub(crate) fn tracked_file_lexical_global_name_references(
    db: &dyn SummaryDb,
    file: SummarySourceFileInput,
    config: SummaryConfigInput,
    name: SmolStr,
) -> Vec<SalsaNameUseSummary> {
    let lexical_uses = tracked_file_lexical_use_index(db, file, config);
    collect_global_name_references_in_index(lexical_uses.as_ref(), name.as_str())
}

pub(crate) fn file_use_site_summary(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaUseSiteIndexSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_use_site_summary(db, file, config))
}

pub(crate) fn file_lexical_name_resolution(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_offset: TextSize,
) -> Option<SalsaNameUseSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_lexical_name_resolution(db, file, config, syntax_offset)
}

pub(crate) fn file_lexical_name_resolution_by_syntax_id(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaNameUseSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_lexical_name_resolution_by_syntax_id(db, file, config, syntax_id)
}

pub(crate) fn file_lexical_use_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaLexicalUseIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_lexical_use_index(db, file, config))
}

pub(crate) fn file_lexical_use(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_offset: TextSize,
) -> Option<SalsaLexicalUseSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_lexical_use(db, file, config, syntax_offset)
}

pub(crate) fn file_lexical_member_resolution(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_offset: TextSize,
) -> Option<SalsaMemberUseSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_lexical_member_resolution(db, file, config, syntax_offset)
}

pub(crate) fn file_lexical_member_resolution_by_syntax_id(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaMemberUseSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_lexical_member_resolution_by_syntax_id(db, file, config, syntax_id)
}

pub(crate) fn file_lexical_call_use(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_offset: TextSize,
) -> Option<SalsaCallUseSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_lexical_call_use(db, file, config, syntax_offset)
}

pub(crate) fn file_lexical_call_use_by_syntax_id(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaCallUseSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_lexical_call_use_by_syntax_id(db, file, config, syntax_id)
}

pub(crate) fn file_decl_type_query_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaDeclTypeQueryIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_decl_type_query_index(db, file, config))
}

pub(crate) fn file_decl_type_info(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    decl_id: SalsaDeclId,
) -> Option<SalsaDeclTypeInfoSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_decl_type_info(db, file, config, decl_id)
}

pub(crate) fn file_name_type_info(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_offset: TextSize,
) -> Option<SalsaNameTypeInfoSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_name_type_info(db, file, config, syntax_offset)
}

pub(crate) fn file_global_type_query_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaGlobalTypeQueryIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_global_type_query_index(db, file, config))
}

pub(crate) fn file_global_type_info(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    name: SmolStr,
) -> Option<SalsaGlobalTypeInfoSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_global_type_info(db, file, config, name)
}

pub(crate) fn file_global_name_type_info(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_offset: TextSize,
) -> Option<SalsaGlobalTypeInfoSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_global_name_type_info(db, file, config, syntax_offset)
}

pub(crate) fn file_member_type_query_index(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
) -> Option<Arc<SalsaMemberTypeQueryIndex>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_member_type_query_index(db, file, config))
}

pub(crate) fn file_member_type_info(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    member_target: SalsaMemberTargetId,
) -> Option<SalsaMemberTypeInfoSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_member_type_info(db, file, config, member_target)
}

pub(crate) fn file_member_use_type_info(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_offset: TextSize,
) -> Option<SalsaMemberTypeInfoSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_member_use_type_info(db, file, config, syntax_offset)
}

pub(crate) fn file_member_type_at_program_point(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_offset: TextSize,
    program_point_offset: TextSize,
) -> Option<SalsaProgramPointMemberTypeInfoSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_member_type_at_program_point(db, file, config, syntax_offset, program_point_offset)
}

pub(crate) fn file_name_type_at_program_point(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    syntax_offset: TextSize,
    program_point_offset: TextSize,
) -> Option<SalsaProgramPointTypeInfoSummary> {
    let (file, config) = file_and_config(db, file_id)?;
    tracked_file_name_type_at_program_point(db, file, config, syntax_offset, program_point_offset)
}

pub(crate) fn file_lexical_decl_references(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    decl_id: SalsaDeclId,
) -> Option<Vec<SalsaNameUseSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_lexical_decl_references(
        db, file, config, decl_id,
    ))
}

pub(crate) fn file_lexical_member_references(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    member_target: SalsaMemberTargetId,
) -> Option<Vec<SalsaMemberUseSummary>> {
    let lexical_uses = file_lexical_use_index(db, file_id)?;
    Some(collect_member_references_in_index(
        &lexical_uses,
        &member_target,
    ))
}

pub(crate) fn file_lexical_call_references_for_name(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    callee_name: SmolStr,
) -> Option<Vec<SalsaCallUseSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_lexical_call_references_for_name(
        db,
        file,
        config,
        callee_name,
    ))
}

pub(crate) fn file_lexical_global_name_references(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    name: SmolStr,
) -> Option<Vec<SalsaNameUseSummary>> {
    let (file, config) = file_and_config(db, file_id)?;
    Some(tracked_file_lexical_global_name_references(
        db, file, config, name,
    ))
}

pub(crate) fn file_lexical_name_references_by_role(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    role: SalsaUseSiteRoleSummary,
) -> Option<Vec<SalsaNameUseSummary>> {
    let lexical_uses = file_lexical_use_index(db, file_id)?;
    Some(collect_name_references_by_role_in_index(
        &lexical_uses,
        &role,
    ))
}

pub(crate) fn file_lexical_member_references_by_role(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    role: SalsaUseSiteRoleSummary,
) -> Option<Vec<SalsaMemberUseSummary>> {
    let lexical_uses = file_lexical_use_index(db, file_id)?;
    Some(collect_member_references_by_role_in_index(
        &lexical_uses,
        &role,
    ))
}

pub(crate) fn file_lexical_call_references_for_member(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    member_target: SalsaMemberTargetId,
) -> Option<Vec<SalsaCallUseSummary>> {
    let lexical_uses = file_lexical_use_index(db, file_id)?;
    Some(collect_call_references_for_member_in_index(
        &lexical_uses,
        &member_target,
    ))
}
