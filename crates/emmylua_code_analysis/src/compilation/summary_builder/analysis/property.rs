use std::collections::{BTreeMap, HashMap};

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaChunk, LuaComment, LuaDocFieldKey, LuaDocTag, LuaDocType,
    LuaExpr, LuaIndexKey, LuaTableExpr, LuaVarExpr, NumberResult,
};
use rowan::TextSize;
use smol_str::SmolStr;

use super::super::{
    SalsaDeclKindSummary, SalsaDeclTreeSummary, SalsaDocTypeNodeKey, SalsaMemberIndexSummary,
    SalsaPropertyIndexSummary, SalsaPropertyKeySummary, SalsaPropertyKindSummary,
    SalsaPropertyOwnerSummary, SalsaPropertySourceSummary, SalsaPropertySummary,
    SalsaSyntaxIdSummary,
};
use crate::SalsaDeclId;
use crate::compilation::summary_builder::analysis::support::find_visible_decl_before_offset;
use crate::compilation::summary_builder::summary::extend_property_owner_with_key;

pub fn analyze_property_summary(
    decl_tree: &SalsaDeclTreeSummary,
    members: &SalsaMemberIndexSummary,
    chunk: LuaChunk,
) -> SalsaPropertyIndexSummary {
    let decl_attached_named_types = collect_decl_attached_named_types(decl_tree, &chunk);
    let table_exprs = chunk
        .descendants::<LuaTableExpr>()
        .map(|table_expr| {
            (
                SalsaSyntaxIdSummary::from(table_expr.get_syntax_id()),
                table_expr,
            )
        })
        .collect::<HashMap<_, _>>();

    let mut properties = Vec::new();
    collect_decl_properties(
        decl_tree,
        &table_exprs,
        &decl_attached_named_types,
        &mut properties,
    );
    collect_member_properties(members, &table_exprs, &mut properties);
    collect_doc_properties(chunk, &mut properties);

    SalsaPropertyIndexSummary { properties }
}

fn collect_decl_properties(
    decl_tree: &SalsaDeclTreeSummary,
    table_exprs: &HashMap<SalsaSyntaxIdSummary, LuaTableExpr>,
    decl_attached_named_types: &BTreeMap<SalsaDeclId, Vec<SmolStr>>,
    properties: &mut Vec<SalsaPropertySummary>,
) {
    for decl in &decl_tree.decls {
        let Some(value_expr_syntax_id) = decl.value_expr_syntax_id else {
            continue;
        };
        let Some(table_expr) = table_exprs.get(&value_expr_syntax_id) else {
            continue;
        };

        let owner = SalsaPropertyOwnerSummary::Decl {
            name: decl.name.clone(),
            decl_id: decl.id,
            is_global: matches!(decl.kind, SalsaDeclKindSummary::Global),
        };
        collect_table_expr_properties(owner, table_expr.clone(), properties);

        if let Some(type_names) = decl_attached_named_types.get(&decl.id) {
            for type_name in type_names {
                collect_table_expr_properties(
                    SalsaPropertyOwnerSummary::Type(type_name.clone()),
                    table_expr.clone(),
                    properties,
                );
            }
        }
    }
}

fn collect_decl_attached_named_types(
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
) -> BTreeMap<SalsaDeclId, Vec<SmolStr>> {
    let mut attached = BTreeMap::<SalsaDeclId, Vec<SmolStr>>::new();

    for comment in chunk.descendants::<LuaComment>() {
        let type_names = comment
            .get_doc_tags()
            .filter_map(|tag| match tag {
                LuaDocTag::Class(class_tag) => class_tag
                    .get_name_token()
                    .map(|name_token| SmolStr::new(name_token.get_name_text())),
                LuaDocTag::Enum(enum_tag) => enum_tag
                    .get_name_token()
                    .map(|name_token| SmolStr::new(name_token.get_name_text())),
                _ => None,
            })
            .collect::<Vec<_>>();
        if type_names.is_empty() {
            continue;
        }

        let Some(decl_id) = comment
            .get_owner()
            .and_then(|owner| resolve_decl_id_from_doc_owner(decl_tree, owner))
        else {
            continue;
        };

        let entry = attached.entry(decl_id).or_default();
        for type_name in type_names {
            if !entry.contains(&type_name) {
                entry.push(type_name);
            }
        }
    }

    attached
}

