mod auto_require_provider;
pub(super) mod desc_provider;
pub(super) mod doc_name_token_provider;
pub(super) mod doc_tag_provider;
pub(super) mod doc_type_provider;
mod env_provider;
mod equality_provider;
pub(super) mod file_path_provider;
mod function_provider;
mod keywords_provider;
mod member_provider;
pub(super) mod module_path_provider;
mod postfix_provider;
pub(super) mod table_field_provider;

use super::{completion_builder::CompletionBuilder, completion_context::CompletionContext};
use emmylua_parser::LuaAstToken;
use emmylua_parser::LuaStringToken;
pub use function_provider::get_function_remove_nil;
use rowan::TextRange;

type CompletionProvider = fn(&mut CompletionBuilder) -> Option<()>;

pub fn add_completions(builder: &mut CompletionBuilder) -> Option<()> {
    match builder.context {
        CompletionContext::DocTag => return doc_tag_provider::add_completion(builder),
        CompletionContext::DocName => return doc_name_token_provider::add_completion(builder),
        CompletionContext::DocType => return doc_type_provider::add_completion(builder),
        CompletionContext::DocDescription => return desc_provider::add_completions(builder),
        CompletionContext::ModulePath => return module_path_provider::add_completion(builder),
        CompletionContext::FilePath => return file_path_provider::add_completion(builder),
        CompletionContext::TableField => return table_field_provider::add_completion(builder),
        CompletionContext::General => {}
    }

    run_provider_group(
        builder,
        &[
            postfix_provider::add_completion,
            // `function_provider`优先级必须高于`env_provider`
            function_provider::add_completion,
            equality_provider::add_completion,
            // `table_field_provider`执行成功时会中止补全, 且优先级必须高于`env_provider`
            table_field_provider::add_completion,
        ],
    );

    run_provider_group(
        builder,
        &[
            env_provider::add_completion,
            keywords_provider::add_completion,
            member_provider::add_completion,
        ],
    );

    run_provider_group(builder, &[auto_require_provider::add_completion]);

    for (index, item) in builder.get_completion_items_mut().iter_mut().enumerate() {
        if item.sort_text.is_none() {
            item.sort_text = Some(format!("{:04}", index + 32));
        }
    }

    Some(())
}

fn run_provider_group(builder: &mut CompletionBuilder, providers: &[CompletionProvider]) {
    for provider in providers {
        provider(builder);
        if builder.is_cancelled() {
            break;
        }
    }
}

fn get_text_edit_range_in_string(
    builder: &mut CompletionBuilder,
    string_token: LuaStringToken,
) -> Option<lsp_types::Range> {
    let text = string_token.get_text();
    let range = string_token.get_range();
    if text.is_empty() {
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

    builder
        .semantic_model
        .get_document()
        .to_lsp_range(new_text_range)
}
