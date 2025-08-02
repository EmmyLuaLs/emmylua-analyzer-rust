use crate::handlers::completion::add_completions::CompletionTriggerStatus;
use crate::handlers::completion::completion_builder::CompletionBuilder;
use crate::handlers::completion::providers::doc_type_provider::complete_types_by_prefix;
use crate::handlers::completion::providers::env_provider::add_global_env;
use crate::handlers::completion::providers::member_provider::add_completions_for_members;
use crate::handlers::completion::providers::module_path_provider::add_modules;
use crate::util::{find_comment_scope, find_ref_at, resolve_ref};
use emmylua_code_analysis::{LuaType, WorkspaceId};
use emmylua_parser::{LuaAstNode, LuaDocDescription};
use emmylua_parser_desc::LuaDescRefPathItem;
use rowan::TextRange;
use std::collections::HashSet;

pub fn add_completions(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let semantic_model = &builder.semantic_model;
    let document = semantic_model.get_document();
    let description = LuaDocDescription::cast(builder.trigger_token.parent()?)?;

    // Quickly scan the line before actually parsing comment.
    let line = document.get_line(builder.position_offset)?;
    let line_range = document.get_line_range(line)?;
    let line_text =
        &document.get_text()[line_range.intersect(TextRange::up_to(builder.position_offset))?];

    if !line_text.contains('`') {
        return None;
    }

    let path = find_ref_at(
        semantic_model
            .get_module()
            .map(|m| m.workspace_id)
            .unwrap_or(WorkspaceId::MAIN),
        semantic_model.get_emmyrc(),
        document.get_text(),
        description.clone(),
        builder.position_offset,
    )?;

    if path.is_empty() {
        add_global_completions(builder);
    } else {
        add_by_prefix(builder, &path);
    }

    builder.stop_here();

    Some(())
}

fn add_global_completions(builder: &mut CompletionBuilder) -> Option<()> {
    let mut seen_types = HashSet::new();

    // Children in scope.
    if let Some(scope) = find_comment_scope(
        builder.semantic_model.get_db(),
        builder.semantic_model.get_file_id(),
        &builder.trigger_token,
    ) {
        if let Some(member_info_map) = builder
            .semantic_model
            .get_member_info_map(&LuaType::Ref(scope))
        {
            seen_types.extend(member_info_map.iter().flat_map(|(_, members)| {
                members.iter().filter_map(|member| match &member.typ {
                    LuaType::Def(type_id) => Some(type_id.clone()),
                    _ => None,
                })
            }));
            add_completions_for_members(builder, &member_info_map, CompletionTriggerStatus::Dot);
        }
    }

    // Types in namespaces.
    complete_types_by_prefix(builder, "", Some(&seen_types));

    // Types in current module.
    if let Some(module) = builder.semantic_model.get_module() {
        if let Some(member_info_map) = builder
            .semantic_model
            .get_member_info_map(module.export_type.as_ref().unwrap_or(&LuaType::Nil))
        {
            seen_types.extend(member_info_map.iter().flat_map(|(_, members)| {
                members.iter().filter_map(|member| match &member.typ {
                    LuaType::Def(type_id) => Some(type_id.clone()),
                    _ => None,
                })
            }));
            add_completions_for_members(builder, &member_info_map, CompletionTriggerStatus::Dot);
        }
    }

    // Globals.
    add_global_env(builder, &mut HashSet::new(), "");

    // Modules.
    add_modules(builder, "", None);

    Some(())
}

fn add_by_prefix(
    builder: &mut CompletionBuilder,
    mut path: &[(LuaDescRefPathItem, TextRange)],
) -> Option<()> {
    let mut seen_types = HashSet::new();

    while let Some(last) = path.last() {
        if TextRange::up_to(last.1.end()).contains_inclusive(builder.position_offset) {
            path = &path[..path.len() - 1];
        } else {
            break;
        }
    }

    if path.is_empty() {
        add_global_completions(builder);
        return Some(());
    }

    // 1. Type members.
    let parent_semantic_infos = resolve_ref(
        builder.semantic_model.get_db(),
        builder.semantic_model.get_file_id(),
        path,
        &builder.trigger_token,
    );
    for semantic_info in parent_semantic_infos {
        if let Some(member_info_map) = builder
            .semantic_model
            .get_member_info_map(&semantic_info.typ)
        {
            seen_types.extend(member_info_map.iter().flat_map(|(_, members)| {
                members.iter().filter_map(|member| match &member.typ {
                    LuaType::Def(type_id) => Some(type_id.clone()),
                    _ => None,
                })
            }));
            add_completions_for_members(builder, &member_info_map, CompletionTriggerStatus::Dot);
        }
    }

    // 2. Sub-modules and namespaces.
    's: {
        let Some(name_parts) = path
            .iter()
            .map(|(item, _)| item.get_name())
            .collect::<Option<Vec<_>>>()
        else {
            break 's;
        };
        let prefix = name_parts.join(".") + ".";

        // Modules.
        add_modules(builder, &prefix, None);
        complete_types_by_prefix(builder, &prefix, Some(&seen_types));
    }

    None
}
