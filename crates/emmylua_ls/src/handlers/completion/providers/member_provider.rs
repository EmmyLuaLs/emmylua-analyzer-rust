//! Member completion: `.` / `:` / `[]` / string-key contexts, based on salsa `member_infos`.

use emmylua_code_analysis::{
    DeclKind, FileId, LuaAliasCallKind, LuaFunctionType, LuaGenericType, LuaMemberKey, LuaType,
    LuaTypeDeclId, SalsaMemberInfo, SalsaSemanticModel, SemanticId, TypeDefKind,
};
use emmylua_parser::{LuaAstNode, LuaAstToken, LuaIndexExpr, LuaStringToken};
use lsp_types::{CompletionItem, CompletionItemKind, CompletionItemLabelDetails, InsertTextFormat};

use crate::handlers::completion::{
    completion_builder::CompletionBuilder,
    completion_data::{CompletionData, CompletionDataType},
};

use super::{CompletionProvider, ProviderDecision};

pub struct MemberProvider;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompletionTriggerStatus {
    Dot,
    Colon,
    InString,
    LeftBracket,
}

/// Add completion items for a group of member infos (reused by the desc provider; Dot display semantics).
pub(crate) fn add_member_completions(
    builder: &mut CompletionBuilder,
    members: &[SalsaMemberInfo],
) -> Option<()> {
    for member in members {
        add_member_completion(builder, member.clone(), CompletionTriggerStatus::Dot)?;
    }
    Some(())
}

impl CompletionProvider for MemberProvider {
    fn name(&self) -> &'static str {
        "member"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        builder
            .trigger_token
            .parent()
            .and_then(LuaIndexExpr::cast)
            .is_some()
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

    let index_expr = LuaIndexExpr::cast(builder.trigger_token.parent()?)?;
    let index_token = index_expr.get_index_token()?;
    let completion_status = if index_token.is_dot() || index_token.is_safe_navigation() {
        CompletionTriggerStatus::Dot
    } else if index_token.is_colon() {
        CompletionTriggerStatus::Colon
    } else if LuaStringToken::can_cast(builder.trigger_token.kind().into()) {
        CompletionTriggerStatus::InString
    } else {
        CompletionTriggerStatus::LeftBracket
    };

    let prefix_expr = index_expr.get_prefix_expr()?;
    let prefix_type = builder.member_prefix_type().or_else(|| {
        Some(
            builder
                .semantic_model
                .type_of_expr(prefix_expr.get_syntax_id()),
        )
    })?;
    let prefix_type = match &prefix_type {
        LuaType::TplRef(tpl) => tpl
            .get_constraint()
            .filter(|constraint| !constraint_refers_to_tpl(constraint, tpl.get_name()))
            .cloned()
            .or_else(|| param_tpl_constraint(&builder.semantic_model, &prefix_expr))
            .unwrap_or(prefix_type),
        // `---@param file T` may project to Ref("T"); fall back to the enclosing signature's generic constraint.
        LuaType::Ref(_) | LuaType::Def(_) => {
            param_tpl_constraint(&builder.semantic_model, &prefix_expr).unwrap_or(prefix_type)
        }
        _ => prefix_type,
    };
    let _ = prefix_expr;

    // Enums are value sets, so do not offer member-access completion (old `enum_variable_is_param` semantics).
    if let LuaType::Ref(type_id) | LuaType::Def(type_id) = &prefix_type
        && builder
            .semantic_model
            .type_def_of(type_id)
            .is_some_and(|def| def.kind == TypeDefKind::Enum)
    {
        return Some(());
    }

    let mut members = builder.semantic_model.member_infos(&prefix_type);
    // `---@class B : A local b = {}`: merge table-identity members with associated type definition members.
    let associated = associated_type_member_infos(&builder.semantic_model, &prefix_expr);
    for info in associated {
        if !members.iter().any(|member| member.key == info.key) {
            members.push(info);
        }
    }
    if members.is_empty() {
        members = match &prefix_type {
            LuaType::Ref(id) | LuaType::Def(id) => {
                type_def_member_infos_from_facts(&builder.semantic_model, id.clone())
            }
            LuaType::Generic(generic) => {
                let from_type = generic_keyof_member_infos(&builder.semantic_model, generic);
                if from_type.is_empty() {
                    let from_super = generic_super_member_infos(&builder.semantic_model, generic);
                    if from_super.is_empty() {
                        doc_generic_keyof_member_infos(
                            &builder.semantic_model,
                            &prefix_expr,
                            generic,
                        )
                    } else {
                        from_super
                    }
                } else {
                    from_type
                }
            }
            _ => associated_type_member_infos(&builder.semantic_model, &prefix_expr),
        };
    }

