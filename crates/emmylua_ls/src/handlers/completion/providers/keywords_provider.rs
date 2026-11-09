//! Keyword completion: statement / expression keywords + the function template after `local `.

use emmylua_code_analysis::EmmyrcLuaVersion::LuaJIT2;
use emmylua_parser::{LuaAstNode, LuaKind, LuaNameExpr, LuaSyntaxKind, LuaTokenKind};
use lsp_types::{CompletionItem, CompletionItemLabelDetails, InsertTextFormat, InsertTextMode};

use crate::handlers::completion::{
    completion_builder::CompletionBuilder,
    data::{KEYWORD_COMPLETIONS, KEYWORD_EXPR_COMPLETIONS},
};

use super::{CompletionProvider, ProviderDecision};

pub struct KeywordsProvider;

impl CompletionProvider for KeywordsProvider {
    fn name(&self) -> &'static str {
        "keywords"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        matches!(
            builder.trigger_token.kind().into(),
            LuaTokenKind::TkName | LuaTokenKind::TkWhitespace
        ) || is_full_match_keyword(builder).is_some()
    }

    fn complete(&self, builder: &mut CompletionBuilder) -> ProviderDecision {
        if complete_provider(builder).is_some() {
            ProviderDecision::Continue
        } else {
            ProviderDecision::NoMatch
        }
    }
}

fn complete_provider(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    if is_full_match_keyword(builder).is_some() {
        add_stat_keyword_completions(builder, None);
        return Some(());
    }

    match builder.trigger_token.kind().into() {
        LuaTokenKind::TkName => {
            let name_expr = LuaNameExpr::cast(builder.trigger_token.parent()?)?;
            add_stat_keyword_completions(builder, Some(name_expr));
            add_expr_keyword_completions(builder);
        }
        LuaTokenKind::TkWhitespace => {
            let left_token = builder.trigger_token.prev_token()?;
            if Into::<LuaTokenKind>::into(left_token.kind()) == LuaTokenKind::TkLocal {
                add_function_keyword_completions(builder);
            }
        }
        _ => {}
    }

    Some(())
}

/// Handle full words typed through a Chinese IME.
fn is_full_match_keyword(builder: &CompletionBuilder) -> Option<()> {
    match builder.trigger_token.kind() {
        LuaKind::Token(LuaTokenKind::TkIf) => Some(()),
        LuaKind::Token(LuaTokenKind::TkElse) => Some(()),
        LuaKind::Token(LuaTokenKind::TkElseIf) => Some(()),
        LuaKind::Token(LuaTokenKind::TkThen) => Some(()),
        LuaKind::Token(LuaTokenKind::TkEnd) => Some(()),
        LuaKind::Token(LuaTokenKind::TkFor) => Some(()),
        LuaKind::Token(LuaTokenKind::TkWhile) => Some(()),
        LuaKind::Token(LuaTokenKind::TkRepeat) => Some(()),
        LuaKind::Token(LuaTokenKind::TkReturn) => Some(()),
        LuaKind::Token(LuaTokenKind::TkLocal) => Some(()),
        LuaKind::Token(LuaTokenKind::TkBreak) => Some(()),
        LuaKind::Token(LuaTokenKind::TkFunction) => Some(()),
        LuaKind::Token(LuaTokenKind::TkDo) => Some(()),
        LuaKind::Token(LuaTokenKind::TkGoto) => Some(()),
        LuaKind::Token(LuaTokenKind::TkIn) => Some(()),
        LuaKind::Token(LuaTokenKind::TkNil) => Some(()),
        LuaKind::Token(LuaTokenKind::TkNot) => Some(()),
        LuaKind::Token(LuaTokenKind::TkOr) => Some(()),
        _ => None,
    }
}

