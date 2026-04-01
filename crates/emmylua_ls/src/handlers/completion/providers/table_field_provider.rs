use std::collections::HashSet;

use emmylua_code_analysis::{
    InferGuard, LuaMemberInfo, LuaMemberKey, LuaType, get_real_type,
    infer_table_field_value_should_be,
};
use emmylua_parser::{LuaAst, LuaAstNode, LuaKind, LuaTableExpr, LuaTableField, LuaTokenKind};
use lsp_types::{CompletionItem, InsertTextFormat, InsertTextMode};
use rowan::NodeOrToken;

use crate::handlers::completion::{
    add_completions::{check_visibility, is_deprecated},
    completion_builder::CompletionBuilder,
    completion_data::CompletionData,
    providers::function_provider::dispatch_type,
};

use super::{CompletionProvider, ProviderDecision};

pub struct TableFieldProvider;

impl CompletionProvider for TableFieldProvider {
    fn name(&self) -> &'static str {
        "table_field"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        supports_provider(builder)
    }

    fn complete(&self, builder: &mut CompletionBuilder) -> ProviderDecision {
        complete_provider(builder).unwrap_or(ProviderDecision::NoMatch)
    }
}

fn supports_provider(builder: &CompletionBuilder) -> bool {
    has_key_completion(builder) || has_function_value_completion(builder)
}

fn complete_provider(builder: &mut CompletionBuilder) -> Option<ProviderDecision> {
    if add_table_field_key_completion(builder).is_some() {
        return Some(ProviderDecision::Stop);
    }

    add_table_field_value_completion(builder)
}

fn has_key_completion(builder: &CompletionBuilder) -> bool {
    if !can_add_key_completion(builder) {
        return false;
    }

    let Some(prev_token) = builder.trigger_token.prev_token() else {
        return false;
    };
    if builder.trigger_token.kind() == LuaKind::Token(LuaTokenKind::TkWhitespace)
        && prev_token.kind() == LuaKind::Token(LuaTokenKind::TkAssign)
    {
        return false;
    }

    let Some(parent) = builder.trigger_token.parent() else {
        return false;
    };
    let Some(node) = LuaAst::cast(parent) else {
        return false;
    };
    let table_expr = match node {
        LuaAst::LuaTableExpr(table_expr) => Some(table_expr),
        LuaAst::LuaNameExpr(name_expr) => name_expr
            .get_parent::<LuaTableField>()
            .and_then(|field| field.get_parent::<LuaTableExpr>()),
        _ => None,
    };

    let Some(table_expr) = table_expr else {
        return false;
    };
    let Some(table_type) = builder.semantic_model.infer_table_should_be(table_expr) else {
        return false;
    };

    builder
        .semantic_model
        .get_member_infos(&table_type)
        .is_some_and(|member_infos| !member_infos.is_empty())
}

fn has_function_value_completion(builder: &CompletionBuilder) -> bool {
    let mut parent = if builder.trigger_token.kind() == LuaTokenKind::TkWhitespace.into() {
        let Some(prev_token) = builder.trigger_token.prev_token() else {
            return false;
        };
        let Some(parent) = prev_token.parent() else {
            return false;
        };
        parent
    } else {
        let Some(parent) = builder.trigger_token.parent() else {
            return false;
        };
        parent
    };

    for _ in 0..3 {
        match LuaAst::cast(parent.clone()) {
            Some(LuaAst::LuaTableField(field)) if field.is_assign_field() => {
                let Some(table_expr) = field.get_parent::<LuaTableExpr>() else {
                    return false;
                };
                let Some(table_type) = builder.semantic_model.infer_table_should_be(table_expr)
                else {
                    return false;
                };
                let Some(field_key) = field.get_field_key() else {
                    return false;
                };
                let Some(key) = builder.semantic_model.get_member_key(&field_key) else {
                    return false;
                };
                let Some(member_infos) = builder.semantic_model.get_member_infos(&table_type)
                else {
                    return false;
                };
                let Some(member_info) = member_infos.iter().find(|member| member.key == key) else {
                    return false;
                };
                let Some(real_type) =
                    get_real_type(builder.semantic_model.get_db(), &member_info.typ)
                else {
                    return false;
                };

                return real_type.is_function();
            }
            Some(_) => {
                let Some(next_parent) = parent.parent() else {
                    return false;
                };
                parent = next_parent;
            }
            None => return false,
        }
    }

    false
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

    Some(())
}

