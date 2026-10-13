//! Compatibility of base types and constants (fully ported from the old
//! `semantic::type_check::simple_type`, with db access going through the semantic context).

use emmylua_parser::{LuaAstNode, LuaIndexKey};

use crate::semantic_model::table_constructor::{ConstructorValue, key_type};
use crate::semantic_model::table_value::TableValue;
use crate::{LuaMemberKey, LuaType, LuaTypeDeclId};

use super::context::TypeCheckContext;
use super::sub_type::{base_type_name, get_base_type_id, is_sub_type_of};
use super::{TypeCheckResult, check_all};

pub fn check_simple_type_compact(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
) -> TypeCheckResult {
    match source {
        LuaType::Unknown | LuaType::Any => return Ok(()),
        LuaType::Table | LuaType::TableConst(_) => {
            if let (LuaType::TableConst(table), LuaType::Array(array)) = (source, compact_type) {
                return check_table_entries(context, table, &LuaType::Integer, array.get_base());
            }
            if let (LuaType::TableConst(table), LuaType::Generic(generic)) = (source, compact_type)
                && generic.get_base_type_id() == LuaTypeDeclId::global("table")
                && context
                    .type_def_of(&generic.get_base_type_id())
                    .is_none_or(|def| context.model.is_std_file(def.file_id))
                && let [key_type, value_type] = generic.get_params().as_slice()
            {
                return check_table_entries(context, table, key_type, value_type);
            }
            if let LuaType::TableConst(table) = source {
                match compact_type {
                    LuaType::Object(object) => {
                        return check_table_record(
                            context,
                            table,
                            object
                                .get_fields()
                                .iter()
                                .map(|(key, ty)| (key.clone(), ty.clone()))
                                .collect(),
                            object.get_index_access(),
                        );
                    }
                    LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => {
                        let id = match compact_type {
                            LuaType::Ref(id) | LuaType::Def(id) => id.clone(),
                            LuaType::Generic(generic) => generic.get_base_type_id(),
                            _ => unreachable!(),
                        };
                        if context
                            .type_def_of(&id)
                            .is_some_and(|def| def.kind == crate::TypeDefKind::Class)
                        {
                            let members = context.require_complete(|| {
                                crate::semantic_model::member::class_field_infos(
                                    context.model,
                                    compact_type,
                                )
                            })?;
                            let mut fields = Vec::new();
                            let mut indexes = Vec::new();
                            for info in members {
                                if let Some(key) = info.index_key_type {
                                    indexes.push((key, info.typ));
                                } else {
                                    fields.push((info.key, info.typ));
                                }
                            }
                            return check_table_record(context, table, fields, &indexes);
                        }
                    }
                    _ => {}
                }
            }
            if let (LuaType::TableConst(table), LuaType::Tuple(tuple)) = (source, compact_type) {
                context.comparisons.borrow_mut().mark_structural();
                let table_file = table.file_id;
                let table = TableValue::new(context.model, table, context.source_range.as_ref())?;
                return check_all(
                    tuple
                        .get_types()
                        .iter()
                        .enumerate()
                        .map(|(index, expected)| {
                            let key = LuaMemberKey::Integer(index as i64 + 1);
                            check_constructor_value(
                                context,
                                table_file,
                                table.member_value(&key)?,
                                expected,
                            )
                        }),
                );
            }
            if matches!(
                compact_type,
                LuaType::Table
                    | LuaType::TableConst(_)
                    | LuaType::Tuple(_)
                    | LuaType::Array(_)
                    | LuaType::Object(_)
                    | LuaType::Ref(_)
                    | LuaType::Def(_)
                    | LuaType::TableGeneric(_)
                    | LuaType::Generic(_)
                    | LuaType::Global
                    | LuaType::Userdata
                    | LuaType::Instance(_)
                    | LuaType::Any
            ) {
                return Ok(());
            }
        }
        LuaType::Userdata => {
            if matches!(
                compact_type,
                LuaType::Userdata | LuaType::Ref(_) | LuaType::Def(_)
            ) {
                return Ok(());
            }
        }
        LuaType::Function => {
            if matches!(
                compact_type,
                LuaType::Function | LuaType::DocFunction(_) | LuaType::Signature(_)
            ) {
                return Ok(());
            }
        }
        LuaType::Boolean => {
            // Both values must fit, possibly through different target union members.
            return check_all([true, false].into_iter().map(|value| {
                super::check_general_type_compact(
                    context,
                    &LuaType::BooleanConst(value),
                    compact_type,
                )
            }));
        }
        LuaType::BooleanConst(value) | LuaType::DocBooleanConst(value) => match compact_type {
            LuaType::Boolean => return Ok(()),
            LuaType::BooleanConst(other) | LuaType::DocBooleanConst(other) if value == other => {
                return Ok(());
            }
            _ => {}
        },
        LuaType::String => match compact_type {
            LuaType::String | LuaType::StrTplRef(_) | LuaType::Language(_) => return Ok(()),
            LuaType::StringConst(_) | LuaType::DocStringConst(_)
                if !context.strict_subtype
                    && !context.strict_literals
                    && context
                        .model
                        .db()
                        .config_input()
                        .is_none_or(|config| config.strict.doc_base_const_match_base_type) =>
            {
                return Ok(());
            }
            LuaType::Ref(_) => {
                match check_base_type_for_ref_compact(context, source, compact_type) {
                    Ok(_) => return Ok(()),
                    Err(err) if err.is_type_not_match() => {}
                    Err(err) => return Err(err),
                }
            }
            LuaType::Def(id) => {
                if id.get_name() == "string" {
                    return Ok(());
                }
            }
            _ => {}
        },
        LuaType::StringConst(value) | LuaType::DocStringConst(value) => match compact_type {
            LuaType::String | LuaType::StrTplRef(_) | LuaType::Language(_) => return Ok(()),
            LuaType::StringConst(other) | LuaType::DocStringConst(other) if value == other => {
                return Ok(());
            }
            LuaType::Ref(_) => {
                match check_base_type_for_ref_compact(context, source, compact_type) {
                    Ok(_) => return Ok(()),
                    Err(err) if err.is_type_not_match() => {}
                    Err(err) => return Err(err),
                }
            }
            LuaType::Def(id) => {
                if id.get_name() == "string" {
                    return Ok(());
                }
            }
            _ => {}
        },
        LuaType::Integer => match compact_type {
            LuaType::Integer => return Ok(()),
            LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_)
                if !context.strict_subtype
                    && !context.strict_literals
                    && context
                        .model
                        .db()
                        .config_input()
                        .is_none_or(|config| config.strict.doc_base_const_match_base_type) =>
            {
                return Ok(());
            }
            LuaType::Ref(_) => {
                match check_base_type_for_ref_compact(context, source, compact_type) {
                    Ok(_) => return Ok(()),
                    Err(err) if err.is_type_not_match() => {}
                    Err(err) => return Err(err),
                }
            }
            _ => {}
        },
        LuaType::IntegerConst(value) | LuaType::DocIntegerConst(value) => match compact_type {
            LuaType::Integer | LuaType::Number => return Ok(()),
            LuaType::IntegerConst(other) | LuaType::DocIntegerConst(other) if value == other => {
                return Ok(());
            }
            LuaType::Ref(_) => {
                match check_base_type_for_ref_compact(context, source, compact_type) {
                    Ok(_) => return Ok(()),
                    Err(err) if err.is_type_not_match() => {}
                    Err(err) => return Err(err),
                }
            }
            _ => {}
        },
        LuaType::FloatConst(value) => match compact_type {
            LuaType::Number => return Ok(()),
            LuaType::FloatConst(other) if value == other => return Ok(()),
            _ => {}
        },
        LuaType::StrTplRef(_) => {
            if compact_type.is_string() {
                return Ok(());
            }
        }
        LuaType::TplRef(_) => return Ok(()),
        LuaType::Variadic(_) => {
            return check_variadic_type_compact(context, source, compact_type);
        }
        LuaType::Language(lang_str) => match compact_type {
            LuaType::Language(compact_lang_str) => {
                if lang_str == compact_lang_str {
                    return Ok(());
                }
            }
            LuaType::DocStringConst(_) | LuaType::String | LuaType::StringConst(_) => {
                return Ok(());
            }
            _ => {}
        },
        _ => {}
    }

    Err(context.mismatch(source, compact_type))
}