    // `emmyrc.doc.private_name` (e.g. `_*`): hide private members when accessing through
    // local variables/parameters; do not hide when accessing directly through a type name (global surface).
    if is_local_prefix(&builder.semantic_model, &prefix_expr) {
        let patterns = &builder.get_emmyrc().doc.private_name;
        members.retain(|member| {
            let name = match &member.key {
                LuaMemberKey::Name(name) => name.as_str(),
                _ => return true,
            };
            !matches_private_name(name, patterns)
        });
    }

    if completion_status == CompletionTriggerStatus::Colon {
        for member in members {
            if member.typ.is_function() {
                add_member_completion(builder, member, completion_status)?;
            }
        }
        return Some(());
    }

    for member in members {
        add_member_completion(builder, member, completion_status)?;
    }
    Some(())
}

/// `---@class box<T>: T`: replace parent-type placeholders with argument type members using generic arguments.
fn generic_super_member_infos(
    model: &SalsaSemanticModel<'_>,
    generic: &LuaGenericType,
) -> Vec<SalsaMemberInfo> {
    let base = generic.get_base_type_id();
    let Some(def) = model.type_def_of(&base) else {
        return Vec::new();
    };
    let params = generic.get_params().to_vec();

    // `Partial<AA>`: the old humanize stack expands keyof T to all fields of T.
    if def.kind == TypeDefKind::Alias
        && def.name.as_str() == "Partial"
        && let Some(bound) = params.first()
    {
        return model
            .member_infos(bound)
            .into_iter()
            .map(|info| SalsaMemberInfo { id: None, ..info })
            .collect();
    }

    let mut out = Vec::new();
    let mut seen = Vec::new();
    for super_name in &def.super_names {
        // `box<T>: T`: the parent type is directly a generic placeholder.
        if let Some(index) = def
            .generic_params
            .iter()
            .position(|param| param.name.as_str() == super_name.as_str())
            && let Some(bound) = params.get(index)
        {
            for info in model.member_infos(bound) {
                if seen.contains(&info.key) {
                    continue;
                }
                seen.push(info.key.clone());
                out.push(info);
            }
            continue;
        }
        // `Matchers<T>` / `Inverse<T>`: base name + generic arguments.
        let (base_name, args_text) = match super_name.split_once('<') {
            Some((base, rest)) => (base, Some(rest.trim_end_matches('>').to_string())),
            None => (super_name.as_str(), None),
        };
        let Some(parent) = model.resolve_type_def_in(def.file_id, base_name) else {
            continue;
        };
        let bound_params = if let Some(args_text) = args_text {
            args_text
                .split(',')
                .map(|arg| {
                    let arg = arg.trim();
                    def.generic_params
                        .iter()
                        .position(|param| param.name.as_str() == arg)
                        .and_then(|index| params.get(index).cloned())
                        .unwrap_or_else(|| LuaType::Any)
                })
                .collect()
        } else {
            Vec::new()
        };
        let parent_type = if bound_params.is_empty() {
            model.type_def_ref(&parent)
        } else {
            LuaType::Generic(std::sync::Arc::new(LuaGenericType::new(
                LuaTypeDeclId::global(parent.full_name.as_str()),
                bound_params,
            )))
        };
        for info in model.member_infos(&parent_type) {
            if seen.contains(&info.key) {
                continue;
            }
            seen.push(info.key.clone());
            out.push(info);
        }
    }
    out
}

