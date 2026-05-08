use std::sync::Arc;

use crate::{
    LuaAliasCallKind, LuaAliasCallType, LuaType, VariadicType, db_index::union_type_shallow,
};

pub(crate) fn get_overload_row_slot(row: &[LuaType], idx: usize) -> LuaType {
    get_overload_row_slot_if_present(row, idx).unwrap_or(LuaType::Nil)
}

pub(crate) fn row_to_return_type(mut row: Vec<LuaType>) -> LuaType {
    match row.len() {
        0 => LuaType::Nil,
        1 => row.pop().unwrap_or(LuaType::Nil),
        _ => LuaType::Variadic(VariadicType::Multi(row).into()),
    }
}

/// Convert a row while preserving call-result arity: no values is not scalar nil.
pub(crate) fn row_to_multi_return_type(row: Vec<LuaType>) -> LuaType {
    if row.is_empty() {
        LuaType::Variadic(VariadicType::Multi(Vec::new()).into())
    } else {
        row_to_return_type(row)
    }
}

pub(crate) fn return_type_to_row(return_type: LuaType) -> Vec<LuaType> {
    match return_type {
        LuaType::Variadic(variadic) => match variadic.as_ref() {
            VariadicType::Multi(types) => types.clone(),
            VariadicType::Base(_) => vec![LuaType::Variadic(variadic)],
        },
        typ => vec![typ],
    }
}

/// Minimum number of values a documented return row requires.
///
/// Explicit `nil` slots still count for arity. They only collapse to
/// `LuaType::Nil` when a row is used as a single expression type.
pub(crate) fn return_row_min_len(row: &[LuaType]) -> Option<usize> {
    let mut min_len = match row.last() {
        None => 0,
        Some(LuaType::Variadic(variadic)) => row.len() - 1 + variadic.get_min_len().unwrap_or(0),
        Some(_) => row.len(),
    };

    for idx in (0..min_len).rev() {
        let ty = get_overload_row_slot_if_present(row, idx)?;
        if matches!(ty, LuaType::Nil) {
            break;
        }
        if ty.is_optional() {
            min_len -= 1;
        } else {
            break;
        }
    }

    Some(min_len)
}

/// Maximum number of values a documented return row can produce.
pub(crate) fn return_row_max_len(row: &[LuaType]) -> Option<usize> {
    match row.last() {
        None => Some(0),
        Some(LuaType::Variadic(variadic)) => variadic.get_max_len().map(|len| row.len() - 1 + len),
        Some(_) => Some(row.len()),
    }
}

pub(crate) fn merge_return_rows(rows: &[&[LuaType]]) -> Vec<LuaType> {
    merge_return_rows_with(rows, LuaType::from_vec)
}

pub(crate) fn merge_return_rows_shallow(rows: &[&[LuaType]]) -> Vec<LuaType> {
    merge_return_rows_with(rows, |types| {
        types
            .into_iter()
            .reduce(union_type_shallow)
            .unwrap_or(LuaType::Never)
    })
}

/// Merges return rows using Lua result-slot adjustment.
///
/// The caller supplies only the slot merge policy. Row shape decisions stay
/// here: missing slots become `nil`, finite variadic tails are expanded, and
/// unbounded tails keep one representative variadic slot.
pub(crate) fn merge_return_rows_with(
    rows: &[&[LuaType]],
    merge_slot_types: impl Fn(Vec<LuaType>) -> LuaType,
) -> Vec<LuaType> {
    let Some(prefix_max_len) = rows.iter().map(|row| row_merge_prefix_len(row)).max() else {
        return Vec::new();
    };
    if prefix_max_len == 0 {
        return Vec::new();
    }

    let (has_unbounded_variadic_tail, has_tpl_unbounded_variadic_tail) =
        rows.iter()
            .fold((false, false), |(has_unbounded, has_tpl_unbounded), row| {
                let Some(last) = row.last() else {
                    return (has_unbounded, has_tpl_unbounded);
                };
                let LuaType::Variadic(variadic) = last else {
                    return (has_unbounded, has_tpl_unbounded);
                };

                let has_unbounded_row = variadic.get_max_len().is_none();
                (
                    has_unbounded || has_unbounded_row,
                    has_tpl_unbounded || (has_unbounded_row && variadic.contain_tpl()),
                )
            });
    let merge_len = if has_unbounded_variadic_tail {
        prefix_max_len + 1
    } else {
        prefix_max_len
    };

    let mut types = Vec::with_capacity(merge_len);
    for idx in 0..merge_len {
        let slot_types = rows
            .iter()
            .map(|row| get_overload_row_slot_if_present(row, idx).unwrap_or(LuaType::Nil))
            .collect::<Vec<_>>();
        types.push(merge_slot_types(slot_types));
    }
    if has_unbounded_variadic_tail
        && !has_tpl_unbounded_variadic_tail
        && let Some(last) = types.last_mut()
        && !matches!(last, LuaType::Variadic(_))
    {
        *last = LuaType::Variadic(VariadicType::Base(last.clone()).into());
    }

    types
}

fn row_merge_prefix_len(row: &[LuaType]) -> usize {
    let Some(last) = row.last() else {
        return 0;
    };

    if let LuaType::Variadic(variadic) = last {
        row.len() - 1 + variadic_merge_prefix_len(variadic)
    } else {
        row.len()
    }
}

fn variadic_merge_prefix_len(variadic: &VariadicType) -> usize {
    if let Some(len) = variadic.get_max_len() {
        return len;
    }

    match variadic {
        VariadicType::Base(_) => 1,
        VariadicType::Multi(types) => match types.last() {
            Some(LuaType::Variadic(variadic)) => {
                types.len() - 1 + variadic_merge_prefix_len(variadic)
            }
            Some(_) => types.len(),
            None => 0,
        },
    }
}

fn overload_row_tpl_slot(
    call_kind: LuaAliasCallKind,
    variadic: &Arc<VariadicType>,
    index: i64,
) -> LuaType {
    LuaType::Call(
        LuaAliasCallType::new(
            call_kind,
            vec![
                LuaType::Variadic(variadic.clone()),
                LuaType::IntegerConst(index),
            ],
        )
        .into(),
    )
}

fn get_overload_row_slot_if_present(row: &[LuaType], idx: usize) -> Option<LuaType> {
    let row_len = row.len();
    if row_len == 0 {
        return None;
    }

    if idx + 1 < row_len {
        return Some(row[idx].clone());
    }

    let last_idx = row_len - 1;
    let last_ty = &row[last_idx];
    let offset = idx - last_idx;
    if let LuaType::Variadic(variadic) = last_ty {
        if let Some(slot) = variadic.get_type(offset).cloned() {
            if slot.contain_tpl() {
                if offset > 0 && matches!(variadic.as_ref(), VariadicType::Base(_)) {
                    return Some(overload_row_tpl_slot(
                        LuaAliasCallKind::Select,
                        variadic,
                        (offset + 1) as i64,
                    ));
                }

                return Some(overload_row_tpl_slot(
                    LuaAliasCallKind::Index,
                    variadic,
                    offset as i64,
                ));
            }
            return Some(slot);
        }

        if variadic.get_max_len().is_some() {
            return None;
        }

        Some(overload_row_tpl_slot(
            LuaAliasCallKind::Select,
            variadic,
            (offset + 1) as i64,
        ))
    } else if offset == 0 {
        Some(last_ty.clone())
    } else {
        None
    }
}