fn add_stat_keyword_completions(
    builder: &mut CompletionBuilder,
    name_expr: Option<LuaNameExpr>,
) -> Option<()> {
    let level = builder.get_emmyrc().runtime.version;
    if let Some(name_expr) = name_expr
        && name_expr.syntax().parent()?.parent()?.kind() != LuaSyntaxKind::Block.into()
    {
        return None;
    }
    let trigger_text = builder.get_trigger_text();
    for keyword_info in KEYWORD_COMPLETIONS {
        if !check_match_word(&trigger_text, keyword_info.label) {
            continue;
        }

        let (label_detail, insert_text) =
            if matches!(keyword_info.label, "function" | "local function")
                && !builder.get_emmyrc().completion.base_function_includes_name
            {
                (
                    keyword_info.detail.replace("name", ""),
                    keyword_info.insert_text.replace("name", ""),
                )
            } else {
                (
                    keyword_info.detail.to_string(),
                    keyword_info.insert_text.to_string(),
                )
            };
        if level != LuaJIT2 && keyword_info.label == "continue" {
            continue;
        }

        let item = CompletionItem {
            label: keyword_info.label.to_string(),
            kind: Some(keyword_info.kind),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(label_detail),
                ..CompletionItemLabelDetails::default()
            }),
            insert_text: Some(insert_text),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            insert_text_mode: Some(InsertTextMode::ADJUST_INDENTATION),
            ..CompletionItem::default()
        };

        builder.add_completion_item(item)?;
    }

    Some(())
}

fn add_expr_keyword_completions(builder: &mut CompletionBuilder) -> Option<()> {
    let trigger_text = builder.get_trigger_text();
    for keyword_info in KEYWORD_EXPR_COMPLETIONS {
        if !check_match_word(&trigger_text, keyword_info.label) {
            continue;
        }
        let item = CompletionItem {
            label: keyword_info.label.to_string(),
            kind: Some(keyword_info.kind),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(keyword_info.detail.to_string()),
                ..CompletionItemLabelDetails::default()
            }),
            insert_text: Some(keyword_info.insert_text.to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            insert_text_mode: Some(InsertTextMode::ADJUST_INDENTATION),
            ..CompletionItem::default()
        };

        builder.add_completion_item(item)?;
    }

    Some(())
}

fn add_function_keyword_completions(builder: &mut CompletionBuilder) -> Option<()> {
    // Do not add on non-invoked completion.
    if !builder.is_invoked() {
        return None;
    }
    let item = CompletionItem {
        label: "function".to_string(),
        kind: Some(lsp_types::CompletionItemKind::SNIPPET),
        insert_text: Some("function ${1:name}(${2:})\n\t${0}\nend".to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        insert_text_mode: Some(InsertTextMode::ADJUST_INDENTATION),
        sort_text: Some("0000".to_string()),
        ..CompletionItem::default()
    };

    builder.add_completion_item(item)
}

/// Word-prefix matching (underscore / camel-case / CJK-Latin boundaries), consistent with the old provider.
pub fn check_match_word(key: &str, candidate_key: &str) -> bool {
    if key.is_empty() {
        return true;
    }
    if candidate_key.is_empty() {
        return false;
    }

    let key_first_char = key.chars().next().unwrap().to_lowercase().next().unwrap();
    if key_first_char == '_' && candidate_key.starts_with('_') {
        return true;
    }

    let mut prev_char = '\0';
    for (i, curr_char) in candidate_key.chars().enumerate() {
        let is_word_start = (i == 0 && curr_char != '_')
            || (prev_char == '_')
            || (curr_char.is_uppercase() && prev_char.is_lowercase())
            || (curr_char.is_ascii_alphabetic() != prev_char.is_ascii_alphabetic() && i > 0);

        if is_word_start {
            let curr_lowercase = curr_char.to_lowercase().next().unwrap();
            if curr_lowercase == key_first_char {
                let candidate_key_set: std::collections::HashSet<char> =
                    candidate_key.to_lowercase().chars().collect();
                for trigger_char in key.to_lowercase().chars() {
                    if !candidate_key_set.contains(&trigger_char) {
                        return false;
                    }
                }
                return true;
            }
        }
        prev_char = curr_char;
    }

    false
}