/// When projection lowers `keyof A` to Unknown, restore from the local declaration's doc-generic parameter text.
fn doc_generic_keyof_member_infos(
    model: &SalsaSemanticModel<'_>,
    prefix_expr: &emmylua_parser::LuaExpr,
    generic: &LuaGenericType,
) -> Vec<SalsaMemberInfo> {
    let emmylua_parser::LuaExpr::NameExpr(name_expr) = prefix_expr else {
        return Vec::new();
    };
    let Some(decl) = model.resolve_name(name_expr.get_position()) else {
        return Vec::new();
    };
    let SemanticId::Decl(key) = &decl else {
        return Vec::new();
    };
    let Some(facts) = model.file_facts_of(key.file_id) else {
        return Vec::new();
    };
    let Some(decl_info) = facts.decl_by_id(&decl) else {
        return Vec::new();
    };
    let Some(doc_type) = decl_info.doc_type_syntax else {
        return Vec::new();
    };
    let Some(tree) = model.syntax_tree() else {
        return Vec::new();
    };
    let Some(node) = doc_type.to_node_from_root(&tree.get_red_root()) else {
        return Vec::new();
    };
    let text = node.text().to_string();
    let text = text.trim();
    let Some(inner) = text
        .strip_prefix("table<")
        .and_then(|text| text.strip_suffix('>'))
    else {
        return Vec::new();
    };
    let Some(first) = inner.split(',').next() else {
        return Vec::new();
    };
    let first = first.trim();
    let Some(type_name) = first.strip_prefix("keyof ") else {
        return Vec::new();
    };
    let type_name = type_name.trim();
    let Some(def) = model.resolve_type_def(type_name) else {
        return Vec::new();
    };
    let _ = generic;
    model
        .member_infos(&model.type_def_ref(&def))
        .into_iter()
        .filter_map(|info| match info.key {
            LuaMemberKey::Name(name) => Some(SalsaMemberInfo {
                key: LuaMemberKey::Name(name),
                typ: LuaType::Unknown,
                id: None,
                file_id: None,
                is_method: false,
            }),
            _ => None,
        })
        .collect()
}

/// `table<keyof A, string>`: member keys of the keyof enum generic argument become member candidates.
fn generic_keyof_member_infos(
    model: &SalsaSemanticModel<'_>,
    generic: &LuaGenericType,
) -> Vec<SalsaMemberInfo> {
    let Some(operand) = generic.get_params().first() else {
        return Vec::new();
    };
    let LuaType::Call(call) = operand else {
        return Vec::new();
    };
    if call.get_call_kind() != LuaAliasCallKind::KeyOf {
        return Vec::new();
    }
    let Some(operand) = call.get_operands().first() else {
        return Vec::new();
    };
    model
        .member_infos(operand)
        .into_iter()
        .filter_map(|info| match info.key {
            LuaMemberKey::Name(name) => Some(SalsaMemberInfo {
                key: LuaMemberKey::Name(name),
                typ: LuaType::Unknown,
                id: None,
                file_id: None,
                is_method: false,
            }),
            _ => None,
        })
        .collect()
}

/// When cross-file `member_infos` is empty, directly scan runtime members in the type definition file's facts.
fn type_def_member_infos_from_facts(
    model: &SalsaSemanticModel<'_>,
    type_id: LuaTypeDeclId,
) -> Vec<SalsaMemberInfo> {
    let Some(def) = model.type_def_of(&type_id) else {
        return Vec::new();
    };
    let Some(facts) = model.file_facts_of(def.file_id) else {
        return Vec::new();
    };
    facts
        .members
        .iter()
        .map(|member| {
            let key = member.key.clone();
            SalsaMemberInfo {
                key,
                typ: model.type_of_member(&member.id).unwrap_or(LuaType::Unknown),
                id: Some(member.id.clone()),
                file_id: Some(def.file_id),
                is_method: member.is_method,
            }
        })
        .collect()
}

/// When `---@class Test1` follows `local Test = {}` (the class name differs from the runtime
/// variable name), `member_infos` may return empty for the table identity; here complete members
/// directly from the type definition associated with the same owner statement.
fn associated_type_member_infos(
    model: &SalsaSemanticModel<'_>,
    prefix_expr: &emmylua_parser::LuaExpr,
) -> Vec<SalsaMemberInfo> {
    let emmylua_parser::LuaExpr::NameExpr(name_expr) = prefix_expr else {
        return Vec::new();
    };
    let Some(decl) = model.resolve_name(name_expr.get_position()) else {
        return Vec::new();
    };
    let SemanticId::Decl(key) = &decl else {
        return Vec::new();
    };
    let Some(facts) = model.file_facts_of(key.file_id) else {
        return Vec::new();
    };
    let Some(decl_info) = facts.decl_by_id(&decl) else {
        return Vec::new();
    };
    let Some(def) = facts
        .type_defs
        .iter()
        .find(|def| def.owner_syntax.is_some() && def.owner_syntax == decl_info.owner_syntax)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen_keys = Vec::new();
    let mut owners = vec![def.id.clone(), decl.clone()];
    owners.dedup();
    let mut super_stack = def.super_names.clone();
    let mut visited_supers = Vec::new();
    while let Some(super_name) = super_stack.pop() {
        if visited_supers.contains(&super_name) {
            continue;
        }
        visited_supers.push(super_name.clone());
        if let Some(parent) = model.resolve_type_def_in(def.file_id, super_name.as_str()) {
            owners.push(parent.id.clone());
            super_stack.extend(parent.super_names.iter().cloned());
        }
    }
    owners.dedup();
    for owner in owners {
        for member_ref in model.members_of_owner(&owner) {
            let Some(member_facts) = model.file_facts_of(member_ref.file_id) else {
                continue;
            };
            let Some(member) = member_facts.member_by_id(&member_ref.id) else {
                continue;
            };
            let key = member.key.clone();
            if seen_keys.contains(&key) {
                continue;
            }
            seen_keys.push(key.clone());
            out.push(SalsaMemberInfo {
                key,
                typ: model.type_of_member(&member.id).unwrap_or(LuaType::Unknown),
                id: Some(member.id.clone()),
                file_id: Some(member_ref.file_id),
                is_method: member.is_method,
            });
        }
    }
    out
}

