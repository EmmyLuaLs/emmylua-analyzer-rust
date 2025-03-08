use std::collections::HashSet;

use emmylua_code_analysis::DiagnosticCode;
use emmylua_parser::{
    LuaAstNode, LuaClosureExpr, LuaComment, LuaDocTagParam, LuaSyntaxKind, LuaTokenKind,
};
use lsp_types::CompletionItem;

use crate::handlers::completion::completion_builder::CompletionBuilder;

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let trigger_token = &builder.trigger_token;
    if !matches!(trigger_token.kind().into(), LuaTokenKind::TkName) {
        return None;
    }

    let parent_node = trigger_token.parent()?;
    match parent_node.kind().into() {
        LuaSyntaxKind::DocTagParam => {
            add_tag_param_name_completion(builder, LuaDocTagParam::cast(parent_node)?);
        }
        LuaSyntaxKind::DocTagCast => {
            add_tag_cast_name_completion(builder);
        }
        LuaSyntaxKind::DocTagDiagnostic => {
            add_tag_diagnostic_action_completion(builder);
        }
        LuaSyntaxKind::DocDiagnosticCodeList => {
            add_tag_diagnostic_code_completion(builder);
        }
        _ => {}
    }

    builder.stop_here();

    Some(())
}

fn add_tag_param_name_completion(
    builder: &mut CompletionBuilder,
    node: LuaDocTagParam,
) -> Option<()> {
    let comment = node.ancestors::<LuaComment>().next()?;
    let owner = comment.get_owner()?;
    let closure = owner.descendants::<LuaClosureExpr>().next()?;
    let params = closure.get_params_list()?.get_params();
    for param in params {
        let completion_item = CompletionItem {
            label: param.get_name_token()?.get_name_text().to_string(),
            kind: Some(lsp_types::CompletionItemKind::VARIABLE),
            ..Default::default()
        };

        builder.add_completion_item(completion_item);
    }

    Some(())
}

fn add_tag_cast_name_completion(builder: &mut CompletionBuilder) -> Option<()> {
    let file_id = builder.semantic_model.get_file_id();
    let decl_tree = builder
        .semantic_model
        .get_db()
        .get_decl_index()
        .get_decl_tree(&file_id)?;
    let mut duplicated_name = HashSet::new();
    let local_env = decl_tree.get_env_decls(builder.trigger_token.text_range().start())?;
    for decl_id in local_env.iter() {
        let name = {
            let decl = builder
                .semantic_model
                .get_db()
                .get_decl_index()
                .get_decl(&decl_id)?;

            decl.get_name().to_string()
        };
        if duplicated_name.contains(&name) {
            continue;
        }

        duplicated_name.insert(name.clone());
        let completion_item = CompletionItem {
            label: name,
            kind: Some(lsp_types::CompletionItemKind::VARIABLE),
            ..Default::default()
        };
        builder.add_completion_item(completion_item);
    }

    Some(())
}

fn add_tag_diagnostic_action_completion(builder: &mut CompletionBuilder) {
    let actions = vec!["disable", "disable-next-line", "disable-line"];
    for (sorted_index, action) in actions.iter().enumerate() {
        let completion_item = CompletionItem {
            label: action.to_string(),
            kind: Some(lsp_types::CompletionItemKind::EVENT),
            sort_text: Some(format!("{:03}", sorted_index)),
            ..Default::default()
        };

        builder.add_completion_item(completion_item);
    }
}

fn add_tag_diagnostic_code_completion(builder: &mut CompletionBuilder) {
    let codes = DiagnosticCode::all();
    for (sorted_index, code) in codes.iter().enumerate() {
        let completion_item = CompletionItem {
            label: code.get_name().to_string(),
            kind: Some(lsp_types::CompletionItemKind::EVENT),
            sort_text: Some(format!("{:03}", sorted_index)),
            ..Default::default()
        };

        builder.add_completion_item(completion_item);
    }
}
