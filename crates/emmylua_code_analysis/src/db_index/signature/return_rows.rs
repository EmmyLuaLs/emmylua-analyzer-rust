use std::sync::Arc;

use crate::{
    LuaAliasCallKind, LuaAliasCallType, LuaDocReturnInfo, LuaDocReturnOverloadInfo, LuaType,
    VariadicType, db_index::union_type_shallow,
};

pub(super) fn get_return_type(
    return_docs: &[LuaDocReturnInfo],
    return_overloads: &[LuaDocReturnOverloadInfo],
) -> LuaType {
    let return_docs_type = row_to_return_type(
        return_docs
            .iter()
            .map(|info| info.type_ref.clone())
            .collect(),
    );
    if return_overloads.is_empty() {
        return return_docs_type;
    }

    let overload_return_type = rows_to_return_type(
        &return_overloads
            .iter()
            .map(|overload| overload.type_refs.as_slice())
            .collect::<Vec<_>>(),
    );
    if return_docs.is_empty() {
        overload_return_type
    } else {
        merge_return_type(overload_return_type, return_docs_type)
    }
}

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

pub(crate) fn return_type_to_row(return_type: LuaType) -> Vec<LuaType> {
    match return_type {
        LuaType::Variadic(variadic) => match variadic.as_ref() {
            VariadicType::Multi(types) => types.clone(),
            VariadicType::Base(_) => vec![LuaType::Variadic(variadic)],
        },
        typ => vec![typ],
    }
}

fn rows_to_return_type(rows: &[&[LuaType]]) -> LuaType {
    let Some(base_max_len) = rows.iter().map(|row| row.len()).max() else {
        return LuaType::Nil;
    };
    if base_max_len == 0 {
        return LuaType::Nil;
    }

    let (has_variadic_tail, has_unbounded_variadic_tail, has_tpl_unbounded_variadic_tail) =
        rows.iter().fold(
            (false, false, false),
            |(has_var, has_unbounded, has_tpl_unbounded), row| {
                let Some(last) = row.last() else {
                    return (has_var, has_unbounded, has_tpl_unbounded);
                };
                let LuaType::Variadic(variadic) = last else {
                    return (has_var, has_unbounded, has_tpl_unbounded);
                };

                let has_unbounded_row = variadic.get_max_len().is_none();
                (
                    true,
                    has_unbounded || has_unbounded_row,
                    has_tpl_unbounded || (has_unbounded_row && variadic.contain_tpl()),
                )
            },
        );
    let max_len = if has_variadic_tail {
        base_max_len + 1
    } else {
        base_max_len
    };
    let fill_missing_with_nil = |idx: usize| idx < base_max_len || has_unbounded_variadic_tail;

    let mut types = Vec::with_capacity(max_len);
    for idx in 0..max_len {
        let slot_types = rows
            .iter()
            .filter_map(|row| {
                get_overload_row_slot_if_present(row, idx)
                    .or(fill_missing_with_nil(idx).then_some(LuaType::Nil))
            })
            .collect();
        types.push(LuaType::from_vec(slot_types));
    }
    if has_unbounded_variadic_tail
        && !has_tpl_unbounded_variadic_tail
        && let Some(last) = types.last_mut()
        && !matches!(last, LuaType::Variadic(_))
    {
        *last = LuaType::Variadic(VariadicType::Base(last.clone()).into());
    }

    row_to_return_type(types)
}

fn merge_return_rows(left_row: &[LuaType], right_row: &[LuaType]) -> LuaType {
    let base_max_len = left_row.len().max(right_row.len());
    let (has_variadic_tail, has_unbounded_variadic_tail, has_tpl_unbounded_variadic_tail) =
        [left_row, right_row].iter().fold(
            (false, false, false),
            |(has_var, has_unbounded, has_tpl_unbounded), row| {
                let Some(last) = row.last() else {
                    return (has_var, has_unbounded, has_tpl_unbounded);
                };
                let LuaType::Variadic(variadic) = last else {
                    return (has_var, has_unbounded, has_tpl_unbounded);
                };

                let has_unbounded_row = variadic.get_max_len().is_none();
                (
                    true,
                    has_unbounded || has_unbounded_row,
                    has_tpl_unbounded || (has_unbounded_row && variadic.contain_tpl()),
                )
            },
        );
    let max_len = if has_variadic_tail {
        base_max_len + 1
    } else {
        base_max_len
    };
    let fill_missing_with_nil = |idx: usize| idx < base_max_len || has_unbounded_variadic_tail;

    let mut types = Vec::with_capacity(max_len);
    for idx in 0..max_len {
        let left_type = get_overload_row_slot_if_present(left_row, idx)
            .or(fill_missing_with_nil(idx).then_some(LuaType::Nil));
        let right_type = get_overload_row_slot_if_present(right_row, idx)
            .or(fill_missing_with_nil(idx).then_some(LuaType::Nil));

        let merged_type = match (left_type, right_type) {
            (Some(left), Some(right)) => union_type_shallow(left, right),
            (Some(left), None) | (None, Some(left)) => left,
            (None, None) => continue,
        };
        types.push(merged_type);
    }
    if has_unbounded_variadic_tail
        && !has_tpl_unbounded_variadic_tail
        && let Some(last) = types.last_mut()
        && !matches!(last, LuaType::Variadic(_))
    {
        *last = LuaType::Variadic(VariadicType::Base(last.clone()).into());
    }

    row_to_return_type(types)
}

fn merge_return_type(left: LuaType, right: LuaType) -> LuaType {
    if left == LuaType::Unknown {
        return right;
    }
    if right == LuaType::Unknown {
        return left;
    }

    match (&left, &right) {
        (LuaType::Variadic(_), _) | (_, LuaType::Variadic(_)) => {
            let left_row = return_type_to_row(left);
            let right_row = return_type_to_row(right);
            merge_return_rows(&left_row, &right_row)
        }
        _ => union_type_shallow(left, right),
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
