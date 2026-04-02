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
pub use auto_require_provider::AutoRequireProvider;
pub use desc_provider::DescProvider;
pub use doc_name_token_provider::DocNameTokenProvider;
pub use doc_tag_provider::DocTagProvider;
pub use doc_type_provider::DocTypeProvider;
use emmylua_parser::LuaAstToken;
use emmylua_parser::LuaStringToken;
pub use env_provider::EnvProvider;
pub use equality_provider::EqualityProvider;
pub use file_path_provider::FilePathProvider;
pub use function_provider::FunctionProvider;
pub use function_provider::get_function_remove_nil;
pub use keywords_provider::KeywordsProvider;
pub use member_provider::MemberProvider;
pub use module_path_provider::ModulePathProvider;
pub use postfix_provider::PostfixProvider;
use rowan::TextRange;
pub use table_field_provider::TableFieldProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderDecision {
    NoMatch,
    Continue,
    Stop,
}

pub trait CompletionProvider: Sync {
    #[allow(unused)]
    fn name(&self) -> &'static str;

    fn supports(&self, builder: &CompletionBuilder) -> bool;

    fn complete(&self, builder: &mut CompletionBuilder) -> ProviderDecision;
}

static GENERAL_PRIMARY_PROVIDERS: &[&dyn CompletionProvider] = &[
    &PostfixProvider,
    &FunctionProvider,
    &EqualityProvider,
    &TableFieldProvider,
];

static GENERAL_SECONDARY_PROVIDERS: &[&dyn CompletionProvider] =
    &[&EnvProvider, &KeywordsProvider, &MemberProvider];

static GENERAL_TERTIARY_PROVIDERS: &[&dyn CompletionProvider] = &[&AutoRequireProvider];

pub fn add_completions(builder: &mut CompletionBuilder) -> Option<()> {
    if let Some(provider) = get_context_provider(builder.context) {
        run_provider(builder, provider);
        return Some(());
    }

    if matches!(
        run_provider_group(builder, GENERAL_PRIMARY_PROVIDERS),
        ProviderDecision::Stop
    ) {
        return Some(());
    }

    if matches!(
        run_provider_group(builder, GENERAL_SECONDARY_PROVIDERS),
        ProviderDecision::Stop
    ) {
        return Some(());
    }

    if matches!(
        run_provider_group(builder, GENERAL_TERTIARY_PROVIDERS),
        ProviderDecision::Stop
    ) {
        return Some(());
    }

    for (index, item) in builder.get_completion_items_mut().iter_mut().enumerate() {
        if item.sort_text.is_none() {
            item.sort_text = Some(format!("{:04}", index + 32));
        }
    }

    Some(())
}

fn get_context_provider(context: CompletionContext) -> Option<&'static dyn CompletionProvider> {
    match context {
        CompletionContext::DocTag => Some(&DocTagProvider),
        CompletionContext::DocName => Some(&DocNameTokenProvider),
        CompletionContext::DocType => Some(&DocTypeProvider),
        CompletionContext::DocDescription => Some(&DescProvider),
        CompletionContext::ModulePath => Some(&ModulePathProvider),
        CompletionContext::FilePath => Some(&FilePathProvider),
        CompletionContext::TableField => Some(&TableFieldProvider),
        CompletionContext::General => None,
    }
}

fn run_provider(builder: &mut CompletionBuilder, provider: &dyn CompletionProvider) {
    if provider.supports(builder) {
        let _ = provider.complete(builder);
    }
}

fn run_provider_group(
    builder: &mut CompletionBuilder,
    providers: &[&dyn CompletionProvider],
) -> ProviderDecision {
    for provider in providers {
        if !provider.supports(builder) {
            continue;
        }
        match provider.complete(builder) {
            ProviderDecision::NoMatch | ProviderDecision::Continue => {}
            ProviderDecision::Stop => return ProviderDecision::Stop,
        }
        if builder.is_cancelled() {
            return ProviderDecision::Stop;
        }
    }

    ProviderDecision::Continue
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
