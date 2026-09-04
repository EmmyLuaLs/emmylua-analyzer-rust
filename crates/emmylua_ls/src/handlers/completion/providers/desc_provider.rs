//! Doc description reference completion: `:lua:obj:`...`` / `{lua:obj}`...`` (Rst / Myst).
//!
//! The salsa version resolves the reference path segment by segment: an empty path offers
//! global types/namespaces/modules/files/current-scope members; a non-empty path resolves the
//! prefix (type / module export) and completes members while also enumerating sub-namespaces
//! and submodule files.

use std::collections::HashSet;

use emmylua_code_analysis::{LuaMemberKey, TypeDefKind};
use emmylua_parser::{LuaAstNode, LuaComment, LuaDocDescription, LuaNameExpr};
use lsp_types::{CompletionItem, CompletionItemKind};
use rowan::TextRange;

use crate::handlers::completion::completion_builder::CompletionBuilder;

use super::member_provider::add_member_completions;
use super::{CompletionProvider, ProviderDecision};

pub struct DescProvider;

impl CompletionProvider for DescProvider {
    fn name(&self) -> &'static str {
        "doc_description"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        detect_path(builder).is_some()
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

    let mut path = detect_path(builder)?;
    while let Some(last) = path.last() {
        if TextRange::up_to(last.1.end()).contains_inclusive(builder.position_offset) {
            path.truncate(path.len() - 1);
        } else {
            break;
        }
    }

    if path.is_empty() {
        add_global_completions(builder);
    } else {
        add_by_prefix(builder, &path);
    }

    Some(())
}

fn detect_path(
    builder: &CompletionBuilder,
) -> Option<Vec<(emmylua_parser_desc::LuaDescRefPathItem, TextRange)>> {
    let description = LuaDocDescription::cast(builder.trigger_token.parent()?)?;
    let document = builder.get_document();
    let offset = builder.position_offset;
    let workspace_id = builder
        .semantic_model
        .workspace_id_of(builder.semantic_model.file_id())
        .unwrap_or(emmylua_code_analysis::WorkspaceId::MAIN);
    let items = crate::util::parse_desc(
        workspace_id,
        builder.get_emmyrc(),
        document.get_text(),
        description,
        Some(offset.into()),
    );

    // Only complete when the cursor is inside the reference content of
    // `:lua:obj:`...`` / `{lua:obj}`...``; an empty closed code block (``) is not a Lua reference.
    let ref_range = items
        .iter()
        .find(|item| {
            item.kind == emmylua_parser_desc::DescItemKind::Ref
                && item.range.contains_inclusive(offset)
        })?
        .range;
    if ref_range.is_empty() {
        return Some(Vec::new());
    }
    emmylua_parser_desc::parse_ref_target(document.get_text(), ref_range, offset)
}

fn add_global_completions(builder: &mut CompletionBuilder) {
    let mut seen_labels = HashSet::new();

    // Members in the current file scope (statements after the comment owner): `Foo.y = ...` / `self.y = ...`.
    add_comment_scope_members(builder, &mut seen_labels);

    // Current module exported members (`return { foo = 1 }`).
    if let Some(export_ty) = builder.semantic_model.type_of_module_export() {
        add_members_for_type(builder, &export_ty, &mut seen_labels, None, None);
    }

    add_desc_types_by_prefix(builder, "", &mut seen_labels);
    add_desc_globals(builder, &mut seen_labels);
    add_desc_files_by_prefix(builder, "", &mut seen_labels);
}

fn add_by_prefix(
    builder: &mut CompletionBuilder,
    path: &[(emmylua_parser_desc::LuaDescRefPathItem, TextRange)],
) -> Option<()> {
    let mut seen_labels = HashSet::new();
    let name_parts = path
        .iter()
        .map(|(item, _)| item.get_name().map(str::to_string))
        .collect::<Option<Vec<_>>>()?;
    let prefix = format!("{}.", name_parts.join("."));

    // Prefix resolves to a type: complete its members.
    if let Some(def) = builder
        .semantic_model
        .resolve_type_def(&name_parts.join("."))
    {
        let ty = builder.semantic_model.type_def_ref(&def);
        add_members_for_type(builder, &ty, &mut seen_labels, None, None);
    }

    // Prefix resolves to a module: complete its exported members.
    let module_name = name_parts.join(".");
    if builder
        .semantic_model
        .module_file_of(&module_name)
        .is_some()
    {
        let ty = builder.semantic_model.require_module_type(&module_name);
        add_members_for_type(builder, &ty, &mut seen_labels, Some(&module_name), None);
    }

    // Sub-namespaces / types + submodule files.
    add_desc_types_by_prefix(builder, &prefix, &mut seen_labels);
    add_desc_files_by_prefix(builder, &prefix, &mut seen_labels);

    Some(())
}

