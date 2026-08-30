//! Table-field completion: `{ name = <??> }` / `{ <??> }` / expected types for table constructor arguments.

use emmylua_code_analysis::{LuaMemberKey, LuaType, SalsaMemberInfo, SemanticId};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallExpr, LuaExpr, LuaKind, LuaTableExpr, LuaTableField, LuaTokenKind,
};
use lsp_types::{CompletionItem, CompletionItemKind};

use crate::handlers::completion::completion_builder::CompletionBuilder;
use crate::handlers::completion::completion_data::{CompletionData, CompletionDataType};
use crate::handlers::signature_helper::get_current_param_index;

use super::{CompletionProvider, ProviderDecision, function_provider::callable_candidates};

pub struct TableFieldProvider;

impl CompletionProvider for TableFieldProvider {
    fn name(&self) -> &'static str {
        "table_field"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        table_field_context(builder).is_some()
    }

    fn complete(&self, builder: &mut CompletionBuilder) -> ProviderDecision {
        complete_provider(builder).unwrap_or(ProviderDecision::NoMatch)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TableFieldContext {
    Key(LuaTableExpr),
    Value(LuaTableField),
}

fn table_field_context(builder: &CompletionBuilder) -> Option<TableFieldContext> {
    if builder.is_space_trigger_character {
        return None;
    }

    // Do not complete table fields inside comments/doc descriptions (handled by doc providers).
    // Comment text may be attached to adjacent table nodes by the parser, so check the comment line at the cursor.
    if let Some((line, _)) = builder.get_document().get_line_col(builder.position_offset)
        && let Some(line_range) = builder.get_document().get_line_range(line)
        && let Some(prefix) = line_range.intersect(rowan::TextRange::up_to(builder.position_offset))
        && builder.get_document().get_text()[prefix]
            .trim_start()
            .starts_with("--")
    {
        return None;
    }

    // `func = <??>`: the trigger token is whitespace after an assignment, so enter value completion.
    if builder.trigger_token.kind() == LuaKind::Token(LuaTokenKind::TkWhitespace)
        && builder
            .trigger_token
            .prev_token()
            .is_some_and(|prev| prev.kind() == LuaKind::Token(LuaTokenKind::TkAssign))
    {
        let mut parent = builder.trigger_token.prev_token()?.parent();
        for _ in 0..3 {
            let Some(node) = parent else {
                break;
            };
            if let Some(field) = LuaTableField::cast(node.clone()) {
                return Some(TableFieldContext::Value(field));
            }
            parent = node.parent();
        }
        return None;
    }

    let node = LuaAst::cast(builder.trigger_token.parent()?)?;
    match node {
        LuaAst::LuaTableExpr(table_expr) => Some(TableFieldContext::Key(table_expr)),
        LuaAst::LuaNameExpr(name_expr) => name_expr
            .get_parent::<LuaTableField>()
            .and_then(|field| field.get_parent::<LuaTableExpr>())
            .map(TableFieldContext::Key),
        LuaAst::LuaTableField(field) => Some(TableFieldContext::Value(field)),
        _ => {
            // String/number/name is inside a table field: walk up to LuaTableField.
            let mut parent = node.syntax().parent();
            for _ in 0..4 {
                let Some(node) = parent else {
                    break;
                };
                if let Some(field) = LuaTableField::cast(node.clone()) {
                    return Some(TableFieldContext::Value(field));
                }
                parent = node.parent();
            }
            None
        }
    }
}

fn complete_provider(builder: &mut CompletionBuilder) -> Option<ProviderDecision> {
    match table_field_context(builder)? {
        TableFieldContext::Key(table_expr) => add_table_field_key_completion(builder, table_expr),
        TableFieldContext::Value(field) => add_table_field_value_completion(builder, field),
    }
}

fn add_table_field_key_completion(
    builder: &mut CompletionBuilder,
    table_expr: LuaTableExpr,
) -> Option<ProviderDecision> {
    // Members of the table literal itself or its containing local declaration's doc type (`---@type T local t = {...}`).
    let table_type = table_expected_type(builder, &table_expr)?;
    let members = builder.semantic_model.member_infos(&table_type);
    if members.is_empty() {
        return Some(ProviderDecision::Continue);
    }

    let mut used = std::collections::HashSet::new();
    for field in table_expr.get_fields() {
        if let Some(key) = field.get_field_key() {
            used.insert(key.get_path_part());
        }
    }
    for member in members {
        if used.contains(&member.key.to_path()) {
            continue;
        }
        used.insert(member.key.to_path());
        add_field_key_completion(builder, &member);
    }
    Some(ProviderDecision::Continue)
}

fn add_field_key_completion(builder: &mut CompletionBuilder, member: &SalsaMemberInfo) {
    let name = match &member.key {
        LuaMemberKey::Name(name) => name.to_string(),
        LuaMemberKey::Integer(index) => format!("[{}]", index),
        _ => return,
    };
    let nullable = if member.typ.is_nullable() { "?" } else { "" };
    let space = if member.typ.is_function() { "" } else { " " };
    builder.add_completion_item(CompletionItem {
        label: format!("{name}{nullable} ={space}"),
        kind: Some(CompletionItemKind::PROPERTY),
        insert_text: Some(format!("{name} ={space}")),
        data: member_completion_data(builder, member),
        ..Default::default()
    });
}

fn member_completion_data(
    builder: &CompletionBuilder,
    member: &SalsaMemberInfo,
) -> Option<serde_json::Value> {
    let id = member.id.as_ref()?;
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
}

fn add_table_field_value_completion(
    builder: &mut CompletionBuilder,
    field: LuaTableField,
) -> Option<ProviderDecision> {
    // 1. Named field: use the member type already inferred for the table object.
    if field.is_assign_field()
        && let Some(key) = field.get_field_key()
    {
        let table_expr = field.get_parent::<LuaTableExpr>()?;
        let table_type = table_expected_type(builder, &table_expr)?;
        let key_text = key.get_path_part();
        if let Some(member) = builder
            .semantic_model
            .member_infos(&table_type)
            .into_iter()
            .find(|member| member.key.to_path() == key_text)
        {
            let typ = member.typ.clone();
            if !matches!(typ, LuaType::Unknown) {
                if let LuaType::DocFunction(func) = &typ {
                    let params = func
                        .get_params()
                        .iter()
                        .map(|(name, _)| name.clone())
                        .collect::<Vec<_>>()
                        .join(", ");
                    builder.add_completion_item(CompletionItem {
                        label: "fun".to_string(),
                        kind: Some(CompletionItemKind::SNIPPET),
                        label_details: Some(lsp_types::CompletionItemLabelDetails {
                            detail: Some(format!("({params})")),
                            description: None,
                        }),
                        insert_text: Some(format!("function({params})\n\t${{0}}\nend")),
                        insert_text_format: Some(lsp_types::InsertTextFormat::SNIPPET),
                        data: member_completion_data(builder, &member),
                        ..Default::default()
                    });
                    return Some(ProviderDecision::Stop);
                }
                super::function_provider::dispatch_type(builder, &typ);
                return Some(ProviderDecision::Stop);
            }
        }
    }

    // 2. Table constructor argument (`foo({'...'})`): dispatch by the expected call-argument type.
    let expected = expected_table_field_value_type(builder, &field)?;
    let before = builder.get_completion_items_mut().len();
    match &expected {
        LuaType::Array(array) => {
            super::function_provider::dispatch_type(builder, array.get_base());
        }
        LuaType::Union(union) => {
            for component in union.into_vec().iter() {
                if let LuaType::Array(array) = component {
                    super::function_provider::dispatch_type(builder, array.get_base());
                } else {
                    super::function_provider::dispatch_type(builder, component);
                }
            }
        }
        _ => {
            super::function_provider::dispatch_type(builder, &expected);
        }
    }
    // `Name | Name[]` may dispatch the same alias repeatedly; deduplicate the newly produced candidates.
    let mut seen = Vec::new();
    let mut keep = Vec::new();
    let items = builder.get_completion_items_mut();
    for (index, item) in items.iter().enumerate() {
        if index < before {
            keep.push(true);
            continue;
        }
        let key = (item.label.clone(), item.kind);
        if seen.contains(&key) {
            keep.push(false);
        } else {
            seen.push(key);
            keep.push(true);
        }
    }
    for index in (0..items.len()).rev() {
        if !keep[index] {
            items.remove(index);
        }
    }
    Some(ProviderDecision::Stop)
}

fn table_expected_type(builder: &CompletionBuilder, table_expr: &LuaTableExpr) -> Option<LuaType> {
    // `---@type T local t = {...}`: prefer the doc type of the containing local declaration / assignment target.
    let syntax_id = table_expr.get_syntax_id();
    if let Some(local_type) = local_table_decl_type(builder, table_expr, syntax_id)
        && !matches!(local_type, LuaType::Unknown)
    {
        return Some(local_type);
    }

    // Nested table literal: `{ foo = { <??> } }` first gets the parent table's expected type, then drills down through the `foo` field.
    if let Some(field) = table_expr.ancestors::<LuaTableField>().next()
        && let Some(parent) = field.get_parent::<LuaTableExpr>()
        && let Some(parent_type) = table_expected_type(builder, &parent)
        && let Some(key) = field.get_field_key()
        && let Some(info) = builder
            .semantic_model
            .member_infos(&parent_type)
            .into_iter()
            .find(|info| info.key.to_path() == key.get_path_part())
    {
        return Some(info.typ);
    }

    // Table constructor argument: `buz({ ... })` completes by the parameter type.
    if let Some(call_type) = expected_call_arg_table_type(builder, table_expr) {
        return Some(call_type);
    }

    let projected = builder.semantic_model.type_of_expr(syntax_id);
    if !matches!(projected, LuaType::Unknown) {
        return Some(projected);
    }
    None
}

/// Parameter type when a table literal is a call argument (`buz({ ... })` → type of `buz`'s 1st parameter).
fn expected_call_arg_table_type(
    builder: &CompletionBuilder,
    table_expr: &LuaTableExpr,
) -> Option<LuaType> {
    let call = table_expr.ancestors::<LuaCallExpr>().next()?;
    let args = call.get_args_list()?.get_args().collect::<Vec<_>>();
    let arg_idx = args
        .iter()
        .position(|arg| arg.get_syntax_id() == table_expr.get_syntax_id())?;
    let prefix = call.get_prefix_expr()?;
    // Rich parameter projection: structured annotations like `---@param a { foo: { bar: number } }`
    // are often lowered to a broad `Table` in `LuaFunctionType`; restore the object structure here
    // from the signature doc source.
    if let Some(rich) = expected_rich_param_type(builder, &call, arg_idx) {
        return Some(rich);
    }
    for func in callable_candidates(&builder.semantic_model, &prefix) {
        let mut param_idx = arg_idx;
        if call.is_colon_call() && !func.is_colon_define() {
            param_idx += 1;
        }
        if let Some(ty) = func
            .get_params()
            .get(param_idx)
            .and_then(|(_, ty)| ty.clone())
            && !matches!(ty, LuaType::Unknown)
        {
            return Some(ty);
        }
    }
    None
}

/// Get the rich projection of a parameter type from the callee's `---@param` doc source.
fn expected_rich_param_type(
    builder: &CompletionBuilder,
    call: &LuaCallExpr,
    param_idx: usize,
) -> Option<LuaType> {
    let prefix = call.get_prefix_expr()?;
    let LuaExpr::NameExpr(name_expr) = prefix else {
        return None;
    };
    let decl = builder
        .semantic_model
        .resolve_name(name_expr.get_position())?;
    let SemanticId::Decl(ref key) = decl else {
        return None;
    };
    let facts = builder.semantic_model.file_facts_of(key.file_id)?;
    let decl_info = facts.decl_by_id(&decl)?;
    let closure = decl_info.value_expr_syntax?;
    let signature = facts.signature_by_closure(closure)?;
    let param_name = signature.param_names.get(param_idx)?;
    let (_, syntax) = signature
        .docs
        .as_ref()?
        .param_types
        .iter()
        .find(|(name, _)| name == param_name)?;
    let ty = builder
        .semantic_model
        .doc_type_lua_rich_in(key.file_id, *syntax);
    (!matches!(ty, LuaType::Unknown)).then_some(ty)
}

fn local_table_decl_type(
    builder: &CompletionBuilder,
    table_expr: &LuaTableExpr,
    syntax_id: emmylua_parser::LuaSyntaxId,
) -> Option<LuaType> {
    for local in table_expr.ancestors::<emmylua_parser::LuaLocalStat>() {
        if !local
            .get_value_exprs()
            .any(|expr| expr.get_syntax_id() == syntax_id)
        {
            continue;
        }
        let name = local.get_local_name_list().next()?;
        let decl = builder.semantic_model.decl_by_offset(name.get_position())?;
        return builder.semantic_model.type_of_decl(&decl);
    }
    for assign in table_expr.ancestors::<emmylua_parser::LuaAssignStat>() {
        let (vars, exprs) = assign.get_var_and_expr_list();
        if exprs.iter().any(|expr| expr.get_syntax_id() == syntax_id)
            && let Some(var) = vars.first()
        {
            return Some(
                builder
                    .semantic_model
                    .type_of_expr(var.to_expr().get_syntax_id()),
            );
        }
    }
    None
}

/// Expected type of a table-field value at the current call argument position.
fn expected_table_field_value_type(
    builder: &CompletionBuilder,
    field: &LuaTableField,
) -> Option<LuaType> {
    let table_expr = field
        .ancestors::<LuaTableExpr>()
        .next()
        .or_else(|| field.get_parent::<LuaTableExpr>())?;
    let call = table_expr.ancestors::<LuaCallExpr>().next()?;
    let param_idx = get_current_param_index(&call, &builder.trigger_token)?;
    let prefix = call.get_prefix_expr()?;
    let mut types = Vec::new();
    for func in callable_candidates(&builder.semantic_model, &prefix) {
        let mut param_idx = param_idx;
        if call.is_colon_call() && !func.is_colon_define() {
            param_idx += 1;
        }
        if let Some(ty) = func
            .get_params()
            .get(param_idx)
            .and_then(|param| param.1.clone())
        {
            // Named field: continue drilling down through the field key to the field value type.
            let ty = if let Some(key) = field.get_field_key() {
                let key_text = key.get_path_part();
                builder
                    .semantic_model
                    .member_infos(&ty)
                    .into_iter()
                    .find(|info| info.key.to_path() == key_text)
                    .map(|info| info.typ)
                    .unwrap_or(ty)
            } else {
                ty
            };
            if !types.contains(&ty) {
                types.push(ty);
            }
        }
    }
    types.into_iter().next()
}