fn resolve_decl_id_from_doc_owner(
    decl_tree: &SalsaDeclTreeSummary,
    owner: LuaAst,
) -> Option<SalsaDeclId> {
    match owner {
        LuaAst::LuaLocalStat(local_stat) => local_stat
            .get_local_name_list()
            .next()
            .and_then(|local_name| local_name.get_name_token())
            .and_then(|name| {
                decl_tree
                    .decls
                    .iter()
                    .find(|decl| decl.start_offset == name.get_position())
                    .map(|decl| decl.id)
            }),
        LuaAst::LuaAssignStat(assign_stat) => {
            let (vars, _) = assign_stat.get_var_and_expr_list();
            match vars.first()? {
                LuaVarExpr::NameExpr(name_expr) => {
                    let name = name_expr.get_name_text()?;
                    find_visible_decl_before_offset(decl_tree, &name, name_expr.get_position())
                        .or_else(|| {
                            decl_tree.decls.iter().find(|decl| {
                                decl.start_offset == name_expr.get_position() && decl.name == name
                            })
                        })
                        .map(|decl| decl.id)
                }
                LuaVarExpr::IndexExpr(_) => None,
            }
        }
        _ => None,
    }
}

fn collect_member_properties(
    members: &SalsaMemberIndexSummary,
    table_exprs: &HashMap<SalsaSyntaxIdSummary, LuaTableExpr>,
    properties: &mut Vec<SalsaPropertySummary>,
) {
    for member in &members.members {
        let Some(value_expr_syntax_id) = member.value_expr_syntax_id else {
            continue;
        };
        let Some(table_expr) = table_exprs.get(&value_expr_syntax_id) else {
            continue;
        };

        collect_table_expr_properties(
            SalsaPropertyOwnerSummary::Member(member.target.clone()),
            table_expr.clone(),
            properties,
        );
    }
}

fn collect_table_expr_properties(
    owner: SalsaPropertyOwnerSummary,
    table_expr: LuaTableExpr,
    properties: &mut Vec<SalsaPropertySummary>,
) {
    let fields = table_expr.get_fields_with_keys();
    let last_field_index = fields.len().saturating_sub(1);

    for (field_index, (field, field_key)) in fields.into_iter().enumerate() {
        let Some(key) = build_property_key_from_table_key(&field_key) else {
            continue;
        };
        let value_expr = field.get_value_expr();
        let source_call_syntax_id = value_expr.as_ref().and_then(call_expr_syntax_id_of_expr);
        let expands_multi_result_tail = field_index == last_field_index
            && matches!(field_key, LuaIndexKey::Idx(_))
            && source_call_syntax_id.is_some();
        let kind = match &value_expr {
            Some(emmylua_parser::LuaExpr::ClosureExpr(_)) => SalsaPropertyKindSummary::Function,
            Some(emmylua_parser::LuaExpr::TableExpr(_)) => SalsaPropertyKindSummary::Table,
            _ => SalsaPropertyKindSummary::Value,
        };

        properties.push(SalsaPropertySummary {
            owner: owner.clone(),
            key: key.clone(),
            source: SalsaPropertySourceSummary::TableField,
            kind,
            syntax_offset: TextSize::from(u32::from(field.get_position())),
            value_expr_offset: value_expr
                .as_ref()
                .map(|expr| TextSize::from(u32::from(expr.get_position()))),
            value_expr_syntax_id: value_expr.as_ref().map(|expr| expr.get_syntax_id().into()),
            value_result_index: 0,
            source_call_syntax_id,
            expands_multi_result_tail,
            doc_type_offset: None,
            is_nullable: false,
        });

        if let Some(emmylua_parser::LuaExpr::TableExpr(nested_table)) = value_expr
            && let Some(child_target) = extend_property_owner_with_key(&owner, &key)
        {
            collect_table_expr_properties(
                SalsaPropertyOwnerSummary::Member(child_target),
                nested_table,
                properties,
            );
        }
    }
}

