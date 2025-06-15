mod auto_require_provider;
mod doc_name_token_provider;
mod doc_tag_provider;
mod doc_type_provider;
mod env_provider;
mod equality_comparison_provider;
mod file_path_provider;
mod function_provider;
mod keywords_provider;
mod member_provider;
mod module_path_provider;
mod postfix_provider;
mod table_field_provider;

use emmylua_parser::LuaAstToken;
use emmylua_parser::LuaStringToken;
use rowan::TextRange;

use super::completion_builder::CompletionBuilder;

pub fn add_completions(builder: &mut CompletionBuilder) -> Option<()> {
    postfix_provider::add_completion(builder);
    // `function_provider`优先级必须高于`env_provider`
    function_provider::add_completion(builder);
    equality_comparison_provider::add_completion(builder);
    // 如果`table_field_provider`执行成功会中止补全, 同时优先级必须高于`env_provider`
    table_field_provider::add_completion(builder);
    env_provider::add_completion(builder);
    keywords_provider::add_completion(builder);
    member_provider::add_completion(builder);

    module_path_provider::add_completion(builder);
    file_path_provider::add_completion(builder);
    auto_require_provider::add_completion(builder);
    doc_tag_provider::add_completion(builder);
    doc_type_provider::add_completion(builder);
    doc_name_token_provider::add_completion(builder);

    for (index, item) in builder.get_completion_items_mut().iter_mut().enumerate() {
        if item.sort_text.is_none() {
            item.sort_text = Some(format!("{:04}", index + 1));
        }
    }

    Some(())
}

fn get_text_edit_range_in_string(
    builder: &mut CompletionBuilder,
    string_token: LuaStringToken,
) -> Option<lsp_types::Range> {
    let text = string_token.get_text();
    let range = string_token.get_range();
    if text.len() == 0 {
        return None;
    }

    let mut start_offset = u32::from(range.start());
    let mut end_offset = u32::from(range.end());
    if text.starts_with('"') || text.starts_with('\'') {
        start_offset += 1;
    }

    if text.ends_with('"') || text.ends_with('\'') {
        end_offset -= 1;
    }

    let new_text_range = TextRange::new(start_offset.into(), end_offset.into());
    let lsp_range = builder
        .semantic_model
        .get_document()
        .to_lsp_range(new_text_range);

    lsp_range
}