fn check_table_record(
    context: &mut TypeCheckContext,
    table: &crate::InFiled<rowan::TextRange>,
    fields: Vec<(LuaMemberKey, LuaType)>,
    indexes: &[(LuaType, LuaType)],
) -> TypeCheckResult {
    context.comparisons.borrow_mut().mark_structural();
    let mut context = TypeCheckContext {
        strict_literals: true,
        ..context.clone()
    };
    let table_file = table.file_id;
    let table = TableValue::new(context.model, table, context.source_range.as_ref())?;
    let field_checks = check_all(fields.iter().map(|(key, expected)| {
        let actual = table.member_value(key)?;
        check_constructor_value(&mut context, table_file, actual, expected)
    }));
    if indexes.is_empty() {
        return field_checks;
    }
    let index_checks = check_all(table.field_entries().map(|entry| {
        let (key, actual) = entry?;
        if fields
            .iter()
            .any(|(field, _)| field.to_index_type().as_ref() == Some(&key))
        {
            return Ok(());
        }
        let mut checks = super::RequiredChecks::default();
        for (index, expected) in indexes {
            match super::check_general_type_compact(&mut context, &key, index) {
                Ok(()) => {
                    let expected = LuaType::from_vec(vec![expected.clone(), LuaType::Nil]);
                    return check_constructor_value(
                        &mut context,
                        table_file,
                        Some(actual),
                        &expected,
                    );
                }
                Err(err) if err.is_type_not_match() => {}
                Err(err) => checks.require(Err(err))?,
            }
        }
        checks.finish()
    }));
    check_all([field_checks, index_checks])
}

