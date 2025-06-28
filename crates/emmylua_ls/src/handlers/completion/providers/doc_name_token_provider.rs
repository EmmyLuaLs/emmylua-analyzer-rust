use std::collections::HashSet;

use emmylua_code_analysis::{DiagnosticCode, LuaTypeAttribute};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaClosureExpr, LuaComment, LuaDocAttribute, LuaSyntaxKind, LuaSyntaxToken,
    LuaTokenKind,
};
use lsp_types::CompletionItem;

use crate::handlers::completion::completion_builder::CompletionBuilder;

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let trigger_token = &builder.trigger_token;
    let expected = get_doc_completion_expected(trigger_token)?;
    match expected {
        DocCompletionExpected::ParamName => {
            add_tag_param_name_completion(builder);
        }
        DocCompletionExpected::Cast => {
            add_tag_cast_name_completion(builder);
        }
        DocCompletionExpected::DiagnosticAction => {
            add_tag_diagnostic_action_completion(builder);
        }
        DocCompletionExpected::DiagnosticCode => {
            add_tag_diagnostic_code_completion(builder);
        }
        DocCompletionExpected::ClassAttr => {
            add_tag_class_attr_completion(builder);
        }
        DocCompletionExpected::Namespace => {
            add_tag_namespace_completion(builder);
        }
        DocCompletionExpected::Using => {
            add_tag_using_completion(builder);
        }
        DocCompletionExpected::Export => {
            add_tag_export_completion(builder);
        }
    }

    builder.stop_here();

    Some(())
}

