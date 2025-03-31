use std::collections::HashSet;

use emmylua_code_analysis::SemanticModel;
use emmylua_parser::LuaSyntaxToken;
use lsp_types::{CompletionItem, CompletionTriggerKind};
use tokio_util::sync::CancellationToken;

pub struct CompletionBuilder<'a> {
    pub trigger_token: LuaSyntaxToken,
    pub semantic_model: SemanticModel<'a>,
    pub env_duplicate_name: HashSet<String>,
    completion_items: Vec<CompletionItem>,
    cancel_token: CancellationToken,
    stopped: bool,
    pub trigger_kind: CompletionTriggerKind,
    pub env_range: (usize, usize),
    // 是否为空格字符触发的补全(非主动触发)
    pub is_space_trigger_character: bool,
}

impl<'a> CompletionBuilder<'a> {
    pub fn new(
        trigger_token: LuaSyntaxToken,
        semantic_model: SemanticModel<'a>,
        cancel_token: CancellationToken,
        trigger_kind: CompletionTriggerKind,
    ) -> Self {
        let is_space_trigger_character = if trigger_kind == CompletionTriggerKind::TRIGGER_CHARACTER
        {
            trigger_token.text().trim_end().is_empty()
        } else {
            false
        };

        Self {
            trigger_token,
            semantic_model,
            env_duplicate_name: HashSet::new(),
            completion_items: Vec::new(),
            cancel_token,
            stopped: false,
            trigger_kind,
            env_range: (0, 0),
            is_space_trigger_character,
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.stopped || self.cancel_token.is_cancelled()
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

    pub fn stop_here(&mut self) {
        self.stopped = true;
    }

    pub fn get_trigger_text(&self) -> String {
        self.trigger_token.text().trim_end().to_string()
    }

    pub fn remove_env_completion_items(&mut self) {
        if self.env_range == (0, 0) {
            return;
        }
        if self.env_range.0 <= self.env_range.1 && self.env_range.1 < self.completion_items.len() {
            self.completion_items
                .drain(self.env_range.0..=self.env_range.1 - 1);
        }
        self.env_range = (0, 0);
    }

    /// 是否是触发主动补全
    pub fn is_invoked(&self) -> bool {
        self.trigger_kind == CompletionTriggerKind::INVOKED
    }
}