fn check_constructor_value(
    context: &mut TypeCheckContext,
    file_id: crate::FileId,
    actual: Option<ConstructorValue<'_>>,
    expected: &LuaType,
) -> TypeCheckResult {
    let source = actual.as_ref().map_or(&LuaType::Nil, |value| &value.typ);
    let result = super::check_general_type_compact(context, source, expected);
    if context.detail
        && let Some(expr) = actual.as_ref().and_then(|value| value.expr)
        && let Err(super::TypeCheckFailReason::TypeNotMatchWithReason(reason)) = &result
    {
        return Err(super::TypeCheckFailReason::TypeNotMatchAtField(Box::new(
            super::fail_reason::FieldMismatch {
                location: crate::InFiled::new(file_id, expr.get_range()),
                source: source.clone(),
                target: expected.clone(),
                reason: reason.clone(),
            },
        )));
    }
    result
}

/// Use the constructor's key rules for field diagnostics as well as compatibility.
pub(crate) fn table_index_accepts(
    model: &crate::semantic_model::SemanticModel,
    index: &LuaType,
    key: &LuaIndexKey,
) -> bool {
    let mut context = TypeCheckContext::new(model, false);
    context.strict_literals = true;
    let result = key_type(model, key.clone())
        .and_then(|key| super::check_general_type_compact(&mut context, &key, index));
    !result.is_err_and(|err| err.is_type_not_match())
}

/// Check table entries through the shared compatibility relation.
/// The shared comparison queue handles aliases, unions, and recursive values.
fn check_table_entries(
    context: &mut TypeCheckContext,
    table: &crate::InFiled<rowan::TextRange>,
    key_type: &LuaType,
    value_type: &LuaType,
) -> TypeCheckResult {
    let mut context = TypeCheckContext {
        strict_literals: true,
        ..context.clone()
    };
    let table_file = table.file_id;
    let table = TableValue::new(context.model, table, context.source_range.as_ref())?;
    context.comparisons.borrow_mut().mark_structural();
    // A nil value removes the entry, including a nil alternative of a union.
    let value_type = LuaType::from_vec(vec![value_type.clone(), LuaType::Nil]);
    check_all(table.field_entries().map(|entry| {
        let (key, actual) = entry?;
        super::check_general_type_compact(&mut context, &key, key_type)?;
        check_constructor_value(&mut context, table_file, Some(actual), &value_type)
    }))
}

/// Match primitive values against named base types and their descendants.
fn check_base_type_for_ref_compact(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
) -> TypeCheckResult {
    if let LuaType::Ref(type_decl_id) = compact_type {
        if let Some(base_type) = base_type_name_of_ref(type_decl_id) {
            if let Some(source_name) = base_type_name(source)
                && source_name == base_type
            {
                return Ok(());
            }
        }
        // Base type names are equal (integer type definition vs integer constant).
        if let Some(source_id) = get_base_type_id(source)
            && is_sub_type_of(context, type_decl_id, &source_id)
        {
            return Ok(());
        }
    }
    Err(context.mismatch(source, compact_type))
}

fn base_type_name_of_ref(id: &LuaTypeDeclId) -> Option<&'static str> {
    match id.get_name() {
        "integer" => Some("integer"),
        "number" => Some("number"),
        "string" => Some("string"),
        "boolean" => Some("boolean"),
        "table" => Some("table"),
        "function" => Some("function"),
        _ => None,
    }
}

pub(super) fn check_variadic_type_compact(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
) -> TypeCheckResult {
    let LuaType::Variadic(target_results) = compact_type else {
        // A single-value context uses only the first result.
        let first = source.get_result_slot_type(0).unwrap_or(LuaType::Nil);
        return super::check_general_type_compact(context, &first, compact_type);
    };
    let (source_max, source_count) = match source {
        LuaType::Variadic(results) => (
            results.get_max_len(),
            results
                .get_max_len()
                .unwrap_or_else(|| results.get_min_len().unwrap_or(0) + 1),
        ),
        _ => (Some(1), 1),
    };
    let target_min = target_results.get_min_len().unwrap_or(0);
    // One position after both fixed prefixes covers repeated tails. Finite
    // targets ignore extra results; finite sources may omit a target's tail.
    let count = target_results
        .get_max_len()
        .unwrap_or_else(|| source_count.max(target_min + 1));
    let mut checks = super::RequiredChecks::default();
    for index in 0..count {
        if index >= target_min && source_max.is_some_and(|len| index >= len) {
            break;
        }
        let actual = source.get_result_slot_type(index).unwrap_or(LuaType::Nil);
        let expected = compact_type
            .get_result_slot_type(index)
            .unwrap_or(LuaType::Nil);
        checks.require(super::check_general_type_compact(
            context, &actual, &expected,
        ))?;
    }
    checks.finish()
}
