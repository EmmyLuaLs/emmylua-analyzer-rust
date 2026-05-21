use crate::{DbIndex, GenericTpl, LuaType, TypeOps, instantiate_type_generic};

use crate::semantic::generic::{
    TypeMapper, is_primitive_or_literal_type, regularize_tpl_candidate_type,
    widen_tpl_candidate_type,
};

use super::context::{InferenceInfo, InferenceRecord};
use super::{InferenceCandidate, InferenceCandidateView, InferencePriority, InferenceResult};

pub(super) fn resolve_info(
    db: &DbIndex,
    tpl: &GenericTpl,
    info: &InferenceInfo,
    return_top_level: bool,
    mapper: &TypeMapper,
) -> Option<InferenceResult> {
    if let Some(fixed) = &info.fixed {
        return Some(fixed.clone());
    }

    if !info.multi.is_empty() {
        let primitive_constraint = tpl.get_constraint().is_some_and(|constraint| {
            let constraint = instantiate_type_generic(db, constraint, mapper);
            is_primitive_or_literal_type(&constraint)
        });
        let const_preserving = info
            .multi
            .iter()
            .any(|record| record.candidate.is_const_preserving());
        let preserve_root_literal_form =
            primitive_constraint || const_preserving || return_top_level;
        return Some(InferenceResult::MultiTypes(
            info.multi
                .iter()
                .map(|record| {
                    resolve_candidate_type(db, &record.candidate, preserve_root_literal_form)
                })
                .collect(),
        ));
    }

    if !info.covariant.is_empty() {
        return Some(InferenceResult::Type(resolve_covariant_candidates(
            db,
            tpl,
            info,
            return_top_level,
            mapper,
        )));
    }

    if !info.contravariant.is_empty() {
        return Some(InferenceResult::Type(combine_records(
            db,
            &info.contravariant,
            info.priority.unwrap_or(InferencePriority::Normal),
            true,
            TypeOps::Intersect,
        )));
    }

    None
}

fn resolve_covariant_candidates(
    db: &DbIndex,
    tpl: &GenericTpl,
    info: &InferenceInfo,
    return_top_level: bool,
    mapper: &TypeMapper,
) -> LuaType {
    let primitive_constraint = tpl
        .get_constraint()
        .map(|constraint| {
            let constraint = instantiate_type_generic(db, constraint, mapper);
            is_primitive_or_literal_type(&constraint)
        })
        .unwrap_or(false);
    let const_preserving = info
        .covariant
        .iter()
        .any(|record| record.candidate.is_const_preserving());
    let preserve_root_literal_form =
        primitive_constraint || const_preserving || !info.top_level || return_top_level;

    combine_records(
        db,
        &info.covariant,
        info.priority.unwrap_or(InferencePriority::Normal),
        preserve_root_literal_form,
        TypeOps::Union,
    )
}

fn combine_records(
    db: &DbIndex,
    records: &[InferenceRecord],
    max_priority: InferencePriority,
    preserve_root_literal_form: bool,
    op: TypeOps,
) -> LuaType {
    let mut selected = records
        .iter()
        .filter(|record| record.priority == max_priority)
        .map(|record| resolve_candidate_type(db, &record.candidate, preserve_root_literal_form));

    let Some(first) = selected.next() else {
        return LuaType::Unknown;
    };

    selected.fold(first, |acc, ty| op.apply(db, &acc, &ty))
}

fn resolve_candidate_type(
    db: &DbIndex,
    candidate: &InferenceCandidate,
    preserve_root_literal_form: bool,
) -> LuaType {
    if candidate.is_const_preserving() {
        // std.ConstTpl 需要保留结构字面量, 例如 tuple/object/table const.
        return candidate.candidate_type();
    }

    if preserve_root_literal_form {
        return regularize_tpl_candidate_type(db, candidate.candidate_type());
    }

    match candidate.view {
        InferenceCandidateView::FreshExpression | InferenceCandidateView::Ordinary => {
            widen_tpl_candidate_type(db, candidate.candidate_type())
        }
        _ => regularize_tpl_candidate_type(db, candidate.candidate_type()),
    }
}
