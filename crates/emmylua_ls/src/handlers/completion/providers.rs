//! # providers — Completion provider pipeline (pure salsa)
//!
//! The directory structure mirrors `providers_legacy/`: the old provider set is a migration
//! reference, and old files are not deleted until each provider is rewritten on the salsa
//! semantic model and passes its corresponding tests.
//!
//! Pipeline:
//! 1. Context providers (doc tag / doc name / doc type / module path / file path);
//! 2. General primary providers (array append / postfix / function / equality / table field);
//! 3. General secondary providers (env / keywords / member);
//! 4. auto_require.

mod array_append_provider;
mod auto_require_provider;
mod desc_provider;
mod doc_name_token_provider;
mod doc_tag_provider;
mod doc_type_provider;
mod env_provider;
mod equality_provider;
mod file_path_provider;
mod function_provider;
mod keywords_provider;
mod member_provider;
mod module_path_provider;
mod postfix_provider;
mod see_provider;
mod table_field_provider;

use emmylua_parser::{LuaAstToken, LuaStringToken};
use rowan::TextRange;

use super::completion_builder::CompletionBuilder;

pub(crate) use function_provider::callable_candidates;

pub use array_append_provider::ArrayAppendProvider;
pub use auto_require_provider::AutoRequireProvider;
pub use desc_provider::DescProvider;
pub use doc_name_token_provider::DocNameTokenProvider;
pub use doc_tag_provider::DocTagProvider;
pub use doc_type_provider::DocTypeProvider;
pub use env_provider::EnvProvider;
pub use equality_provider::EqualityProvider;
pub use file_path_provider::FilePathProvider;
pub use function_provider::FunctionProvider;
pub use keywords_provider::KeywordsProvider;
pub use member_provider::MemberProvider;
pub use module_path_provider::ModulePathProvider;
pub use postfix_provider::PostfixProvider;
pub use see_provider::SeeCompletionProvider;
pub use table_field_provider::TableFieldProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionContext {
    DocTag,
    DocName,
    DocType,
    DocDescription,
    See,
    ModulePath,
    FilePath,
    TableField,
    General,
}

impl CompletionContext {
    pub fn analyze(builder: &CompletionBuilder) -> Self {
        if DocTagProvider.supports(builder) {
            return Self::DocTag;
        }
        if DocNameTokenProvider.supports(builder) {
            return Self::DocName;
        }
        if DocTypeProvider.supports(builder) {
            return Self::DocType;
        }
        if DescProvider.supports(builder) {
            return Self::DocDescription;
        }
        if SeeCompletionProvider.supports(builder) {
            return Self::See;
        }
        if ModulePathProvider.supports(builder) {
            return Self::ModulePath;
        }
        if FilePathProvider.supports(builder) {
            return Self::FilePath;
        }
        if TableFieldProvider.supports(builder) {
            return Self::TableField;
        }
        Self::General
    }
}

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
    &ArrayAppendProvider,
    &PostfixProvider,
    &FunctionProvider,
    &EqualityProvider,
];

static GENERAL_SECONDARY_PROVIDERS: &[&dyn CompletionProvider] =
    &[&EnvProvider, &KeywordsProvider, &MemberProvider];

static GENERAL_TERTIARY_PROVIDERS: &[&dyn CompletionProvider] = &[&AutoRequireProvider];

pub fn add_completions(builder: &mut CompletionBuilder) -> Option<()> {
    if let Some(provider) = get_context_provider(builder.context) {
        run_provider(builder, provider);
        return Some(());
    }

    // `c1.on_add = <??>` is a block-context special case for the function provider; keep it at highest priority.
    if let Some(ty) = builder.assignment_target_member_type()
        && function_provider::add_function_impl_completion(builder, &ty)
    {
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
        CompletionContext::See => Some(&SeeCompletionProvider),
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

/// Replaceable text range inside a string literal (excluding quotes).
fn get_text_edit_range_in_string(
    builder: &CompletionBuilder,
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
        end_offset = end_offset.saturating_sub(1);
    }

    if end_offset <= start_offset {
        let pos = builder
            .get_document()
            .to_lsp_position(start_offset.into())?;
        return Some(lsp_types::Range {
            start: pos,
            end: pos,
        });
    }

    let new_text_range = TextRange::new(start_offset.into(), end_offset.into());
    builder.get_document().to_lsp_range(new_text_range)
}
