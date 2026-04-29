use hashbrown::{HashMap, HashSet};

use super::tpl_pattern::constant_decay;
use crate::{DbIndex, GenericTplId, LuaType, LuaTypeDeclId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UninferredTplPolicy {
    /// 未推断模板按 `default -> constraint -> unknown` 推断成实际类型.
    Fallback,
    /// 没有默认值的未推断模板仍保留为 `TplRef`, 让后续调用点继续参与参数推导.
    PreserveTplRef,
}

#[derive(Debug)]
pub struct GenericInstantiateContext<'a> {
    pub db: &'a DbIndex,
    pub substitutor: &'a TypeSubstitutor,
    policy: UninferredTplPolicy,
}

impl<'a> GenericInstantiateContext<'a> {
    pub fn new(db: &'a DbIndex, substitutor: &'a TypeSubstitutor) -> Self {
        Self {
            db,
            substitutor,
            policy: UninferredTplPolicy::Fallback,
        }
    }

    pub(super) fn with_policy(&self, policy: UninferredTplPolicy) -> GenericInstantiateContext<'a> {
        GenericInstantiateContext {
            db: self.db,
            substitutor: self.substitutor,
            policy,
        }
    }

    pub fn with_substitutor<'b>(
        &'b self,
        substitutor: &'b TypeSubstitutor,
    ) -> GenericInstantiateContext<'b> {
        GenericInstantiateContext {
            db: self.db,
            substitutor,
            policy: self.policy,
        }
    }

    pub fn should_preserve_tpl_ref(&self) -> bool {
        self.policy == UninferredTplPolicy::PreserveTplRef
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
                SubstitutorValue::Type(SubstitutorTypeValue::new(ty, true)),
            );
        }
        Self {
            tpl_replace_map,
            alias_type_id: None,
            self_type: None,
        }
    }

    pub fn from_alias(type_array: Vec<LuaType>, alias_type_id: LuaTypeDeclId) -> Self {
        let mut tpl_replace_map = HashMap::new();
        for (i, ty) in type_array.into_iter().enumerate() {
            tpl_replace_map.insert(
                GenericTplId::Type(i as u32),
                SubstitutorValue::Type(SubstitutorTypeValue::new(ty, true)),
            );
        }
        Self {
            tpl_replace_map,
            alias_type_id: Some(alias_type_id),
            self_type: None,
        }
    }

    pub fn add_need_infer_tpls(&mut self, tpl_ids: HashSet<GenericTplId>) {
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

    pub fn is_infer_all_tpl(&self) -> bool {
        for value in self.tpl_replace_map.values() {
            if let SubstitutorValue::None = value {
                return false;
            }
        }
        true
    }

    pub fn insert_type(&mut self, tpl_id: GenericTplId, replace_type: LuaType, decay: bool) {
        // 普通替换入口不能写入 conditional infer, 避免条件类型局部绑定泄露到外层.
        if tpl_id.is_conditional_infer() {
            return;
        }

        self.insert_type_value(tpl_id, SubstitutorTypeValue::new(replace_type, decay));
    }

    pub fn insert_conditional_infer_type(&mut self, tpl_id: GenericTplId, replace_type: LuaType) {
        // 只有 conditional true 分支提交 infer 结果时允许写入 scoped conditional infer id.
        if !tpl_id.is_conditional_infer() {
            return;
        }

        self.tpl_replace_map.insert(
            tpl_id,
            SubstitutorValue::Type(SubstitutorTypeValue::new(replace_type, false)),
        );
    }

    fn insert_type_value(&mut self, tpl_id: GenericTplId, value: SubstitutorTypeValue) {
        if !self.can_insert_type(tpl_id) {
            return;
        }

        self.tpl_replace_map
            .insert(tpl_id, SubstitutorValue::Type(value));
    }

    fn can_insert_type(&self, tpl_id: GenericTplId) -> bool {
        if let Some(value) = self.tpl_replace_map.get(&tpl_id) {
            return value.is_none();
        }

        true
    }

    pub fn insert_params(&mut self, tpl_id: GenericTplId, params: Vec<(String, Option<LuaType>)>) {
        if tpl_id.is_conditional_infer() {
            return;
        }

        if !self.can_insert_type(tpl_id) {
            return;
        }

        let params = params
            .into_iter()
            .map(|(name, ty)| (name, ty.map(into_ref_type)))
            .collect();

        self.tpl_replace_map
            .insert(tpl_id, SubstitutorValue::Params(params));
    }

    pub fn insert_multi_types(&mut self, tpl_id: GenericTplId, types: Vec<LuaType>) {
        if tpl_id.is_conditional_infer() {
            return;
        }

        if !self.can_insert_type(tpl_id) {
            return;
        }

        self.tpl_replace_map
            .insert(tpl_id, SubstitutorValue::MultiTypes(types));
    }

    pub fn insert_multi_base(&mut self, tpl_id: GenericTplId, type_base: LuaType) {
        if tpl_id.is_conditional_infer() {
            return;
        }

        if !self.can_insert_type(tpl_id) {
            return;
        }

        self.tpl_replace_map
            .insert(tpl_id, SubstitutorValue::MultiBase(type_base));
    }

    pub fn get(&self, tpl_id: GenericTplId) -> Option<&SubstitutorValue> {
        self.tpl_replace_map.get(&tpl_id)
    }

    pub fn get_raw_type(&self, tpl_id: GenericTplId) -> Option<&LuaType> {
        match self.tpl_replace_map.get(&tpl_id) {
            Some(SubstitutorValue::Type(ty)) => Some(ty.raw()),
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
    decayed: DecayedType,
}

#[derive(Debug, Clone)]
enum DecayedType {
    Same,
    Cached(LuaType),
}

impl SubstitutorTypeValue {
    pub fn new(raw: LuaType, decay: bool) -> Self {
        let raw = into_ref_type(raw);
        let decayed = if decay {
            let decayed = into_ref_type(constant_decay(raw.clone()));
            if decayed == raw {
                DecayedType::Same
            } else {
                DecayedType::Cached(decayed)
            }
        } else {
            DecayedType::Same
        };
        Self { raw, decayed }
    }

    pub fn raw(&self) -> &LuaType {
        &self.raw
    }

    pub fn default(&self) -> &LuaType {
        match &self.decayed {
            DecayedType::Same => &self.raw,
            DecayedType::Cached(decayed) => decayed,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SubstitutorValue {
    None,
    Type(SubstitutorTypeValue),
    Params(Vec<(String, Option<LuaType>)>),
    MultiTypes(Vec<LuaType>),
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
