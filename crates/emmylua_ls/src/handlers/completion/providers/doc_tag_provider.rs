use crate::handlers::completion::{completion_builder::CompletionBuilder, data::DOC_TAGS};
use crate::meta_text::meta_doc_tag;
use emmylua_parser::{
    LuaAst, LuaAstToken, LuaComment, LuaDocTag, LuaExpr, LuaGeneralToken, LuaTokenKind,
};
use lsp_types::{CompletionItem, MarkupContent};

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let trigger_token = &builder.trigger_token;
    let trigger_token_kind: LuaTokenKind = trigger_token.kind().into();
    if !matches!(
        trigger_token_kind,
        LuaTokenKind::TkDocStart | LuaTokenKind::TkDocLongStart | LuaTokenKind::TkTagOther
    ) {
        return None;
    }

    let emmyrc = builder.semantic_model.get_emmyrc_arc();
    let known_other_tags = emmyrc.doc.known_tags.iter().map(|tag| tag.as_str());

    for (sorted_index, tag) in DOC_TAGS.iter().copied().chain(known_other_tags).enumerate() {
        add_tag_completion(builder, sorted_index, tag);
    }

    if matches!(
        trigger_token_kind,
        LuaTokenKind::TkDocStart | LuaTokenKind::TkTagOther
    ) {
        let last_index = DOC_TAGS.len() + emmyrc.doc.known_tags.len();
        add_tag_param_return_completion(builder, last_index);
    }

    builder.stop_here();
    Some(())
}

fn add_tag_completion(builder: &mut CompletionBuilder, sorted_index: usize, tag: &str) {
    let completion_item = CompletionItem {
        label: tag.to_string(),
        kind: Some(lsp_types::CompletionItemKind::EVENT),
        documentation: Some(lsp_types::Documentation::MarkupContent(MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: meta_doc_tag(tag),
        })),
        sort_text: Some(format!("{:03}", sorted_index)),
        ..Default::default()
    };

    builder.add_completion_item(completion_item);
}

fn add_tag_param_return_completion(
    builder: &mut CompletionBuilder,
    sorted_index: usize,
) -> Option<()> {
    let token = LuaGeneralToken::cast(builder.trigger_token.clone())?;
    let comment = token.ancestors::<LuaComment>().next()?;
    let comment_owner = comment.get_owner()?;
    let closure = match comment_owner {
        LuaAst::LuaAssignStat(stat) => {
            let (_, expr_list) = stat.get_var_and_expr_list();
            let mut result_closure = None;
            for value_expr in expr_list {
                if let LuaExpr::ClosureExpr(closure) = value_expr {
                    result_closure = Some(closure.clone());
                    break;
                }
            }

            result_closure
        }
        LuaAst::LuaLocalFuncStat(f) => f.get_closure(),
        LuaAst::LuaFuncStat(f) => f.get_closure(),
        LuaAst::LuaLocalStat(local_stat) => {
            let mut result_closure = None;
            let expr_list = local_stat.get_value_exprs();
            for value_expr in expr_list {
                if let LuaExpr::ClosureExpr(closure) = value_expr {
                    result_closure = Some(closure.clone());
                    break;
                }
            }

            result_closure
        }
        LuaAst::LuaTableField(field) => {
            let value_expr = field.get_value_expr()?;
            if let LuaExpr::ClosureExpr(closure) = value_expr {
                Some(closure)
            } else {
                None
            }
        }
        _ => return None,
    }?;

    let mut param_orders = vec![];

    for param in closure.get_params_list()?.get_params() {
        if let Some(name_token) = param.get_name_token() {
            param_orders.push(Some(name_token.get_text().to_string()));
        } else {
            param_orders.push(Some("...".to_string()));
        }
    }

    for doc_tag in comment.get_doc_tags() {
        if let LuaDocTag::Param(param_tag) = doc_tag {
            if let Some(param_name) = param_tag.get_name_token() {
                let name_text = param_name.get_text();
                for param_order in param_orders.iter_mut() {
                    if let Some(name) = param_order {
                        if name == name_text {
                            *param_order = None;
                            break;
                        }
                    }
                }
            }
        }
    }

    if param_orders.iter().all(|p| p.is_none()) {
        return None;
    }

    let prev_token_text = builder.trigger_token.text();
    let prefix = if prev_token_text.starts_with("--- ") {
        "--- @"
    } else {
        "---@"
    };

    let mut insert_text = String::new();
    for (i, param_name) in param_orders.iter().enumerate() {
        let indent = if i == 0 { "" } else { prefix };

        if let Some(name) = param_name {
            let insert_snippet = format!(
                "{}param {} ${{{}:any}}\n",
                indent,
                name,
                insert_text.len() + 1
            );
            insert_text.push_str(&insert_snippet);
        }
    }

    let idx = insert_text.len() + 1;
    insert_text.push_str(&format!("{}return ${{{}:any}}", prefix, idx));

    let completion_item = CompletionItem {
        label: "param/@return".to_string(),
        kind: Some(lsp_types::CompletionItemKind::EVENT),
        insert_text: Some(insert_text),
        insert_text_format: Some(lsp_types::InsertTextFormat::SNIPPET),
        sort_text: Some(format!("{:03}", sorted_index)),
        insert_text_mode: Some(lsp_types::InsertTextMode::ADJUST_INDENTATION),
        ..Default::default()
    };

    builder.add_completion_item(completion_item);
    Some(())
}
