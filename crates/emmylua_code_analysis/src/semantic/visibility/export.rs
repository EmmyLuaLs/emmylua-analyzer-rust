use crate::{LuaExport, LuaExportScope, ModuleInfo, SemanticModel};

/// 检查模块是否可见, 如果没有 export 标记, 视为可见
pub fn check_export_visibility(
    semantic_model: &SemanticModel,
    module_info: &ModuleInfo,
) -> Option<bool> {
    // 检查模块是否有 export 标记
    let Some(export) = get_export(semantic_model, module_info) else {
        return Some(true);
    };

    match export.scope {
        LuaExportScope::Namespace => {
            let type_index = semantic_model.get_db().get_type_index();
            let module_namespace = type_index.get_file_namespace(&module_info.file_id)?;

            if let Some(using_namespaces) =
                type_index.get_file_using_namespace(&semantic_model.get_file_id())
            {
                for using_namespace in using_namespaces {
                    if using_namespace == module_namespace {
                        return Some(true);
                    }
                }
            }
            if type_index.get_file_namespace(&semantic_model.get_file_id())? == module_namespace {
                return Some(true);
            }
        }
        _ => {
            return Some(true);
        }
    }

    Some(false)
}

fn get_export<'a>(
    semantic_model: &'a SemanticModel,
    module_info: &ModuleInfo,
) -> Option<&'a LuaExport> {
    // 检查模块是否有 export 标记
    let property_owner_id = module_info.property_owner_id.clone()?;
    let export = semantic_model
        .get_db()
        .get_property_index()
        .get_property(&property_owner_id)?
        .export
        .as_ref()?;

    Some(export)
}
