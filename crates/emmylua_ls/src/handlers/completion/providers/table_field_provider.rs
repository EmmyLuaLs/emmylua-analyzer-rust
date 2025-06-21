use std::collections::HashSet;

use emmylua_code_analysis::{get_real_type, LuaMemberInfo, LuaMemberKey, LuaType};
use emmylua_parser::{LuaAst, LuaAstNode, LuaKind, LuaTableExpr, LuaTableField, LuaTokenKind};
use lsp_types::{CompletionItem, InsertTextFormat, InsertTextMode};
use rowan::NodeOrToken;

use crate::handlers::completion::{
    add_completions::{check_visibility, is_deprecated},
    completion_builder::CompletionBuilder,
    completion_data::CompletionData,
};

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    add_table_field_key_completion(builder);
    add_table_field_value_completion(builder);

    Some(())
}

fn add_table_field_key_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if !can_add_key_completion(builder) {
        return None;
    }
    // 出现以下情况则代表是补全 value
    let prev_token = builder.trigger_token.prev_token()?;
    if builder.trigger_token.kind() == LuaKind::Token(LuaTokenKind::TkWhitespace)
        && prev_token.kind() == LuaKind::Token(LuaTokenKind::TkAssign)
    {
        return None;
    }

    let node = LuaAst::cast(builder.trigger_token.parent()?)?;
    let table_expr = match node {
        LuaAst::LuaTableExpr(table_expr) => Some(table_expr),
        LuaAst::LuaNameExpr(name_expr) => name_expr
            .get_parent::<LuaTableField>()?
            .get_parent::<LuaTableExpr>(),
        _ => None,
    }?;

    let table_type = builder
        .semantic_model
        .infer_table_should_be(table_expr.clone())?;
    let member_infos = builder.semantic_model.get_member_infos(&table_type)?;

    let mut duplicated_set = HashSet::new();
    for field in table_expr.get_fields() {
        let key = field.get_field_key();
        if let Some(key) = key {
            duplicated_set.insert(key.get_path_part());
        }
    }

    for member_info in member_infos {
        if duplicated_set.contains(&member_info.key.to_path()) {
            continue;
        }

        duplicated_set.insert(member_info.key.to_path());
        add_field_key_completion(builder, member_info);
    }

    builder.stop_here();
    Some(())
}

fn can_add_key_completion(builder: &mut CompletionBuilder) -> bool {
    if builder.is_cancelled() {
        return false;
    }
    if builder.is_space_trigger_character {
        return false;
    }

    if let Some(NodeOrToken::Node(node)) = builder.trigger_token.prev_sibling_or_token() {
        if let Some(LuaAst::LuaComment(_)) = LuaAst::cast(node) {
            return false;
        }
    }
    true
}

fn add_field_key_completion(
    builder: &mut CompletionBuilder,
    member_info: LuaMemberInfo,
) -> Option<()> {
    let property_owner = &member_info.property_owner_id;
    if let Some(property_owner) = &property_owner {
        check_visibility(builder, property_owner.clone())?;
    }

    let name = match member_info.key {
        LuaMemberKey::Name(name) => name.to_string(),
        LuaMemberKey::Integer(index) => format!("[{}]", index),
        _ => return None,
    };
    let typ = member_info.typ;

    let (label, insert_text) = {
        let is_nullable = if typ.is_nullable() { "?" } else { "" };
        if in_env(builder, &name, &typ).is_some() {
            (
                format!(
                    "{name}{nullable} = {name},",
                    name = name,
                    nullable = is_nullable,
                ),
                format!("{name} = {name},", name = name,),
            )
        } else {
            // 函数类型不补空格, 留空格让用户触发字符补全
            let space = if typ.is_function() { "" } else { " " };
            (
                format!(
                    "{name}{nullable} ={space}",
                    name = name,
                    nullable = is_nullable,
                    space = space
                ),
                format!("{name} ={space}", name = name, space = space),
            )
        }
    };

    let property_owner = &member_info.property_owner_id;
    if let Some(property_owner) = &property_owner {
        check_visibility(builder, property_owner.clone())?;
    }

    let data = if let Some(id) = &property_owner {
        CompletionData::from_property_owner_id(builder, id.clone().into(), None)
    } else {
        None
    };
    let deprecated = if let Some(id) = &property_owner {
        Some(is_deprecated(builder, id.clone()))
    } else {
        None
    };

    let completion_item = CompletionItem {
        label,
        kind: Some(lsp_types::CompletionItemKind::PROPERTY),
        data,
        deprecated,
        insert_text: Some(insert_text),
        ..Default::default()
    };

    builder.add_completion_item(completion_item);
    Some(())
}

