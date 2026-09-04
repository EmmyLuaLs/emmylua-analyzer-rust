//! Doc tag name token completion: `@param` parameter names / `@cast` local names / `@diagnostic` actions and codes / type flags.

use std::collections::HashSet;

use emmylua_code_analysis::{DiagnosticCode, LuaTypeFlag};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaClosureExpr, LuaComment, LuaDocTag, LuaDocTypeFlag, LuaSyntaxKind,
    LuaSyntaxToken, LuaTokenKind,
};
use lsp_types::{CompletionItem, CompletionItemTag, Documentation, MarkupContent, MarkupKind};

use crate::handlers::completion::completion_builder::CompletionBuilder;

use super::{CompletionProvider, ProviderDecision, env_provider::visible_local_decls};

pub struct DocNameTokenProvider;

impl CompletionProvider for DocNameTokenProvider {
    fn name(&self) -> &'static str {
        "doc_name_token"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        get_doc_completion_expected(&builder.trigger_token).is_some()
    }

    fn complete(&self, builder: &mut CompletionBuilder) -> ProviderDecision {
        if complete_provider(builder).is_some() {
            ProviderDecision::Stop
        } else {
            ProviderDecision::NoMatch
        }
    }
}

fn complete_provider(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let expected = get_doc_completion_expected(&builder.trigger_token)?;
    match expected {
        DocCompletionExpected::ParamName => add_tag_param_name_completion(builder),
        DocCompletionExpected::Cast => add_tag_cast_name_completion(builder),
        DocCompletionExpected::DiagnosticAction => {
            add_tag_diagnostic_action_completion(builder);
            Some(())
        }
        DocCompletionExpected::DiagnosticCode => {
            add_tag_diagnostic_code_completion(builder);
            Some(())
        }
        DocCompletionExpected::TypeFlag(node) => add_tag_type_flag_completion(builder, node),
        DocCompletionExpected::Namespace => {
            add_tag_namespace_completion(builder);
            Some(())
        }
        DocCompletionExpected::Using => {
            add_tag_using_completion(builder);
            Some(())
        }
    }
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
                LuaTokenKind::TkComma => {
                    let parent = left_token.parent()?;
                    match parent.kind().into() {
                        LuaSyntaxKind::DocDiagnosticCodeList => {
                            Some(DocCompletionExpected::DiagnosticCode)
                        }
                        LuaSyntaxKind::DocTypeFlag => Some(DocCompletionExpected::TypeFlag(
                            LuaDocTypeFlag::cast(parent.clone())?,
                        )),
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
                LuaSyntaxKind::DocTypeFlag => Some(DocCompletionExpected::TypeFlag(
                    LuaDocTypeFlag::cast(parent.clone())?,
                )),
                _ => None,
            }
        }
        LuaTokenKind::TkLeftParen => {
            let parent = trigger_token.parent()?;
            match parent.kind().into() {
                LuaSyntaxKind::DocTypeFlag => Some(DocCompletionExpected::TypeFlag(
                    LuaDocTypeFlag::cast(parent.clone())?,
                )),
                _ => None,
            }
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum DocCompletionExpected {
    ParamName,
    Cast,
    DiagnosticAction,
    DiagnosticCode,
    TypeFlag(LuaDocTypeFlag),
    Namespace,
    Using,
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
    let mut duplicated_name = HashSet::new();
    let local_env = visible_local_decls(&builder.semantic_model, builder.position_offset);
    for decl_id in local_env {
        let Some(facts) = builder.semantic_model.file_facts() else {
            continue;
        };
        let Some(decl) = facts.decl_by_id(&decl_id) else {
            continue;
        };
        let name = decl.name.to_string();
        if !duplicated_name.insert(name.clone()) {
            continue;
        }
        builder.add_completion_item(CompletionItem {
            label: name,
            kind: Some(lsp_types::CompletionItemKind::VARIABLE),
            ..Default::default()
        });
    }
    Some(())
}

fn add_tag_diagnostic_action_completion(builder: &mut CompletionBuilder) {
    let actions = ["disable", "disable-next-line", "disable-line", "enable"];
    for (sorted_index, action) in actions.iter().enumerate() {
        builder.add_completion_item(CompletionItem {
            label: action.to_string(),
            kind: Some(lsp_types::CompletionItemKind::EVENT),
            sort_text: Some(format!("{:03}", sorted_index)),
            ..Default::default()
        });
    }
}

fn add_tag_diagnostic_code_completion(builder: &mut CompletionBuilder) {
    let codes = DiagnosticCode::all();
    for (sorted_index, code) in codes.iter().enumerate() {
        builder.add_completion_item(CompletionItem {
            label: code.get_name().to_string(),
            kind: Some(lsp_types::CompletionItemKind::EVENT),
            sort_text: Some(format!("{:03}", sorted_index)),
            ..Default::default()
        });
    }
}

#[derive(Clone, Copy)]
enum TypeFlagCompletion {
    Key,
    Partial,
    Exact,
    Constructor,
    Public,
    Internal,
    File,
    Private,
}

impl TypeFlagCompletion {
    fn iter() -> impl Iterator<Item = Self> {
        [
            Self::Key,
            Self::Partial,
            Self::Exact,
            Self::Constructor,
            Self::Public,
            Self::Internal,
            Self::File,
            Self::Private,
        ]
        .into_iter()
    }

    fn flag(self) -> LuaTypeFlag {
        match self {
            Self::Key => LuaTypeFlag::Key,
            Self::Partial => LuaTypeFlag::Partial,
            Self::Exact => LuaTypeFlag::Exact,
            Self::Constructor => LuaTypeFlag::Constructor,
            Self::Public => LuaTypeFlag::Public,
            Self::Internal => LuaTypeFlag::Internal,
            Self::File | Self::Private => LuaTypeFlag::File,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Partial => "partial",
            Self::Exact => "exact",
            Self::Constructor => "constructor",
            Self::Public => "public",
            Self::Internal => "internal",
            Self::File => "file",
            Self::Private => "private",
        }
    }

    fn is_deprecated(self) -> bool {
        matches!(self, Self::Private)
    }
}

fn add_tag_type_flag_completion(
    builder: &mut CompletionBuilder,
    node: LuaDocTypeFlag,
) -> Option<()> {
    let flags: &[TypeFlagCompletion] = match LuaDocTag::cast(node.syntax().parent()?)? {
        LuaDocTag::Alias(_) => &[
            TypeFlagCompletion::Internal,
            TypeFlagCompletion::File,
            TypeFlagCompletion::Public,
            TypeFlagCompletion::Private,
        ],
        LuaDocTag::Class(_) => &[
            TypeFlagCompletion::Partial,
            TypeFlagCompletion::Internal,
            TypeFlagCompletion::Exact,
            TypeFlagCompletion::Constructor,
            TypeFlagCompletion::File,
            TypeFlagCompletion::Public,
            TypeFlagCompletion::Private,
        ],
        LuaDocTag::Enum(_) => &[
            TypeFlagCompletion::Key,
            TypeFlagCompletion::Partial,
            TypeFlagCompletion::Internal,
            TypeFlagCompletion::File,
            TypeFlagCompletion::Public,
            TypeFlagCompletion::Private,
        ],
        _ => &[],
    };

    let mut existing_flags = Vec::new();
    for token in node.get_attrib_tokens() {
        let name_text = token.get_name_text();
        if let Some(completion) =
            TypeFlagCompletion::iter().find(|completion| completion.label() == name_text)
        {
            existing_flags.push(completion.flag());
        }
    }

    for (sorted_index, completion) in flags.iter().enumerate() {
        if existing_flags.contains(&completion.flag()) {
            continue;
        }
        let label = completion.label();
        let completion_item = CompletionItem {
            label: label.to_string(),
            kind: Some(lsp_types::CompletionItemKind::ENUM_MEMBER),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: t!(format!("completion.typeFlag.{}", label)).to_string(),
            })),
            sort_text: Some(format!("{:03}", sorted_index)),
            tags: completion
                .is_deprecated()
                .then(|| vec![CompletionItemTag::DEPRECATED]),
            ..Default::default()
        };
        builder.add_completion_item(completion_item);
    }

    Some(())
}

