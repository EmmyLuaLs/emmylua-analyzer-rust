use crate::{LuaExportScope, ModuleInfo, SemanticModel};

/// 检查模块是否可见.
///
/// 如果没有 export 标记, 视为可见.
pub fn check_export_visibility(
    semantic_model: &SemanticModel,
    module_info: &ModuleInfo,
) -> Option<bool> {
    // 检查模块是否有 export 标记
    let Some(export) = module_info.get_export(semantic_model.get_db()) else {
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
                    if using_namespace == module_namespace
                        || using_namespace.starts_with(&format!("{}.", module_namespace))
                    {
                        return Some(true);
                    }
                }
            }
            let file_namespace = type_index.get_file_namespace(&semantic_model.get_file_id())?;
            if file_namespace == module_namespace
                || file_namespace.starts_with(&format!("{}.", module_namespace))
            {
                return Some(true);
            }
        }
        _ => {
            return Some(true);
        }
    }

    Some(false)
}
