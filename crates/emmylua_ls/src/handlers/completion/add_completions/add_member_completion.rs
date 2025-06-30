use emmylua_code_analysis::{DbIndex, LuaMemberInfo, LuaMemberKey, LuaSemanticDeclId, LuaType};
use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaAstToken, LuaFuncStat, LuaGeneralToken, LuaIndexExpr,
    LuaParenExpr, LuaTokenKind,
};
use lsp_types::CompletionItem;

use crate::handlers::completion::{
    completion_builder::CompletionBuilder, completion_data::CompletionData,
    providers::get_function_remove_nil,
};

use super::{
    check_visibility, get_completion_kind, get_description, get_detail, is_deprecated, CallDisplay,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompletionTriggerStatus {
    Dot,
    Colon,
    InString,
    LeftBracket,
}

pub fn add_member_completion(
    builder: &mut CompletionBuilder,
    member_info: LuaMemberInfo,
    status: CompletionTriggerStatus,
    overload_count: Option<usize>,
) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }
    let property_owner = &member_info.property_owner_id;
    if let Some(property_owner) = &property_owner {
        check_visibility(builder, property_owner.clone())?;
    }

    let member_key = &member_info.key;
    let label = match status {
        CompletionTriggerStatus::Dot => match member_key {
            LuaMemberKey::Name(name) => name.to_string(),
            LuaMemberKey::Integer(index) => format!("[{}]", index),
            _ => return None,
        },
        CompletionTriggerStatus::Colon => match member_key {
            LuaMemberKey::Name(name) => name.to_string(),
            _ => return None,
        },
        CompletionTriggerStatus::InString => match member_key {
            LuaMemberKey::Name(name) => name.to_string(),
            _ => return None,
        },
        CompletionTriggerStatus::LeftBracket => match member_key {
            LuaMemberKey::Name(name) => format!("\"{}\"", name.to_string()),
            LuaMemberKey::Integer(index) => format!("{}", index),
            _ => return None,
        },
    };

    let typ = &member_info.typ;
    let remove_nil_type =
        get_function_remove_nil(&builder.semantic_model.get_db(), typ).unwrap_or(typ.clone());
    if status == CompletionTriggerStatus::Colon && !remove_nil_type.is_function() {
        return None;
    }

    // 附加数据, 用于在`resolve`时进一步处理
    let completion_data = if let Some(id) = &property_owner {
        if let Some(index) = member_info.overload_index {
            CompletionData::from_overload(builder, id.clone().into(), index, overload_count)
        } else {
            CompletionData::from_property_owner_id(builder, id.clone().into(), overload_count)
        }
    } else {
        None
    };

    let call_display = get_call_show(builder.semantic_model.get_db(), &remove_nil_type, status)
        .unwrap_or(CallDisplay::None);
    // 紧靠着 label 显示的描述
    let detail = get_detail(builder, &remove_nil_type, call_display);
    // 在`detail`更右侧, 且不紧靠着`detail`显示
    let description = get_description(builder, &remove_nil_type);

    let deprecated = if let Some(id) = &property_owner {
        Some(is_deprecated(builder, id.clone()))
    } else {
        None
    };

    let mut completion_item = CompletionItem {
        label: label.clone(),
        kind: Some(get_completion_kind(&remove_nil_type)),
        data: completion_data,
        label_details: Some(lsp_types::CompletionItemLabelDetails {
            detail,
            description,
        }),
        deprecated,
        ..Default::default()
    };

    if status == CompletionTriggerStatus::Dot
        && member_key.is_integer()
        && builder.trigger_token.kind() == LuaTokenKind::TkDot.into()
    {
        let document = builder.semantic_model.get_document();
        let remove_range = builder.trigger_token.text_range();
        let lsp_remove_range = document.to_lsp_range(remove_range)?;
        completion_item.additional_text_edits = Some(vec![lsp_types::TextEdit {
            range: lsp_remove_range,
            new_text: "".to_string(),
        }]);
    }
    // 对于函数的定义时的特殊处理
    if matches!(
        status,
        CompletionTriggerStatus::Dot | CompletionTriggerStatus::Colon
    ) && (builder.trigger_token.kind() == LuaTokenKind::TkDot.into()
        || builder.trigger_token.kind() == LuaTokenKind::TkColon.into())
    {
        resolve_function_params(
            builder,
            &mut completion_item,
            &remove_nil_type,
            call_display,
        );
    }

    // 尝试添加别名补全项, 如果添加成功, 则不添加原本 `[index]` 补全项
    if !try_add_alias_completion_item(builder, &member_info, &completion_item, &label)
        .unwrap_or(false)
    {
        builder.add_completion_item(completion_item)?;
    }

    // add overloads if the type is function
    add_signature_overloads(
        builder,
        property_owner,
        &remove_nil_type,
        call_display,
        deprecated,
        label,
        overload_count,
    );

    Some(())
}

