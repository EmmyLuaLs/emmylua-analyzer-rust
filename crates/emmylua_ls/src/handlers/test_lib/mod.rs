use emmylua_code_analysis::{EmmyLuaAnalysis, Emmyrc, FileId, VirtualUrlGenerator};
use googletest::prelude::*;
use itertools::Itertools;
use lsp_types::{
    ClientCapabilities, CodeActionOrCommand, CompletionItem, CompletionItemKind,
    CompletionResponse, CompletionTriggerKind, GotoDefinitionResponse, Hover, HoverContents,
    InlayHintLabel, Location, MarkupContent, Position, SemanticTokenModifier, SemanticTokenType,
    SemanticTokensResult, SignatureHelpContext, SignatureHelpTriggerKind, SignatureInformation,
    TextEdit,
};
use std::collections::HashSet;
use std::{ops::Deref, sync::Arc};
use tokio_util::sync::CancellationToken;

use crate::{
    context::ClientId,
    handlers::{
        code_actions::code_action,
        completion::{completion, completion_resolve},
        inlay_hint::inlay_hint,
        rename::rename,
        semantic_token::semantic_token,
        signature_helper::signature_help,
    },
};

use super::{hover::hover, implementation::implementation, references::references};
use crate::handlers::semantic_token::{SEMANTIC_TOKEN_MODIFIERS, SEMANTIC_TOKEN_TYPES};

/// Calling this macro on a [`Result`] is equivalent to `result?`,
/// but adds info about current location to the error message.
macro_rules! check {
    ($e:expr $(,)?) => {
        googletest::prelude::OrFail::or_fail($e)?
    };
    ($e:expr, $($t:tt)+) => {
        googletest::prelude::OrFail::or_fail($e).with_failure_message(|| format!($($t)+))?
    };
}
pub(crate) use check;

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
pub struct VirtualLocation {
    pub file: String,
    pub line: u32,
}

#[derive(Debug)]
pub struct VirtualSignatureHelp {
    pub target_label: String,
    pub active_signature: usize,
    pub active_parameter: usize,
}

#[derive(Debug)]
pub struct VirtualInlayHint {
    pub label: String,
    pub line: u32,
    pub pos: u32,
    pub ref_file: Option<String>,
}

#[derive(Debug)]
pub struct VirtualCodeAction {
    pub title: String,
}

#[derive(Debug, Eq, PartialEq)]
pub struct VirtualSemanticToken {
    pub line: u32,
    pub start: u32,
    pub length: u32,
    pub token_type: SemanticTokenType,
    pub token_modifier: HashSet<SemanticTokenModifier>,
}

#[allow(unused)]
impl ProviderVirtualWorkspace {
    pub fn new() -> Self {
        let generator = VirtualUrlGenerator::new();
        let mut analysis = EmmyLuaAnalysis::new();
        let base = &generator.base;
        analysis.add_main_workspace(base.clone());
        ProviderVirtualWorkspace {
            virtual_url_generator: generator,
            analysis,
            id_counter: 0,
        }
    }

