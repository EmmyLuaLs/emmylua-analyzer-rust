use std::collections::HashSet;

use emmylua_code_analysis::{LuaMemberInfo, LuaMemberKey};
use emmylua_parser::{LuaAst, LuaAstNode, LuaTableExpr, LuaTableField};
use lsp_types::CompletionItem;

use crate::handlers::completion::{
    add_completions::{check_visibility, is_deprecated, CompletionData},
    completion_builder::CompletionBuilder,
};

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    let table_expr = get_table_expr(builder)?;
    let table_type = builder
        .semantic_model
        .infer_table_should_be(table_expr.clone())?;
    let member_infos = builder.semantic_model.infer_member_infos(&table_type)?;

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
        add_table_field_completion(builder, member_info);
    }

    // 删除env补全项
    builder.remove_env_completion_items();
    // 中止补全
    builder.stop_here();
    Some(())
}

fn get_table_expr(builder: &mut CompletionBuilder) -> Option<LuaTableExpr> {
    let node = LuaAst::cast(builder.trigger_token.parent()?)?;

    match node {
        LuaAst::LuaTableExpr(table_expr) => Some(table_expr),
        LuaAst::LuaNameExpr(name_expr) => name_expr
            .get_parent::<LuaTableField>()?
            .get_parent::<LuaTableExpr>(),
        _ => None,
    }
}

fn add_table_field_completion(
    builder: &mut CompletionBuilder,
    member_info: LuaMemberInfo,
) -> Option<()> {
    let env_completion = &builder.env_duplicate_name;
    let name = match member_info.key {
        LuaMemberKey::Name(name) => name.to_string(),
        LuaMemberKey::Integer(index) => format!("[{}]", index),
        _ => return None,
    };
    let typ = member_info.typ;

    let (label, insert_text) = {
        let is_nullable = if typ.is_nullable() { "?" } else { "" };
        if env_completion.contains(&name) {
            (
                format!("{0}{1} = {0},", name, is_nullable),
                format!("{0} = {0},", name),
            )
        } else {
            (
                format!("{0}{1} = ", name, is_nullable),
                format!("{0} = ", name),
            )
        }
    };

    let property_owner = &member_info.property_owner_id;
    if let Some(property_owner) = &property_owner {
        check_visibility(builder, property_owner.clone())?;
    }

    let data = if let Some(id) = &property_owner {
        CompletionData::from_property_owner_id(builder, id.clone().into())
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
