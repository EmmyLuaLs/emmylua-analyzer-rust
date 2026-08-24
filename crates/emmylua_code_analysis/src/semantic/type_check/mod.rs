use std::sync::Arc;

mod callable;
mod intersection;
mod mismatch;
mod relation;
mod simple;
mod structured;
mod sub_type;
mod test;
mod union;

pub use mismatch::{
    OverflowKind, TypeMismatch, TypeMismatchKind, TypePathInfo, TypePathSegment, TypePathStep,
};
pub(crate) use relation::RelationOutcome;
use relation::RelationSession;
pub use sub_type::is_sub_type_of;

use crate::{
    DbIndex, GenericTpl, LuaAliasCallKind, LuaType, LuaUnionType, TypeSubstitutor,
    instantiate_type_generic,
};

/// 保守类型检查, 对于无法确定的类型关系也会返回 `true`, 仅在确定不兼容时返回 `false`.
pub fn is_assignable(db: &DbIndex, source: &LuaType, target: &LuaType) -> bool {
    !matches!(
        probe_assignable(db, source, target),
        RelationOutcome::Unrelated
    )
}

pub fn probe_assignable(db: &DbIndex, source: &LuaType, target: &LuaType) -> RelationOutcome {
    if fast_eq_check(source, target) {
        return RelationOutcome::Related;
    }
    RelationSession::probe(db, source, target)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignabilityResult {
    Assignable,
    NotAssignable(TypeMismatch),
    Indeterminate(OverflowKind),
}

/// 检查 source 到 target 的赋值关系, 并在最终失败分支保留最小诊断证据.
pub fn check_assignable(db: &DbIndex, source: &LuaType, target: &LuaType) -> AssignabilityResult {
    if fast_eq_check(source, target) {
        return AssignabilityResult::Assignable;
    }

    RelationSession::explain(db, source, target)
}

pub fn fast_eq_check(source: &LuaType, target: &LuaType) -> bool {
    match (source, target) {
        (LuaType::Any, _) => true,
        (_, LuaType::Any | LuaType::Unknown) => true,
        (LuaType::SelfInfer, _) | (_, LuaType::SelfInfer) => true,
        (LuaType::TplRef(tpl), _) | (_, LuaType::TplRef(tpl)) if tpl.get_constraint().is_none() => {
            true
        }
        (LuaType::Nil, LuaType::Nil)
        | (LuaType::Table, LuaType::Table)
        | (LuaType::Userdata, LuaType::Userdata)
        | (LuaType::Function, LuaType::Function)
        | (LuaType::Thread, LuaType::Thread)
        | (LuaType::Boolean, LuaType::Boolean)
        | (LuaType::String, LuaType::String)
        | (LuaType::Integer, LuaType::Integer)
        | (LuaType::Number, LuaType::Number)
        | (LuaType::Io, LuaType::Io)
        | (LuaType::Global, LuaType::Global)
        | (LuaType::Never, LuaType::Never) => true,
        (
            LuaType::BooleanConst(source_value) | LuaType::DocBooleanConst(source_value),
            LuaType::BooleanConst(target_value) | LuaType::DocBooleanConst(target_value),
        ) => source_value == target_value,
        (
            LuaType::StringConst(source_value) | LuaType::DocStringConst(source_value),
            LuaType::StringConst(target_value) | LuaType::DocStringConst(target_value),
        ) => source_value == target_value,
        (
            LuaType::IntegerConst(source_value) | LuaType::DocIntegerConst(source_value),
            LuaType::IntegerConst(target_value) | LuaType::DocIntegerConst(target_value),
        ) => source_value == target_value,
        (LuaType::FloatConst(source_value), LuaType::FloatConst(target_value)) => {
            source_value == target_value
        }
        (LuaType::TableConst(source_id), LuaType::TableConst(target_id)) => source_id == target_id,
        (
            LuaType::Ref(source_id) | LuaType::Def(source_id),
            LuaType::Ref(target_id) | LuaType::Def(target_id),
        ) => source_id == target_id,
        (LuaType::Ref(source_id), LuaType::Union(union)) => matches!(
            union.as_ref(),
            LuaUnionType::Nullable(LuaType::Ref(target_id)) if source_id == target_id
        ),
        (LuaType::Array(source), LuaType::Array(target)) => Arc::ptr_eq(source, target),
        (LuaType::Tuple(source), LuaType::Tuple(target)) => Arc::ptr_eq(source, target),
        (LuaType::DocFunction(source), LuaType::DocFunction(target)) => Arc::ptr_eq(source, target),
        (LuaType::Object(source), LuaType::Object(target)) => Arc::ptr_eq(source, target),
        (LuaType::Union(source), LuaType::Union(target)) => Arc::ptr_eq(source, target),
        (LuaType::Intersection(source), LuaType::Intersection(target)) => {
            Arc::ptr_eq(source, target)
        }
        (LuaType::Generic(source), LuaType::Generic(target)) => source == target,
        (LuaType::TableGeneric(source), LuaType::TableGeneric(target)) => {
            Arc::ptr_eq(source, target)
        }
        (LuaType::TplRef(source), LuaType::TplRef(target)) => Arc::ptr_eq(source, target),
        (LuaType::StrTplRef(source), LuaType::StrTplRef(target)) => Arc::ptr_eq(source, target),
        (LuaType::Variadic(source), LuaType::Variadic(target)) => Arc::ptr_eq(source, target),
        (LuaType::Signature(source_id), LuaType::Signature(target_id)) => source_id == target_id,
        (LuaType::Instance(source), LuaType::Instance(target)) => Arc::ptr_eq(source, target),
        (LuaType::Namespace(source_id), LuaType::Namespace(target_id)) => source_id == target_id,
        (LuaType::Call(source), LuaType::Call(target)) => Arc::ptr_eq(source, target),
        (LuaType::MultiLineUnion(source), LuaType::MultiLineUnion(target)) => {
            Arc::ptr_eq(source, target)
        }
        (LuaType::TypeGuard(source), LuaType::TypeGuard(target)) => Arc::ptr_eq(source, target),
        (LuaType::Language(source_id), LuaType::Language(target_id)) => source_id == target_id,
        (LuaType::ModuleRef(source_id), LuaType::ModuleRef(target_id)) => source_id == target_id,
        _ => false,
    }
}

pub fn normalize_type(db: &DbIndex, typ: &LuaType) -> Option<LuaType> {
    // 禁止在此对泛型进行展开
    match typ {
        LuaType::TplRef(tpl) if !is_circular_tpl_constraint(tpl) => tpl
            .get_constraint()
            .filter(|constraint| *constraint != typ)
            .cloned(),
        LuaType::Ref(type_id) => {
            let type_decl = db.get_type_index().get_type_decl(type_id)?;
            if type_decl.is_alias() {
                return type_decl.get_alias_origin(db, None);
            }
            None
        }
        LuaType::Call(alias_call)
            if matches!(
                alias_call.get_call_kind(),
                LuaAliasCallKind::Index | LuaAliasCallKind::RawGet | LuaAliasCallKind::KeyOf
            ) && !typ.contain_tpl() =>
        {
            let resolved = instantiate_type_generic(db, typ, &TypeSubstitutor::new());
            (resolved != *typ).then_some(resolved)
        }
        LuaType::Instance(instance) => Some(instance.get_base().clone()),
        LuaType::MultiLineUnion(union) => Some(union.to_union()),
        LuaType::TypeGuard(_) => Some(LuaType::Boolean),
        LuaType::ModuleRef(file_id) => db
            .get_module_index()
            .get_module(*file_id)
            .and_then(|module| module.export_type.clone()),
        _ => None,
    }
}

pub(super) fn is_circular_tpl_constraint(tpl: &GenericTpl) -> bool {
    matches!(
        tpl.get_constraint(),
        Some(LuaType::TplRef(constraint_tpl))
            if constraint_tpl.get_tpl_id() == tpl.get_tpl_id()
    )
}