fn add_signature_overloads(
    builder: &mut CompletionBuilder,
    property_owner: &Option<LuaSemanticDeclId>,
    typ: &LuaType,
    call_display: CallDisplay,
    deprecated: Option<bool>,
    label: String,
    overload_count: Option<usize>,
) -> Option<()> {
    let signature_id = match typ {
        LuaType::Signature(signature_id) => signature_id,
        _ => return None,
    };

    let overloads = builder
        .semantic_model
        .get_db()
        .get_signature_index()
        .get(&signature_id)?
        .overloads
        .clone();

    overloads
        .into_iter()
        .enumerate()
        .for_each(|(index, overload)| {
            let typ = LuaType::DocFunction(overload);
            let description = get_description(builder, &typ);
            let detail = get_detail(builder, &typ, call_display);
            let data = if let Some(id) = &property_owner {
                CompletionData::from_overload(builder, id.clone().into(), index, overload_count)
            } else {
                None
            };
            let completion_item = CompletionItem {
                label: label.clone(),
                kind: Some(get_completion_kind(&typ)),
                data,
                label_details: Some(lsp_types::CompletionItemLabelDetails {
                    detail,
                    description,
                }),
                deprecated,
                ..Default::default()
            };

            builder.add_completion_item(completion_item);
        });
    Some(())
}

fn get_call_show(
    db: &DbIndex,
    typ: &LuaType,
    status: CompletionTriggerStatus,
) -> Option<CallDisplay> {
    let (colon_call, colon_define) = match typ {
        LuaType::Signature(sig_id) => {
            let signature = db.get_signature_index().get(sig_id)?;
            let colon_define = signature.is_colon_define;
            let colon_call = status == CompletionTriggerStatus::Colon;
            (colon_call, colon_define)
        }
        LuaType::DocFunction(func) => {
            let colon_define = func.is_colon_define();
            let colon_call = status == CompletionTriggerStatus::Colon;
            (colon_call, colon_define)
        }
        _ => return None,
    };

    match (colon_call, colon_define) {
        (false, true) => Some(CallDisplay::AddSelf),
        (true, false) => Some(CallDisplay::RemoveFirst),
        _ => Some(CallDisplay::None),
    }
}

