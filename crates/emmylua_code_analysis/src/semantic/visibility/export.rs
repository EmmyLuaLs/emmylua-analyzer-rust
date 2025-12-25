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
        return check_default_export_visibility(semantic_model, module_info);
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
        LuaExportScope::Global => {
            return Some(true);
        }
        LuaExportScope::Default => {
            return check_default_export_visibility(semantic_model, module_info);
        }
    }

    Some(false)
}

/// 检查默认导出作用域下的可见性
fn check_default_export_visibility(
    semantic_model: &SemanticModel,
    module_info: &ModuleInfo,
) -> Option<bool> {
    // 如果没有启用 require_export_global, 则默认认为是可见的.
    if !semantic_model.emmyrc.strict.require_export_global {
        return Some(true);
    }

    // 如果被声明为库文件, 则我们不认为是可见的.
    if semantic_model
        .db
        .get_module_index()
        .is_library(&module_info.file_id)
    {
        return Some(false);
    }
    Some(true)
}
