use hashbrown::{HashMap, HashSet};

use crate::{DbIndex, GenericTplId, LuaType, LuaTypeDeclId};
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
    ReplaceConstType(LuaType),
    ConditionalInferType(LuaType),
    VariadicParams(Vec<(String, Option<LuaType>)>),
    InferredMultiTypes(Vec<LuaType>),
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
                    value: SubstitutorTypeValue::new(ty),
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
                    value: SubstitutorTypeValue::new(ty),
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
                        value: SubstitutorTypeValue::new(replace_type),
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
                        value: SubstitutorTypeValue::new(replace_type),
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
                        value: SubstitutorTypeValue::new(replace_type),
                    },
                    TplBinding::VariadicParams(params) => {
                        let params = params
                            .into_iter()
                            .map(|(name, ty)| (name, ty.map(into_ref_type)))
                            .collect();
                        SubstitutorValue::Params(params)
                    }
                    TplBinding::InferredMultiTypes(types) => SubstitutorValue::MultiTypes {
                        values: types.into_iter().map(SubstitutorTypeValue::new).collect(),
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
}

impl SubstitutorTypeValue {
    fn new(raw: LuaType) -> Self {
        Self {
            raw: into_ref_type(raw),
        }
    }

    pub fn raw(&self) -> &LuaType {
        &self.raw
    }

    pub(super) fn resolved(&self) -> &LuaType {
        &self.raw
    }
}

#[derive(Debug, Clone)]
pub(super) enum SubstitutorValue {
    None,
    Type { value: SubstitutorTypeValue },
    Params(Vec<(String, Option<LuaType>)>),
    MultiTypes { values: Vec<SubstitutorTypeValue> },
    MultiBase(LuaType),
}

impl SubstitutorValue {
    pub fn is_none(&self) -> bool {
        matches!(self, SubstitutorValue::None)
    }
}

fn into_ref_type(ty: LuaType) -> LuaType {
    match ty {
        LuaType::Def(type_decl_id) => LuaType::Ref(type_decl_id),
        _ => ty,
    }
}
