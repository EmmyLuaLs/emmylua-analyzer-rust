use emmylua_parser::LuaExpr;

use crate::{
    DbIndex, FileId, LuaType,
    compilation::{CompilationModuleInfo, analyze_module_return_points},
};

pub(crate) mod identity {
    use super::*;

    pub(crate) fn find_db_module_file_id(db: &DbIndex, module_path: &str) -> Option<FileId> {
        crate::compilation::find_module_by_require_path(db, module_path).map(|module| module.file_id)
    }

    pub(crate) fn find_db_module_info(
        db: &DbIndex,
        module_path: &str,
    ) -> Option<CompilationModuleInfo> {
        crate::compilation::find_module_by_require_path(db, module_path)
    }
}

pub(crate) mod export {
    use super::*;

    fn fallback_module_export_type(db: &DbIndex, file_id: FileId) -> Option<LuaType> {
        let export_type = crate::resolve_projected_module_export_type(db, file_id)?;

        Some(match export_type {
            LuaType::Def(id) => LuaType::Ref(id),
            other => other.get_result_slot_type(0).unwrap_or(other),
        })
    }

    pub(crate) fn module_export_expr(db: &DbIndex, file_id: FileId) -> Option<LuaExpr> {
        let tree = db.get_vfs().get_syntax_tree(&file_id)?;
        let chunk = tree.get_chunk_node();
        analyze_module_return_points(chunk.get_block()?)
            .into_iter()
            .next()
    }

    pub(crate) fn infer_module_export_type(db: &DbIndex, file_id: FileId) -> Option<LuaType> {
        if let Some(export_expr) = module_export_expr(db, file_id) {
            if matches!(export_expr, LuaExpr::NameExpr(_)) {
                if let Some(export_type) = fallback_module_export_type(db, file_id) {
                    return Some(export_type);
                }
            }

            let mut cache = crate::LuaInferCache::new(file_id, Default::default());
            if let Ok(export_type) = crate::infer_expr(db, &mut cache, export_expr) {
                Some(match export_type {
                    LuaType::Def(id) => LuaType::Ref(id),
                    other => other.get_result_slot_type(0).unwrap_or(other),
                })
            } else {
                fallback_module_export_type(db, file_id)
            }
        } else {
            fallback_module_export_type(db, file_id)
        }
    }
}
