use hashbrown::{HashMap, HashSet};

use super::instantiate_type::{TplCandidateSource, finalize_inferred_tpl_candidate};
use crate::{DbIndex, GenericTpl, GenericTplId, LuaType, LuaTypeDeclId};
use std::sync::Arc;

const MAX_INSTANTIATION_DEPTH: usize = 128;
const MAX_ALIAS_STACK: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UninferredTplPolicy {
    /// 未推断模板按 `default -> constraint -> unknown` 推断成实际类型.
    Fallback,
    /// 没有默认值的未推断模板仍保留为 `TplRef`, 让后续调用点继续参与参数推导.
    PreserveTplRef,
}

pub(in crate::semantic::generic) enum TplBinding {
    FinalizedType(LuaType),
    InferredType {
        ty: LuaType,
        source: TplCandidateSource,
        top_level: bool,
    },
    ReplaceConstType(LuaType),
    ConditionalInferType(LuaType),
    VariadicParams(Vec<(String, Option<LuaType>)>),
    InferredMultiTypes {
        types: Vec<LuaType>,
        source: TplCandidateSource,
        top_level: bool,
    },
    VariadicBase(LuaType),
}

#[derive(Debug)]
pub struct GenericInstantiateContext<'a> {
    pub db: &'a DbIndex,
    pub substitutor: &'a TypeSubstitutor,
    alias_stack: Arc<[LuaTypeDeclId]>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct GenericInstantiateFrame {
    policy: UninferredTplPolicy,
    depth: usize,
}

impl<'a> GenericInstantiateContext<'a> {
    pub fn new(db: &'a DbIndex, substitutor: &'a TypeSubstitutor) -> Self {
        Self {
            db,
            substitutor,
            alias_stack: Arc::from([]),
        }
    }

    pub(super) fn root_frame(&self) -> GenericInstantiateFrame {
        GenericInstantiateFrame {
            policy: UninferredTplPolicy::Fallback,
            depth: 0,
        }
    }

    pub(super) fn with_substitutor<'b>(
        &'b self,
        substitutor: &'b TypeSubstitutor,
    ) -> GenericInstantiateContext<'b> {
        GenericInstantiateContext {
            db: self.db,
            substitutor,
            alias_stack: self.alias_stack.clone(),
        }
    }

    pub(super) fn enter_alias(
        &self,
        alias_type_id: &LuaTypeDeclId,
    ) -> Option<GenericInstantiateContext<'a>> {
        if self.alias_stack.len() >= MAX_ALIAS_STACK
            || self.alias_stack.iter().any(|id| id == alias_type_id)
        {
            return None;
        }

        let mut alias_stack = Vec::with_capacity(self.alias_stack.len() + 1);
        alias_stack.extend(self.alias_stack.iter().cloned());
        alias_stack.push(alias_type_id.clone());
        Some(GenericInstantiateContext {
            db: self.db,
            substitutor: self.substitutor,
            alias_stack: Arc::from(alias_stack),
        })
    }
}

impl GenericInstantiateFrame {
    pub(super) fn with_policy(self, policy: UninferredTplPolicy) -> Self {
        Self { policy, ..self }
    }

    pub fn should_preserve_tpl_ref(&self) -> bool {
        self.policy == UninferredTplPolicy::PreserveTplRef
    }

    pub(super) fn enter(self) -> Option<Self> {
        if self.depth >= MAX_INSTANTIATION_DEPTH {
            return None;
        }

        Some(Self {
            depth: self.depth + 1,
            ..self
        })
    }
}

#[derive(Debug, Clone)]
pub struct TypeSubstitutor {
    tpl_replace_map: HashMap<GenericTplId, SubstitutorValue>,
    alias_type_id: Option<LuaTypeDeclId>,
    self_type: Option<LuaType>,
}

impl Default for TypeSubstitutor {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeSubstitutor {
    pub fn new() -> Self {
        Self {
            tpl_replace_map: HashMap::new(),
            alias_type_id: None,
            self_type: None,
        }
    }

    pub fn from_type_array(type_array: Vec<LuaType>) -> Self {
        let mut tpl_replace_map = HashMap::new();
        for (i, ty) in type_array.into_iter().enumerate() {
            tpl_replace_map.insert(
                GenericTplId::Type(i as u32),
                SubstitutorValue::Type {
                    value: SubstitutorTypeValue::new(ty, true),
                    source: TplCandidateSource::Finalized,
                    top_level: true,
                },
            );
        }
        Self {
            tpl_replace_map,
            alias_type_id: None,
            self_type: None,
        }
    }

