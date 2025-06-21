use emmylua_parser::{LuaAstNode, LuaKind, LuaNameExpr, LuaSyntaxKind, LuaTokenKind};
use lsp_types::{CompletionItem, CompletionItemLabelDetails, InsertTextFormat, InsertTextMode};

use crate::handlers::completion::{
    add_completions::check_match_word,
    completion_builder::CompletionBuilder,
    data::{KEYWORD_COMPLETIONS, KEYWORD_EXPR_COMPLETIONS},
};

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
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
            match left_token.kind().into() {
                LuaTokenKind::TkLocal => {
                    add_function_keyword_completions(builder);
                }
                _ => {}
            }
        }
        _ => {}
    }

    Some(())
}

/// 处理中文输入法下输入完整单词的情况
fn is_full_match_keyword(builder: &mut CompletionBuilder) -> Option<()> {
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
    if let Some(name_expr) = name_expr {
        if name_expr.syntax().parent()?.parent()?.kind() != LuaSyntaxKind::Block.into() {
            return None;
        }
    }
    let trigger_text = builder.get_trigger_text();
    for keyword_info in KEYWORD_COMPLETIONS {
        if !check_match_word(&trigger_text, keyword_info.label) {
            continue;
        }

        let (label_detail, insert_text) =
            if matches!(keyword_info.label, "function" | "local function")
                && !base_function_includes_name(builder)
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
    // 非主动补全不添加
    if !builder.is_invoked() {
        return None;
    }
    let item = CompletionItem {
        label: "function".to_string(),
        kind: Some(lsp_types::CompletionItemKind::SNIPPET),
        insert_text: Some("function ${1:name}(${2:})\n\t${0}\nend".to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        insert_text_mode: Some(InsertTextMode::ADJUST_INDENTATION),
        sort_text: Some("0000".to_string()), // 优先级较高
        ..CompletionItem::default()
    };

    builder.add_completion_item(item)?;

    Some(())
}

fn base_function_includes_name(builder: &CompletionBuilder) -> bool {
    builder
        .semantic_model
        .get_emmyrc()
        .completion
        .base_function_includes_name
}
