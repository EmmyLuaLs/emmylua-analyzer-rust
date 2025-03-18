mod auto_require_provider;
mod doc_name_token_provider;
mod doc_tag_provider;
mod doc_type_provider;
mod env_provider;
mod file_path_provider;
mod keywords_provider;
mod member_provider;
mod module_path_provider;
mod postfix_provider;
mod table_decl_field_provider;
mod type_special_provider;

use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaStringToken};
use rowan::TextRange;

use super::completion_builder::CompletionBuilder;

pub fn add_completions(builder: &mut CompletionBuilder) -> Option<()> {
    // 空格补全只允许在非主动且位于函数调用参数列表中触发
    if builder.get_trigger_text().is_empty() && !builder.is_invoke_completion {
        let node = LuaAst::cast(builder.trigger_token.parent()?)?;
        match node {
            LuaAst::LuaCallArgList(_) => {
                type_special_provider::add_completion(builder);
                return Some(());
            }
            _ => return Some(()),
        }
    }

    module_path_provider::add_completion(builder);
    file_path_provider::add_completion(builder);
    keywords_provider::add_completion(builder);
    type_special_provider::add_completion(builder);
    env_provider::add_completion(builder);
    member_provider::add_completion(builder);
    table_decl_field_provider::add_completion(builder);
    auto_require_provider::add_completion(builder);
    doc_tag_provider::add_completion(builder);
    doc_type_provider::add_completion(builder);
    doc_name_token_provider::add_completion(builder);
    postfix_provider::add_completion(builder);

    for (index, item) in builder.get_completion_items_mut().iter_mut().enumerate() {
        if item.sort_text.is_none() {
            item.sort_text = Some(format!("{:04}", index));
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