fn all_file_namespaces(builder: &CompletionBuilder) -> Vec<String> {
    let mut namespaces = HashSet::new();
    for file_id in builder.semantic_model.main_workspace_file_ids() {
        let Some(model) = builder.semantic_model.model_for(file_id) else {
            continue;
        };
        let Some(facts) = model.file_facts() else {
            continue;
        };
        if let Some(namespace) = &facts.namespace {
            namespaces.insert(namespace.to_string());
        }
    }
    let mut namespaces: Vec<_> = namespaces.into_iter().collect();
    namespaces.sort();
    namespaces
}

fn add_tag_namespace_completion(builder: &mut CompletionBuilder) {
    let current = builder
        .semantic_model
        .file_facts()
        .and_then(|facts| facts.namespace.clone());
    if current.is_some() {
        return;
    }
    let namespaces = all_file_namespaces(builder);
    for (sorted_index, namespace) in namespaces.iter().enumerate() {
        builder.add_completion_item(CompletionItem {
            label: namespace.clone(),
            kind: Some(lsp_types::CompletionItemKind::MODULE),
            sort_text: Some(format!("{:03}", sorted_index)),
            ..Default::default()
        });
    }
}

fn add_tag_using_completion(builder: &mut CompletionBuilder) {
    let current_namespace = builder
        .semantic_model
        .file_facts()
        .and_then(|facts| facts.namespace.as_ref())
        .map(|namespace| namespace.to_string());
    let mut namespaces = all_file_namespaces(builder);
    if let Some(current_namespace) = current_namespace {
        namespaces.retain(|namespace| namespace != &current_namespace);
    }

    for (sorted_index, namespace) in namespaces.iter().enumerate() {
        builder.add_completion_item(CompletionItem {
            label: format!("using {}", namespace),
            kind: Some(lsp_types::CompletionItemKind::MODULE),
            sort_text: Some(format!("{:03}", sorted_index)),
            insert_text: Some(namespace.to_string()),
            ..Default::default()
        });
    }
}
