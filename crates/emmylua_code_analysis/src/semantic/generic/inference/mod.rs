use std::ops::Deref;

use emmylua_parser::LuaExpr;
use hashbrown::HashSet;

use crate::{DbIndex, GenericTplId, LuaType, LuaTypeDeclId, VariadicType};

mod context;
mod infer_types;
mod resolve;

#[cfg(test)]
mod tests;

pub(in crate::semantic::generic) use context::InferenceContext;
pub(in crate::semantic::generic) use infer_types::{
    infer_type_list, infer_types_from_expr, multi_param_infer_multi_return,
    return_type_infer_types, variadic_infer_types,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::semantic::generic) enum InferenceCandidateView {
    FreshExpression,
    RegularType,
    ConstPreserving,
    Ordinary,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::semantic::generic) struct InferenceCandidate {
    pub(super) ty: LuaType,
    pub(super) view: InferenceCandidateView,
}

impl InferenceCandidate {
    pub(in crate::semantic::generic) fn from_expr_arg(expr: Option<&LuaExpr>, ty: LuaType) -> Self {
        if is_literal_candidate(&ty) {
            if expr.is_some_and(is_fresh_literal_expr) {
                return Self::fresh_expression(ty);
            }

            return Self::regular_type(ty);
        }

        Self::ordinary(ty)
    }

    pub(in crate::semantic::generic) fn regular_type(ty: LuaType) -> Self {
        Self {
            ty,
            view: InferenceCandidateView::RegularType,
        }
    }

    pub(in crate::semantic::generic) fn const_preserving(ty: LuaType) -> Self {
        Self {
            ty,
            view: InferenceCandidateView::ConstPreserving,
        }
    }

    pub(in crate::semantic::generic) fn ordinary(ty: LuaType) -> Self {
        Self {
            ty,
            view: InferenceCandidateView::Ordinary,
        }
    }

    fn fresh_expression(ty: LuaType) -> Self {
        Self {
            ty,
            view: InferenceCandidateView::FreshExpression,
        }
    }

    pub(super) fn candidate_type(&self) -> LuaType {
        self.ty.clone()
    }

