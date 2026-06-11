//! 表字面量推断 — `{ a = 1, b = "hello" }`

use emmylua_parser::{LuaIndexKey, LuaTableExpr};
use smol_str::SmolStr;

use crate::{LuaArrayLen, LuaArrayType, LuaType, LuaUnionType};

use super::{InferFailReason, InferQuery, InferResult};

pub(super) fn infer_table_expr(
    infer: &InferQuery,
    table_expr: LuaTableExpr,
) -> InferResult {
    let fields_with_keys = table_expr.get_fields_with_keys();
    if fields_with_keys.is_empty() {
        return Ok(LuaType::Table);
    }

    let mut all_arrays = true;
    let mut array_types: Vec<LuaType> = Vec::new();

    for (field, key) in &fields_with_keys {
        let value_type = field
            .get_value_expr()
            .and_then(|expr| infer.infer_expr(expr).ok())
            .unwrap_or(LuaType::Unknown);

        match key {
            LuaIndexKey::Name(_) | LuaIndexKey::String(_) => {
                all_arrays = false;
            }
            LuaIndexKey::Integer(_) | LuaIndexKey::Idx(_) => {
                array_types.push(value_type);
            }
            LuaIndexKey::Expr(_) => {
                all_arrays = false;
            }
        }
    }

    if all_arrays && !array_types.is_empty() {
        let base = union_types(array_types);
        return Ok(LuaType::Array(
            LuaArrayType::new(base, LuaArrayLen::None).into(),
        ));
    }

    Ok(LuaType::Table)
}

fn union_types(types: Vec<LuaType>) -> LuaType {
    let mut unique = Vec::new();
    for ty in types {
        if !unique.contains(&ty) {
            unique.push(ty);
        }
    }
    match unique.len() {
        0 => LuaType::Unknown,
        1 => unique.into_iter().next().expect("len checked above"),
        _ => LuaType::Union(LuaUnionType::from_vec(unique).into()),
    }
}
