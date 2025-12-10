use std::ops::Deref;

use crate::{
    DbIndex, LuaType, LuaTypeDeclId, TypeSubstitutor, VariadicType,
    semantic::type_check::{
        is_sub_type_of,
        type_check_context::{TypeCheckCheckLevel, TypeCheckContext},
    },
};

use super::{
    TypeCheckResult, check_general_type_compact, sub_type::get_base_type_id,
    type_check_fail_reason::TypeCheckFailReason, type_check_guard::TypeCheckGuard,
};

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
        LuaType::StringConst(s1) => match compact_type {
            LuaType::String
            | LuaType::StringConst(_)
            | LuaType::StrTplRef(_)
            | LuaType::Language(_) => {
                return Ok(());
            }
            LuaType::DocStringConst(s2) => {
                if context.level == TypeCheckCheckLevel::GenericConditional && s1 != s2 {
                    return Err(TypeCheckFailReason::TypeNotMatch);
                }
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
        LuaType::Integer | LuaType::IntegerConst(_) => match compact_type {
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

                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            LuaType::Integer => {
                if context
                    .db
                    .get_emmyrc()
                    .strict
                    .doc_base_const_match_base_type
                {
                    return Ok(());
                }
                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            LuaType::DocIntegerConst(j) => {
                if i == j {
                    return Ok(());
                }

                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            LuaType::Ref(_) => {
                if context
                    .db
                    .get_emmyrc()
                    .strict
                    .doc_base_const_match_base_type
                {
                    match check_base_type_for_ref_compact(
                        context,
                        source,
                        compact_type,
                        check_guard,
                    ) {
                        Ok(_) => return Ok(()),
                        Err(err) if err.is_type_not_match() => {}
                        Err(err) => return Err(err),
                    }
                }
            }
            _ => {}
        },
        LuaType::DocStringConst(s) => match compact_type {
            LuaType::StringConst(t) => {
                if s == t {
                    return Ok(());
                }

                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            LuaType::String => return Err(TypeCheckFailReason::TypeNotMatch),
            LuaType::DocStringConst(t) => {
                if s == t {
                    return Ok(());
                }

                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            LuaType::Ref(_) => {
                if context
                    .db
                    .get_emmyrc()
                    .strict
                    .doc_base_const_match_base_type
                {
                    match check_base_type_for_ref_compact(
                        context,
                        source,
                        compact_type,
                        check_guard,
                    ) {
                        Ok(_) => return Ok(()),
                        Err(err) if err.is_type_not_match() => {}
                        Err(err) => return Err(err),
                    }
                }
            }
            _ => {}
        },
        LuaType::DocBooleanConst(b) => match compact_type {
            LuaType::BooleanConst(t) => {
                if b == t {
                    return Ok(());
                }

                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            LuaType::Boolean => return Err(TypeCheckFailReason::TypeNotMatch),
            LuaType::DocBooleanConst(t) => {
                if b == t {
                    return Ok(());
                }

                return Err(TypeCheckFailReason::TypeNotMatch);
            }
            _ => {}
        },
        LuaType::StrTplRef(_) => {
            if compact_type.is_string() {
                return Ok(());
            }
        }
        LuaType::TplRef(_) | LuaType::ConstTplRef(_) => return Ok(()),
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

    match compact_type {
        LuaType::Union(union) => {
            for sub_compact in union.into_vec() {
                match check_simple_type_compact(
                    context,
                    source,
                    &sub_compact,
                    check_guard.next_level()?,
                ) {
                    Ok(_) => {}
                    Err(err) => return Err(err),
                }
            }

            return Ok(());
        }
        LuaType::Generic(generic) => {
            if !generic.contain_tpl() {
                let base_id = generic.get_base_type_id();
                if let Some(decl) = context.db.get_type_index().get_type_decl(&base_id)
                    && decl.is_alias()
                {
                    let substitutor =
                        TypeSubstitutor::from_alias(generic.get_params().clone(), base_id.clone());
                    if let Some(alias_origin) =
                        decl.get_alias_origin(context.db, Some(&substitutor))
                    {
                        return check_general_type_compact(
                            context,
                            source,
                            &alias_origin,
                            check_guard.next_level()?,
                        );
                    }
                }
            }
        }
        _ => {}
    }

    // complex infer
    Err(TypeCheckFailReason::TypeNotMatch)
}

fn get_alias_real_type<'a>(
    db: &'a DbIndex,
    compact_type: &'a LuaType,
    check_guard: TypeCheckGuard,
) -> Result<&'a LuaType, TypeCheckFailReason> {
    if let LuaType::Ref(type_decl_id) = compact_type {
        let type_decl = db
            .get_type_index()
            .get_type_decl(type_decl_id)
            .ok_or(TypeCheckFailReason::DonotCheck)?;
        if type_decl.is_alias() {
            return get_alias_real_type(
                db,
                type_decl
                    .get_alias_ref()
                    .ok_or(TypeCheckFailReason::DonotCheck)?,
                check_guard.next_level()?,
            );
        }
    }

    Ok(compact_type)
}

/// 检查基础类型是否匹配自定义类型
fn check_base_type_for_ref_compact(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    if let LuaType::Ref(_) = compact_type {
        let real_type = get_alias_real_type(context.db, compact_type, check_guard.next_level()?)?;
        match &real_type {
            LuaType::MultiLineUnion(multi_line_union) => {
                for (sub_type, _) in multi_line_union.get_unions() {
                    match check_general_type_compact(
                        context,
                        source,
                        sub_type,
                        check_guard.next_level()?,
                    ) {
                        Ok(_) => {}
                        Err(e) => return Err(e),
                    }
                }

                return Ok(());
            }
            LuaType::Ref(type_decl_id) => {
                if let Some(source_id) = get_base_type_id(source)
                    && is_sub_type_of(context.db, type_decl_id, &source_id)
                {
                    return Ok(());
                }
                if let Some(decl) = context.db.get_type_index().get_type_decl(type_decl_id)
                    && decl.is_enum()
                {
                    return check_enum_fields_match_source(
                        context,
                        source,
                        type_decl_id,
                        check_guard,
                    );
                }
            }
            _ => {}
        }
    }
    Err(TypeCheckFailReason::TypeNotMatch)
}

/// 检查`enum`的所有字段是否匹配`source`
fn check_enum_fields_match_source(
    context: &mut TypeCheckContext,
    source: &LuaType,
    enum_type_decl_id: &LuaTypeDeclId,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    if let Some(decl) = context.db.get_type_index().get_type_decl(enum_type_decl_id)
        && let Some(LuaType::Union(enum_fields)) = decl.get_enum_field_type(context.db)
    {
        for field in enum_fields.into_vec() {
            check_general_type_compact(context, source, &field, check_guard.next_level()?)?;
        }

        return Ok(());
    }
    Err(TypeCheckFailReason::TypeNotMatch)
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
