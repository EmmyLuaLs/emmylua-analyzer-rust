//! Compatibility of base types and constants (fully ported from the old
//! `semantic::type_check::simple_type`, with db access going through the semantic context).

use std::ops::Deref;
use std::rc::Rc;

use crate::{LuaMemberKey, LuaType, LuaTypeDeclId, VariadicType};

use super::context::{TypeCheckContext, TypeCheckMode};
use super::sub_type::{base_type_name, get_base_type_id, is_sub_type_of};
use super::{TypeCheckResult, check_all, check_alternatives};

pub fn check_simple_type_compact(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
) -> TypeCheckResult {
    match source {
        LuaType::Unknown | LuaType::Any => return Ok(()),
        LuaType::Nil => {
            if let LuaType::Nil = compact_type {
                return Ok(());
            }
        }
        LuaType::Table | LuaType::TableConst(_) => {
            if let (LuaType::TableConst(_), LuaType::Tuple(tuple)) = (source, compact_type) {
                // Table entries use assignment rules, including optional and union elements.
                let mut element_context = TypeCheckContext {
                    mode: match context.mode {
                        TypeCheckMode::Compatibility => TypeCheckMode::Assignment,
                        mode => mode,
                    },
                    ..context.clone()
                };
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
                            super::check_general_type_compact(
                                &mut element_context,
                                &actual,
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
        LuaType::Thread => {
            if let LuaType::Thread = compact_type {
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
            LuaType::Union(union) => {
                return check_alternatives(
                    context.mismatch(source, compact_type),
                    union.iter().map(|component| {
                        super::check_general_type_compact(context, source, component)
                    }),
                );
            }
            _ => {}
        },
        LuaType::String => match compact_type {
            LuaType::String
            | LuaType::StringConst(_)
            | LuaType::DocStringConst(_)
            | LuaType::StrTplRef(_)
            | LuaType::Language(_) => {
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
            LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => {
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
        LuaType::IntegerConst(value) => match compact_type {
            LuaType::IntegerConst(other) => {
                if context.mode.uses_assignment_rules() || value == other {
                    return Ok(());
                }
                return Err(context.mismatch(source, compact_type));
            }
            LuaType::DocIntegerConst(other) => {
                if context.mode.uses_assignment_rules() || value == other {
                    return Ok(());
                }
                return Err(context.mismatch(source, compact_type));
            }
            LuaType::Integer | LuaType::Number => {
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
                return check_base_type_for_ref_compact(context, source, compact_type);
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
            return check_variadic_type_compact(context, source_type, compact_type);
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
        return check_all(
            union
                .iter()
                .map(|sub_compact| super::check_general_type_compact(context, source, sub_compact)),
        );
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

fn check_variadic_type_compact(
    context: &mut TypeCheckContext,
    source_type: &VariadicType,
    compact_type: &LuaType,
) -> TypeCheckResult {
    let mut element_context = TypeCheckContext {
        mode: match context.mode {
            TypeCheckMode::Compatibility => TypeCheckMode::Result,
            mode => mode,
        },
        ..context.clone()
    };
    match &source_type {
        VariadicType::Base(source_base) | VariadicType::Repeat(source_base) => match compact_type {
            LuaType::Variadic(compact_variadic) => match compact_variadic.deref() {
                VariadicType::Base(_) | VariadicType::Repeat(_) => {
                    let count = compact_variadic
                        .get_max_len()
                        .or_else(|| source_type.get_max_len())
                        .unwrap_or(1);
                    return check_all((0..count).map(|index| {
                        let source = source_type.get_type(index).cloned().unwrap_or(LuaType::Nil);
                        let compact = compact_variadic
                            .get_type(index)
                            .cloned()
                            .unwrap_or(LuaType::Nil);
                        super::check_general_type_compact(&mut element_context, &source, &compact)
                    }));
                }
                VariadicType::Multi(compact_multi) => {
                    return check_all(compact_multi.iter().enumerate().map(
                        |(index, compact_type)| {
                            let source = if compact_type.contain_multi_return() {
                                LuaType::Variadic(Rc::new(source_type.get_new_variadic_from(index)))
                            } else {
                                source_type.get_type(index).cloned().unwrap_or(LuaType::Nil)
                            };
                            super::check_general_type_compact(
                                &mut element_context,
                                &source,
                                compact_type,
                            )
                        },
                    ));
                }
            },
            _ => {
                super::check_general_type_compact(context, source_base, compact_type)?;
            }
        },
        VariadicType::Multi(_) => {}
    }
    Ok(())
}
