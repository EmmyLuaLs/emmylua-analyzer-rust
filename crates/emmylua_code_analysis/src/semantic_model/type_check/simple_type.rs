//! Compatibility of base types and constants (fully ported from the old
//! `semantic::type_check::simple_type`, with db access going through the salsa context).

use std::ops::Deref;

use crate::{LuaType, VariadicType};

use super::TypeCheckResult;
use super::context::TypeCheckContext;
use super::guard::TypeCheckGuard;
use super::sub_type::{base_type_name, get_base_type_id, is_sub_type_of};

pub fn check_simple_type_compact(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match source {
        LuaType::Unknown | LuaType::Any => return Ok(()),
        LuaType::Nil => {
            if let LuaType::Nil = compact_type {
                return Ok(());
            }
        }
        LuaType::Table | LuaType::TableConst(_) => {
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
        LuaType::Thread => {
            if let LuaType::Thread = compact_type {
                return Ok(());
            }
        }
        LuaType::Boolean | LuaType::BooleanConst(_) => {
            if compact_type.is_boolean() {
                return Ok(());
            }
            // Union targets such as `string|true`: a boolean constant matching any component is compatible.
            if let LuaType::Union(union) = compact_type
                && union.into_vec().iter().any(|ty| ty.is_boolean())
            {
                return Ok(());
            }
        }
        LuaType::String => match compact_type {
            LuaType::String
            | LuaType::StringConst(_)
            | LuaType::DocStringConst(_)
            | LuaType::StrTplRef(_)
            | LuaType::Language(_) => {
                return Ok(());
            }
            LuaType::Ref(_) => {
                match check_base_type_for_ref_compact(context, source, compact_type, check_guard) {
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
        LuaType::StringConst(_) => match compact_type {
            LuaType::String
            | LuaType::StringConst(_)
            | LuaType::StrTplRef(_)
            | LuaType::Language(_) => {
                return Ok(());
            }
            LuaType::DocStringConst(_) => {
                return Ok(());
            }
            LuaType::Ref(_) => {
                match check_base_type_for_ref_compact(context, source, compact_type, check_guard) {
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
            LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => {
                return Ok(());
            }
            LuaType::Ref(_) => {
                match check_base_type_for_ref_compact(context, source, compact_type, check_guard) {
                    Ok(_) => return Ok(()),
                    Err(err) if err.is_type_not_match() => {}
                    Err(err) => return Err(err),
                }
            }
            _ => {}
        },
        LuaType::IntegerConst(value) => match compact_type {
            LuaType::IntegerConst(other) => {
                if context.assign_mode || value == other {
                    return Ok(());
                }
                return Err(context.mismatch(source, compact_type));
            }
            LuaType::DocIntegerConst(other) => {
                if context.assign_mode || value == other {
                    return Ok(());
                }
                return Err(context.mismatch(source, compact_type));
            }
            LuaType::Integer | LuaType::Number => {
                return Ok(());
            }
            LuaType::Ref(_) => {
                match check_base_type_for_ref_compact(context, source, compact_type, check_guard) {
                    Ok(_) => return Ok(()),
                    Err(err) if err.is_type_not_match() => {}
                    Err(err) => return Err(err),
                }
            }
            _ => {}
        },
        LuaType::Number | LuaType::FloatConst(_) => {
            if matches!(
                compact_type,
                LuaType::Number
                    | LuaType::FloatConst(_)
                    | LuaType::Integer
                    | LuaType::IntegerConst(_)
                    | LuaType::DocIntegerConst(_)
            ) {
                return Ok(());
            }
        }
        LuaType::Io => {
            if let LuaType::Io = compact_type {
                return Ok(());
            }
        }
        LuaType::Global => {
            if let LuaType::Global = compact_type {
                return Ok(());
            }
        }
        LuaType::DocIntegerConst(i) => match compact_type {
            LuaType::IntegerConst(j) => {
                if i == j {
                    return Ok(());
                }
                return Err(context.mismatch(source, compact_type));
            }
            LuaType::Integer | LuaType::Number => {
                return Ok(());
            }
            LuaType::DocIntegerConst(j) => {
                if i == j {
                    return Ok(());
                }
                return Err(context.mismatch(source, compact_type));
            }
            LuaType::Ref(_) => {
                // M0: doc constants are checked against custom integer types by nominal base type (integer).
                return check_base_type_for_ref_compact(context, source, compact_type, check_guard);
            }
            _ => {}
        },
        LuaType::DocStringConst(s) => match compact_type {
            LuaType::StringConst(t) => {
                if s == t {
                    return Ok(());
                }
                return Err(context.mismatch(source, compact_type));
            }
            LuaType::String => return Err(context.mismatch(source, compact_type)),
            LuaType::DocStringConst(t) => {
                if s == t {
                    return Ok(());
                }
                return Err(context.mismatch(source, compact_type));
            }
            _ => {}
        },
        LuaType::DocBooleanConst(b) => match compact_type {
            LuaType::BooleanConst(t) => {
                if b == t {
                    return Ok(());
                }
                return Err(context.mismatch(source, compact_type));
            }
            LuaType::Boolean => return Err(context.mismatch(source, compact_type)),
            LuaType::DocBooleanConst(t) => {
                if b == t {
                    return Ok(());
                }
                return Err(context.mismatch(source, compact_type));
            }
            _ => {}
        },
        LuaType::StrTplRef(_) => {
            if compact_type.is_string() {
                return Ok(());
            }
        }
        LuaType::TplRef(_) => return Ok(()),
        LuaType::Namespace(source_namespace) => {
            if let LuaType::Namespace(compact_namespace) = compact_type
                && source_namespace == compact_namespace
            {
                return Ok(());
            }
        }
        LuaType::Variadic(source_type) => {
            return check_variadic_type_compact(context, source_type, compact_type, check_guard);
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

    if let LuaType::Union(union) = compact_type {
        for sub_compact in union.into_vec() {
            check_simple_type_compact(context, source, &sub_compact, check_guard.next_level()?)?;
        }
        return Ok(());
    }

    Err(context.mismatch(source, compact_type))
}

/// Base-type matching against custom types (scenarios like `---@alias integer = ...`; when salsa has no origin, compare by base type name).
fn check_base_type_for_ref_compact(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    if let LuaType::Ref(type_decl_id) = compact_type {
        // Alias expansion (salsa has no origin type, so use the base type name).
        if let Some(base_type) = base_type_name_of_ref(context, type_decl_id, check_guard) {
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

/// Alias → base type name (walk the alias chain to find a non-alias base name). Since salsa has no origin type, fall back to direct name matching.
fn base_type_name_of_ref(
    context: &mut TypeCheckContext,
    id: &crate::LuaTypeDeclId,
    _check_guard: TypeCheckGuard,
) -> Option<&'static str> {
    if context.is_alias(id) {
        return None;
    }
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

fn check_variadic_type_compact(
    context: &mut TypeCheckContext,
    source_type: &VariadicType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match &source_type {
        VariadicType::Base(source_base) => match compact_type {
            LuaType::Variadic(compact_variadic) => match compact_variadic.deref() {
                VariadicType::Base(compact_base) => {
                    if source_base == compact_base {
                        return Ok(());
                    }
                }
                VariadicType::Multi(compact_multi) => {
                    for compact_type in compact_multi {
                        check_simple_type_compact(
                            context,
                            source_base,
                            compact_type,
                            check_guard.next_level()?,
                        )?;
                    }
                }
            },
            _ => {
                check_simple_type_compact(
                    context,
                    source_base,
                    compact_type,
                    check_guard.next_level()?,
                )?;
            }
        },
        VariadicType::Multi(_) => {}
    }
    Ok(())
}
