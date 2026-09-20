//! Unified callable candidate set (P6b).

use super::vm::{InferVm, expand_callable_types_in_model};
use crate::semantic_model::infer::overload;
use crate::semantic_model::infer::unify::TplBindings;
use crate::semantic_model::{SemanticModel, member};
use crate::{DeclKind, FileId, LuaFunctionType, LuaMemberKey, LuaType, SemanticId};

#[derive(Debug, Clone, Default)]
pub(crate) struct CallableCandidateSet {
    candidates: Vec<LuaFunctionType>,
}
impl CallableCandidateSet {
    pub(crate) fn new(candidates: Vec<LuaFunctionType>) -> Self {
        let mut deduped = Vec::new();
        for candidate in candidates {
            if !deduped.contains(&candidate) {
                deduped.push(candidate);
            }
        }
        Self {
            candidates: deduped,
        }
    }

    pub(crate) fn candidates(&self) -> &[LuaFunctionType] {
        &self.candidates
    }

    pub(crate) fn into_candidates(self) -> Vec<LuaFunctionType> {
        self.candidates
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }
    /// All callable candidates for `prefix_type.key` (all same-key/inherited members).
    pub(crate) fn from_prefix_type(
        model: &SemanticModel<'_>,
        prefix_type: &LuaType,
        key: &LuaMemberKey,
    ) -> Self {
        Self::from_prefix_type_filtered(model, prefix_type, key, None)
    }

    /// Candidate set scoped to one resolved member.
    ///
    /// File-local owners (`local M`) must not merge same-named members from a
    /// different file; named types / globals / module owners may (cross-file
    /// `@field` overloads, module mutations).
    pub(crate) fn from_prefix_type_for_member(
        model: &SemanticModel<'_>,
        prefix_type: &LuaType,
        key: &LuaMemberKey,
        member_id: &SemanticId,
    ) -> Self {
        Self::from_prefix_type_filtered(model, prefix_type, key, Some(member_id))
    }

    fn from_prefix_type_filtered(
        model: &SemanticModel<'_>,
        prefix_type: &LuaType,
        key: &LuaMemberKey,
        member_id: Option<&SemanticId>,
    ) -> Self {
        let allow_cross_file = member_id
            .and_then(|member_id| member_owner_allows_cross_file(model, member_id))
            .unwrap_or(true);
        let member_file = member_id.and_then(|member_id| member_file_of(model, member_id));
        let mut candidates = Vec::new();
        // The resolved member identity is authoritative: std global members
        // (`table.insert`) are not necessarily reachable from the prefix type
        // surface, but still carry main + `---@overload` signatures.
        if let Some(member_id) = member_id {
            let vm = InferVm::new(model, &[]);
            if let Some(resolved_candidates) = vm.callable_candidates_for_owner_single(member_id) {
                candidates.extend(resolved_candidates);
            }
        }
        let infos = member::member_infos_with_key_all(model, prefix_type, key);
        for info in infos {
            if !allow_cross_file
                && let (Some(info_file), Some(member_file)) = (info.file_id, member_file)
                && info_file != member_file
            {
                continue;
            }
            Self::push_member_info(model, &info, &mut candidates);
        }
        Self::new(candidates)
    }

    fn push_member_info(
        model: &SemanticModel<'_>,
        info: &member::MemberInfo,
        out: &mut Vec<LuaFunctionType>,
    ) {
        // Prefer the identity-level signature projection: it preserves the
        // declaration main signature plus every `---@overload` (std
        // `table.insert` relies on this for its 2-argument form).
        let before = out.len();
        if let (Some(member_id), Some(_file_id)) = (&info.id, info.file_id) {
            let vm = InferVm::new(model, &[]);
            if let Some(candidates) = vm.callable_candidates_for_owner_single(member_id) {
                out.extend(candidates);
            }
        }
        if out.len() == before {
            // `@field fun(...)` has no closure value syntax: project the member type.
            out.extend(expand_callable_types_in_model(model, &info.typ));
        }
        if out.len() == before
            && let (Some(member_id), Some(file_id)) = (&info.id, info.file_id)
        {
            // Runtime closure members are often projected as broad `Function` /
            // `Signature`; recover the real closure signature.
            if let Some(fun) = member_closure_function(model, file_id, member_id) {
                out.push(fun);
            }
        }
    }
    pub(crate) fn select(
        &self,
        model: &SemanticModel<'_>,
        args: &[overload::CallArg],
        colon_call: bool,
        receiver: Option<&LuaType>,
    ) -> Option<(LuaFunctionType, TplBindings)> {
        overload::select_callable(model, self.candidates(), args, colon_call, receiver)
    }

    pub(crate) fn select_partial(
        &self,
        model: &SemanticModel<'_>,
        args: &[overload::CallArg],
        colon_call: bool,
        receiver: Option<&LuaType>,
    ) -> Option<(LuaFunctionType, TplBindings)> {
        overload::select_callable_partial(model, self.candidates(), args, colon_call, receiver)
    }

    pub(crate) fn select_all(
        &self,
        model: &SemanticModel<'_>,
        args: &[overload::CallArg],
        colon_call: bool,
        receiver: Option<&LuaType>,
    ) -> Vec<(LuaFunctionType, TplBindings)> {
        overload::select_callable_all(model, self.candidates(), args, colon_call, receiver)
    }
}

fn member_closure_function(
    model: &SemanticModel<'_>,
    file_id: FileId,
    member_id: &SemanticId,
) -> Option<LuaFunctionType> {
    let facts = model.file_facts_of(file_id)?;
    let member = facts.member_by_id(member_id)?;
    let value_syntax = member.value_syntax?;
    model.type_of_signature_in_file(file_id, value_syntax)
}

fn member_file_of(model: &SemanticModel<'_>, member_id: &SemanticId) -> Option<FileId> {
    match member_id {
        SemanticId::Member(key) => Some(key.file_id),
        SemanticId::Decl(key) => Some(key.file_id),
        _ => {
            let _ = model;
            None
        }
    }
}

fn member_owner_allows_cross_file(
    model: &SemanticModel<'_>,
    member_id: &SemanticId,
) -> Option<bool> {
    let SemanticId::Member(key) = member_id else {
        return Some(true);
    };
    let facts = model.file_facts_of(key.file_id)?;
    let member = facts.member_by_id(member_id)?;
    match &member.owner {
        SemanticId::TypeDef(_) | SemanticId::Name(_) => Some(true),
        SemanticId::Decl(decl_key) => {
            let decl = facts.decl_by_id(&SemanticId::Decl(decl_key.clone()))?;
            Some(matches!(decl.kind, DeclKind::Global))
        }
        _ => Some(false),
    }
}
