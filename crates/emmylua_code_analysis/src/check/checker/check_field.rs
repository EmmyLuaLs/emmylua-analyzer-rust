use std::collections::HashMap;

use emmylua_parser::{LuaAssignStat, LuaAstNode, LuaDocType, LuaTableExpr, LuaVarExpr};

use crate::DiagnosticCode;
use crate::semantic_db::def::{TypeDef, TypeDefKind};
use crate::semantic_model::SemanticModel;
use crate::semantic_model::type_eval::{expand_alias_branches, expand_alias_head};
use crate::{LuaMemberKey, LuaType, LuaTypeDeclId};

use super::{CheckContext, Checker, Mismatch};
use crate::semantic_model::render::humanize_type;
use crate::semantic_model::table_constructor::{TableConstructor, static_member_key, table_expr};
use crate::semantic_model::type_check::table_index_accepts;

pub struct CheckFieldChecker;

impl Checker for CheckFieldChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::InjectField];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        for stat in tree
            .get_red_root()
            .descendants()
            .filter_map(LuaAssignStat::cast)
        {
            check_index_assign(context, semantic_model, &stat);
        }
    }
}

/// `test.a = 1` (test is a named type): if the target type has no such static member -> InjectField.
/// M0: only handle named classes without a parent type; dynamic keys, index signatures, and Object types are left for later.
fn check_index_assign(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    assign_stat: &LuaAssignStat,
) {
    let (vars, _) = assign_stat.get_var_and_expr_list();
    for var in vars {
        let LuaVarExpr::IndexExpr(index_expr) = var else {
            continue;
        };
        let Some(prefix) = index_expr.get_prefix_expr() else {
            continue;
        };
        let prefix_ty = semantic_model.type_of_expr(prefix.get_syntax_id());
        let Some(index_key) = index_expr.get_index_key() else {
            continue;
        };
        let Some(key) = static_member_key(semantic_model, &index_key) else {
            continue;
        };
        // Aliases must be expanded before deciding whether a field can be injected:
        // `---@alias Anything unknown` should allow `a.foo = 1`, while a class alias should
        // use the underlying class member surface.
        let prefix_ty = expand_alias_type(semantic_model, &prefix_ty).unwrap_or(prefix_ty);
        // `---@type { [number]: number }`: keys outside the object's index signature report injection errors.
        if let LuaType::Object(object) = &prefix_ty {
            if object.get_field(&key).is_none()
                && !object
                    .get_index_access()
                    .iter()
                    .any(|(key_ty, _)| index_key_type_accepts(key_ty, &key))
            {
                context.add_diagnostic(
                    DiagnosticCode::InjectField,
                    index_key
                        .get_range()
                        .unwrap_or_else(|| index_expr.get_range()),
                    t!(
                        "Fields cannot be injected into the reference of `%{class}` for `%{field}`. ",
                        class = humanize_type(semantic_model, &prefix_ty),
                        field = index_key.get_path_part()
                    ),
                );
            }
            continue;
        }
        let Some(def) = target_def(semantic_model, &prefix_ty) else {
            continue;
        };
        // Classes with a parent type or index signature (`[string]`) are handled broadly (M0).
        if !def.base_types.is_empty()
            || semantic_model
                .members_of_owner(&def.id)
                .iter()
                .any(|member| member.name.starts_with('['))
            || is_mapped_alias(semantic_model, &def)
        {
            continue;
        }
        if semantic_model.member_info(&prefix_ty, &key).is_some() {
            continue;
        }
        context.add_diagnostic(
            DiagnosticCode::InjectField,
            index_key
                .get_range()
                .unwrap_or_else(|| index_expr.get_range()),
            t!(
                "Fields cannot be injected into the reference of `%{class}` for `%{field}`. ",
                class = humanize_type(semantic_model, &prefix_ty),
                field = index_key.get_path_part()
            ),
        );
    }
}

