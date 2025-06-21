use std::collections::HashMap;

use emmylua_code_analysis::{humanize_type, LuaSignatureId, LuaType, RenderLevel, SemanticModel};
use emmylua_parser::{LuaAstNode, LuaClosureExpr};
use itertools::Itertools;
use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, InlayHintLabelPart, Location};

pub fn build_closure_hint(
    semantic_model: &SemanticModel,
    result: &mut Vec<InlayHint>,
    closure: LuaClosureExpr,
) -> Option<()> {
    if !semantic_model.get_emmyrc().hint.param_hint {
        return Some(());
    }
    let file_id = semantic_model.get_file_id();
    let signature_id = LuaSignatureId::from_closure(file_id, &closure);
    let signature = semantic_model
        .get_db()
        .get_signature_index()
        .get(&signature_id)?;

    let lua_params = closure.get_params_list()?;
    let signature_params = signature.get_type_params();
    let mut lua_params_map = HashMap::new();
    for param in lua_params.get_params() {
        if let Some(name_token) = param.get_name_token() {
            let name = name_token.get_name_text().to_string();
            lua_params_map.insert(name, param);
        } else if param.is_dots() {
            lua_params_map.insert("...".to_string(), param);
        }
    }

    let document = semantic_model.get_document();
    for (signature_param_name, typ) in &signature_params {
        if let Some(typ) = typ {
            if typ.is_any() {
                continue;
            }

            if let Some(lua_param) = lua_params_map.get(signature_param_name) {
                let lsp_range = document.to_lsp_range(lua_param.get_range())?;
                // 构造 label
                let mut label_parts = build_label_parts(semantic_model, &typ);
                // 为空时添加默认值
                if label_parts.is_empty() {
                    let typ_desc = format!(
                        ": {}",
                        humanize_type(semantic_model.get_db(), &typ, RenderLevel::Simple)
                    );
                    label_parts.push(InlayHintLabelPart {
                        value: typ_desc,
                        location: Some(
                            get_type_location(semantic_model, typ)
                                .unwrap_or(Location::new(document.get_uri(), lsp_range)),
                        ),
                        ..Default::default()
                    });
                }
                let hint = InlayHint {
                    kind: Some(InlayHintKind::TYPE),
                    label: InlayHintLabel::LabelParts(label_parts),
                    position: lsp_range.end,
                    text_edits: None,
                    tooltip: None,
                    padding_left: Some(true),
                    padding_right: None,
                    data: None,
                };
                result.push(hint);
            }
        }
    }

    Some(())
}

pub fn build_label_parts(semantic_model: &SemanticModel, typ: &LuaType) -> Vec<InlayHintLabelPart> {
    let mut parts: Vec<InlayHintLabelPart> = Vec::new();
    match typ {
        LuaType::Union(union) => {
            for typ in union.get_types() {
                if let Some(part) = get_part(semantic_model, typ) {
                    parts.push(part);
                }
            }
        }
        _ => {
            if let Some(part) = get_part(semantic_model, typ) {
                parts.push(part);
            }
        }
    }
    // 去重
    let parts: Vec<InlayHintLabelPart> = parts
        .into_iter()
        .unique_by(|part| part.value.clone())
        .collect();
    // 将 "?" 标签移到最后
    let mut normal_parts = Vec::new();
    let mut nil_parts = Vec::new();
    for part in parts {
        if part.value == "?" {
            nil_parts.push(part);
        } else {
            normal_parts.push(part);
        }
    }
    normal_parts.append(&mut nil_parts);
    let mut result = Vec::new();
    for (i, part) in normal_parts.into_iter().enumerate() {
        let mut part = part;
        if part.value != "?" {
            part.value = format!("{}{}", if i == 0 { ": " } else { "|" }, part.value);
        }
        result.push(part);
    }
    // 如果只有一个`nil`标签, 那么将其改为": nil"
    if result.len() == 1 && result[0].value == "?" {
        result[0].value = ": nil".to_string();
    }
    result
}

fn get_part(semantic_model: &SemanticModel, typ: &LuaType) -> Option<InlayHintLabelPart> {
    match typ {
        LuaType::Union(_) => None,
        LuaType::Nil => {
            return Some(InlayHintLabelPart {
                value: "?".to_string(),
                location: get_type_location(semantic_model, typ),
                ..Default::default()
            });
        }
        _ => {
            let value = humanize_type(semantic_model.get_db(), typ, RenderLevel::Simple);
            let location = get_type_location(semantic_model, typ);
            return Some(InlayHintLabelPart {
                value,
                location,
                ..Default::default()
            });
        }
    }
}

fn get_type_location(semantic_model: &SemanticModel, typ: &LuaType) -> Option<Location> {
    match typ {
        LuaType::Ref(id) | LuaType::Def(id) => {
            let type_decl = semantic_model
                .get_db()
                .get_type_index()
                .get_type_decl(&id)?;
            let location = type_decl.get_locations().first()?;
            let document = semantic_model.get_document_by_file_id(location.file_id)?;
            let lsp_range = document.to_lsp_range(location.range)?;
            Some(Location::new(document.get_uri(), lsp_range))
        }
        LuaType::Array(base) => get_type_location(semantic_model, base),
        LuaType::Any => get_base_type_location(semantic_model, "any"),
        LuaType::Nil => get_base_type_location(semantic_model, "nil"),
        LuaType::Unknown => get_base_type_location(semantic_model, "unknown"),
        LuaType::Userdata => get_base_type_location(semantic_model, "userdata"),
        LuaType::Function => get_base_type_location(semantic_model, "function"),
        LuaType::Thread => get_base_type_location(semantic_model, "thread"),
        LuaType::Table => get_base_type_location(semantic_model, "table"),
        _ if typ.is_string() => get_base_type_location(semantic_model, "string"),
        _ if typ.is_integer() => get_base_type_location(semantic_model, "integer"),
        _ if typ.is_number() => get_base_type_location(semantic_model, "number"),
        _ if typ.is_boolean() => get_base_type_location(semantic_model, "boolean"),
        _ => None,
    }
}

fn get_base_type_location(semantic_model: &SemanticModel, name: &str) -> Option<Location> {
    let type_decl = semantic_model
        .get_db()
        .get_type_index()
        .find_type_decl(semantic_model.get_file_id(), name)?;
    let location = type_decl.get_locations().first()?;
    let document = semantic_model.get_document_by_file_id(location.file_id)?;
    let lsp_range = document.to_lsp_range(location.range)?;
    Some(Location::new(document.get_uri(), lsp_range))
}