fn is_local_prefix(model: &SalsaSemanticModel<'_>, prefix_expr: &emmylua_parser::LuaExpr) -> bool {
    let emmylua_parser::LuaExpr::NameExpr(name_expr) = prefix_expr else {
        return false;
    };
    let Some(decl) = model.resolve_name(name_expr.get_position()) else {
        return false;
    };
    model
        .file_facts()
        .and_then(|facts| facts.decl_by_id(&decl))
        .is_some_and(|decl| matches!(decl.kind, DeclKind::Local { .. } | DeclKind::Param))
}

fn matches_private_name(name: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        if let Some(prefix) = pattern.strip_suffix('*') {
            name.starts_with(prefix)
        } else if let Some(suffix) = pattern.strip_prefix('*') {
            name.ends_with(suffix)
        } else {
            name == pattern
        }
    })
}

fn constraint_refers_to_tpl(constraint: &LuaType, tpl_name: &str) -> bool {
    matches!(constraint, LuaType::Ref(id) | LuaType::Def(id) if id.get_name() == tpl_name)
}

fn param_tpl_constraint(
    model: &SalsaSemanticModel<'_>,
    prefix_expr: &emmylua_parser::LuaExpr,
) -> Option<LuaType> {
    let emmylua_parser::LuaExpr::NameExpr(name_expr) = prefix_expr else {
        return None;
    };
    let closure = name_expr
        .ancestors::<emmylua_parser::LuaClosureExpr>()
        .next()?;
    let facts = model.file_facts()?;
    let signature = facts.signature_by_closure(closure.get_syntax_id())?;
    let docs = signature.docs.as_ref()?;
    let constraint = docs.generic_params.first()?.constraint?;
    let tree = model.syntax_tree()?;
    let node = constraint.to_node_from_root(&tree.get_red_root())?;
    let name = node.text().to_string();
    let def = model.resolve_type_def(name.trim())?;
    Some(model.type_def_ref(&def))
}

fn index_alias_name(builder: &CompletionBuilder, member_file: Option<FileId>) -> Option<String> {
    let document = member_file
        .and_then(|file_id| builder.semantic_model.db().document(file_id))
        .unwrap_or_else(|| builder.document.clone());
    let text = document.get_text();
    let marker = "index_alias(";
    let start = text.find(marker)? + marker.len();
    let rest = &text[start..];
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let inner = &rest[quote.len_utf8()..];
    let end = inner.find(quote)?;
    Some(inner[..end].to_string())
}

