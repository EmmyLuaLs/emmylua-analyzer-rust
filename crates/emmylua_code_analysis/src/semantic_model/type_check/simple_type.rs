//! Compatibility of base types and constants (fully ported from the old
//! `semantic::type_check::simple_type`, with db access going through the semantic context).

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
            if let (LuaType::TableConst(_), LuaType::Tuple(tuple)) = (source, compact_type) {
                return check_all(
                    tuple
                        .get_types()
                        .iter()
                        .enumerate()
                        .map(|(index, expected)| {
                            let key = LuaMemberKey::Integer(index as i64 + 1);
                            let actual = context
                                .require_complete(|| context.model.member_type(source, &key))?
                                .unwrap_or(LuaType::Nil);
                            super::check_general_type_compact(context, &actual, expected)
                        }),
                );
            }
            if let (LuaType::TableConst(_), LuaType::Object(object)) = (source, compact_type)
                && context.strict_subtype
            {
                return check_all(object.get_fields().iter().map(|(key, expected)| {
                    let actual = context
                        .require_complete(|| context.model.member_type(source, key))?
                        .unwrap_or(LuaType::Nil);
                    super::check_general_type_compact(context, &actual, expected)
                }));
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