/// 在定义函数时, 是否需要补全参数列表, 只补全原类型为`docfunction`的函数
/// ```lua
/// ---@class A
/// ---@field on_add fun(self: A, a: string, b: string)
///
/// ---@type A
/// local a
/// function a:<??>() end
/// ```
fn resolve_function_params(
    builder: &mut CompletionBuilder,
    completion_item: &mut CompletionItem,
    typ: &LuaType,
    call_display: CallDisplay,
) -> Option<()> {
    // 目前仅允许`completion_item.label`存在值时触发
    if completion_item.insert_text.is_some() || completion_item.text_edit.is_some() {
        return None;
    }
    let new_text = get_resolve_function_params_str(&typ, call_display)?;
    let index_expr = LuaIndexExpr::cast(builder.trigger_token.parent()?)?;
    let func_stat = index_expr.get_parent::<LuaFuncStat>()?;
    // 从 ast 解析
    if func_stat.get_closure().is_some() {
        return None;
    }
    let next_sibling = func_stat.syntax().next_sibling()?;
    let assign_stat = LuaAssignStat::cast(next_sibling)?;
    let paren_expr = assign_stat.child::<LuaParenExpr>()?;
    // 如果 ast 中包含了参数, 则不补全
    if let Some(_) = paren_expr.get_expr() {
        return None;
    }
    let left_paren = paren_expr.token::<LuaGeneralToken>()?;
    if left_paren.get_token_kind() != LuaTokenKind::TkLeftParen.into() {
        return None;
    }
    // 可能不稳定! 因为 completion_item.label 先被应用, 然后再应用本项, 此时 range 发生了改变
    let document = builder.semantic_model.get_document();
    // 先取得左括号位置
    let add_range = left_paren.syntax().text_range();
    let mut lsp_add_range = document.to_lsp_range(add_range)?;
    // 必须要移动一位字符, 不能与 label 的插入位置重复
    lsp_add_range.start.character += 1;
    if new_text.is_empty() {
        return None;
    }

    completion_item.additional_text_edits = Some(vec![lsp_types::TextEdit {
        range: lsp_add_range,
        new_text: new_text,
    }]);

    Some(())
}

fn get_resolve_function_params_str(typ: &LuaType, display: CallDisplay) -> Option<String> {
    match typ {
        LuaType::DocFunction(f) => {
            let mut params_str = f
                .get_params()
                .iter()
                .map(|param| param.0.clone())
                .collect::<Vec<_>>();

            match display {
                CallDisplay::AddSelf => {
                    params_str.insert(0, "self".to_string());
                }
                CallDisplay::RemoveFirst => {
                    if !params_str.is_empty() {
                        params_str.remove(0);
                    }
                }
                _ => {}
            }
            Some(format!("{}", params_str.join(", ")))
        }
        _ => None,
    }
}

/// 添加索引成员的别名补全项
fn try_add_alias_completion_item(
    builder: &mut CompletionBuilder,
    member_info: &LuaMemberInfo,
    completion_item: &CompletionItem,
    label: &String,
) -> Option<bool> {
    let alias_label = extract_index_member_alias(builder.semantic_model.get_db(), member_info)?;

    let mut alias_completion_item = completion_item.clone();
    alias_completion_item.label = alias_label;
    alias_completion_item.insert_text = Some(label.clone());

    // 更新 label_details 添加别名提示
    let index_hint = t!("completion.index %{label}", label = label).to_string();
    let label_details = alias_completion_item
        .label_details
        .get_or_insert_with(Default::default);
    label_details.description = match label_details.description.take() {
        Some(desc) => Some(format!("({}) {} ", index_hint, desc)),
        None => Some(index_hint),
    };
    builder.add_completion_item(alias_completion_item)?;
    Some(true)
}

/// 从注释中提取索引成员的别名, 只处理整数成员.
/// 格式为`-- [nameX]`.
pub fn extract_index_member_alias(db: &DbIndex, member_info: &LuaMemberInfo) -> Option<String> {
    let LuaMemberKey::Integer(_) = member_info.key else {
        return None;
    };

    let property_owner_id = member_info.property_owner_id.as_ref()?;
    let LuaSemanticDeclId::Member(_) = property_owner_id else {
        return None;
    };

    let description = db
        .get_property_index()
        .get_property(property_owner_id)?
        .description
        .as_ref()?;

    // 只去掉左侧空白字符，保留右侧内容以支持后续文本
    let left_trimmed = description.trim_start();
    if !left_trimmed.starts_with('[') {
        return None;
    }

    // 找到对应的右方括号
    let close_bracket_pos = left_trimmed.find(']')?;

    let content = left_trimmed[1..close_bracket_pos].trim();

    if content.is_empty() {
        return None;
    }

    let first_char = content.chars().next()?;
    if !first_char.is_alphabetic() && first_char != '_' {
        return None;
    }

    if !content.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    if content.parse::<i64>().is_ok() || content.parse::<f64>().is_ok() {
        return None;
    }

    Some(content.to_string())
}