fn collect_doc_properties(chunk: LuaChunk, properties: &mut Vec<SalsaPropertySummary>) {
    for comment in chunk.descendants::<LuaComment>() {
        let mut current_type_name = None;
        for tag in comment.get_doc_tags() {
            match tag {
                LuaDocTag::Class(class_tag) => {
                    current_type_name = class_tag
                        .get_name_token()
                        .map(|name_token| SmolStr::new(name_token.get_name_text()));
                }
                LuaDocTag::Field(field_tag) => {
                    let Some(type_name) = current_type_name.clone() else {
                        continue;
                    };
                    let Some(key) = build_property_key_from_doc_field(&field_tag) else {
                        continue;
                    };
                    let kind = match field_tag.get_type() {
                        Some(LuaDocType::Func(_)) => SalsaPropertyKindSummary::Function,
                        _ => SalsaPropertyKindSummary::Value,
                    };

                    properties.push(SalsaPropertySummary {
                        owner: SalsaPropertyOwnerSummary::Type(type_name),
                        key,
                        source: SalsaPropertySourceSummary::DocField,
                        kind,
                        syntax_offset: TextSize::from(u32::from(field_tag.get_position())),
                        value_expr_offset: None,
                        value_expr_syntax_id: None,
                        value_result_index: 0,
                        source_call_syntax_id: None,
                        expands_multi_result_tail: false,
                        doc_type_offset: field_tag.get_type().map(SalsaDocTypeNodeKey::from),
                        is_nullable: field_tag.is_nullable(),
                    });
                }
                _ => {}
            }
        }
    }
}

fn call_expr_syntax_id_of_expr(expr: &LuaExpr) -> Option<SalsaSyntaxIdSummary> {
    match expr {
        LuaExpr::CallExpr(call_expr) => Some(call_expr.get_syntax_id().into()),
        _ => None,
    }
}

fn build_property_key_from_table_key(key: &LuaIndexKey) -> Option<SalsaPropertyKeySummary> {
    Some(match key {
        LuaIndexKey::Name(name_token) => {
            SalsaPropertyKeySummary::Name(name_token.get_name_text().into())
        }
        LuaIndexKey::String(string_token) => {
            SalsaPropertyKeySummary::Name(string_token.get_value().into())
        }
        LuaIndexKey::Integer(number_token) => match number_token.get_number_value() {
            NumberResult::Int(value) => SalsaPropertyKeySummary::Integer(value),
            _ => return None,
        },
        LuaIndexKey::Idx(index) => SalsaPropertyKeySummary::Sequence(*index),
        LuaIndexKey::Expr(expr) => SalsaPropertyKeySummary::Expr(expr.get_syntax_id().into()),
    })
}

fn build_property_key_from_doc_field(
    field_tag: &emmylua_parser::LuaDocTagField,
) -> Option<SalsaPropertyKeySummary> {
    let key = field_tag.get_field_key()?;
    Some(match key {
        LuaDocFieldKey::Name(name_token) => {
            SalsaPropertyKeySummary::Name(name_token.get_name_text().into())
        }
        LuaDocFieldKey::String(string_token) => {
            SalsaPropertyKeySummary::Name(string_token.get_value().into())
        }
        LuaDocFieldKey::Integer(number_token) => match number_token.get_number_value() {
            NumberResult::Int(value) => SalsaPropertyKeySummary::Integer(value),
            _ => return None,
        },
        LuaDocFieldKey::Type(doc_type) => {
            SalsaPropertyKeySummary::Type(SalsaDocTypeNodeKey::from(doc_type))
        }
    })
}
