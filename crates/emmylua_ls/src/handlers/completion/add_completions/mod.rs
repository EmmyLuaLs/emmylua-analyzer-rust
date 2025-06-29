mod add_decl_completion;
mod add_member_completion;
mod check_match_word;

pub use add_decl_completion::add_decl_completion;
pub use add_member_completion::extract_index_member_alias;
pub use add_member_completion::{add_member_completion, CompletionTriggerStatus};
pub use check_match_word::check_match_word;
use emmylua_code_analysis::{LuaSemanticDeclId, LuaType, RenderLevel};
use lsp_types::CompletionItemKind;

use emmylua_code_analysis::humanize_type;

use super::completion_builder::CompletionBuilder;

pub fn check_visibility(builder: &mut CompletionBuilder, id: LuaSemanticDeclId) -> Option<()> {
    match id {
        LuaSemanticDeclId::Member(_) => {}
        LuaSemanticDeclId::LuaDecl(_) => {}
        _ => return Some(()),
    }

    if !builder
        .semantic_model
        .is_semantic_visible(builder.trigger_token.clone(), id)
    {
        return None;
    }

    Some(())
}

fn get_completion_kind(typ: &LuaType) -> CompletionItemKind {
    if typ.is_function() {
        return CompletionItemKind::FUNCTION;
    } else if typ.is_const() {
        return CompletionItemKind::CONSTANT;
    } else if typ.is_def() {
        return CompletionItemKind::CLASS;
    } else if typ.is_namespace() {
        return CompletionItemKind::MODULE;
    }

    CompletionItemKind::VARIABLE
}

pub fn is_deprecated(builder: &CompletionBuilder, id: LuaSemanticDeclId) -> bool {
    let property = builder
        .semantic_model
        .get_db()
        .get_property_index()
        .get_property(&id);

    if let Some(property) = property {
        if property.deprecated.is_some() {
            return true;
        }
    }

    false
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CallDisplay {
    None,
    AddSelf,
    RemoveFirst,
}

pub fn get_detail(
    builder: &CompletionBuilder,
    typ: &LuaType,
    display: CallDisplay,
) -> Option<String> {
    match typ {
        LuaType::Signature(signature_id) => {
            let signature = builder
                .semantic_model
                .get_db()
                .get_signature_index()
                .get(&signature_id)?;

            let mut params_str = signature
                .get_type_params()
                .iter()
                .map(|param| param.0.clone())
                .collect::<Vec<_>>();

            match display {
                CallDisplay::AddSelf => {
                    params_str.insert(0, "self".to_string());
                }
                CallDisplay::RemoveFirst => {
                    if !params_str.is_empty() {
                        params_str.remove(0);
                    }
                }
                _ => {}
            }
            let rets = &signature.return_docs;
            let rets_detail = if rets.len() == 1 {
                let detail = humanize_type(
                    builder.semantic_model.get_db(),
                    &rets[0].type_ref,
                    RenderLevel::Minimal,
                );
                format!(" -> {}", detail)
            } else if rets.len() > 1 {
                let detail = humanize_type(
                    builder.semantic_model.get_db(),
                    &rets[0].type_ref,
                    RenderLevel::Minimal,
                );
                format!(" -> {} ...", detail)
            } else {
                "".to_string()
            };

            Some(format!("({}){}", params_str.join(", "), rets_detail))
        }
        LuaType::DocFunction(f) => {
            let mut params_str = f
                .get_params()
                .iter()
                .map(|param| param.0.clone())
                .collect::<Vec<_>>();

            match display {
                CallDisplay::AddSelf => {
                    params_str.insert(0, "self".to_string());
                }
                CallDisplay::RemoveFirst => {
                    if !params_str.is_empty() {
                        params_str.remove(0);
                    }
                }
                _ => {}
            }
            let ret_type = f.get_ret();
            let rets_detail = match ret_type {
                LuaType::Nil => "".to_string(),
                _ => {
                    let type_detail = humanize_type(
                        builder.semantic_model.get_db(),
                        &ret_type,
                        RenderLevel::Minimal,
                    );
                    format!("-> {}", type_detail)
                }
            };
            Some(format!("({}){}", params_str.join(", "), rets_detail))
        }
        _ => None,
    }
}

#[allow(unused)]
fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let truncated: String = s.chars().take(max_len).collect();
        format!("   {}...", truncated)
    } else {
        format!("   {}", s)
    }
}

fn get_description(builder: &CompletionBuilder, typ: &LuaType) -> Option<String> {
    match typ {
        LuaType::Signature(_) => None,
        LuaType::DocFunction(_) => None,
        _ if typ.is_unknown() => None,
        _ => Some(humanize_type(
            builder.semantic_model.get_db(),
            typ,
            RenderLevel::Minimal,
        )),
    }
}