/// Resolve alias heads through the shared evaluator.
fn expand_alias_type(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> Option<LuaType> {
    let expanded = expand_alias_head(semantic_model, ty)?;
    (expanded != *ty).then_some(expanded)
}

/// Whether an object index-signature key type accepts this member key (`{[number]: ...}` does not accept string keys).
fn index_key_type_accepts(key_ty: &LuaType, member_key: &LuaMemberKey) -> bool {
    match member_key {
        LuaMemberKey::Name(_) => key_type_contains(key_ty, &LuaType::String),
        LuaMemberKey::Integer(_) => key_type_contains(key_ty, &LuaType::Integer),
        _ => true,
    }
}

fn key_type_contains(key_ty: &LuaType, expected: &LuaType) -> bool {
    match key_ty {
        LuaType::Any | LuaType::Unknown => true,
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .any(|ty| key_type_contains(&ty, expected)),
        _ => key_ty == expected || (expected == &LuaType::Integer && key_ty == &LuaType::Number),
    }
}

/// Whether the alias target is a mapped type.
fn is_mapped_alias(semantic_model: &SemanticModel<'_>, def: &TypeDef) -> bool {
    let Some(syntax) = def.alias_type else {
        return false;
    };
    let Some(tree) = semantic_model.syntax_tree_of(def.file_id) else {
        return false;
    };
    let Some(node) = syntax.to_node_from_root(&tree.get_red_root()) else {
        return false;
    };
    matches!(LuaDocType::cast(node), Some(LuaDocType::Mapped(_)))
}

pub(super) fn table_literal_mismatches(
    semantic_model: &SemanticModel<'_>,
    target: &LuaType,
    table: &LuaTableExpr,
) -> Vec<Mismatch> {
    let mut mismatches = Vec::new();
    collect_table_mismatches(&mut mismatches, semantic_model, target, table);
    mismatches
}

fn collect_table_mismatches(
    mismatches: &mut Vec<Mismatch>,
    semantic_model: &SemanticModel<'_>,
    target: &LuaType,
    table: &LuaTableExpr,
) {
    // Resolve union aliases once. A remaining alias is an unresolved cycle;
    // following it again here would bypass the evaluator's active alias path.
    let Some(target) = expand_alias_branches(semantic_model, target) else {
        return;
    };
    check_table_head(mismatches, semantic_model, &target, table);
}

fn check_table_head(
    mismatches: &mut Vec<Mismatch>,
    semantic_model: &SemanticModel<'_>,
    target: &LuaType,
    table: &LuaTableExpr,
) {
    match target {
        LuaType::Intersection(intersection) => {
            for component in intersection.get_types() {
                check_table_head(mismatches, semantic_model, component, table);
            }
            return;
        }
        LuaType::Union(union) => {
            let source = LuaType::TableConst(crate::InFiled::new(
                semantic_model.file_id(),
                table.get_range(),
            ));
            let mut best: Option<Vec<Mismatch>> = None;
            for component in union.iter() {
                // An unresolved alias must not restart expansion of this branch.
                if target_def(semantic_model, component)
                    .is_some_and(|def| def.kind == TypeDefKind::Alias)
                    || crate::semantic_model::type_check::check_type_at(
                        semantic_model,
                        &source,
                        component,
                        table.get_range(),
                        false,
                    )
                    .is_err()
                {
                    continue;
                }
                let mut errors = Vec::new();
                check_table_head(&mut errors, semantic_model, component, table);
                if errors.is_empty() {
                    return;
                }
                if best
                    .as_ref()
                    .is_none_or(|current| errors.len() < current.len())
                {
                    best = Some(errors);
                }
            }
            if let Some(errors) = best {
                mismatches.extend(errors);
            } else if let Some(component) = union.iter().find(|component| {
                matches!(component, LuaType::Object(_))
                    || target_def(semantic_model, component)
                        .is_some_and(|def| def.kind == TypeDefKind::Class)
            }) {
                // Keep a missing-field diagnostic when no alternative accepts the record.
                check_table_head(mismatches, semantic_model, component, table);
            }
            return;
        }
        LuaType::Object(object) => {
            let constructor = TableConstructor::new(semantic_model, table);
            let mut missing = Vec::new();
            for (key, ty) in object.get_fields() {
                let name = match key {
                    LuaMemberKey::Name(name) => name.to_string(),
                    LuaMemberKey::Integer(i) => i.to_string(),
                    _ => continue,
                };
                match constructor.member_value(key) {
                    Ok(None) if !is_optional(semantic_model, ty) => missing.push(name),
                    Ok(Some(value)) => {
                        if let Some(child) = value.expr.and_then(table_expr) {
                            collect_table_mismatches(mismatches, semantic_model, ty, &child);
                        }
                    }
                    _ => {}
                }
            }
            report_missing_fields(mismatches, table, missing);
            return;
        }
        _ => {}
    }
    let Some(def) = target_def(semantic_model, target) else {
        return;
    };
    if def.kind != TypeDefKind::Class {
        return;
    }

    // Only the canonical standard-library dictionary bypasses closed-record
    // checks. A user-defined `table<T>` still has a normal class field surface.
    // This is after compound-type handling, so an intersection checks its other
    // components even when its dictionary component returns here.
    if let LuaType::Generic(generic) = target
        && generic.get_base_type_id() == LuaTypeDeclId::global("table")
        && semantic_model.is_std_file(def.file_id)
    {
        return;
    }

    let (members, index_signatures) = class_fields(semantic_model, target);

    let fields = table.get_fields_with_keys();
    let constructor = TableConstructor::new(semantic_model, table);

    // InjectField: table field is not in the target members.
    for (_, key) in &fields {
        let member_key = static_member_key(semantic_model, key);
        if !member_key
            .as_ref()
            .is_some_and(|key| members.contains_key(key))
            && !index_signatures
                .iter()
                .any(|(index, _)| table_index_accepts(semantic_model, index, key))
        {
            mismatches.push(Mismatch {
                code: DiagnosticCode::InjectField,
                range: table.get_range(),
                message: format!(
                    "Field `{}` cannot be injected into the target type.",
                    key.get_path_part()
                ),
            });
        }
    }

    // MissingFields: required (non-nullable) members are missing.
    let mut missing: Vec<_> = members
        .iter()
        .filter(|(key, ty)| {
            matches!(constructor.member_value(key), Ok(None)) && !is_optional(semantic_model, ty)
        })
        .map(|(key, _)| key.to_path())
        .collect();
    missing.sort_unstable();
    report_missing_fields(mismatches, table, missing);
}

fn report_missing_fields(
    mismatches: &mut Vec<Mismatch>,
    table: &LuaTableExpr,
    missing: Vec<String>,
) {
    if !missing.is_empty() {
        mismatches.push(Mismatch {
            code: DiagnosticCode::MissingFields,
            range: table.get_range(),
            message: format!(
                "Missing required fields: {}",
                missing
                    .iter()
                    .map(|name| format!("`{}`", name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
}

/// The definition of a named target, before alias expansion.
fn target_def(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> Option<TypeDef> {
    match ty {
        LuaType::Ref(id) | LuaType::Def(id) => semantic_model.type_def_of(id),
        LuaType::Generic(generic) => semantic_model.type_def_of(&generic.get_base_type_id()),
        _ => None,
    }
}

/// Project class fields and index signatures, including inherited generic bindings.
pub(super) fn class_fields(
    semantic_model: &SemanticModel<'_>,
    target: &LuaType,
) -> (HashMap<LuaMemberKey, LuaType>, Vec<(LuaType, LuaType)>) {
    let mut fields = HashMap::new();
    let mut indexes = Vec::new();
    for info in crate::semantic_model::member::class_field_infos(semantic_model, target) {
        if let Some(key) = info.index_key_type {
            indexes.push((key, info.typ));
        } else {
            fields.insert(info.key, info.typ);
        }
    }
    (fields, indexes)
}

/// Whether the field is optional (nullable / unknown).
fn is_optional(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> bool {
    fn contains_nil(ty: &LuaType) -> bool {
        match ty {
            LuaType::Any | LuaType::Unknown | LuaType::Nil => true,
            LuaType::Union(union) => union.iter().any(contains_nil),
            LuaType::Intersection(intersection) => {
                intersection.get_types().iter().all(contains_nil)
            }
            _ => false,
        }
    }

    let ty = expand_alias_branches(semantic_model, ty).unwrap_or_else(|| ty.clone());
    contains_nil(&ty)
}
