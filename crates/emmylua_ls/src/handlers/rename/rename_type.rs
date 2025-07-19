use std::collections::HashMap;

use emmylua_code_analysis::{LuaTypeDeclId, SemanticModel};
use lsp_types::Uri;

pub fn rename_type_references(
    semantic_model: &SemanticModel,
    type_decl_id: LuaTypeDeclId,
    new_name: String,
    result: &mut HashMap<Uri, HashMap<lsp_types::Range, String>>,
) -> Option<()> {
    let type_index = semantic_model.get_db().get_type_index();
    let type_decl = type_index.get_type_decl(&type_decl_id)?;
    let full_name = type_decl.get_full_name();
    let mut reserved_namespace = String::new();

    // 取出`full_name`在当前文件中使用的命名空间
    if let Some(file_namespace) = type_index.get_file_namespace(&semantic_model.get_file_id()) {
        if full_name.starts_with(&format!("{}.", file_namespace)) {
            reserved_namespace = file_namespace.clone();
        }
    }
    if reserved_namespace.is_empty() {
        if let Some(using_namespaces) =
            type_index.get_file_using_namespace(&semantic_model.get_file_id())
        {
            for using_namespace in using_namespaces {
                if full_name.starts_with(&format!("{}.", using_namespace)) {
                    reserved_namespace = using_namespace.clone();
                    break;
                }
            }
        }
    }

    let locations = type_decl.get_locations();
    for decl_location in locations {
        let document = semantic_model.get_document_by_file_id(decl_location.file_id)?;
        let range = document.to_lsp_range(decl_location.range)?;
        result
            .entry(document.get_uri())
            .or_insert_with(HashMap::new)
            .insert(range, new_name.clone());
    }

    let refs = semantic_model
        .get_db()
        .get_reference_index()
        .get_type_references(&type_decl_id)?;
    let mut document_cache = HashMap::new();
    for in_filed_reference_range in refs {
        let document = if let Some(document) = document_cache.get(&in_filed_reference_range.file_id)
        {
            document
        } else {
            let document =
                semantic_model.get_document_by_file_id(in_filed_reference_range.file_id)?;
            document_cache.insert(in_filed_reference_range.file_id, document);
            document_cache.get(&in_filed_reference_range.file_id)?
        };

        // 根据引用文件的命名空间上下文决定使用简名还是全名
        let actual_new_name = determine_type_name_for_file(
            semantic_model,
            in_filed_reference_range.file_id,
            &reserved_namespace,
            &new_name,
        );

        let location = document.to_lsp_location(in_filed_reference_range.value)?;
        result
            .entry(location.uri)
            .or_insert_with(HashMap::new)
            .insert(location.range, actual_new_name);
    }

    Some(())
}

/// 根据引用文件的命名空间上下文决定使用简名还是全名
fn determine_type_name_for_file(
    semantic_model: &SemanticModel,
    reference_file_id: emmylua_code_analysis::FileId,
    reserved_namespace: &str,
    new_simple_name: &str,
) -> String {
    let type_index = semantic_model.get_db().get_type_index();

    // 检查引用文件是否声明了相同的命名空间
    if let Some(file_namespace) = type_index.get_file_namespace(&reference_file_id) {
        if reserved_namespace.starts_with(&format!("{}.", file_namespace)) {
            return new_simple_name.to_string();
        }
    }

    // 检查引用文件是否使用了相应的命名空间
    if let Some(using_namespaces) = type_index.get_file_using_namespace(&reference_file_id) {
        for using_namespace in using_namespaces {
            if reserved_namespace.starts_with(&format!("{}.", using_namespace)) {
                return new_simple_name.to_string();
            }
        }
    }

    // `reserved_namespace`不为空，则使用保留的命名空间与新名称组合
    if !reserved_namespace.is_empty() {
        return format!("{}.{}", reserved_namespace, new_simple_name);
    }

    new_simple_name.to_string()
}