fn get_doc_completion_expected(trigger_token: &LuaSyntaxToken) -> Option<DocCompletionExpected> {
    match trigger_token.kind().into() {
        LuaTokenKind::TkName => {
            let parent_node = trigger_token.parent()?;
            match parent_node.kind().into() {
                LuaSyntaxKind::DocTagParam => Some(DocCompletionExpected::ParamName),
                LuaSyntaxKind::DocTagCast => Some(DocCompletionExpected::Cast),
                LuaSyntaxKind::DocTagDiagnostic => Some(DocCompletionExpected::DiagnosticAction),
                LuaSyntaxKind::DocDiagnosticCodeList => Some(DocCompletionExpected::DiagnosticCode),
                _ => None,
            }
        }
        LuaTokenKind::TkWhitespace => {
            let left_token = trigger_token.prev_token()?;
            match left_token.kind().into() {
                LuaTokenKind::TkTagParam => Some(DocCompletionExpected::ParamName),
                LuaTokenKind::TkTagCast => Some(DocCompletionExpected::Cast),
                LuaTokenKind::TkTagDiagnostic => Some(DocCompletionExpected::DiagnosticAction),
                LuaTokenKind::TkColon => {
                    let parent = left_token.parent()?;
                    match parent.kind().into() {
                        LuaSyntaxKind::DocTagDiagnostic => {
                            Some(DocCompletionExpected::DiagnosticCode)
                        }
                        _ => None,
                    }
                }
                LuaTokenKind::TkTagNamespace => Some(DocCompletionExpected::Namespace),
                LuaTokenKind::TkTagUsing => Some(DocCompletionExpected::Using),
                LuaTokenKind::TkTagExport => Some(DocCompletionExpected::Export),
                LuaTokenKind::TkComma => {
                    let parent = left_token.parent()?;
                    match parent.kind().into() {
                        LuaSyntaxKind::DocDiagnosticCodeList => {
                            Some(DocCompletionExpected::DiagnosticCode)
                        }
                        LuaSyntaxKind::DocAttribute => Some(DocCompletionExpected::ClassAttr),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        LuaTokenKind::TkColon => {
            let parent = trigger_token.parent()?;
            match parent.kind().into() {
                LuaSyntaxKind::DocTagDiagnostic => Some(DocCompletionExpected::DiagnosticCode),
                _ => None,
            }
        }
        LuaTokenKind::TkComma => {
            let parent = trigger_token.parent()?;
            match parent.kind().into() {
                LuaSyntaxKind::DocDiagnosticCodeList => Some(DocCompletionExpected::DiagnosticCode),
                LuaSyntaxKind::DocAttribute => Some(DocCompletionExpected::ClassAttr),
                _ => None,
            }
        }
        LuaTokenKind::TkLeftParen => {
            let parent = trigger_token.parent()?;
            match parent.kind().into() {
                LuaSyntaxKind::DocAttribute => Some(DocCompletionExpected::ClassAttr),
                _ => None,
            }
        }
        _ => return None,
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum DocCompletionExpected {
    ParamName,
    Cast,
    DiagnosticAction,
    DiagnosticCode,
    ClassAttr,
    Namespace,
    Using,
    Export,
}

fn add_tag_param_name_completion(builder: &mut CompletionBuilder) -> Option<()> {
    let node = match builder.trigger_token.kind().into() {
        LuaTokenKind::TkWhitespace => {
            let left = builder.trigger_token.prev_token()?;
            left.parent()?
        }
        _ => builder.trigger_token.parent()?,
    };
    let ast_node = LuaAst::cast(node)?;

    let comment = ast_node.ancestors::<LuaComment>().next()?;
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
    let actions = vec!["disable", "disable-next-line", "disable-line", "enable"];
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

fn add_tag_class_attr_completion(builder: &mut CompletionBuilder) {
    let attributes = [
        (LuaTypeAttribute::Partial, "partial"),
        (LuaTypeAttribute::Key, "key"),
        (LuaTypeAttribute::Constructor, "constructor"),
        (LuaTypeAttribute::Exact, "exact"),
        (LuaTypeAttribute::Meta, "meta"),
    ];

    // 已存在的属性
    let mut existing_attrs = HashSet::new();
    match builder.trigger_token.kind().into() {
        LuaTokenKind::TkLeftParen | LuaTokenKind::TkComma => {
            let parent = builder.trigger_token.parent().unwrap();
            let attr = LuaDocAttribute::cast(parent).unwrap();
            for token in attr.get_attrib_tokens() {
                let name_text = token.get_name_text().to_string();
                existing_attrs.insert(name_text);
            }
        }
        LuaTokenKind::TkWhitespace => {
            let left_token = builder.trigger_token.prev_token().unwrap();
            match left_token.kind().into() {
                LuaTokenKind::TkComma => {
                    let parent = left_token.parent().unwrap();
                    let attr = LuaDocAttribute::cast(parent).unwrap();
                    for token in attr.get_attrib_tokens() {
                        let name_text = token.get_name_text().to_string();
                        existing_attrs.insert(name_text);
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }

    for (_, name) in attributes.iter() {
        if existing_attrs.contains(*name) {
            continue;
        }
        let completion_item = CompletionItem {
            label: name.to_string(),
            kind: Some(lsp_types::CompletionItemKind::ENUM_MEMBER),
            ..Default::default()
        };
        builder.add_completion_item(completion_item);
    }
}

fn add_tag_namespace_completion(builder: &mut CompletionBuilder) {
    let type_index = builder.semantic_model.get_db().get_type_index();
    let file_id = builder.semantic_model.get_file_id();
    if type_index.get_file_namespace(&file_id).is_some() {
        return;
    }
    let mut namespaces = type_index.get_file_namespaces();

    namespaces.sort();

    for (sorted_index, namespace) in namespaces.iter().enumerate() {
        let completion_item = CompletionItem {
            label: namespace.clone(),
            kind: Some(lsp_types::CompletionItemKind::MODULE),
            sort_text: Some(format!("{:03}", sorted_index)),
            ..Default::default()
        };
        builder.add_completion_item(completion_item);
    }
}

fn add_tag_using_completion(builder: &mut CompletionBuilder) {
    let type_index = builder.semantic_model.get_db().get_type_index();
    let file_id = builder.semantic_model.get_file_id();
    let current_namespace = type_index.get_file_namespace(&file_id);
    let mut namespaces = type_index.get_file_namespaces();
    if let Some(current_namespace) = current_namespace {
        namespaces.retain(|namespace| namespace != current_namespace);
    }
    namespaces.sort();

    for (sorted_index, namespace) in namespaces.iter().enumerate() {
        let completion_item = CompletionItem {
            label: format!("using {}", namespace),
            kind: Some(lsp_types::CompletionItemKind::MODULE),
            sort_text: Some(format!("{:03}", sorted_index)),
            insert_text: Some(format!("{}", namespace)),
            ..Default::default()
        };
        builder.add_completion_item(completion_item);
    }
}

fn add_tag_export_completion(builder: &mut CompletionBuilder) {
    let key = vec!["namespace", "global"];
    for (sorted_index, key) in key.iter().enumerate() {
        let completion_item = CompletionItem {
            label: key.to_string(),
            kind: Some(lsp_types::CompletionItemKind::ENUM_MEMBER),
            sort_text: Some(format!("{:03}", sorted_index)),
            ..Default::default()
        };
        builder.add_completion_item(completion_item);
    }
}
