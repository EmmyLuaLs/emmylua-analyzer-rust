use crate::{
    DbIndex, LuaIntersectionType, LuaMemberOwner, LuaType, TypeCheckFailReason, TypeCheckResult,
    semantic::type_check::{check_general_type_compact, type_check_guard::TypeCheckGuard},
};

pub fn check_intersection_type_compact(
    db: &DbIndex,
    source_intersection: &LuaIntersectionType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    match compact_type {
        LuaType::TableConst(range) => check_intersection_type_compact_table(
            db,
            source_intersection,
            LuaMemberOwner::Element(range.clone()),
            check_guard.next_level()?,
        ),
        LuaType::Object(_) => {
            // 检查对象是否满足交叉类型的所有组成部分
            for intersection_component in source_intersection.get_types() {
                check_general_type_compact(
                    db,
                    intersection_component,
                    compact_type,
                    check_guard.next_level()?,
                )?;
            }
            Ok(())
        }
        LuaType::Intersection(compact_intersection) => {
            // 交叉类型对交叉类型：检查所有组成部分
            check_intersection_type_compact_intersection(
                db,
                source_intersection,
                compact_intersection,
                check_guard.next_level()?,
            )
        }
        LuaType::Table => Ok(()), // 通用表类型可以匹配任何交叉类型
        _ => {
            // 对于其他类型，检查是否至少满足一个组成部分
            for intersection_component in source_intersection.get_types() {
                if check_general_type_compact(
                    db,
                    intersection_component,
                    compact_type,
                    check_guard.next_level()?,
                )
                .is_ok()
                {
                    return Ok(());
                }
            }
            Err(TypeCheckFailReason::TypeNotMatch)
        }
    }
}

fn check_intersection_type_compact_table(
    db: &DbIndex,
    source_intersection: &LuaIntersectionType,
    table_owner: LuaMemberOwner,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    // 交叉类型要求 TableConst 必须满足所有组成部分
    for intersection_component in source_intersection.get_types() {
        check_general_type_compact(
            db,
            intersection_component,
            &LuaType::TableConst(
                table_owner
                    .get_element_range()
                    .ok_or(TypeCheckFailReason::TypeNotMatch)?
                    .clone(),
            ),
            check_guard.next_level()?,
        )?;
    }

    Ok(())
}

fn check_intersection_type_compact_intersection(
    db: &DbIndex,
    source_intersection: &LuaIntersectionType,
    compact_intersection: &LuaIntersectionType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    // 检查源交叉类型的每个组成部分是否都能在目标交叉类型中找到匹配
    for source_component in source_intersection.get_types() {
        let mut component_matched = false;

        for compact_component in compact_intersection.get_types() {
            if check_general_type_compact(
                db,
                source_component,
                compact_component,
                check_guard.next_level()?,
            )
            .is_ok()
            {
                component_matched = true;
                break;
            }
        }

        if !component_matched {
            return Err(TypeCheckFailReason::TypeNotMatch);
        }
    }

    Ok(())
}
