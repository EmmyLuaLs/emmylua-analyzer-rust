use std::collections::HashSet;

use emmylua_code_analysis::SemanticModel;
use emmylua_parser::LuaSyntaxToken;
use lsp_types::CompletionItem;
use tokio_util::sync::CancellationToken;

pub struct CompletionBuilder<'a> {
    pub trigger_token: LuaSyntaxToken,
    pub semantic_model: SemanticModel<'a>,
    pub env_duplicate_name: HashSet<String>,
    completion_items: Vec<CompletionItem>,
    cancel_token: CancellationToken,
    stopped: bool,
    // 主动触发补全
    pub is_invoke_completion: bool,
}

impl<'a> CompletionBuilder<'a> {
    pub fn new(
        trigger_token: LuaSyntaxToken,
        semantic_model: SemanticModel<'a>,
        cancel_token: CancellationToken,
        is_invoke_completion: bool,
    ) -> Self {
        Self {
            trigger_token,
            semantic_model,
            env_duplicate_name: HashSet::new(),
            completion_items: Vec::new(),
            cancel_token,
            stopped: false,
            is_invoke_completion,
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
}