/// Split type full names by desc reference semantics: empty prefix yields top-level segments, non-empty prefix yields the next segment.
fn add_desc_types_by_prefix(
    builder: &mut CompletionBuilder,
    prefix: &str,
    seen: &mut HashSet<String>,
) {
    let partial = builder.partial_name();
    let mut file_ids = builder.semantic_model.main_workspace_file_ids();
    file_ids.sort();
    for file_id in file_ids {
        let Some(model) = builder.semantic_model.model_for(file_id) else {
            continue;
        };
        let Some(exports) = model.file_exports_current() else {
            continue;
        };
        for def in exports.types.iter() {
            if def.flags.meta {
                continue;
            }
            let full_name = def.full_name.as_str();
            let Some(relative) = full_name.strip_prefix(prefix) else {
                continue;
            };
            if relative.is_empty() {
                continue;
            }
            let label = match relative.find('.') {
                Some(dot) => relative[..dot].to_string(),
                None => relative.to_string(),
            };
            if !partial.is_empty() && !label.starts_with(&partial) {
                continue;
            }
            if !seen.insert(label.clone()) {
                continue;
            }
            let kind = if relative.contains('.') {
                CompletionItemKind::MODULE
            } else {
                match def.kind {
                    TypeDefKind::Enum => CompletionItemKind::ENUM,
                    TypeDefKind::Class => CompletionItemKind::CLASS,
                    TypeDefKind::Alias => CompletionItemKind::STRUCT,
                }
            };
            builder.add_completion_item(CompletionItem {
                label,
                kind: Some(kind),
                ..Default::default()
            });
        }
    }
}

/// Workspace global declarations (GLOBAL etc.; no trigger-word filtering in desc reference context).
fn add_desc_globals(builder: &mut CompletionBuilder, seen: &mut HashSet<String>) {
    let mut file_ids = builder.semantic_model.file_ids();
    file_ids.sort();
    for file_id in file_ids {
        let Some(model) = builder.semantic_model.model_for(file_id) else {
            continue;
        };
        let Some(exports) = model.file_exports_current() else {
            continue;
        };
        for global in exports.globals.iter() {
            let name = global.name.to_string();
            if !seen.insert(name.clone()) {
                continue;
            }
            let typ = builder
                .semantic_model
                .type_of_decl(&global.decl)
                .unwrap_or(emmylua_code_analysis::LuaType::Unknown);
            let kind = if typ.is_const() {
                CompletionItemKind::CONSTANT
            } else if typ.is_function() {
                CompletionItemKind::FUNCTION
            } else {
                CompletionItemKind::VARIABLE
            };
            builder.add_completion_item(CompletionItem {
                label: name,
                kind: Some(kind),
                ..Default::default()
            });
        }
    }
}

/// Segment workspace files by module name / virtual-file stem (current file is included, hence `virtual_0`).
fn add_desc_files_by_prefix(
    builder: &mut CompletionBuilder,
    prefix: &str,
    _seen: &mut HashSet<String>,
) {
    let partial = builder.partial_name();
    let mut seen_files = HashSet::new();

    let mut file_ids = builder.semantic_model.main_workspace_file_ids();
    file_ids.sort();
    for file_id in file_ids {
        let module_name = builder.semantic_model.module_name_of(file_id);
        // Virtual/unrooted files can get drive-letter names like `C:`; fall back to the file stem.
        let name = module_name
            .filter(|name| !name.contains(':') && !name.contains('/') && !name.contains('\\'))
            .or_else(|| {
                builder
                    .semantic_model
                    .file_path_of(file_id)
                    .and_then(|path| {
                        path.file_stem()
                            .map(|stem| stem.to_string_lossy().to_string())
                    })
            });
        let Some(name) = name else {
            continue;
        };
        let Some(relative) = name.strip_prefix(prefix) else {
            continue;
        };
        if relative.is_empty() {
            continue;
        }
        let label = match relative.find('.') {
            Some(dot) => relative[..dot].to_string(),
            None => relative.to_string(),
        };
        if !partial.is_empty() && !label.starts_with(&partial) {
            continue;
        }
        if !seen_files.insert(label.clone()) {
            continue;
        }
        builder.add_completion_item(CompletionItem {
            label,
            kind: Some(CompletionItemKind::FILE),
            ..Default::default()
        });
    }
}