fn can_add_key_completion(builder: &CompletionBuilder) -> bool {
    if builder.is_cancelled() {
        return false;
    }
    if builder.is_space_trigger_character {
        return false;
    }

    if let Some(NodeOrToken::Node(node)) = builder.trigger_token.prev_sibling_or_token()
        && let Some(LuaAst::LuaComment(_)) = LuaAst::cast(node)
    {
        return false;
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

    let (label, insert_text, insert_text_format) = {
        let is_nullable = if typ.is_nullable() { "?" } else { "" };
        if in_env(builder, &name, &typ).is_some() {
            (
                format!(
                    "{name}{nullable} = {name},",
                    name = name,
                    nullable = is_nullable,
                ),
                format!("{name} = ${{1:{name}}},", name = name),
                Some(InsertTextFormat::SNIPPET),
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
                None,
            )
        }
    };

    let property_owner = &member_info.property_owner_id;
    if let Some(property_owner) = &property_owner {
        check_visibility(builder, property_owner.clone())?;
    }

    let data = if let Some(id) = &property_owner {
        CompletionData::from_property_owner_id(builder, id.clone(), None)
    } else {
        None
    };
    let deprecated = property_owner
        .as_ref()
        .map(|id| is_deprecated(builder, id.clone()));

    let completion_item = CompletionItem {
        label,
        kind: Some(lsp_types::CompletionItemKind::PROPERTY),
        data,
        deprecated,
        insert_text: Some(insert_text),
        insert_text_format,
        ..Default::default()
    };

    builder.add_completion_item(completion_item);
    Some(())
}

/// 是否在当前文件的 env 中, 将会排除掉`std`
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
        .get_all_global_decl_ids()
        .into_iter()
        .filter(|id| {
            !builder
                .semantic_model
                .get_db()
                .get_module_index()
                .is_std(&id.file_id)
        })
        .collect();
    let all_env = [local_env, global_env].concat();

    for decl_id in all_env.iter() {
        let decl = builder
            .semantic_model
            .get_db()
            .get_decl_index()
            .get_decl(decl_id)?;
        let (name, typ) = {
            (
                decl.get_name().to_string(),
                builder
                    .semantic_model
                    .get_db()
                    .get_type_index()
                    .get_type_cache(&(*decl_id).into())
                    .map(|cache| cache.as_type().clone())
                    .unwrap_or(LuaType::Unknown),
            )
        };
        // 必须要名称相同 + 类型兼容
        if name == target_name && builder.semantic_model.type_check(target_type, &typ).is_ok() {
            return Some(());
        }
    }
    None
}

fn add_table_field_value_completion(builder: &mut CompletionBuilder) -> Option<ProviderDecision> {
    if builder.is_cancelled() {
        return None;
    }
    // 仅在 value 为空的时候触发
    let mut parent = if builder.trigger_token.kind() == LuaTokenKind::TkWhitespace.into() {
        builder.trigger_token.prev_token()?.parent()?
    } else {
        builder.trigger_token.parent()?
    };
    for _ in 0..3 {
        match LuaAst::cast(parent.clone())? {
            LuaAst::LuaTableField(field) => {
                if field.is_assign_field() {
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
                        return Some(ProviderDecision::Stop);
                    }
                } else {
                    let table_field_should = infer_table_field_value_should_be(
                        builder.semantic_model.get_db(),
                        &mut builder.semantic_model.get_cache().borrow_mut(),
                        field,
                    )
                    .ok()?;
                    return dispatch_type(builder, table_field_should, &InferGuard::new());
                }
                return Some(ProviderDecision::Continue);
            }
            _ => parent = parent.parent()?,
        }
    }

    Some(ProviderDecision::Continue)
}

fn add_field_value_completion(
    builder: &mut CompletionBuilder,
    member_info: LuaMemberInfo,
) -> Option<()> {
    let real_type = get_real_type(builder.semantic_model.get_db(), &member_info.typ)?;
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
    } else {
        dispatch_type(builder, real_type.clone(), &InferGuard::new())?;
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
                .get(signature_id)?;

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
