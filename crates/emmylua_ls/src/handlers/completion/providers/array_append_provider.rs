use emmylua_code_analysis::{LuaMemberKey, LuaType, get_real_type};
use emmylua_parser::{LuaAstNode, LuaIndexExpr, LuaKind, LuaTokenKind};
use lsp_types::{CompletionItem, CompletionTextEdit, InsertTextFormat, TextEdit};
use rowan::TextRange;

use crate::handlers::completion::completion_builder::CompletionBuilder;

use super::{CompletionProvider, ProviderDecision};

pub struct ArrayAppendProvider;

impl CompletionProvider for ArrayAppendProvider {
    fn name(&self) -> &'static str {
        "array_append"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        builder.trigger_token.kind() == LuaKind::Token(LuaTokenKind::TkLen)
            && get_array_append_index_expr(builder).is_some()
    }

    fn complete(&self, builder: &mut CompletionBuilder) -> ProviderDecision {
        complete_provider(builder).unwrap_or(ProviderDecision::NoMatch)
    }
}

fn complete_provider(builder: &mut CompletionBuilder) -> Option<ProviderDecision> {
    if builder.is_cancelled() {
        return None;
    }

    let index_expr = get_array_append_index_expr(builder)?;
    let prefix_expr = index_expr.get_prefix_expr()?;
    let prefix_type = builder
        .semantic_model
        .infer_expr(prefix_expr.clone())
        .ok()?;
    if !can_use_as_array(builder, &prefix_type) {
        return None;
    }

    let table_text = prefix_expr.syntax().text().to_string();
    if table_text.trim().is_empty() {
        return None;
    }

    // 用户已经输入了 `#`, 候选只补齐数组尾部索引和赋值位置.
    let insert_text = format!("{table_text} + 1] = $0");
    let mut next_token = builder.trigger_token.next_token();
    while next_token
        .as_ref()
        .is_some_and(|token| token.kind() == LuaKind::Token(LuaTokenKind::TkWhitespace))
    {
        next_token = next_token?.next_token();
    }
    let edit_end = next_token
        .filter(|token| token.kind() == LuaKind::Token(LuaTokenKind::TkRightBracket))
        .map(|token| token.text_range().end())
        .unwrap_or(builder.position_offset);
    let edit_range = builder
        .semantic_model
        .get_document()
        .to_lsp_range(TextRange::new(builder.position_offset, edit_end))?;

    builder.add_completion_item(CompletionItem {
        label: format!("#{table_text} + 1"),
        kind: Some(lsp_types::CompletionItemKind::SNIPPET),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range: edit_range,
            new_text: insert_text,
        })),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        sort_text: Some("0000".to_string()),
        ..CompletionItem::default()
    });

    Some(ProviderDecision::Stop)
}

fn get_array_append_index_expr(builder: &CompletionBuilder) -> Option<LuaIndexExpr> {
    let mut prev_token = builder.trigger_token.prev_token()?;
    while prev_token.kind() == LuaKind::Token(LuaTokenKind::TkWhitespace) {
        prev_token = prev_token.prev_token()?;
    }
    if prev_token.kind() != LuaKind::Token(LuaTokenKind::TkLeftBracket) {
        return None;
    }

    let mut next_token = builder.trigger_token.next_token();
    while next_token
        .as_ref()
        .is_some_and(|token| token.kind() == LuaKind::Token(LuaTokenKind::TkWhitespace))
    {
        next_token = next_token?.next_token();
    }
    if next_token
        .as_ref()
        .is_some_and(|token| token.kind() != LuaKind::Token(LuaTokenKind::TkRightBracket))
    {
        return None;
    }

    builder
        .trigger_token
        .parent_ancestors()
        .find_map(LuaIndexExpr::cast)
}

fn can_use_as_array(builder: &CompletionBuilder, typ: &LuaType) -> bool {
    let real_type = get_real_type(builder.semantic_model.get_db(), typ).unwrap_or(typ);
    match real_type {
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .any(|typ| can_use_as_array(builder, typ)),
        LuaType::TplRef(tpl) => tpl
            .get_constraint()
            .is_some_and(|constraint| can_use_as_array(builder, constraint)),
        _ => {
            real_type.is_table()
                || builder
                    .semantic_model
                    .get_member_infos(real_type)
                    .is_some_and(|members| {
                        members.iter().any(|member| {
                            matches!(
                                &member.key,
                                LuaMemberKey::Integer(_) | LuaMemberKey::TypeKey(LuaType::Integer)
                            )
                        })
                    })
        }
    }
}