fn add_members_for_type(
    builder: &mut CompletionBuilder,
    ty: &emmylua_code_analysis::LuaType,
    seen: &mut HashSet<String>,
    module_prefix: Option<&str>,
    owner_range: Option<TextRange>,
) {
    let infos = builder
        .semantic_model
        .member_infos(ty)
        .into_iter()
        .filter(|info| {
            let label = member_label(&info.key);
            seen.insert(label)
        })
        .collect::<Vec<_>>();
    let desired_kinds = infos
        .iter()
        .map(|info| {
            let label = member_label(&info.key);
            (
                label.clone(),
                desc_member_kind(
                    builder,
                    info,
                    module_prefix.map(|prefix| format!("{prefix}.{label}")),
                ),
            )
        })
        .collect::<Vec<_>>();
    // Members being defined by the current comment owner: doc-reference completion does not
    // attach the fixed `-> nil` return for that method definition; existing methods (such as
    // completing `init` inside a method body) still keep their full signature.
    let owned_labels = infos
        .iter()
        .filter_map(|info| {
            let owner_range = owner_range?;
            let file_id = info.file_id?;
            if file_id != builder.semantic_model.file_id() {
                return None;
            }
            let id = info.id.as_ref()?;
            let facts = builder.semantic_model.file_facts_of(file_id)?;
            let member = facts.member_by_id(id)?;
            let value = member.value_syntax?;
            owner_range
                .contains_range(value.get_range())
                .then(|| member_label(&info.key))
        })
        .collect::<HashSet<_>>();
    let _ = add_member_completions(builder, &infos);
    for item in builder.get_completion_items_mut() {
        // Doc-reference completion only shows parameter form; do not attach the fixed `-> nil` return of the member being defined.
        if owned_labels.contains(item.label.as_str())
            && let Some(detail) = item.label_details.as_mut().and_then(|d| d.detail.as_mut())
            && let Some(stripped) = detail.strip_suffix(" -> nil")
        {
            *detail = stripped.to_string();
        }
        if let Some((_, kind)) = desired_kinds.iter().find(|(label, _)| *label == item.label) {
            item.kind = Some(*kind);
        }
    }
}

fn desc_plain_type_kind(typ: &emmylua_code_analysis::LuaType) -> CompletionItemKind {
    if typ.is_function() {
        CompletionItemKind::FUNCTION
    } else if matches!(
        typ,
        emmylua_code_analysis::LuaType::Ref(_) | emmylua_code_analysis::LuaType::Def(_)
    ) {
        CompletionItemKind::CLASS
    } else if typ.is_const() {
        CompletionItemKind::CONSTANT
    } else {
        CompletionItemKind::VARIABLE
    }
}

fn desc_member_kind(
    builder: &CompletionBuilder,
    info: &emmylua_code_analysis::SalsaMemberInfo,
    module_type: Option<String>,
) -> CompletionItemKind {
    let typ = &info.typ;
    if typ.is_function() {
        return CompletionItemKind::FUNCTION;
    }
    // If a module-exported member is associated with a type definition named `module.member`, show it as that type (`Cls` class table).
    if let Some(full_name) = module_type
        && let Some(def) = builder.semantic_model.resolve_type_def(&full_name)
    {
        return match def.kind {
            TypeDefKind::Enum => CompletionItemKind::ENUM,
            TypeDefKind::Class => CompletionItemKind::CLASS,
            TypeDefKind::Alias => CompletionItemKind::STRUCT,
        };
    }
    if matches!(
        typ,
        emmylua_code_analysis::LuaType::Ref(_) | emmylua_code_analysis::LuaType::Def(_)
    ) {
        CompletionItemKind::CLASS
    } else if is_literal_member(builder, info) || typ.is_const() {
        CompletionItemKind::CONSTANT
    } else {
        CompletionItemKind::VARIABLE
    }
}

/// Show runtime members assigned literal values (`M.y = 0` / `M.y = "s"`) as constants.
/// `type_of_member` keeps a TypeShell projection (`integer`) for non-initializer members,
/// so use the value expression's constant type to restore constant semantics in doc references.
fn is_literal_member(
    builder: &CompletionBuilder,
    info: &emmylua_code_analysis::SalsaMemberInfo,
) -> bool {
    let Some(emmylua_code_analysis::SemanticId::Member(key)) = &info.id else {
        return false;
    };
    let Some(file_id) = info.file_id else {
        return false;
    };
    let Some(facts) = builder.semantic_model.file_facts_of(file_id) else {
        return false;
    };
    let Some(member) = facts.member_by_id(&emmylua_code_analysis::SemanticId::Member(key.clone()))
    else {
        return false;
    };
    let Some(value_syntax) = member.value_syntax else {
        return false;
    };
    let expr_ty = if file_id == builder.semantic_model.file_id() {
        builder.semantic_model.type_of_expr(value_syntax)
    } else if let Some(model) = builder.semantic_model.model_for(file_id) {
        model.type_of_expr(value_syntax)
    } else {
        return false;
    };
    expr_ty.is_const()
}

fn member_label(key: &LuaMemberKey) -> String {
    match key {
        LuaMemberKey::Name(name) => name.to_string(),
        LuaMemberKey::Integer(i) => format!("[{}]", i),
        _ => key.to_path(),
    }
}

