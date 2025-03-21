use emmylua_code_analysis::{LuaDeclId, LuaSemanticDeclId, LuaType};
use lsp_types::CompletionItem;

use crate::handlers::completion::completion_builder::CompletionBuilder;

use super::{
    check_visibility, get_completion_kind, get_description, get_detail, is_deprecated, CallDisplay,
    CompletionData,
};

pub fn add_decl_completion(
    builder: &mut CompletionBuilder,
    decl_id: LuaDeclId,
    name: &str,
    typ: &LuaType,
) -> Option<()> {
    let property_owner = LuaSemanticDeclId::LuaDecl(decl_id);
    check_visibility(builder, property_owner.clone())?;

    let mut completion_item = CompletionItem {
        label: name.to_string(),
        kind: Some(get_completion_kind(&typ)),
        data: CompletionData::from_property_owner_id(builder, decl_id.into()),
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
