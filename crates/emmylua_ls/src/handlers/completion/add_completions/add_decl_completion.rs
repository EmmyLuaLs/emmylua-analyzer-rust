use emmylua_code_analysis::{DbIndex, LuaDeclId, LuaSemanticDeclId, LuaType};
use lsp_types::CompletionItem;

use crate::handlers::completion::{
    completion_builder::CompletionBuilder, completion_data::CompletionData,
};

use super::{
    check_visibility, get_completion_kind, get_description, get_detail, is_deprecated, CallDisplay,
};

pub fn add_decl_completion(
    builder: &mut CompletionBuilder,
    decl_id: LuaDeclId,
    name: &str,
    typ: &LuaType,
) -> Option<()> {
    let property_owner = LuaSemanticDeclId::LuaDecl(decl_id);
    check_visibility(builder, property_owner.clone())?;

    let overload_count = count_function_overloads(builder.semantic_model.get_db(), typ);
    let mut completion_item = CompletionItem {
        label: name.to_string(),
        kind: Some(get_completion_kind(&typ)),
        data: CompletionData::from_property_owner_id(builder, decl_id.into(), overload_count),
        label_details: Some(lsp_types::CompletionItemLabelDetails {
            detail: get_detail(builder, &typ, CallDisplay::None),
            description: get_description(builder, &typ),
        }),
        ..Default::default()
    };

    if is_deprecated(builder, property_owner.clone()) {
        completion_item.deprecated = Some(true);
    }

    builder.add_completion_item(completion_item)?;
    Some(())
}

fn count_function_overloads(db: &DbIndex, typ: &LuaType) -> Option<usize> {
    let mut count = 0;
    match typ {
        LuaType::DocFunction(_) => {
            count += 1;
        }
        LuaType::Signature(id) => {
            count += 1;
            if let Some(signature) = db.get_signature_index().get(&id) {
                count += signature.overloads.len();
            }
        }
        _ => {}
    }
    if count > 1 {
        count -= 1;
    }
    if count == 0 {
        None
    } else {
        Some(count)
    }
}