    pub fn new_with_init_std_lib() -> Self {
        let generator = VirtualUrlGenerator::new();
        let mut analysis = EmmyLuaAnalysis::new();
        analysis.init_std_lib(None);
        let base = &generator.base;
        analysis.add_main_workspace(base.clone());
        ProviderVirtualWorkspace {
            virtual_url_generator: generator,
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
    fn handle_file_content(content: &str) -> Result<(String, Position)> {
        let content = content.to_string();
        let cursor_byte_pos = content
            .find("<??>")
            .ok_or("module content should include <??>")
            .or_fail()?;
        if content.matches("<??>").count() > 1 {
            return Err("found multiple <??>").or_fail();
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
        Ok((new_content, Position::new(line as u32, column as u32)))
    }

    pub fn check_hover(&mut self, block_str: &str, expected: VirtualHoverResult) -> Result<()> {
        let (content, position) = Self::handle_file_content(block_str)?;
        let file_id = self.def(&content);
        let result = hover(&self.analysis, file_id, position)
            .ok_or("couldn't get a hover")
            .or_fail()?;
        let Hover { contents, range } = result;
        let HoverContents::Markup(MarkupContent { kind, value }) = contents else {
            return fail!("expected HoverContents::Markup, got {contents:?}");
        };
        verify_eq!(value, expected.value)
    }

    pub fn check_completion(
        &mut self,
        block_str: &str,
        expected: Vec<VirtualCompletionItem>,
    ) -> Result<()> {
        self.check_completion_with_kind(block_str, expected, CompletionTriggerKind::INVOKED)
    }

    pub fn check_completion_with_kind(
        &mut self,
        block_str: &str,
        expected: Vec<VirtualCompletionItem>,
        trigger_kind: CompletionTriggerKind,
    ) -> Result<()> {
        let (content, position) = Self::handle_file_content(block_str)?;
        let file_id = self.def(&content);
        let result = completion(
            &self.analysis,
            file_id,
            position,
            trigger_kind,
            CancellationToken::new(),
        )
        .ok_or("failed to get completion")
        .or_fail()?;
        // 对比
        let items = match result {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };

        fn get_item_detail(i: &CompletionItem) -> Option<&String> {
            i.label_details.as_ref().and_then(|d| d.detail.as_ref())
        }

        verify_that!(
            &items,
            pointwise!(
                |expected| all![
                    field!(CompletionItem.label, eq(&expected.label)),
                    field!(CompletionItem.kind, points_to(eq(Some(expected.kind)))),
                    result_of!(get_item_detail, eq(expected.label_detail.as_ref())),
                ],
                &expected
            )
        )
    }

    pub fn check_completion_resolve(
        &mut self,
        block_str: &str,
        expected: VirtualCompletionResolveItem,
    ) -> Result<()> {
        let (content, position) = Self::handle_file_content(block_str)?;
        let file_id = self.def(&content);
        let result = completion(
            &self.analysis,
            file_id,
            position,
            CompletionTriggerKind::INVOKED,
            CancellationToken::new(),
        )
        .ok_or("failed to get completion")
        .or_fail()?;
        let items = match result {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        let param = items
            .get(0)
            .ok_or("failed to get completion item")
            .or_fail()?;
        let item = completion_resolve(&self.analysis, param.clone(), ClientId::VSCode);
        let item_detail = item.detail.ok_or("item detail is empty").or_fail()?;
        verify_eq!(item_detail, expected.detail)
    }

    pub fn check_implementation(
        &mut self,
        block_str: &str,
        expected: Vec<VirtualLocation>,
    ) -> Result<()> {
        let (content, position) = Self::handle_file_content(block_str)?;
        let file_id = self.def(&content);
        let result = implementation(&self.analysis, file_id, position)
            .ok_or("failed to get go to definition response")
            .or_fail()?;

        Self::assert_definition(result, expected)
    }

    pub fn check_definition(
        &mut self,
        block_str: &str,
        expected: Vec<VirtualLocation>,
    ) -> Result<()> {
        let (content, position) = Self::handle_file_content(block_str)?;
        let file_id = self.def(&content);
        let result = super::definition::definition(&self.analysis, file_id, position)
            .ok_or("failed to get go to definition response")
            .or_fail()?;

        Self::assert_definition(result, expected)
    }

    fn assert_definition(
        result: GotoDefinitionResponse,
        expected: Vec<VirtualLocation>,
    ) -> Result<()> {
        let mut items = match result {
            GotoDefinitionResponse::Scalar(item) => vec![item],
            GotoDefinitionResponse::Array(array) => array,
            GotoDefinitionResponse::Link(_) => {
                return fail!("unexpected go to definition response {result:?}");
            }
        };

        Self::assert_locations(items, expected)
    }

    fn assert_locations(result: Vec<Location>, mut expected: Vec<VirtualLocation>) -> Result<()> {
        let mut items = result
            .iter()
            .map(|l| VirtualLocation {
                file: l.uri.path().segments().last().unwrap().to_string(),
                line: l.range.start.line,
            })
            .collect::<Vec<_>>();

        items.sort_by_key(|item| item.line);
        expected.sort_by_key(|item| item.line);

        verify_that!(
            &items,
            pointwise!(
                |expected| {
                    let is_virtual_file =
                        |file: &String| expected.file.is_empty() && file.starts_with("virtual_");

                    all![
                        field!(VirtualLocation.line, eq(&expected.line)),
                        field!(
                            VirtualLocation.file,
                            ends_with(expected.file.deref()).or(predicate(is_virtual_file))
                        ),
                    ]
                },
                &expected
            )
        )
    }

    pub fn check_signature_helper(
        &mut self,
        block_str: &str,
        expected: VirtualSignatureHelp,
    ) -> Result<()> {
        let (content, position) = Self::handle_file_content(block_str)?;
        let file_id = self.def(&content);
        let param_context = SignatureHelpContext {
            trigger_kind: SignatureHelpTriggerKind::INVOKED,
            trigger_character: None,
            is_retrigger: false,
            active_signature_help: None,
        };
        let result = signature_help(&self.analysis, file_id, position, param_context)
            .ok_or("failed to get signature help")
            .or_fail()?;
        let signature = result
            .signatures
            .get(expected.active_signature)
            .ok_or_else(|| {
                format!(
                    "active signature {} not found in {result:?}",
                    expected.active_signature
                )
            })
            .or_fail()?;
        verify_that!(
            signature,
            all![
                field!(SignatureInformation.label, eq(&expected.target_label)),
                field!(
                    SignatureInformation.active_parameter,
                    eq(&Some(expected.active_parameter as u32))
                )
            ]
        )
    }

    pub fn check_inlay_hint(
        &mut self,
        block_str: &str,
        expected: Vec<VirtualInlayHint>,
    ) -> Result<()> {
        let file_id = self.def(&block_str);
        let result = inlay_hint(&self.analysis, file_id)
            .ok_or("failed to get inlay hints")
            .or_fail()?;

        let items = result
            .into_iter()
            .map(|item| VirtualInlayHint {
                label: match &item.label {
                    InlayHintLabel::String(s) => s.clone(),
                    InlayHintLabel::LabelParts(parts) => {
                        parts.iter().map(|part| &part.value).join("")
                    }
                },
                line: item.position.line,
                pos: item.position.character,
                ref_file: match &item.label {
                    InlayHintLabel::LabelParts(parts) => match parts.first() {
                        Some(part) => part
                            .location
                            .as_ref()
                            .map(|loc| loc.uri.path().segments().last().unwrap().to_string()),
                        None => None,
                    },
                    InlayHintLabel::String(_) => None,
                },
            })
            .collect::<Vec<_>>();

        verify_that!(
            &items,
            pointwise!(
                |expected| {
                    let is_virtual_file = |file: &Option<String>| {
                        expected.ref_file.as_deref() == Some("")
                            && file
                                .as_ref()
                                .is_some_and(|file| file.starts_with("virtual_"))
                    };

                    all![
                        field!(VirtualInlayHint.label, eq(&expected.label)),
                        field!(VirtualInlayHint.line, eq(&expected.line)),
                        field!(VirtualInlayHint.pos, eq(&expected.pos)),
                        field!(
                            VirtualInlayHint.ref_file,
                            eq(&expected.ref_file).or(predicate(is_virtual_file))
                        ),
                    ]
                },
                &expected
            )
        )
    }

    pub fn check_code_action(
        &mut self,
        block_str: &str,
        expected: Vec<VirtualCodeAction>,
    ) -> Result<()> {
        let file_id = self.def(block_str);
        let result = self
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .ok_or("failed to diagnose file")
            .or_fail()?;
        let result = code_action(&self.analysis, file_id, result)
            .ok_or("failed to generate code action")
            .or_fail()?;

        fn get_code_action_label(response: &CodeActionOrCommand) -> String {
            match response {
                CodeActionOrCommand::Command(command) => command.title.clone(),
                CodeActionOrCommand::CodeAction(action) => action.title.clone(),
            }
        }

        verify_that!(
            &result,
            pointwise!(
                |expected| result_of_ref!(get_code_action_label, eq(&expected.title)),
                &expected
            )
        )
    }

    pub fn check_semantic_token(
        &mut self,
        block_str: &str,
        expected: Vec<VirtualSemanticToken>,
    ) -> Result<()> {
        let file_id = self.def(block_str);
        let result = semantic_token(
            &self.analysis,
            file_id,
            &ClientCapabilities::default(),
            ClientId::VSCode,
        )
        .ok_or("failed to get semantic tokens")
        .or_fail()?;
        let SemanticTokensResult::Tokens(result) = result else {
            return fail!("expected SemanticTokensResult::Tokens, got {result:?}");
        };

        fn type_index_to_type(index: u32) -> Result<SemanticTokenType> {
            SEMANTIC_TOKEN_TYPES
                .get(index as usize)
                .cloned()
                .ok_or_else(|| format!("unknown semantic token {index}"))
                .or_fail()
        }

        fn modifier_bitmap_to_modifiers(bitmap: u32) -> Result<HashSet<SemanticTokenModifier>> {
            (0..32)
                .filter_map(|i| {
                    if bitmap & (1 << i) != 0 {
                        Some(
                            SEMANTIC_TOKEN_MODIFIERS
                                .get(i as usize)
                                .cloned()
                                .ok_or_else(|| format!("unknown semantic token modifier {i}"))
                                .or_fail(),
                        )
                    } else {
                        None
                    }
                })
                .collect()
        }

        let mut virtual_result = Vec::new();
        let mut line = 0;
        let mut start = 0;
        for token in result.data {
            if token.delta_line > 0 {
                line += token.delta_line;
                start = 0;
            }
            start += token.delta_start;
            virtual_result.push(VirtualSemanticToken {
                line,
                start,
                length: token.length,
                token_type: type_index_to_type(token.token_type)?,
                token_modifier: modifier_bitmap_to_modifiers(token.token_modifiers_bitset)?,
            });
        }

        verify_eq!(virtual_result, expected)
    }

    pub fn check_rename(
        &mut self,
        block_str: &str,
        new_name: String,
        mut expected: Vec<(String, Vec<TextEdit>)>,
    ) -> Result<()> {
        let (content, position) = Self::handle_file_content(block_str)?;
        let file_id = self.def(&content);
        let result = rename(&self.analysis, file_id, position, new_name.clone())
            .ok_or("failed to rename")
            .or_fail()?;
        let mut items = result
            .changes
            .or_fail()?
            .into_iter()
            .map(|(uri, edits)| Ok((uri.path().segments().last().or_fail()?.to_string(), edits)))
            .collect::<Result<Vec<_>>>()?;
        items.sort_by_key(|(path, _)| path.clone());
        for (_, edits) in &mut items {
            edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
        }
        expected.sort_by_key(|(path, _)| path.clone());
        for (_, edits) in &mut expected {
            edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
        }
        verify_eq!(items, expected)
    }

    pub fn check_references(
        &mut self,
        block_str: &str,
        expected: Vec<VirtualLocation>,
    ) -> Result<()> {
        let (content, position) = Self::handle_file_content(block_str)?;
        let file_id = self.def(&content);
        let result = references(&self.analysis, file_id, position)
            .ok_or("failed to get references")
            .or_fail()?;
        Self::assert_locations(result, expected)
    }
}
