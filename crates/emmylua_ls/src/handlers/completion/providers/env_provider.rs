//! Env completion: visible locals/params/globals at the current position + current file type definitions.

use std::collections::HashSet;

use emmylua_code_analysis::{DeclKind, LuaType, SalsaSemanticModel};
use emmylua_parser::{LuaAst, LuaAstNode, LuaCallArgList, LuaParamList, LuaTokenKind};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionTriggerKind,
};

use crate::handlers::completion::completion_builder::CompletionBuilder;
use crate::handlers::completion::completion_data::{CompletionData, CompletionDataType};
use crate::handlers::completion::providers::keywords_provider::check_match_word;
use crate::handlers::hover::render::humanize;

use super::{CompletionProvider, ProviderDecision};

pub struct EnvProvider;

impl CompletionProvider for EnvProvider {
    fn name(&self) -> &'static str {
        "env"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        supports_provider(builder)
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
    if !supports_provider(builder) {
        return Some(());
    }

    let parent_node = LuaAst::cast(builder.trigger_token.parent()?)?;
    match parent_node {
        LuaAst::LuaNameExpr(_) => {}
        LuaAst::LuaBlock(_) => {}
        LuaAst::LuaClosureExpr(_) => {}
        LuaAst::LuaCallArgList(_) => {}
        // Completions triggered inside strings are delegated to the context provider.
        LuaAst::LuaLiteralExpr(_) => return None,
        _ => return None,
    };

    let mut duplicated_name = HashSet::new();
    if has_std_library(&builder.semantic_model) {
        add_builtin_types(builder, &mut duplicated_name);
    }
    add_local_env(builder, &mut duplicated_name);
    add_global_env(builder, &mut duplicated_name);
    builder.env_duplicate_name.extend(duplicated_name);

    Some(())
}

fn supports_provider(builder: &CompletionBuilder) -> bool {
    if builder.is_space_trigger_character {
        return false;
    }

    let trigger_text = builder.get_trigger_text();
    if builder.trigger_kind == CompletionTriggerKind::TRIGGER_CHARACTER {
        let Some(parent) = builder.trigger_token.parent() else {
            return false;
        };
        if trigger_text == "("
            && (LuaCallArgList::can_cast(parent.kind().into())
                || LuaParamList::can_cast(parent.kind().into()))
        {
            return false;
        }
    } else if builder.trigger_kind == CompletionTriggerKind::INVOKED {
        let Some(parent) = builder.trigger_token.parent() else {
            return false;
        };
        if let Some(prev_token) = builder.trigger_token.prev_token() {
            match prev_token.kind().into() {
                LuaTokenKind::TkTagUsing | LuaTokenKind::TkTagNamespace => {
                    return false;
                }
                _ => {}
            }
        }
        // Do not provide ordinary env completions in function-definition parameter lists.
        if trigger_text == "(" && LuaParamList::can_cast(parent.kind().into()) {
            return false;
        }
    }

    true
}

fn has_std_library(model: &SalsaSemanticModel<'_>) -> bool {
    let db = model.db();
    db.file_ids()
        .iter()
        .filter_map(|file_id| db.file_path(*file_id))
        .any(|path| {
            let text = path.to_string_lossy();
            text.contains("resources") || text.contains("std")
        })
}

fn add_builtin_types(builder: &mut CompletionBuilder, duplicated: &mut HashSet<String>) {
    let partial = builder.partial_name();
    for builtin in ["table", "any", "unknown", "void"] {
        if builtin.starts_with(&partial) && duplicated.insert(builtin.to_string()) {
            builder.add_completion_item(CompletionItem {
                label: builtin.to_string(),
                kind: Some(CompletionItemKind::CLASS),
                insert_text: Some(builtin.to_string()),
                ..Default::default()
            });
        }
    }
}

/// Declarations visible in the lexical scope at the current position (facts visibility scope).
pub fn visible_local_decls(
    model: &SalsaSemanticModel<'_>,
    position_offset: rowan::TextSize,
) -> Vec<emmylua_code_analysis::SemanticId> {
    let Some(facts) = model.file_facts() else {
        return Vec::new();
    };
    facts
        .visible_decls_at_offset(position_offset)
        .into_iter()
        .filter(|decl| !matches!(decl.kind, DeclKind::Global))
        .map(|decl| decl.id.clone())
        .collect()
}

fn add_local_env(
    builder: &mut CompletionBuilder,
    duplicated_name: &mut HashSet<String>,
) -> Option<()> {
    let trigger_text = builder.get_trigger_text();
    let decls = visible_local_decls(&builder.semantic_model, builder.position_offset);
    for decl_id in decls {
        let facts = builder.semantic_model.file_facts()?;
        let decl = facts.decl_by_id(&decl_id)?;
        let name = decl.name.to_string();
        let typ = builder
            .semantic_model
            .type_of_decl(&decl_id)
            .unwrap_or(LuaType::Unknown);
        if is_typing_name(builder, &decl_id) || duplicated_name.contains(&name) {
            continue;
        }
        if !env_check_match_word(&trigger_text, name.as_str()) {
            duplicated_name.insert(name.clone());
            continue;
        }
        duplicated_name.insert(name.clone());
        add_decl_completion(builder, &decl_id, &name, &typ);
    }
    Some(())
}