fn add_member_completion(
    builder: &mut CompletionBuilder,
    info: SalsaMemberInfo,
    status: CompletionTriggerStatus,
) -> Option<()> {
    let member_key = &info.key;
    let mut can_add_snippet = true;
    let index_alias = matches!(member_key, LuaMemberKey::Integer(_))
        .then(|| index_alias_name(builder, info.file_id))
        .flatten();
    let label = match status {
        CompletionTriggerStatus::Dot => match member_key {
            LuaMemberKey::Name(name) => name.to_string(),
            LuaMemberKey::Integer(index) => format!("[{}]", index),
            _ => return None,
        },
        CompletionTriggerStatus::Colon => match member_key {
            LuaMemberKey::Name(name) => name.to_string(),
            _ => return None,
        },
        CompletionTriggerStatus::InString => {
            can_add_snippet = false;
            match member_key {
                LuaMemberKey::Name(name) => name.to_string(),
                _ => return None,
            }
        }
        CompletionTriggerStatus::LeftBracket => {
            can_add_snippet = false;
            match member_key {
                LuaMemberKey::Name(name) => format!("\"{}\"", name),
                LuaMemberKey::Integer(index) => index.to_string(),
                _ => return None,
            }
        }
    };
    let label = index_alias.clone().unwrap_or(label);

    // Prefix filtering: `T.pa<??>` only keeps members starting with the current partial name.
    let partial = builder.partial_name();
    if !partial.is_empty() && !label.starts_with(&partial) {
        return None;
    }

    let typ = &info.typ;
    let skip_self = info.is_method
        || matches!(typ, LuaType::DocFunction(func) if func.is_colon_define())
        || matches!(typ, LuaType::DocFunction(func)
            if func.get_params().first().is_some_and(|(name, _)| name == "self"));

    if status == CompletionTriggerStatus::Colon && !typ.is_function() {
        return None;
    }

    let kind = if info.id.is_none() {
        CompletionItemKind::VARIABLE
    } else if info.is_method || typ.is_function() {
        CompletionItemKind::FUNCTION
    } else if index_alias.is_some() || typ.is_const() {
        CompletionItemKind::CONSTANT
    } else if typ.is_def() {
        CompletionItemKind::CLASS
    } else if typ.is_namespace() {
        CompletionItemKind::MODULE
    } else {
        CompletionItemKind::VARIABLE
    };

    let call_display = match (status, info.is_method, skip_self) {
        // Colon-defined methods for `t.method`: add back self and nil return in the display.
        (CompletionTriggerStatus::Dot, true, _) => CallDisplay::AddSelf,
        // Colon-triggered and function lacks a self first param: remove the first param occupied by self in the display.
        (CompletionTriggerStatus::Colon, _, false) => CallDisplay::RemoveFirst,
        _ => CallDisplay::None,
    };

    let detail = member_main_detail(&builder.semantic_model, &info, call_display, skip_self)
        .or_else(|| function_detail(typ, call_display, skip_self));

    let data = info.id.as_ref().and_then(|id| {
        let SemanticId::Member(key) = id else {
            return None;
        };
        CompletionData {
            field_id: builder.semantic_model.file_id().id,
            trigger_offset: Some(builder.position_offset.into()),
            typ: CompletionDataType::Member {
                file_id: key.file_id.id,
                range: (key.key_range.start().into(), key.key_range.end().into()),
            },
        }
        .to_value()
    });

    let mut completion_item = CompletionItem {
        label: label.clone(),
        kind: Some(kind),
        label_details: Some(CompletionItemLabelDetails {
            detail,
            description: None,
        }),
        data,
        ..Default::default()
    };

    if can_add_snippet
        && builder.support_snippets(typ)
        && let Some(snippet) = function_snippet(&label, typ, call_display, skip_self)
    {
        completion_item.insert_text = Some(snippet);
        completion_item.insert_text_format = Some(InsertTextFormat::SNIPPET);
    } else if matches!(typ, LuaType::DocFunction(_)) {
        completion_item.insert_text = Some(format!("{}(", label));
    }

    builder.add_completion_item(completion_item.clone());
    add_member_overloads(builder, &info, &completion_item, status);

    Some(())
}

/// `---@overload` on the same member declaration: append as same-named completion items,
/// using literal parameter values + return value in the detail (consistent with old `show_literal_params`).
fn add_member_overloads(
    builder: &mut CompletionBuilder,
    info: &SalsaMemberInfo,
    base: &CompletionItem,
    status: CompletionTriggerStatus,
) {
    if status != CompletionTriggerStatus::Dot && status != CompletionTriggerStatus::Colon {
        return;
    }
    let Some(SemanticId::Member(key)) = &info.id else {
        return;
    };
    let Some(member_identity) = info.id.as_ref() else {
        return;
    };
    let Some(facts) = builder.semantic_model.file_facts_of(key.file_id) else {
        return;
    };
    let Some(member) = facts.member_by_id(member_identity) else {
        return;
    };
    let Some(value) = member.value_syntax else {
        return;
    };
    let Some(signature) = facts.signature_by_closure(value) else {
        return;
    };
    let Some(docs) = signature.docs.as_ref() else {
        return;
    };

    for overload in &docs.overloads {
        let LuaType::DocFunction(func) =
            builder
                .semantic_model
                .doc_type_lua_in(key.file_id, *overload, &[])
        else {
            continue;
        };
        let detail = overload_detail(&builder.semantic_model, &func);
        let mut item = base.clone();
        item.label_details = Some(CompletionItemLabelDetails {
            detail: Some(detail),
            description: None,
        });
        item.insert_text = None;
        item.insert_text_format = None;
        item.data = info.id.as_ref().and_then(|id| {
            let SemanticId::Member(member_key) = id else {
                return None;
            };
            CompletionData {
                field_id: builder.semantic_model.file_id().id,
                trigger_offset: Some(builder.position_offset.into()),
                typ: CompletionDataType::Member {
                    file_id: member_key.file_id.id,
                    range: (
                        member_key.key_range.start().into(),
                        member_key.key_range.end().into(),
                    ),
                },
            }
            .to_value()
        });
        builder.add_completion_item(item);
    }
}

