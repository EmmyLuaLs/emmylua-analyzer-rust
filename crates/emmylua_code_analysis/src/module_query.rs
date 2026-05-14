use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaLiteralToken};

use crate::{
    DbIndex, FileId, LuaCompilation, LuaDeclId, LuaSemanticDeclId, LuaSignatureId, LuaType,
    compilation::{CompilationModuleIndex, CompilationModuleInfo, analyze_module_return_points},
};

pub(crate) mod identity {
    use super::*;

    pub(crate) fn find_compilation_module<'a>(
        modules: &'a CompilationModuleIndex,
        module_path: &str,
    ) -> Option<&'a CompilationModuleInfo> {
        modules.find_module(module_path)
    }

    pub(crate) fn find_compilation_module_by_file_id(
        compilation: &LuaCompilation,
        file_id: FileId,
    ) -> Option<&CompilationModuleInfo> {
        compilation.find_module_by_file_id(file_id)
    }

    pub(crate) fn find_compilation_module_by_path<'a>(
        compilation: &'a LuaCompilation,
        module_path: &str,
    ) -> Option<&'a CompilationModuleInfo> {
        compilation.find_module_by_require_path(module_path)
    }

    pub(crate) fn find_db_module_by_file_id(
        db: &DbIndex,
        file_id: FileId,
    ) -> Option<CompilationModuleInfo> {
        crate::compilation::find_compilation_module_by_file_id(db, file_id)
    }

    pub(crate) fn find_db_module_by_path(
        db: &DbIndex,
        module_path: &str,
    ) -> Option<CompilationModuleInfo> {
        crate::compilation::find_compilation_module_by_require_path(db, module_path)
    }

    pub(crate) fn find_db_module_file_id(db: &DbIndex, module_path: &str) -> Option<FileId> {
        find_db_module_by_path(db, module_path).map(|module| module.file_id)
    }

    pub(crate) fn db_module_is_meta_file(db: &DbIndex, file_id: FileId) -> bool {
        find_db_module_by_file_id(db, file_id)
            .map(|module| module.is_meta)
            .unwrap_or(false)
    }

    pub(crate) fn find_require_call_module_file_id(
        db: &DbIndex,
        call_expr: LuaCallExpr,
    ) -> Option<FileId> {
        let module_path = require_call_module_path(call_expr)?;
        find_db_module_file_id(db, &module_path)
    }

    pub(crate) fn require_call_module_path(call_expr: LuaCallExpr) -> Option<String> {
        let first_arg = call_expr.get_args_list()?.get_args().next()?;
        match first_arg {
            LuaExpr::LiteralExpr(literal_expr) => match literal_expr.get_literal()? {
                LuaLiteralToken::String(string_token) => Some(string_token.get_value()),
                _ => None,
            },
            _ => None,
        }
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
            if let Ok(export_type) = crate::infer_expr_root(db, &mut cache, export_expr) {
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

    pub(crate) fn semantic_id_from_compilation_module(
        module: &CompilationModuleInfo,
    ) -> Option<LuaSemanticDeclId> {
        match module.semantic_target.as_ref()? {
            crate::compilation::SalsaSemanticTargetSummary::Decl(decl_id) => Some(
                LuaSemanticDeclId::LuaDecl(LuaDeclId::new(module.file_id, decl_id.as_position())),
            ),
            crate::compilation::SalsaSemanticTargetSummary::Signature(signature_offset) => {
                Some(LuaSemanticDeclId::Signature(LuaSignatureId::new(
                    module.file_id,
                    *signature_offset,
                )))
            }
            crate::compilation::SalsaSemanticTargetSummary::Member(_) => None,
        }
    }

    pub(crate) fn compilation_module_has_export_type(
        compilation: &LuaCompilation,
        file_id: FileId,
    ) -> bool {
        super::identity::find_compilation_module_by_file_id(compilation, file_id)
            .is_some_and(CompilationModuleInfo::has_export_type)
    }

    pub(crate) fn compilation_module_semantic_id(
        compilation: &LuaCompilation,
        file_id: FileId,
    ) -> Option<LuaSemanticDeclId> {
        let module = super::identity::find_compilation_module_by_file_id(compilation, file_id)?;
        semantic_id_from_compilation_module(module).or_else(|| {
            match module_export_expr(compilation.legacy_db(), file_id)? {
                LuaExpr::IndexExpr(index_expr) => Some(LuaSemanticDeclId::Member(
                    crate::LuaMemberId::new(index_expr.get_syntax_id(), file_id),
                )),
                _ => None,
            }
        })
    }
}
