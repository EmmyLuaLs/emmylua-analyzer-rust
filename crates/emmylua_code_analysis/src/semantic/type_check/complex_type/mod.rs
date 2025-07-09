mod array_type_check;
mod intersection_type_check;
mod object_type_check;
mod table_generic_check;
mod tuple_type_check;

use array_type_check::check_array_type_compact;
use intersection_type_check::check_intersection_type_compact;
use object_type_check::check_object_type_compact;
use table_generic_check::check_table_generic_type_compact;
use tuple_type_check::check_tuple_type_compact;

use crate::{DbIndex, LuaType, LuaUnionType};

use super::{
    check_general_type_compact, type_check_fail_reason::TypeCheckFailReason,
    type_check_guard::TypeCheckGuard, TypeCheckResult,
};

// all is duck typing
pub fn check_complex_type_compact(
    db: &DbIndex,
    source: &LuaType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match source {
        LuaType::Array(source_base) => {
            match check_array_type_compact(db, source_base, compact_type, check_guard) {
                Err(TypeCheckFailReason::DonotCheck) => {}
                result => return result,
            }
        }
        LuaType::Tuple(tuple) => {
            match check_tuple_type_compact(db, &tuple, compact_type, check_guard) {
                Err(TypeCheckFailReason::DonotCheck) => {}
                result => return result,
            }
        }
        LuaType::Object(source_object) => {
            match check_object_type_compact(db, source_object, compact_type, check_guard) {
                Err(TypeCheckFailReason::DonotCheck) => {}
                result => return result,
            }
        }
        LuaType::TableGeneric(source_generic_param) => {
            match check_table_generic_type_compact(
                db,
                source_generic_param,
                compact_type,
                check_guard,
            ) {
                Err(TypeCheckFailReason::DonotCheck) => {}
                result => return result,
            }
        }
        LuaType::Intersection(source_intersection) => {
            match check_intersection_type_compact(
                db,
                source_intersection,
                compact_type,
                check_guard,
            ) {
                Err(TypeCheckFailReason::DonotCheck) => {}
                result => return result,
            }
        }
        LuaType::Union(union_type) => {
            match compact_type {
                LuaType::Union(compact_union) => {
                    return check_union_type_compact_union(
                        db,
                        source,
                        compact_union,
                        check_guard.next_level()?,
                    );
                }
                _ => {}
            }
            for sub_type in union_type.into_vec() {
                match check_general_type_compact(
                    db,
                    &sub_type,
                    compact_type,
                    check_guard.next_level()?,
                ) {
                    Ok(_) => return Ok(()),
                    Err(e) if e.is_type_not_match() => {}
                    Err(e) => return Err(e),
                }
            }

            return Err(TypeCheckFailReason::TypeNotMatch);
        }
        // need check later
        LuaType::Generic(_) => {
            return Ok(());
        }
        LuaType::MultiLineUnion(multi_union) => {
            let union = multi_union.to_union();
            return check_complex_type_compact(
                db,
                &union,
                &compact_type,
                check_guard.next_level()?,
            );
        }
        _ => {}
    }
    // Do I need to check union types?
    if let LuaType::Union(union) = compact_type {
        for sub_compact in union.into_vec() {
            match check_complex_type_compact(db, source, &sub_compact, check_guard.next_level()?) {
                Ok(_) => {}
                Err(e) => return Err(e),
            }
        }

        return Ok(());
    }

    Err(TypeCheckFailReason::TypeNotMatch)
}

// too complex
fn check_union_type_compact_union(
    db: &DbIndex,
    source: &LuaType,
    compact_union: &LuaUnionType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    let compact_types = compact_union.into_vec();
    for compact_sub_type in compact_types {
        check_general_type_compact(db, source, &compact_sub_type, check_guard.next_level()?)?;
    }

    Ok(())
}
