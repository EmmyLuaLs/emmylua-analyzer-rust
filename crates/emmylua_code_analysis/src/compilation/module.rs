use crate::{
    DbIndex, FileId, InFiled, LuaDeclId, LuaType, ModuleVisibility, SalsaDocVisibilityKindSummary,
    SalsaExportTargetSummary, SalsaModuleExportQuerySummary, SalsaModuleExportSummary,
    SalsaSemanticTargetSummary, WorkspaceId,
};
use emmylua_parser::{LuaAstNode, LuaClosureExpr, LuaTableExpr};
use rowan::{TextRange, TextSize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilationModuleInfo {
    pub file_id: FileId,
    pub full_module_name: String,
    pub name: String,
    pub visible: ModuleVisibility,
    pub workspace_id: WorkspaceId,
    pub is_meta: bool,
    pub export_target: Option<SalsaExportTargetSummary>,
    pub export: Option<SalsaModuleExportSummary>,
    pub semantic_target: Option<SalsaSemanticTargetSummary>,
}

impl CompilationModuleInfo {
    pub fn has_export_type(&self) -> bool {
        self.export.is_some() || self.semantic_target.is_some()
    }
}

pub(crate) fn project_module_info(db: &DbIndex, file_id: FileId) -> Option<CompilationModuleInfo> {
    let (full_module_name, workspace_id) = project_module_path(db, file_id)?;
    let name = full_module_name
        .rsplit('.')
        .next()
        .unwrap_or(full_module_name.as_str())
        .to_string();
    let summary_db = db.get_summary_db();
    let export_query = (*summary_db).semantic().file().module_export_query(file_id);
    let (visible, is_meta, export_target, export, semantic_target) =
        project_module_export_metadata(export_query.as_ref());

    Some(CompilationModuleInfo {
        file_id,
        full_module_name,
        name,
        visible,
        workspace_id,
        is_meta,
        export_target,
        export,
        semantic_target,
    })
}

pub(crate) fn find_module_by_require_path(
    db: &DbIndex,
    module_path: &str,
) -> Option<CompilationModuleInfo> {
    let normalized = module_path.replace(['\\', '/'], ".");
    let mut exact_match = None;
    let mut fuzzy_match: Option<(usize, CompilationModuleInfo)> = None;

    for file_id in db.get_vfs().get_all_file_ids() {
        let info = project_module_info(db, file_id)?;
        if info.full_module_name == normalized {
            exact_match = Some(info);
            break;
        }

        if let Some(prefix) = info
            .full_module_name
            .strip_suffix(&format!(".{}", normalized))
        {
            let leading_segments = prefix
                .split('.')
                .filter(|segment| !segment.is_empty())
                .count();
            match &fuzzy_match {
                Some((best_segments, best_info)) => {
                    if leading_segments < *best_segments
                        || (leading_segments == *best_segments
                            && info.full_module_name < best_info.full_module_name)
                    {
                        fuzzy_match = Some((leading_segments, info));
                    }
                }
                None => fuzzy_match = Some((leading_segments, info)),
            }
        }
    }

    exact_match.or_else(|| fuzzy_match.map(|(_, info)| info))
}

pub fn resolve_projected_module_export_type(db: &DbIndex, file_id: FileId) -> Option<LuaType> {
    let module_info = project_module_info(db, file_id)?;

    match module_info.semantic_target {
        Some(SalsaSemanticTargetSummary::Decl(decl_id)) => db
            .get_type_index()
            .get_type_cache(&LuaDeclId::new(file_id, decl_id.as_position()).into())
            .map(|type_cache| type_cache.as_type().clone()),
        Some(SalsaSemanticTargetSummary::Signature(signature_offset)) => {
            let closure = find_closure_by_offset(db, file_id, signature_offset)?;
            Some(LuaType::Signature(crate::LuaSignatureId::from_closure(
                file_id, &closure,
            )))
        }
        Some(SalsaSemanticTargetSummary::Member(_)) => None,
        None => resolve_projected_export_summary_type(db, file_id, module_info.export),
    }
}

fn project_module_path(db: &DbIndex, file_id: FileId) -> Option<(String, WorkspaceId)> {
    let path = db.get_vfs().get_file_path(&file_id)?;
    let path_str = path.to_str()?;
    let (module_path, workspace_id) = db.get_module_index().extract_module_path(path_str)?;

    Some((module_path.replace(['\\', '/'], "."), workspace_id))
}

fn resolve_projected_export_summary_type(
    db: &DbIndex,
    file_id: FileId,
    export: Option<SalsaModuleExportSummary>,
) -> Option<LuaType> {
    match export? {
        SalsaModuleExportSummary::Table { table_offset } => {
            let table_range = find_table_expr_range_by_offset(db, file_id, table_offset)?;
            Some(LuaType::TableConst(InFiled::new(file_id, table_range)))
        }
        SalsaModuleExportSummary::Closure { signature_offset } => {
            let closure = find_closure_by_offset(db, file_id, signature_offset)?;
            Some(LuaType::Signature(crate::LuaSignatureId::from_closure(
                file_id, &closure,
            )))
        }
        SalsaModuleExportSummary::LocalDecl { decl_id, .. } => db
            .get_type_index()
            .get_type_cache(&LuaDeclId::new(file_id, decl_id.as_position()).into())
            .map(|type_cache| type_cache.as_type().clone()),
        SalsaModuleExportSummary::Member(_) => None,
        SalsaModuleExportSummary::GlobalFunction(_) => None,
        SalsaModuleExportSummary::GlobalVariable(_) => None,
    }
}

fn find_closure_by_offset(
    db: &DbIndex,
    file_id: FileId,
    offset: TextSize,
) -> Option<LuaClosureExpr> {
    let root = db.get_vfs().get_syntax_tree(&file_id)?.get_red_root();
    root.descendants()
        .filter_map(LuaClosureExpr::cast)
        .find(|closure| closure.get_position() == offset)
}

fn find_table_expr_range_by_offset(
    db: &DbIndex,
    file_id: FileId,
    offset: TextSize,
) -> Option<TextRange> {
    let root = db.get_vfs().get_syntax_tree(&file_id)?.get_red_root();
    root.descendants()
        .filter_map(LuaTableExpr::cast)
        .find(|table| table.get_position() == offset)
        .map(|table| table.get_range())
}

fn project_module_export_metadata(
    export_query: Option<&SalsaModuleExportQuerySummary>,
) -> (
    ModuleVisibility,
    bool,
    Option<SalsaExportTargetSummary>,
    Option<SalsaModuleExportSummary>,
    Option<SalsaSemanticTargetSummary>,
) {
    let Some(export_query) = export_query else {
        return (ModuleVisibility::Default, false, None, None, None);
    };

    let mut visible = ModuleVisibility::Default;
    let mut is_meta = false;
    for property in &export_query.tag_properties {
        if let Some(meta_name) = property.meta() {
            is_meta = true;
            if meta_name == "no-require" || meta_name == "_" {
                visible = visible.merge(ModuleVisibility::Hide);
            }
        }

        if let Some(visibility) = property.visibility() {
            visible = visible.merge(map_visibility(visibility));
        }
    }

    (
        visible,
        is_meta,
        Some(export_query.export_target.clone()),
        export_query.export.clone(),
        export_query.semantic_target.clone(),
    )
}

fn map_visibility(visibility: &SalsaDocVisibilityKindSummary) -> ModuleVisibility {
    match visibility {
        SalsaDocVisibilityKindSummary::Public => ModuleVisibility::Public,
        SalsaDocVisibilityKindSummary::Internal => ModuleVisibility::Internal,
        _ => ModuleVisibility::Default,
    }
}
