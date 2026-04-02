use std::collections::HashSet;

use emmylua_code_analysis::{LuaType, SemanticModel};
use emmylua_parser::LuaSyntaxToken;
use lsp_types::{CompletionItem, CompletionTriggerKind};
use rowan::TextSize;
use tokio_util::sync::CancellationToken;

use super::completion_context::CompletionContext;

pub struct CompletionBuilder<'a> {
    pub trigger_token: LuaSyntaxToken,
    pub semantic_model: SemanticModel<'a>,
    pub context: CompletionContext,
    pub env_duplicate_name: HashSet<String>,
    completion_items: Vec<CompletionItem>,
    cancel_token: CancellationToken,
    pub trigger_kind: CompletionTriggerKind,
    /// 是否为空格字符触发的补全(非主动触发)
    pub is_space_trigger_character: bool,
    pub position_offset: TextSize,
}

impl<'a> CompletionBuilder<'a> {
    pub fn new(
        trigger_token: LuaSyntaxToken,
        semantic_model: SemanticModel<'a>,
        cancel_token: CancellationToken,
        trigger_kind: CompletionTriggerKind,
        position_offset: TextSize,
    ) -> Self {
        let is_space_trigger_character = if trigger_kind == CompletionTriggerKind::TRIGGER_CHARACTER
        {
            trigger_token.text().trim_end().is_empty()
        } else {
            false
        };

        let mut builder = Self {
            trigger_token,
            semantic_model,
            context: CompletionContext::General,
            env_duplicate_name: HashSet::new(),
            completion_items: Vec::new(),
            cancel_token,
            trigger_kind,
            is_space_trigger_character,
            position_offset,
        };
        builder.context = CompletionContext::analyze(&builder);
        builder
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    pub fn add_completion_item(&mut self, item: CompletionItem) -> Option<()> {
        self.completion_items.push(item);
        Some(())
    }

    pub fn get_completion_items(self) -> Vec<CompletionItem> {
        self.completion_items
    }

    pub fn get_completion_items_mut(&mut self) -> &mut Vec<CompletionItem> {
        &mut self.completion_items
    }

    pub fn get_trigger_text(&self) -> String {
        self.trigger_token.text().trim_end().to_string()
    }

    /// 主动补全
    pub fn is_invoked(&self) -> bool {
        self.trigger_kind == CompletionTriggerKind::INVOKED
    }

    pub fn support_snippets(&self, ty: &LuaType) -> bool {
        ty.is_function()
            && self
                .semantic_model
                .get_db()
                .get_emmyrc()
                .completion
                .call_snippet
    }
}
