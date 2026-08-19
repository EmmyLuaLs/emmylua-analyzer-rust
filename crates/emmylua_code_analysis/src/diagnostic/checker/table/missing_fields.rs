use hashbrown::{HashMap, HashSet};
use itertools::Itertools;

use emmylua_parser::{LuaAstNode, LuaExpr, LuaTableExpr};

use crate::{DiagnosticCode, LuaMemberKey, LuaType, SemanticModel, TypeSubstitutor};

use super::super::{Checker, DiagnosticContext, humanize_lint_type};

pub struct MissingFieldsChecker;

#[derive(Default)]
struct MissingFieldsState {
    required_fields_cache: HashMap<LuaType, Vec<String>>,
}

impl Checker for MissingFieldsChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::MissingFields];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let mut state = MissingFieldsState::default();
        for table_expr in semantic_model.get_root().descendants::<LuaTableExpr>() {
            check_table_missing_fields(context, semantic_model, &table_expr, &mut state);
        }
    }
}

const MAX_TABLE_FIELDS: usize = 50;

fn check_table_missing_fields(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    table_expr: &LuaTableExpr,
    state: &mut MissingFieldsState,
) {
    let Some(current_fields) = collect_current_fields(table_expr) else {
        return;
    };

    let Some(table_type) = semantic_model
        .infer_table_should_be(table_expr.clone())
        .and_then(|table_type| resolve_table_type(semantic_model, table_expr, table_type))
    else {
        return;
    };

    if is_same_file_table_const(semantic_model, table_expr, &table_type) {
        return;
    }

    let Some(required_fields) = get_required_fields(semantic_model, &table_type, state) else {
        return;
    };

    let missing_fields = required_fields
        .iter()
        .filter(|field| !current_fields.contains(*field))
        .map(|field| format!("`{field}`"))
        .join(", ");
    if missing_fields.is_empty() {
        return;
    }

    context.add_diagnostic(
        DiagnosticCode::MissingFields,
        table_expr.get_range(),
        t!(
            "Missing required fields in type `%{typ}`: %{fields}",
            typ = humanize_lint_type(context.db, &table_type),
            fields = missing_fields
        )
        .to_string(),
        None,
    );
}

fn resolve_table_type(
    semantic_model: &SemanticModel,
    expr: &LuaTableExpr,
    table_type: LuaType,
) -> Option<LuaType> {
    let table_type = match resolve_alias_type(semantic_model, table_type)? {
        LuaType::MultiLineUnion(union) => union.to_union(),
        table_type => table_type,
    };
    match table_type {
        LuaType::Union(union) => {
            let mut check_type = None;
            let array_like_expr_type = if expr.is_array() || expr.is_empty() {
                semantic_model
                    .infer_expr(LuaExpr::TableExpr(expr.clone()))
                    .ok()
            } else {
                None
            };

            for ty in union.into_vec() {
                match &ty {
                    LuaType::Ref(_)
                    | LuaType::Object(_)
                    | LuaType::Generic(_)
                    | LuaType::Intersection(_) => {
                        if check_type.as_ref().is_some_and(|exists| exists != &ty) {
                            return None;
                        }
                        check_type = Some(ty);
                    }
                    LuaType::Table | LuaType::Userdata | LuaType::TableGeneric(_) => {
                        return None;
                    }
                    LuaType::Array(_) | LuaType::Tuple(_)
                        if array_like_expr_type.as_ref().is_some_and(|expr_type| {
                            semantic_model.is_assignable(expr_type, &ty)
                        }) =>
                    {
                        return None;
                    }
                    _ => {}
                }
            }

            check_type
        }
        table_type => Some(table_type),
    }
}

fn resolve_alias_type(semantic_model: &SemanticModel, table_type: LuaType) -> Option<LuaType> {
    let mut table_type = table_type;
    let mut visited = HashSet::new();
    loop {
        let (type_decl_id, substitutor) = match &table_type {
            LuaType::Ref(type_decl_id) => (type_decl_id, None),
            LuaType::Generic(generic_type) => (
                generic_type.get_base_type_id_ref(),
                Some(TypeSubstitutor::from_alias(
                    generic_type.get_params().clone(),
                    generic_type.get_base_type_id(),
                )),
            ),
            _ => return Some(table_type),
        };
        if !visited.insert(table_type.clone()) {
            return Some(table_type);
        }

        let type_decl = semantic_model
            .get_db()
            .get_type_index()
            .get_type_decl(type_decl_id)?;
        if !type_decl.is_alias() {
            return Some(table_type);
        }
        table_type = type_decl.get_alias_origin(semantic_model.get_db(), substitutor.as_ref())?;
    }
}

fn is_same_file_table_const(
    semantic_model: &SemanticModel,
    expr: &LuaTableExpr,
    table_type: &LuaType,
) -> bool {
    let LuaType::TableConst(in_file_range) = table_type else {
        return false;
    };

    in_file_range.file_id == semantic_model.get_file_id() && in_file_range.value == expr.get_range()
}

fn collect_current_fields(expr: &LuaTableExpr) -> Option<HashSet<String>> {
    let mut current_fields = HashSet::new();
    for (index, (_, key)) in expr.get_fields_with_keys().enumerate() {
        if index >= MAX_TABLE_FIELDS {
            return None;
        }
        current_fields.insert(key.get_path_part());
    }
    Some(current_fields)
}

fn get_required_fields<'a>(
    semantic_model: &SemanticModel,
    table_type: &LuaType,
    state: &'a mut MissingFieldsState,
) -> Option<&'a [String]> {
    if !is_required_fields_target(table_type) {
        return None;
    }

    if !state.required_fields_cache.contains_key(table_type) {
        let mut fields = HashMap::new();
        for member in semantic_model
            .get_member_infos(table_type)
            .into_iter()
            .flatten()
        {
            let Some(name) = member_key_to_field_name(&member.key) else {
                continue;
            };
            if fields.contains_key(&name) {
                continue;
            }
            let field_type = member.typ;
            fields.insert(name, field_type);
        }

        let mut required_fields = fields
            .into_iter()
            .filter_map(|(name, field_type)| {
                (!semantic_model.is_assignable(&LuaType::Nil, &field_type)).then_some(name)
            })
            .collect::<Vec<_>>();
        required_fields.sort_unstable();
        state
            .required_fields_cache
            .insert(table_type.clone(), required_fields);
    }

    state
        .required_fields_cache
        .get(table_type)
        .map(Vec::as_slice)
}

fn is_required_fields_target(table_type: &LuaType) -> bool {
    matches!(
        table_type,
        LuaType::Ref(_) | LuaType::Object(_) | LuaType::Generic(_) | LuaType::Intersection(_)
    )
}

fn member_key_to_field_name(key: &LuaMemberKey) -> Option<String> {
    match key {
        LuaMemberKey::Name(name) => Some(name.to_string()),
        LuaMemberKey::Integer(index) => Some(format!("[{}]", index)),
        LuaMemberKey::None | LuaMemberKey::TypeKey(_) => None,
    }
}
