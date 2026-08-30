use emmylua_code_analysis::Emmyrc;
use emmylua_parser::{LuaAstNode, LuaExpr, LuaIndexKey, LuaSyntaxToken, LuaTokenKind};
use lsp_types::{CompletionItem, Range};
use rowan::{TextRange, TextSize, TokenAtOffset};

use crate::handlers::completion::completion_builder::CompletionBuilder;

use super::{CompletionProvider, ProviderDecision};

pub struct PostfixProvider;

struct PostfixTarget {
    expr: LuaExpr,
    text: String,
    replace_range: Range,
}

impl PostfixTarget {
    fn is_assignable(&self) -> bool {
        matches!(self.expr, LuaExpr::NameExpr(_) | LuaExpr::IndexExpr(_))
    }

    fn is_paren_call(&self) -> bool {
        matches!(
            &self.expr,
            LuaExpr::CallExpr(call_expr) if call_expr.get_args_list().is_some()
        )
    }
}

impl CompletionProvider for PostfixProvider {
    fn name(&self) -> &'static str {
        "postfix"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        is_postfix_trigger(
            builder.trigger_token.kind().into(),
            builder.semantic_model.get_emmyrc(),
        )
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

    let emmyrc = builder.semantic_model.get_emmyrc();
    let trigger_kind = builder.trigger_token.kind();
    if !is_postfix_trigger(trigger_kind.into(), emmyrc) {
        return None;
    }

    let target = get_postfix_target(builder)?;
    add_local_completion(builder, &target);
    add_control_flow_completion(builder, &target);
    add_table_completion(builder, &target);
    add_function_completion(builder, &target);
    add_assignable_completion(builder, &target);
    Some(())
}

fn get_postfix_target(builder: &CompletionBuilder) -> Option<PostfixTarget> {
    let trigger_pos = u32::from(builder.trigger_token.text_range().start());
    let left_pos = if trigger_pos > 0 {
        trigger_pos - 1
    } else {
        return None;
    };

    let left_token = match builder
        .semantic_model
        .get_root()
        .syntax()
        .token_at_offset(left_pos.into())
    {
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(left, right) => {
            if left.kind() == LuaTokenKind::TkName.into() {
                left
            } else {
                right
            }
        }
        TokenAtOffset::None => return None,
    };

    let expr = get_left_expr(left_token, trigger_pos.into())?;
    let text_range = expr.syntax().text_range();
    let replace_range = TextRange::new(text_range.start(), (trigger_pos + 1).into());
    let document = builder.semantic_model.get_document();
    let replace_range = document.to_lsp_range(replace_range)?;

    Some(PostfixTarget {
        text: document.get_text_slice(text_range).to_string(),
        expr,
        replace_range,
    })
}

fn add_control_flow_completion(builder: &mut CompletionBuilder, target: &PostfixTarget) {
    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "if",
        format!("if {} then\n\t$0\nend", target.text),
    );

    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "ifn",
        format!("if not {} then\n\t$0\nend", target.text),
    );

    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "while",
        format!("while {} do\n\t$0\nend", target.text),
    );

    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "forp",
        format!(
            "for ${{1:k}}, ${{2:v}} in pairs({}) do\n\t$0\nend",
            target.text
        ),
    );

    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "forip",
        format!(
            "for ${{1:i}}, ${{2:v}} in ipairs({}) do\n\t$0\nend",
            target.text
        ),
    );

    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "fori",
        format!("for ${{1:i}} = 1, {} do\n\t$0\nend", target.text),
    );
}

fn add_table_completion(builder: &mut CompletionBuilder, target: &PostfixTarget) {
    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "insert",
        format!("table.insert({}, ${{1:value}})", target.text),
    );

    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "remove",
        format!("table.remove({}, ${{1:index}})", target.text),
    );
}

fn add_function_completion(builder: &mut CompletionBuilder, target: &PostfixTarget) {
    if target.is_paren_call() || !is_function_name_expr(&target.expr) {
        return;
    }

    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "function",
        format!("function {}(${{1:...}})\n\t$0\nend", target.text),
    );
}

fn add_assignable_completion(builder: &mut CompletionBuilder, target: &PostfixTarget) {
    if !target.is_assignable() {
        return;
    }

    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "++",
        format!("{0} = {0} + 1", target.text),
    );

    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "--",
        format!("{0} = {0} - 1", target.text),
    );

    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "+n",
        format!("{0} = {0} + $1", target.text),
    );

    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "-n",
        format!("{0} = {0} - $1", target.text),
    );
}

fn add_local_completion(builder: &mut CompletionBuilder, target: &PostfixTarget) {
    let local_name = {
        let mut chars = target.text.chars();
        match chars.next() {
            Some(first) if first == '_' || first.is_ascii_alphabetic() => {
                if chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
                    target.text.as_str()
                } else {
                    ""
                }
            }
            _ => "",
        }
    };
    add_postfix_completion(
        builder,
        target.replace_range.clone(),
        "local",
        format!("local ${{1:{}}} = {}", local_name, target.text),
    );
}

fn is_function_name_expr(expr: &LuaExpr) -> bool {
    match expr {
        LuaExpr::NameExpr(_) => true,
        LuaExpr::IndexExpr(index_expr) => {
            matches!(index_expr.get_index_key(), Some(LuaIndexKey::Name(_)))
                && index_expr
                    .get_prefix_expr()
                    .is_some_and(|prefix_expr| is_function_name_expr(&prefix_expr))
        }
        _ => false,
    }
}

fn is_postfix_trigger(trigger_kind: LuaTokenKind, emmyrc: &Emmyrc) -> bool {
    let trigger_string = &emmyrc.completion.postfix;
    if trigger_string.is_empty() {
        return false;
    }

    let first_char = trigger_string.chars().next().unwrap();
    match first_char {
        '.' => trigger_kind == LuaTokenKind::TkDot,
        '@' => trigger_kind == LuaTokenKind::TkAt,
        ':' => trigger_kind == LuaTokenKind::TkColon,
        _ => false,
    }
}

fn add_postfix_completion(
    builder: &mut CompletionBuilder,
    replace_range: Range,
    label: &str,
    text: String,
) -> Option<()> {
    let item = CompletionItem {
        label: label.to_string(),
        insert_text: Some(text),
        additional_text_edits: Some(vec![lsp_types::TextEdit {
            range: replace_range,
            new_text: "".to_string(),
        }]),
        insert_text_format: Some(lsp_types::InsertTextFormat::SNIPPET),
        ..Default::default()
    };

    builder.add_completion_item(item);
    Some(())
}

// text_range, replace_range
fn get_left_expr(token: LuaSyntaxToken, trigger_pos: TextSize) -> Option<LuaExpr> {
    token
        .parent_ancestors()
        .take_while(|node| node.text_range().end() == trigger_pos)
        .filter_map(LuaExpr::cast)
        .last()
}