    pub fn from_alias(
        db: &DbIndex,
        type_array: Vec<LuaType>,
        alias_type_id: LuaTypeDeclId,
    ) -> Self {
        let params = db.get_type_index().get_generic_params(&alias_type_id);

        let mut tpl_replace_map = HashMap::new();
        for (i, ty) in type_array.into_iter().enumerate() {
            let tpl_id = params
                .and_then(|params| params.get(i))
                .and_then(|param| param.tpl_id)
                .unwrap_or(GenericTplId::Type(i as u32));
            tpl_replace_map.insert(
                tpl_id,
                SubstitutorValue::Type {
                    value: SubstitutorTypeValue::new(ty, true),
                    source: TplCandidateSource::Finalized,
                    top_level: true,
                },
            );
        }

        Self {
            tpl_replace_map,
            alias_type_id: Some(alias_type_id),
            self_type: None,
        }
    }

    pub fn prepare_inference_slots(&mut self, tpl_ids: HashSet<GenericTplId>) {
        for tpl_id in tpl_ids {
            // conditional infer id 只属于条件类型内部匹配, 不参与普通调用/类型泛型推导.
            if tpl_id.is_conditional_infer() {
                continue;
            }

            self.tpl_replace_map
                .entry(tpl_id)
                .or_insert(SubstitutorValue::None);
        }
    }

    pub fn has_unresolved_inference_slots(&self) -> bool {
        self.tpl_replace_map
            .values()
            .any(|value| matches!(value, SubstitutorValue::None))
    }

    pub fn bind_type(&mut self, tpl_id: GenericTplId, replace_type: LuaType) {
        self.bind(tpl_id, TplBinding::FinalizedType(replace_type));
    }

    pub(in crate::semantic::generic) fn bind(&mut self, tpl_id: GenericTplId, binding: TplBinding) {
        match binding {
            TplBinding::ConditionalInferType(replace_type) => {
                // 只有 conditional true 分支提交 infer 结果时允许写入 scoped conditional infer id.
                if !tpl_id.is_conditional_infer() {
                    return;
                }

                self.tpl_replace_map.insert(
                    tpl_id,
                    SubstitutorValue::Type {
                        value: SubstitutorTypeValue::new(replace_type, false),
                        source: TplCandidateSource::ConstPreserving,
                        top_level: true,
                    },
                );
            }
            TplBinding::ReplaceConstType(replace_type) => {
                if tpl_id.is_conditional_infer() {
                    return;
                }

                self.tpl_replace_map.insert(
                    tpl_id,
                    SubstitutorValue::Type {
                        value: SubstitutorTypeValue::new(replace_type, false),
                        source: TplCandidateSource::ConstPreserving,
                        top_level: true,
                    },
                );
            }
            binding => {
                // 普通替换入口不能写入 conditional infer, 避免条件类型局部绑定泄露到外层.
                if tpl_id.is_conditional_infer() || !self.can_bind(tpl_id) {
                    return;
                }

                let value = match binding {
                    TplBinding::FinalizedType(replace_type) => SubstitutorValue::Type {
                        value: SubstitutorTypeValue::new(replace_type, true),
                        source: TplCandidateSource::Finalized,
                        top_level: true,
                    },
                    TplBinding::InferredType {
                        ty,
                        source,
                        top_level,
                    } => SubstitutorValue::Type {
                        value: SubstitutorTypeValue::new(ty, false),
                        source,
                        top_level,
                    },
                    TplBinding::VariadicParams(params) => {
                        let params = params
                            .into_iter()
                            .map(|(name, ty)| (name, ty.map(into_ref_type)))
                            .collect();
                        SubstitutorValue::Params(params)
                    }
                    TplBinding::InferredMultiTypes {
                        types,
                        source,
                        top_level,
                    } => SubstitutorValue::MultiTypes {
                        values: types
                            .into_iter()
                            .map(|ty| {
                                SubstitutorTypeValue::new(
                                    ty,
                                    source == TplCandidateSource::Finalized,
                                )
                            })
                            .collect(),
                        source,
                        top_level,
                    },
                    TplBinding::VariadicBase(type_base) => SubstitutorValue::MultiBase(type_base),
                    TplBinding::ReplaceConstType(_) | TplBinding::ConditionalInferType(_) => {
                        unreachable!("handled before regular binding")
                    }
                };

                self.tpl_replace_map.insert(tpl_id, value);
            }
        }
    }

    fn can_bind(&self, tpl_id: GenericTplId) -> bool {
        if let Some(value) = self.tpl_replace_map.get(&tpl_id) {
            return value.is_none();
        }

        true
    }