fn overload_detail(model: &SalsaSemanticModel<'_>, func: &LuaFunctionType) -> String {
    let params = func
        .get_params()
        .iter()
        .map(|(name, ty)| match ty {
            Some(LuaType::StringConst(value)) | Some(LuaType::DocStringConst(value)) => {
                format!("\"{}\"", value)
            }
            Some(LuaType::IntegerConst(value)) | Some(LuaType::DocIntegerConst(value)) => {
                format!("{}", value)
            }
            Some(LuaType::BooleanConst(value)) => {
                if *value {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            Some(LuaType::DocBooleanConst(value)) => {
                if *value {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            Some(LuaType::Nil) => "nil".to_string(),
            _ => name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let ret = func.get_ret();
    let ret_text = if matches!(ret, LuaType::Nil) {
        String::new()
    } else {
        format!(
            "-> {}",
            crate::handlers::hover::render::humanize(model, ret)
        )
    };
    format!("({params}){ret_text}")
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CallDisplay {
    None,
    AddSelf,
    RemoveFirst,
}

fn member_main_detail(
    model: &SalsaSemanticModel<'_>,
    info: &SalsaMemberInfo,
    display: CallDisplay,
    _skip_self: bool,
) -> Option<String> {
    let SemanticId::Member(key) = info.id.as_ref()? else {
        return None;
    };
    let facts = model.file_facts_of(key.file_id)?;
    let member = facts.member_by_id(info.id.as_ref()?)?;
    let value = member.value_syntax?;
    let signature = facts.signature_by_closure(value)?;
    let mut params: Vec<_> = signature.param_names.clone();
    if params.is_empty()
        && let Some(docs) = signature.docs.as_ref()
    {
        params = docs
            .param_types
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
    }
    if params.is_empty() {
        let main = model.type_of_signature_in_file(key.file_id, value)?;
        params = main
            .get_params()
            .iter()
            .map(|(name, _)| name.clone().into())
            .collect();
    }
    // The signature parameter list in facts already removes self for method definitions, so do not strip it again.
    match display {
        CallDisplay::RemoveFirst => {
            if !params.is_empty() {
                params.remove(0);
            }
        }
        CallDisplay::AddSelf => {
            params.insert(0, "self".into());
            return Some(format!("({}) -> nil", params.join(", ")));
        }
        CallDisplay::None => {}
    }
    Some(format!("({})", params.join(", ")))
}

fn function_detail(typ: &LuaType, display: CallDisplay, skip_self: bool) -> Option<String> {
    let LuaType::DocFunction(func) = typ else {
        return None;
    };
    let mut params = func
        .get_params()
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    if skip_self && !params.is_empty() {
        params.remove(0);
    }
    match display {
        CallDisplay::AddSelf => {
            params.insert(0, "self".to_string());
            return Some(format!("({}) -> nil", params.join(", ")));
        }
        CallDisplay::RemoveFirst => {
            if !params.is_empty() {
                params.remove(0);
            }
        }
        CallDisplay::None => {}
    }
    Some(format!("({})", params.join(", ")))
}

fn function_snippet(
    label: &str,
    typ: &LuaType,
    display: CallDisplay,
    skip_self: bool,
) -> Option<String> {
    let LuaType::DocFunction(func) = typ else {
        return None;
    };
    let mut params = func
        .get_params()
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    if skip_self && !params.is_empty() {
        params.remove(0);
    }
    match display {
        CallDisplay::AddSelf => params.insert(0, "self".to_string()),
        CallDisplay::RemoveFirst => {
            if !params.is_empty() {
                params.remove(0);
            }
        }
        CallDisplay::None => {}
    }
    let params = params
        .iter()
        .enumerate()
        .map(|(i, name)| format!("${{{}:{}}}", i + 1, name))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!("{label}({params})"))
}