    pub(super) fn is_const_preserving(&self) -> bool {
        self.view == InferenceCandidateView::ConstPreserving
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::semantic::generic) enum InferencePriority {
    Normal,
    Return,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::semantic::generic) enum InferenceVariance {
    Covariant,
    Contravariant,
}

impl InferenceVariance {
    pub(in crate::semantic::generic) fn flip(self) -> Self {
        match self {
            InferenceVariance::Covariant => InferenceVariance::Contravariant,
            InferenceVariance::Contravariant => InferenceVariance::Covariant,
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::semantic::generic) enum InferenceResult {
    Type(LuaType),
    MultiTypes(Vec<LuaType>),
    VariadicParams(Vec<(String, Option<LuaType>)>),
    VariadicBase(LuaType),
}

pub(in crate::semantic::generic) fn is_literal_candidate(ty: &LuaType) -> bool {
    match ty {
        LuaType::StringConst(_)
        | LuaType::DocStringConst(_)
        | LuaType::IntegerConst(_)
        | LuaType::DocIntegerConst(_)
        | LuaType::FloatConst(_)
        | LuaType::BooleanConst(_)
        | LuaType::DocBooleanConst(_)
        | LuaType::TableConst(_) => true,
        LuaType::Union(union) => union.into_vec().iter().any(is_literal_candidate),
        LuaType::Tuple(tuple) => tuple.get_types().iter().any(is_literal_candidate),
        LuaType::Variadic(variadic) => match variadic.deref() {
            VariadicType::Base(base) => is_literal_candidate(base),
            VariadicType::Multi(types) => types.iter().any(is_literal_candidate),
        },
        _ => false,
    }
}

fn is_fresh_literal_expr(expr: &LuaExpr) -> bool {
    match expr {
        LuaExpr::LiteralExpr(_) | LuaExpr::TableExpr(_) => true,
        LuaExpr::ParenExpr(paren) => paren
            .get_expr()
            .is_some_and(|expr| is_fresh_literal_expr(&expr)),
        _ => false,
    }
}

pub(in crate::semantic::generic) fn get_str_tpl_infer_type(name: &str) -> LuaType {
    match name {
        "unknown" => LuaType::Unknown,
        "never" => LuaType::Never,
        "nil" | "void" => LuaType::Nil,
        "any" => LuaType::Any,
        "userdata" => LuaType::Userdata,
        "thread" => LuaType::Thread,
        "boolean" | "bool" => LuaType::Boolean,
        "string" => LuaType::String,
        "integer" | "int" => LuaType::Integer,
        "number" => LuaType::Number,
        "io" => LuaType::Io,
        "self" => LuaType::SelfInfer,
        "global" => LuaType::Global,
        "function" => LuaType::Function,
        _ => LuaType::Ref(LuaTypeDeclId::global(name)),
    }
}

pub(in crate::semantic::generic) fn escape_alias(db: &DbIndex, may_alias: &LuaType) -> LuaType {
    if let LuaType::Ref(type_id) = may_alias
        && let Some(type_decl) = db.get_type_index().get_type_decl(type_id)
        && type_decl.is_alias()
        && let Some(origin_type) = type_decl.get_alias_origin(db, None)
    {
        return origin_type.clone();
    }

    may_alias.clone()
}

pub(super) fn is_tpl_at_top_level(db: &DbIndex, ty: &LuaType, tpl_id: GenericTplId) -> bool {
    is_tpl_at_top_level_with_guard(db, ty, tpl_id, &mut HashSet::new())
}

fn is_tpl_at_top_level_with_guard(
    db: &DbIndex,
    ty: &LuaType,
    tpl_id: GenericTplId,
    visited_aliases: &mut HashSet<LuaTypeDeclId>,
) -> bool {
    match ty {
        LuaType::TplRef(tpl) | LuaType::ConstTplRef(tpl) => tpl.get_tpl_id() == tpl_id,
        LuaType::Union(union) => union.into_vec().iter().any(|member| {
            let mut branch_aliases = visited_aliases.clone();
            is_tpl_at_top_level_with_guard(db, member, tpl_id, &mut branch_aliases)
        }),
        LuaType::MultiLineUnion(multi) => multi.get_unions().iter().any(|(member, _)| {
            let mut branch_aliases = visited_aliases.clone();
            is_tpl_at_top_level_with_guard(db, member, tpl_id, &mut branch_aliases)
        }),
        LuaType::Generic(generic) => {
            let type_decl_id = generic.get_base_type_id_ref();
            let Some(alias_param) =
                get_transparent_alias_param_index(db, type_decl_id, visited_aliases)
            else {
                return false;
            };

            generic.get_params().get(alias_param).is_some_and(|param| {
                is_tpl_at_top_level_with_guard(db, param, tpl_id, visited_aliases)
            })
        }
        _ => false,
    }
}

fn get_transparent_alias_param_index(
    db: &DbIndex,
    type_decl_id: &LuaTypeDeclId,
    visited_aliases: &mut HashSet<LuaTypeDeclId>,
) -> Option<usize> {
    if !visited_aliases.insert(type_decl_id.clone()) {
        return None;
    }

    let type_decl = db.get_type_index().get_type_decl(type_decl_id)?;
    if !type_decl.is_alias() {
        return None;
    };
    let origin = type_decl.get_alias_ref()?;

    match origin {
        LuaType::TplRef(tpl) | LuaType::ConstTplRef(tpl)
            if matches!(tpl.get_tpl_id(), GenericTplId::Type(_)) =>
        {
            Some(tpl.get_tpl_id().get_idx())
        }
        LuaType::Generic(generic) => {
            get_transparent_alias_param_index(db, generic.get_base_type_id_ref(), visited_aliases)
                .and_then(|alias_param| generic.get_params().get(alias_param))
                .and_then(|param| match param {
                    LuaType::TplRef(tpl) | LuaType::ConstTplRef(tpl)
                        if matches!(tpl.get_tpl_id(), GenericTplId::Type(_)) =>
                    {
                        Some(tpl.get_tpl_id().get_idx())
                    }
                    _ => None,
                })
        }
        _ => None,
    }
}
