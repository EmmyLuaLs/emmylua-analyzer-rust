use emmylua_code_analysis::{EmmyLuaAnalysis, FileId, VirtualUrlGenerator};
use lsp_types::{
    CompletionItemKind, CompletionResponse, CompletionTriggerKind, GotoDefinitionResponse, Hover,
    HoverContents, MarkupContent, Position,
};
use tokio_util::sync::CancellationToken;

use crate::{
    context::ClientId,
    handlers::completion::{completion, completion_resolve},
};

use super::{hover::hover, implementation::implementation};

/// A virtual workspace for testing.
#[allow(unused)]
#[derive(Debug)]
pub struct ProviderVirtualWorkspace {
    pub virtual_url_generator: VirtualUrlGenerator,
    pub analysis: EmmyLuaAnalysis,
    id_counter: u32,
}

#[derive(Debug)]
pub struct VirtualHoverResult {
    pub value: String,
}

#[derive(Debug)]
pub struct VirtualCompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
}

#[derive(Debug)]
pub struct VirtualCompletionResolveItem {
    pub detail: String,
}

#[allow(unused)]
impl ProviderVirtualWorkspace {
    pub fn new() -> Self {
        let gen = VirtualUrlGenerator::new();
        let mut analysis = EmmyLuaAnalysis::new();
        let base = &gen.base;
        analysis.add_main_workspace(base.clone());
        ProviderVirtualWorkspace {
            virtual_url_generator: gen,
            analysis,
            id_counter: 0,
        }
    }

    pub fn new_with_init_std_lib() -> Self {
        let gen = VirtualUrlGenerator::new();
        let mut analysis = EmmyLuaAnalysis::new();
        analysis.init_std_lib(None);
        let base = &gen.base;
        analysis.add_main_workspace(base.clone());
        ProviderVirtualWorkspace {
            virtual_url_generator: gen,
            analysis,
            id_counter: 0,
        }
    }

    pub fn def(&mut self, content: &str) -> FileId {
        let id = self.id_counter;
        self.id_counter += 1;
        self.def_file(&format!("virtual_{}.lua", id), content)
    }

    pub fn def_file(&mut self, file_name: &str, content: &str) -> FileId {
        let uri = self.virtual_url_generator.new_uri(file_name);
        let file_id = self
            .analysis
            .update_file_by_uri(&uri, Some(content.to_string()))
            .unwrap();
        file_id
    }

    /// 处理文件内容
    fn handle_file_content(content: &str) -> Option<(String, Position)> {
        let content = content.to_string();
        let cursor_byte_pos = content.find("<??>")?;
        if content.matches("<??>").count() > 1 {
            return None;
        }

        let mut line = 0;
        let mut column = 0;

        for (byte_pos, c) in content.char_indices() {
            if byte_pos >= cursor_byte_pos {
                break;
            }
            if c == '\n' {
                line += 1;
                column = 0;
            } else {
                column += 1;
            }
        }

        let new_content = content.replace("<??>", "");
        Some((new_content, Position::new(line as u32, column as u32)))
    }

    pub fn check_hover(&mut self, block_str: &str, expect: VirtualHoverResult) -> bool {
        let content = Self::handle_file_content(block_str);
        let Some((content, position)) = content else {
            return false;
        };
        let file_id = self.def(&content);
        let result = hover(&self.analysis, file_id, position);
        let Some(result) = result else {
            return false;
        };
        let Hover { contents, range } = result;
        let HoverContents::Markup(MarkupContent { kind, value }) = contents else {
            return false;
        };
        if value != expect.value {
            return false;
        }

        true
    }

    pub fn check_completion(
        &mut self,
        block_str: &str,
        expect: Vec<VirtualCompletionItem>,
    ) -> bool {
        self.check_completion_with_kind(block_str, expect, CompletionTriggerKind::INVOKED)
    }

    pub fn check_completion_with_kind(
        &mut self,
        block_str: &str,
        expect: Vec<VirtualCompletionItem>,
        trigger_kind: CompletionTriggerKind,
    ) -> bool {
        let content = Self::handle_file_content(block_str);
        let Some((content, position)) = content else {
            return false;
        };
        let file_id = self.def(&content);
        let result = completion(
            &self.analysis,
            file_id,
            position,
            trigger_kind,
            CancellationToken::new(),
        );
        let Some(result) = result else {
            return false;
        };
        // 对比
        let items = match result {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        if items.len() != expect.len() {
            return false;
        }
        // 需要顺序一致
        for (item, expect) in items.iter().zip(expect.iter()) {
            if item.label != expect.label || item.kind != Some(expect.kind) {
                return false;
            }
        }
        true
    }

    pub fn check_completion_resolve(
        &mut self,
        block_str: &str,
        expect: VirtualCompletionResolveItem,
    ) -> bool {
        let content = Self::handle_file_content(block_str);
        let Some((content, position)) = content else {
            return false;
        };
        let file_id = self.def(&content);
        let result = completion(
            &self.analysis,
            file_id,
            position,
            CompletionTriggerKind::INVOKED,
            CancellationToken::new(),
        );
        let Some(result) = result else {
            return false;
        };
        let items = match result {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let Some(param) = items.get(0) else {
            return false;
        };
        let item = completion_resolve(&self.analysis, param.clone(), ClientId::VSCode);
        let Some(item_detail) = item.detail else {
            return false;
        };
        if item_detail != expect.detail {
            return false;
        }
        true
    }

    pub fn check_implementation(&mut self, block_str: &str) -> bool {
        let content = Self::handle_file_content(block_str);
        let Some((content, position)) = content else {
            return false;
        };
        let file_id = self.def(&content);
        let result = implementation(&self.analysis, file_id, position);
        dbg!(&result);
        let Some(result) = result else {
            return false;
        };
        let GotoDefinitionResponse::Array(implementations) = result else {
            return false;
        };
        dbg!(&implementations.len());
        true
    }

    pub fn check_definition(&mut self, block_str: &str) -> bool {
        let content = Self::handle_file_content(block_str);
        let Some((content, position)) = content else {
            return false;
        };
        let file_id = self.def(&content);
        let result = super::definition::definition(&self.analysis, file_id, position);
        dbg!(&result);
        let Some(result) = result else {
            return false;
        };
        match result {
            GotoDefinitionResponse::Scalar(_) => true,
            GotoDefinitionResponse::Array(_) => true,
            GotoDefinitionResponse::Link(_) => true,
        }
    }
}