fn in_env(builder: &mut CompletionBuilder, target_name: &str, target_type: &LuaType) -> Option<()> {
    let file_id = builder.semantic_model.get_file_id();
    let decl_tree = builder
        .semantic_model
        .get_db()
        .get_decl_index()
        .get_decl_tree(&file_id)?;
    let local_env = decl_tree.get_env_decls(builder.trigger_token.text_range().start())?;
    let global_env = builder
        .semantic_model
        .get_db()
        .get_global_index()
        .get_all_global_decl_ids();
    let all_env = [local_env, global_env].concat();

    for decl_id in all_env.iter() {
        let decl = builder
            .semantic_model
            .get_db()
            .get_decl_index()
            .get_decl(&decl_id)?;
        let (name, typ) = {
            (
                decl.get_name().to_string(),
                builder
                    .semantic_model
                    .get_db()
                    .get_type_index()
                    .get_type_cache(&decl_id.clone().into())
                    .map(|cache| cache.as_type().clone())
                    .unwrap_or(LuaType::Unknown),
            )
        };
        // 必须要名称相同 + 类型兼容
        if name == target_name
            && builder
                .semantic_model
                .type_check(&target_type, &typ)
                .is_ok()
        {
            return Some(());
        }
    }
    None
}

fn add_table_field_value_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }
    // 仅在 value 为空的时候触发
    let parent = builder.trigger_token.prev_token()?.parent()?;
    let node = LuaAst::cast(parent)?;
    match node {
        LuaAst::LuaTableField(field) => {
            let table_expr = field.get_parent::<LuaTableExpr>()?;
            let table_type = builder
                .semantic_model
                .infer_table_should_be(table_expr.clone())?;
            let key = builder
                .semantic_model
                .get_member_key(&field.get_field_key()?)?;
            let member_infos = builder.semantic_model.get_member_infos(&table_type)?;
            let member_info = member_infos.iter().find(|m| m.key == key)?;

            if add_field_value_completion(builder, member_info.clone()).is_some() {
                // 如果添加了补全项, 则停止
                builder.stop_here();
            }

            Some(())
        }
        _ => None,
    }
}

fn add_field_value_completion(
    builder: &mut CompletionBuilder,
    member_info: LuaMemberInfo,
) -> Option<()> {
    let real_type = get_real_type(&builder.semantic_model.get_db(), &member_info.typ)?;
    if real_type.is_function() {
        let label_detail = get_function_detail(builder, real_type);
        let item = CompletionItem {
            label: "fun".to_string(),
            label_details: Some(lsp_types::CompletionItemLabelDetails {
                detail: label_detail.clone(),
                description: None,
            }),
            kind: Some(lsp_types::CompletionItemKind::SNIPPET),
            insert_text: Some(format!(
                "function{}\n\t${{0}}\nend",
                label_detail.unwrap_or_default()
            )),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            insert_text_mode: Some(InsertTextMode::ADJUST_INDENTATION),
            ..CompletionItem::default()
        };

        return builder.add_completion_item(item);
    }

    None
}

fn get_function_detail(builder: &CompletionBuilder, typ: &LuaType) -> Option<String> {
    match typ {
        LuaType::Signature(signature_id) => {
            let signature = builder
                .semantic_model
                .get_db()
                .get_signature_index()
                .get(&signature_id)?;

            let params_str = signature
                .get_type_params()
                .iter()
                .map(|param| param.0.clone())
                .collect::<Vec<_>>();

            Some(format!("({})", params_str.join(", ")))
        }
        LuaType::DocFunction(f) => {
            let params_str = f
                .get_params()
                .iter()
                .map(|param| param.0.clone())
                .collect::<Vec<_>>();
            Some(format!("({})", params_str.join(", ")))
        }
        _ => None,
    }
}
