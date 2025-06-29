use std::{ops::Deref, sync::Arc};

use emmylua_code_analysis::{EmmyLuaAnalysis, Emmyrc, FileId, VirtualUrlGenerator};
use lsp_types::{
    CodeActionResponse, CompletionItemKind, CompletionResponse, CompletionTriggerKind,
    GotoDefinitionResponse, Hover, HoverContents, InlayHint, MarkupContent, Position,
    SemanticTokensResult, SignatureHelpContext, SignatureHelpTriggerKind,
};
use tokio_util::sync::CancellationToken;

use crate::{
    context::ClientId,
    handlers::{
        code_actions::code_action,
        completion::{completion, completion_resolve},
        inlay_hint::inlay_hint,
        semantic_token::semantic_token,
        signature_helper::signature_help,
    },
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
    pub label_detail: Option<String>,
}

impl Default for VirtualCompletionItem {
    fn default() -> Self {
        Self {
            label: String::new(),
            kind: CompletionItemKind::VARIABLE,
            label_detail: None,
        }
    }
}

#[derive(Debug)]
pub struct VirtualCompletionResolveItem {
    pub detail: String,
}

#[derive(Debug)]
pub struct VirtualSignatureHelp {
    pub target_label: String,
    pub active_signature: usize,
    pub active_parameter: usize,
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

    pub fn get_emmyrc(&self) -> Emmyrc {
        self.analysis.emmyrc.deref().clone()
    }

    pub fn update_emmyrc(&mut self, emmyrc: Emmyrc) {
        self.analysis.update_config(Arc::new(emmyrc));
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
        // dbg!(&value);
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
        // dbg!(&items);
        if items.len() != expect.len() {
            return false;
        }
        // 需要顺序一致
        for (item, expect) in items.iter().zip(expect.iter()) {
            if item.label != expect.label || item.kind != Some(expect.kind) {
                return false;
            }
            if let Some(label_detail) = item.label_details.as_ref() {
                if label_detail.detail != expect.label_detail {
                    return false;
                }
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

    pub fn check_implementation(&mut self, block_str: &str, len: usize) -> bool {
        let content = Self::handle_file_content(block_str);
        let Some((content, position)) = content else {
            return false;
        };
        let file_id = self.def(&content);
        let result = implementation(&self.analysis, file_id, position);
        let Some(result) = result else {
            return false;
        };
        let GotoDefinitionResponse::Array(implementations) = result else {
            return false;
        };
        if implementations.len() == len {
            return true;
        }
        false
    }

    pub fn check_definition(&mut self, block_str: &str) -> Option<GotoDefinitionResponse> {
        let content = Self::handle_file_content(block_str);
        let Some((content, position)) = content else {
            return None;
        };
        let file_id = self.def(&content);
        let result: Option<GotoDefinitionResponse> =
            super::definition::definition(&self.analysis, file_id, position);
        let Some(result) = result else {
            return None;
        };
        // dbg!(&result);
        Some(result)
    }

    pub fn check_signature_helper(
        &mut self,
        block_str: &str,
        expect: VirtualSignatureHelp,
    ) -> bool {
        let content = Self::handle_file_content(block_str);
        let Some((content, position)) = content else {
            return false;
        };
        let file_id = self.def(&content);
        let param_context = SignatureHelpContext {
            trigger_kind: SignatureHelpTriggerKind::INVOKED,
            trigger_character: None,
            is_retrigger: false,
            active_signature_help: None,
        };
        let result = signature_help(&self.analysis, file_id, position, param_context);
        dbg!(&result);
        let Some(result) = result else {
            return false;
        };
        let Some(signature) = result.signatures.get(expect.active_signature) else {
            return false;
        };
        if signature.label != expect.target_label {
            return false;
        }
        if signature.active_parameter != Some(expect.active_parameter as u32) {
            return false;
        }
        true
    }

    pub fn check_inlay_hint(&mut self, block_str: &str) -> Option<Vec<InlayHint>> {
        let file_id = self.def(&block_str);
        let result = inlay_hint(&self.analysis, file_id);
        dbg!(&result);
        return result;
    }

    pub fn check_code_action(&mut self, block_str: &str) -> Option<CodeActionResponse> {
        let file_id = self.def(block_str);
        let result = self
            .analysis
            .diagnose_file(file_id, CancellationToken::new());
        let Some(diagnostics) = result else {
            return None;
        };
        let result = code_action(&self.analysis, file_id, diagnostics);
        // dbg!(&result);
        result
    }

    pub fn check_semantic_token(&mut self, block_str: &str) -> Option<SemanticTokensResult> {
        let file_id = self.def(block_str);
        let result = semantic_token(&self.analysis, file_id, ClientId::VSCode);
        let Some(result) = result else {
            return None;
        };

        let data = serde_json::to_string(&result).unwrap();
        dbg!(&data);
        Some(result)
    }

    pub fn check_rename(&mut self, block_str: &str, new_name: String, len: usize) -> bool {
        let content = Self::handle_file_content(block_str);
        let Some((content, position)) = content else {
            return false;
        };
        let file_id = self.def(&content);
        let result = rename(&self.analysis, file_id, position, new_name.clone());
        let Some(result) = result else {
            return false;
        };
        // dbg!(&result);
        if let Some(changes) = result.changes {
            let mut count = 0;
            for (_, edits) in changes {
                count += edits.len();
            }
            if count != len {
                return false;
            }
        }

        true
    }
}