pub fn add_global_env(
    builder: &mut CompletionBuilder,
    duplicated_name: &mut HashSet<String>,
) -> Option<()> {
    let trigger_text = builder.get_trigger_text();
    let db = builder.semantic_model.db();
    // Global env includes the standard library (`table`/`any` etc. from std `---@class` / global declarations).
    let mut file_ids = db.file_ids();
    file_ids.sort();
    let mut globals = Vec::new();
    for file_id in file_ids {
        let Some(model) = SalsaSemanticModel::new(db, file_id) else {
            continue;
        };
        let Some(exports) = model.file_exports_current() else {
            continue;
        };
        for global in exports.globals.iter() {
            globals.push((global.decl.clone(), global.name.to_string()));
        }
    }

    for (decl_id, name) in globals {
        let typ = builder
            .semantic_model
            .type_of_decl(&decl_id)
            .unwrap_or(LuaType::Unknown);
        if is_typing_name(builder, &decl_id) || duplicated_name.contains(&name) {
            continue;
        }
        if !env_check_match_word(&trigger_text, name.as_str()) {
            duplicated_name.insert(name.clone());
            continue;
        }
        // Do not repeat a global with the same name that is currently being defined.
        if let Some(current) = builder.semantic_model.resolve_name(builder.position_offset)
            && current == decl_id
        {
            continue;
        }
        duplicated_name.insert(name.clone());
        add_decl_completion(builder, &decl_id, &name, &typ);
    }
    Some(())
}

pub fn env_check_match_word(trigger_text: &str, name: &str) -> bool {
    // After `(` or `,` allow any candidate (completion triggered at function call arguments).
    if matches!(trigger_text.chars().next(), Some('(') | Some(',')) {
        return true;
    }
    check_match_word(trigger_text, name)
}

fn is_typing_name(
    builder: &CompletionBuilder,
    decl_id: &emmylua_code_analysis::SemanticId,
) -> bool {
    matches!(
        decl_id,
        emmylua_code_analysis::SemanticId::Decl(key)
            if key.name_range == builder.trigger_token.text_range()
    )
}

fn add_decl_completion(
    builder: &mut CompletionBuilder,
    decl_id: &emmylua_code_analysis::SemanticId,
    name: &str,
    typ: &LuaType,
) {
    let _ = decl_id;
    // `---@class Test1` followed by `local Test = {}`: even if the type name differs from the
    // variable name, the type definition belongs to the same owner statement, so name completion
    // shows it as a class.
    let associated_type = builder
        .semantic_model
        .file_facts()
        .and_then(|facts| facts.decl_by_id(decl_id))
        .and_then(|decl| decl.owner_syntax)
        .and_then(|owner| {
            builder.semantic_model.file_facts().and_then(|facts| {
                facts
                    .type_defs
                    .iter()
                    .find(|def| def.owner_syntax == Some(owner))
            })
        });

    let kind = if typ.is_function() {
        CompletionItemKind::FUNCTION
    } else if associated_type.is_some() || typ.is_def() {
        CompletionItemKind::CLASS
    } else if typ.is_namespace() {
        CompletionItemKind::MODULE
    } else if typ.is_const() {
        CompletionItemKind::CONSTANT
    } else {
        CompletionItemKind::VARIABLE
    };

    // Function signatures appear in the detail next to the label; ordinary types appear in the description further right.
    let (detail, description) =
        if let Some(func) = builder.semantic_model.type_of_decl_signature(decl_id) {
            let params = func
                .get_params()
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>()
                .join(", ");
            (Some(format!("({params})")), None)
        } else {
            match typ {
                LuaType::DocFunction(func) => {
                    let params = func
                        .get_params()
                        .iter()
                        .map(|(name, _)| name.clone())
                        .collect::<Vec<_>>()
                        .join(", ");
                    (Some(format!("({params})")), None)
                }
                LuaType::Unknown if associated_type.is_none() => (None, None),
                other => {
                    let text = associated_type
                        .map(|def| def.name.to_string())
                        .unwrap_or_else(|| humanize(&builder.semantic_model, other));
                    (None, Some(text))
                }
            }
        };
    let data = match decl_id {
        emmylua_code_analysis::SemanticId::Decl(key) => CompletionData {
            field_id: builder.semantic_model.file_id().id,
            trigger_offset: Some(builder.position_offset.into()),
            typ: CompletionDataType::Decl {
                file_id: key.file_id.id,
                range: (key.name_range.start().into(), key.name_range.end().into()),
            },
        }
        .to_value(),
        _ => None,
    };
    builder.add_completion_item(CompletionItem {
        label: name.to_string(),
        kind: Some(kind),
        insert_text: Some(name.to_string()),
        detail: None,
        label_details: Some(CompletionItemLabelDetails {
            detail,
            description,
        }),
        data,
        ..Default::default()
    });
}
