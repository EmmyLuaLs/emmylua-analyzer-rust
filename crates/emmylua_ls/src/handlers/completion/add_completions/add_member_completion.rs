use emmylua_code_analysis::{DbIndex, LuaMemberInfo, LuaMemberKey, LuaSemanticDeclId, LuaType};
use emmylua_parser::LuaTokenKind;
use lsp_types::CompletionItem;

use crate::handlers::completion::completion_builder::CompletionBuilder;

use super::{
    check_visibility, get_completion_kind, get_description, get_detail, is_deprecated, CallDisplay,
    CompletionData,
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
    function_overload_count: Option<usize>,
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

    let display = get_call_show(builder.semantic_model.get_db(), &member_info.typ, status)
        .unwrap_or(CallDisplay::None);

    let typ = member_info.typ;
    if status == CompletionTriggerStatus::Colon && !typ.is_function() {
        return None;
    }

    let completion_data = if let Some(id) = &property_owner {
        if let Some(index) = member_info.overload_index {
            CompletionData::from_overload(builder, id.clone().into(), index, function_overload_count)
        } else {
            CompletionData::from_property_owner_id(builder, id.clone().into(), function_overload_count)
        }
    } else {
        None
    };

    // 紧靠着 label 显示的描述
    let detail = get_detail(builder, &typ, display);
    // 在`detail`更右侧, 且不紧靠着`detail`显示
    let description = get_description(builder, &typ);

    let deprecated = if let Some(id) = &property_owner {
        Some(is_deprecated(builder, id.clone()))
    } else {
        None
    };

    let mut completion_item = CompletionItem {
        label: label.clone(),
        kind: Some(get_completion_kind(&typ)),
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

    builder.add_completion_item(completion_item)?;

    // add overloads if the type is function
    add_signature_overloads(builder, property_owner, &typ, display, deprecated, label, function_overload_count)?;

    Some(())
}

fn add_signature_overloads(
    builder: &mut CompletionBuilder,
    property_owner: &Option<LuaSemanticDeclId>,
    typ: &LuaType,
    display: CallDisplay,
    deprecated: Option<bool>,
    label: String,
    function_overload_count: Option<usize>,
) -> Option<()> {
    if let LuaType::Signature(signature_id) = typ {
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
                let detail = get_detail(builder, &typ, display);
                let data = if let Some(id) = &property_owner {
                    CompletionData::from_overload(builder, id.clone().into(), index, function_overload_count)
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
    };
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