    pub(super) fn get(&self, tpl_id: GenericTplId) -> Option<&SubstitutorValue> {
        self.tpl_replace_map.get(&tpl_id)
    }

    pub fn get_raw_type(&self, tpl_id: GenericTplId) -> Option<&LuaType> {
        match self.tpl_replace_map.get(&tpl_id) {
            Some(SubstitutorValue::Type { value, .. }) => Some(value.raw()),
            _ => None,
        }
    }

    pub(super) fn finalize_inferred_types<'a>(
        &mut self,
        db: &DbIndex,
        generic_tpls: impl IntoIterator<Item = &'a Arc<GenericTpl>>,
        return_type: &LuaType,
    ) {
        for tpl in generic_tpls {
            let tpl_id = tpl.get_tpl_id();
            let return_top_level = is_tpl_at_top_level(db, return_type, tpl_id);
            let Some(value) = self.tpl_replace_map.get(&tpl_id) else {
                continue;
            };

            let finalized_value = match value {
                SubstitutorValue::Type {
                    value,
                    source,
                    top_level,
                } => {
                    if value.is_finalized() {
                        None
                    } else {
                        Some(SubstitutorValue::Type {
                            value: value.finalized(
                                db,
                                tpl.as_ref(),
                                *source,
                                *top_level,
                                return_top_level,
                                self,
                            ),
                            source: TplCandidateSource::Finalized,
                            top_level: true,
                        })
                    }
                }
                SubstitutorValue::MultiTypes {
                    values,
                    source,
                    top_level,
                } => {
                    if *source == TplCandidateSource::Finalized {
                        None
                    } else {
                        let values = values
                            .iter()
                            .map(|value| {
                                value.finalized(
                                    db,
                                    tpl.as_ref(),
                                    *source,
                                    *top_level,
                                    return_top_level,
                                    self,
                                )
                            })
                            .collect();
                        Some(SubstitutorValue::MultiTypes {
                            values,
                            source: TplCandidateSource::Finalized,
                            top_level: true,
                        })
                    }
                }
                _ => None,
            };

            if let Some(finalized_value) = finalized_value {
                self.tpl_replace_map.insert(tpl_id, finalized_value);
            }
        }
    }

    pub fn check_recursion(&self, type_id: &LuaTypeDeclId) -> bool {
        if let Some(alias_type_id) = &self.alias_type_id
            && alias_type_id == type_id
        {
            return true;
        }

        false
    }

    pub fn add_self_type(&mut self, self_type: LuaType) {
        self.self_type = Some(self_type);
    }

    pub fn get_self_type(&self) -> Option<&LuaType> {
        self.self_type.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct SubstitutorTypeValue {
    raw: LuaType,
    finalized: Option<LuaType>,
}

impl SubstitutorTypeValue {
    fn new(raw: LuaType, already_finalized: bool) -> Self {
        let raw = into_ref_type(raw);
        let finalized = already_finalized.then(|| raw.clone());
        Self { raw, finalized }
    }

    pub fn raw(&self) -> &LuaType {
        &self.raw
    }

    pub(super) fn resolved(&self) -> &LuaType {
        self.finalized.as_ref().unwrap_or(&self.raw)
    }

    fn is_finalized(&self) -> bool {
        self.finalized.is_some()
    }

    fn finalized(
        &self,
        db: &DbIndex,
        tpl: &GenericTpl,
        source: TplCandidateSource,
        top_level: bool,
        return_top_level: bool,
        substitutor: &TypeSubstitutor,
    ) -> Self {
        let finalized = finalize_inferred_tpl_candidate(
            db,
            tpl,
            &self.raw,
            source,
            top_level,
            return_top_level,
            substitutor,
        );
        Self {
            raw: self.raw.clone(),
            finalized: Some(finalized),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum SubstitutorValue {
    None,
    Type {
        value: SubstitutorTypeValue,
        source: TplCandidateSource,
        top_level: bool,
    },
    Params(Vec<(String, Option<LuaType>)>),
    MultiTypes {
        values: Vec<SubstitutorTypeValue>,
        source: TplCandidateSource,
        top_level: bool,
    },
    MultiBase(LuaType),
}

impl SubstitutorValue {
    pub fn is_none(&self) -> bool {
        matches!(self, SubstitutorValue::None)
    }
}

fn is_tpl_at_top_level(db: &DbIndex, ty: &LuaType, tpl_id: GenericTplId) -> bool {
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

fn into_ref_type(ty: LuaType) -> LuaType {
    match ty {
        LuaType::Def(type_decl_id) => LuaType::Ref(type_decl_id),
        _ => ty,
    }
}
