use crate::{LuaType, VariadicType};

use super::{
    mismatch::TypeMismatch,
    relation::{IntersectionState, Relater, RelationKind, RelationResult},
};

#[inline(always)]
pub(crate) fn relate_simple<const EARLY: bool>(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    let conditional_extends = relater.kind() == RelationKind::ConditionalExtends;
    let (can_reject_simple_target_early, related) = match source {
        LuaType::Unknown => {
            if !conditional_extends {
                return Some(Ok(()));
            }
            if !EARLY {
                return Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)));
            }
            (false, false)
        }
        LuaType::Boolean => (
            true,
            matches!(target, LuaType::Boolean | LuaType::BooleanConst(_)),
        ),
        LuaType::BooleanConst(source_value) | LuaType::DocBooleanConst(source_value) => (
            true,
            matches!(target, LuaType::Boolean | LuaType::BooleanConst(_))
                || matches!(target, LuaType::DocBooleanConst(target_value) if source_value == target_value),
        ),
        LuaType::String => (
            true,
            matches!(target, LuaType::String | LuaType::StringConst(_))
                || matches!(target, LuaType::StrTplRef(_) | LuaType::Language(_)),
        ),
        LuaType::StringConst(source_value) | LuaType::DocStringConst(source_value) => (
            true,
            matches!(target, LuaType::String | LuaType::StringConst(_))
                || matches!(target, LuaType::DocStringConst(target_value) if source_value == target_value)
                || matches!(target, LuaType::StrTplRef(_) | LuaType::Language(_)),
        ),
        LuaType::StrTplRef(_) => (
            true,
            matches!(target, LuaType::String | LuaType::StringConst(_)) || source == target,
        ),
        LuaType::Language(source_language) => (
            true,
            matches!(
                target,
                LuaType::String | LuaType::StringConst(_) | LuaType::StrTplRef(_)
            ) || matches!(target, LuaType::Language(target_language) if source_language == target_language),
        ),
        LuaType::Integer => (
            true,
            matches!(
                target,
                LuaType::Integer
                    | LuaType::IntegerConst(_)
                    | LuaType::Number
                    | LuaType::FloatConst(_)
            ) || matches!(target, LuaType::DocIntegerConst(_) if relater.db().get_emmyrc().strict.doc_base_const_match_base_type),
        ),
        LuaType::IntegerConst(source_value) | LuaType::DocIntegerConst(source_value) => (
            true,
            matches!(
                target,
                LuaType::Integer
                    | LuaType::IntegerConst(_)
                    | LuaType::Number
                    | LuaType::FloatConst(_)
            ) || matches!(target, LuaType::DocIntegerConst(target_value) if source_value == target_value),
        ),
        LuaType::Number | LuaType::FloatConst(_) => (
            true,
            matches!(target, LuaType::Number | LuaType::FloatConst(_)),
        ),
        LuaType::Nil => (true, matches!(target, LuaType::Nil)),
        LuaType::Table => (true, matches!(target, LuaType::Table)),
        LuaType::Userdata => (true, matches!(target, LuaType::Table | LuaType::Userdata)),
        LuaType::Function => (true, matches!(target, LuaType::Function)),
        LuaType::Thread => (true, matches!(target, LuaType::Thread)),
        LuaType::Io => (true, matches!(target, LuaType::Io)),
        LuaType::Global => (true, matches!(target, LuaType::Table | LuaType::Global)),
        LuaType::Namespace(source_namespace) => (
            true,
            matches!(
                target,
                LuaType::Namespace(target_namespace) if source_namespace == target_namespace
            ),
        ),
        LuaType::TableConst(_)
        | LuaType::Tuple(_)
        | LuaType::Array(_)
        | LuaType::Object(_)
        | LuaType::TableGeneric(_) => (false, false),
        LuaType::DocFunction(_) | LuaType::Signature(_) => {
            (false, matches!(target, LuaType::Function))
        }
        LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => (false, false),
        LuaType::Variadic(source_variadic) => {
            if !EARLY {
                return Some(relate_variadic_source(
                    relater,
                    source,
                    target,
                    source_variadic.as_ref(),
                    intersection_state,
                ));
            }
            (false, false)
        }
        _ => (false, false),
    };

    if conditional_extends {
        match target {
            LuaType::StringConst(target_value) | LuaType::DocStringConst(target_value)
                if matches!(
                    source,
                    LuaType::StringConst(source_value) | LuaType::DocStringConst(source_value)
                        if source_value == target_value
                ) =>
            {
                return Some(Ok(()));
            }
            LuaType::IntegerConst(target_value) | LuaType::DocIntegerConst(target_value)
                if matches!(
                    source,
                    LuaType::IntegerConst(source_value) | LuaType::DocIntegerConst(source_value)
                        if source_value == target_value
                ) =>
            {
                return Some(Ok(()));
            }
            LuaType::BooleanConst(target_value) | LuaType::DocBooleanConst(target_value)
                if matches!(
                    source,
                    LuaType::BooleanConst(source_value) | LuaType::DocBooleanConst(source_value)
                        if source_value == target_value
                ) =>
            {
                return Some(Ok(()));
            }
            LuaType::FloatConst(target_value) if matches!(source, LuaType::FloatConst(source_value) if source_value == target_value) =>
            {
                return Some(Ok(()));
            }
            LuaType::StringConst(_)
            | LuaType::DocStringConst(_)
            | LuaType::IntegerConst(_)
            | LuaType::DocIntegerConst(_)
            | LuaType::BooleanConst(_)
            | LuaType::DocBooleanConst(_)
            | LuaType::FloatConst(_) => {
                return if can_reject_simple_target_early || !EARLY {
                    Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)))
                } else {
                    None
                };
            }
            _ => {}
        }
    }

    if related {
        return Some(Ok(()));
    }

    // 终端 source 可在入口阶段结束简单目标失败, 其他 source 仅在完整链路末端处理.
    match target {
        LuaType::Nil
        | LuaType::Table
        | LuaType::Userdata
        | LuaType::Function
        | LuaType::Thread
        | LuaType::Io
        | LuaType::Global
        | LuaType::Namespace(_)
        | LuaType::Boolean
        | LuaType::BooleanConst(_)
        | LuaType::String
        | LuaType::StringConst(_)
        | LuaType::Integer
        | LuaType::IntegerConst(_)
        | LuaType::Number
        | LuaType::FloatConst(_)
        | LuaType::DocStringConst(_)
        | LuaType::DocIntegerConst(_)
        | LuaType::DocBooleanConst(_)
        | LuaType::StrTplRef(_)
        | LuaType::Language(_)
            if can_reject_simple_target_early || !EARLY =>
        {
            Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)))
        }
        _ => None,
    }
}

fn relate_variadic_source(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    source_variadic: &VariadicType,
    intersection_state: IntersectionState,
) -> RelationResult {
    match source_variadic {
        VariadicType::Base(source_base) => match target {
            LuaType::Variadic(target_variadic) => match target_variadic.as_ref() {
                VariadicType::Base(target_base) => {
                    if source_base == target_base {
                        Ok(())
                    } else {
                        relater.unrelated(|| TypeMismatch::incompatible(source, target))
                    }
                }
                VariadicType::Multi(target_types) => {
                    for target_type in target_types {
                        relater.relate(source_base, target_type, intersection_state)?;
                    }
                    Ok(())
                }
            },
            _ => relater.relate(source_base, target, intersection_state),
        },
        VariadicType::Multi(_) => Ok(()),
    }
}