/// Available members in the comment-owner statement (scope members, mirroring the old `find_comment_scope`).
fn add_comment_scope_members(builder: &mut CompletionBuilder, seen: &mut HashSet<String>) {
    let Some(parent) = builder.trigger_token.parent() else {
        return;
    };
    let Some(description) = LuaDocDescription::cast(parent) else {
        return;
    };
    let Some(comment) = description.get_parent::<LuaComment>() else {
        return;
    };
    let Some(owner) = comment.get_owner() else {
        return;
    };
    let owner_range = owner.syntax().text_range();

    // `Foo.y = ...` / `self.y = ...`: name expressions in the owner.
    for name in owner.syntax().descendants().filter_map(LuaNameExpr::cast) {
        let ty = builder
            .semantic_model
            .resolve_name(name.get_position())
            .and_then(|decl| builder.semantic_model.type_of_decl(&decl))
            .or_else(|| {
                let ty = builder.semantic_model.type_of_expr(name.get_syntax_id());
                (!matches!(ty, emmylua_code_analysis::LuaType::Unknown)).then_some(ty)
            });
        if let Some(ty) = ty {
            add_members_for_type(builder, &ty, seen, None, Some(owner_range));
        }
    }

    // Table-field owner (`local Foo = { --- ... y = 0 }`): table-literal members + @field of the same-named class.
    if let Some(table_expr) = owner.ancestors::<emmylua_parser::LuaTableExpr>().next() {
        let table_ty = builder
            .semantic_model
            .type_of_expr(table_expr.get_syntax_id());
        add_members_for_type(builder, &table_ty, seen, None, Some(owner_range));

        // When the table is initialized via a local variable/assignment, also complete @field of the same-named class.
        let table_syntax_id = table_expr.get_syntax_id();
        for local in table_expr.ancestors::<emmylua_parser::LuaLocalStat>() {
            if !local
                .get_value_exprs()
                .any(|expr| expr.get_syntax_id() == table_syntax_id)
            {
                continue;
            }
            for name in local.get_local_name_list() {
                if let Some(decl) = builder.semantic_model.decl_by_offset(name.get_position())
                    && let Some(ty) = builder.semantic_model.type_of_decl(&decl)
                {
                    add_members_for_type(builder, &ty, seen, None, Some(owner_range));
                }
            }
        }
        for assign in table_expr.ancestors::<emmylua_parser::LuaAssignStat>() {
            let (vars, exprs) = assign.get_var_and_expr_list();
            if exprs
                .iter()
                .any(|expr| expr.get_syntax_id() == table_syntax_id)
            {
                for var in vars {
                    let expr = var.to_expr();
                    let ty = builder.semantic_model.type_of_expr(expr.get_syntax_id());
                    if !matches!(ty, emmylua_code_analysis::LuaType::Unknown) {
                        add_members_for_type(builder, &ty, seen, None, Some(owner_range));
                    }
                }
            }
        }
    }

    // Statement owner: types appearing in the containing local declaration/assignment are associated classes.
    for name in owner.syntax().descendants().filter_map(LuaNameExpr::cast) {
        let Some(name_text) = name.get_name_text() else {
            continue;
        };
        if let Some(def) = builder.semantic_model.resolve_type_def(&name_text) {
            let ty = builder.semantic_model.type_def_ref(&def);
            add_members_for_type(builder, &ty, seen, None, Some(owner_range));
        }
    }

    // Directly complete members being defined in the current assignment statement (`y` in `self.y = 0`).
    if let Some(assign) = owner.ancestors::<emmylua_parser::LuaAssignStat>().next() {
        let (vars, exprs) = assign.get_var_and_expr_list();
        for (var, expr) in vars.iter().zip(exprs.iter()) {
            let emmylua_parser::LuaVarExpr::IndexExpr(index_expr) = var else {
                continue;
            };
            let Some(label) = index_expr
                .get_index_key()
                .map(|key| key.get_path_part().to_string())
            else {
                continue;
            };
            if !seen.insert(label.clone()) {
                continue;
            }
            let ty = builder.semantic_model.type_of_expr(expr.get_syntax_id());
            if matches!(ty, emmylua_code_analysis::LuaType::Unknown) {
                continue;
            }
            let info = emmylua_code_analysis::SalsaMemberInfo {
                key: LuaMemberKey::Name(label.clone().into()),
                typ: ty.clone(),
                id: None,
                file_id: None,
                is_method: false,
            };
            let _ = add_member_completions(builder, &[info]);
            let kind = desc_plain_type_kind(&ty);
            for item in builder.get_completion_items_mut() {
                if item.label == label {
                    item.kind = Some(kind);
                }
            }
        }
    }
}
