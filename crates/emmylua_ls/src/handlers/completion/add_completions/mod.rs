mod add_decl_completion;
mod add_member_completion;
mod check_match_word;

pub use add_decl_completion::add_decl_completion;
pub use add_member_completion::{add_member_completion, CompletionTriggerStatus};
pub use check_match_word::check_match_word;
use emmylua_code_analysis::{FileId, LuaSemanticDeclId, LuaType, RenderLevel};
use lsp_types::CompletionItemKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
        .is_property_visible(builder.trigger_token.clone(), id)
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
    if property.is_none() {
        return false;
    }

    property.unwrap().is_deprecated
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CallDisplay {
    None,
    AddSelf,
    RemoveFirst,
}

fn get_detail(builder: &CompletionBuilder, typ: &LuaType, display: CallDisplay) -> Option<String> {
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
            let rets = f.get_ret();
            let rets_detail = if rets.len() == 1 {
                let detail = humanize_type(
                    builder.semantic_model.get_db(),
                    &rets[0],
                    RenderLevel::Minimal,
                );
                format!(" -> {}", detail)
            } else if rets.len() > 1 {
                let detail = humanize_type(
                    builder.semantic_model.get_db(),
                    &rets[0],
                    RenderLevel::Minimal,
                );
                format!(" -> {} ...", detail)
            } else {
                "".to_string()
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompletionDataType {
    PropertyOwnerId(LuaSemanticDeclId),
    Module(String),
    Overload((LuaSemanticDeclId, usize)),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionData {
    pub field_id: FileId,
    pub typ: CompletionDataType,
}

#[allow(unused)]
impl CompletionData {
    pub fn from_property_owner_id(
        builder: &CompletionBuilder,
        id: LuaSemanticDeclId,
    ) -> Option<Value> {
        let data = Self {
            field_id: builder.semantic_model.get_file_id(),
            typ: CompletionDataType::PropertyOwnerId(id),
        };
        Some(serde_json::to_value(data).unwrap())
    }

    pub fn from_overload(
        builder: &CompletionBuilder,
        id: LuaSemanticDeclId,
        index: usize,
    ) -> Option<Value> {
        let data = Self {
            field_id: builder.semantic_model.get_file_id(),
            typ: CompletionDataType::Overload((id, index)),
        };
        Some(serde_json::to_value(data).unwrap())
    }

    pub fn from_module(builder: &CompletionBuilder, module: String) -> Option<Value> {
        let data = Self {
            field_id: builder.semantic_model.get_file_id(),
            typ: CompletionDataType::Module(module),
        };
        Some(serde_json::to_value(data).unwrap())
    }
}
