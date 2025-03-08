use emmylua_code_analysis::Emmyrc;
use emmylua_parser::{LuaAstNode, LuaSyntaxToken, LuaTokenKind};
use lsp_types::{CompletionItem, Range};
use rowan::{TextRange, TextSize, TokenAtOffset};

use crate::handlers::completion::completion_builder::CompletionBuilder;

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let emmyrc = builder.semantic_model.get_emmyrc();
    let trigger_kind = builder.trigger_token.kind();
    if !is_postfix_trigger(trigger_kind.into(), emmyrc) {
        return None;
    }

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
    let (text_range, replace_range) = get_left_valid_range(left_token, trigger_pos.into())?;

    let (left_token_text, replace_lsp_range) = {
        let document = builder.semantic_model.get_document();
        let text = document.get_text_slice(text_range);
        let range = document.to_lsp_range(replace_range)?;
        (text.to_string(), range)
    };

    add_postfix_completion(
        builder,
        replace_lsp_range,
        "if",
        format!("if {} then\n\t$0\nend", left_token_text),
    );

    add_postfix_completion(
        builder,
        replace_lsp_range,
        "ifn",
        format!("if not {} then\n\t$0\nend", left_token_text),
    );

    add_postfix_completion(
        builder,
        replace_lsp_range,
        "while",
        format!("while {} do\n\t$0\nend", left_token_text),
    );

    add_postfix_completion(
        builder,
        replace_lsp_range,
        "forp",
        format!(
            "for ${{1:k}}, ${{2:v}} in pairs({}) do\n\t$0\nend",
            left_token_text
        ),
    );

    add_postfix_completion(
        builder,
        replace_lsp_range,
        "forip",
        format!(
            "for ${{1:i}}, ${{2:v}} in ipairs({}) do\n\t$0\nend",
            left_token_text
        ),
    );

    add_postfix_completion(
        builder,
        replace_lsp_range,
        "fori",
        format!("for ${{1:i}} = 1, {} do\n\t$0\nend", left_token_text),
    );

    add_postfix_completion(
        builder,
        replace_lsp_range,
        "function",
        format!("function {}(${{1:...}})\n\t$0\nend", left_token_text),
    );

    add_postfix_completion(
        builder,
        replace_lsp_range,
        "insert",
        format!("table.insert({}, ${{1:value}})", left_token_text),
    );

    add_postfix_completion(
        builder,
        replace_lsp_range,
        "remove",
        format!("table.remove({}, ${{1:index}})", left_token_text),
    );

    add_postfix_completion(
        builder,
        replace_lsp_range,
        "++",
        format!("{0} = {0} + 1", left_token_text),
    );

    add_postfix_completion(
        builder,
        replace_lsp_range,
        "--",
        format!("{0} = {0} - 1", left_token_text),
    );

    add_postfix_completion(
        builder,
        replace_lsp_range,
        "+n",
        format!("{0} = {0} + $1", left_token_text),
    );

    add_postfix_completion(
        builder,
        replace_lsp_range,
        "-n",
        format!("{0} = {0} - $1", left_token_text),
    );

    Some(())
}

fn is_postfix_trigger(trigger_kind: LuaTokenKind, emmyrc: &Emmyrc) -> bool {
    let trigger_string = &emmyrc.completion.postfix;
    if trigger_string.is_empty() {
        return false;
    }

    let first_char = trigger_string.chars().next().unwrap();
    return match first_char {
        '.' => trigger_kind == LuaTokenKind::TkDot,
        '@' => trigger_kind == LuaTokenKind::TkAt,
        ':' => trigger_kind == LuaTokenKind::TkColon,
        _ => false,
    };
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
fn get_left_valid_range(
    token: LuaSyntaxToken,
    trigger_pos: TextSize,
) -> Option<(TextRange, TextRange)> {
    let node = token.parent()?;
    let range = node.text_range();
    let start = range.start();
    if start < trigger_pos {
        return Some((
            TextRange::new(start, trigger_pos),
            TextRange::new(start, (u32::from(trigger_pos) + 1).into()),
        ));
    }
    None
}
