use emmylua_code_analysis::{EmmyrcFilenameConvention, ModuleInfo};
use emmylua_parser::{LuaAstNode, LuaNameExpr};
use lsp_types::{CompletionItem, Position};

use crate::{
    handlers::{command::make_auto_require, completion::completion_builder::CompletionBuilder},
    util::module_name_convert,
};

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let enable = builder.semantic_model.get_emmyrc().completion.auto_require;
    if !enable {
        return None;
    }

    let name_expr = LuaNameExpr::cast(builder.trigger_token.parent()?)?;
    // optimize for large project
    let prefix = name_expr.get_name_text()?.to_lowercase();
    let emmyrc = builder.semantic_model.get_emmyrc();
    let file_conversion = emmyrc.completion.auto_require_naming_convention;
    let version_number = emmyrc.runtime.version.to_lua_version_number();
    let file_id = builder.semantic_model.get_file_id();
    let module_infos = builder
        .semantic_model
        .get_db()
        .get_module_index()
        .get_module_infos();
    let range = builder.trigger_token.text_range();
    let document = builder.semantic_model.get_document();
    let lsp_position = document.to_lsp_range(range)?.start;

    let mut completions = Vec::new();
    for module_info in module_infos {
        if module_info.is_visible(&version_number)
            && module_info.file_id != file_id
            && module_info.export_type.is_some()
        {
            add_module_completion_item(
                builder,
                &prefix,
                &module_info,
                file_conversion,
                lsp_position,
                &mut completions,
            );
        }
    }

    for completion in completions {
        builder.add_completion_item(completion);
    }

    Some(())
}

fn add_module_completion_item(
    builder: &CompletionBuilder,
    prefix: &str,
    module_info: &ModuleInfo,
    file_conversion: EmmyrcFilenameConvention,
    position: Position,
    completions: &mut Vec<CompletionItem>,
) -> Option<()> {
    let completion_name = module_name_convert(&module_info.name, file_conversion);
    if !completion_name.to_lowercase().starts_with(prefix) {
        return None;
    }

    if builder.env_duplicate_name.contains(&completion_name) {
        return None;
    }

    let completion_item = CompletionItem {
        label: completion_name,
        kind: Some(lsp_types::CompletionItemKind::MODULE),
        label_details: Some(lsp_types::CompletionItemLabelDetails {
            detail: Some(format!("    (in {})", module_info.full_module_name)),
            ..Default::default()
        }),
        command: Some(make_auto_require(
            "",
            builder.semantic_model.get_file_id(),
            module_info.file_id,
            position,
        )),
        ..Default::default()
    };

    completions.push(completion_item);

    Some(())
}
