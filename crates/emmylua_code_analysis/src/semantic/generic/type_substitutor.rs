use hashbrown::{HashMap, HashSet};

use super::tpl_pattern::constant_decay;
use crate::{DbIndex, GenericTplId, LuaType, LuaTypeDeclId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionalCheckMode {
    Normal,
    /// 宽松条件判断模式, 用于证明 conditional 一定不成立的场景.
    Permissive,
    /// 刚性条件判断模式, 只在两侧都足够稳定时证明 conditional 一定成立.
    Rigid,
}

#[derive(Debug, Clone, Copy)]
pub struct GenericEvalEnv<'a> {
    pub db: &'a DbIndex,
    pub substitutor: &'a TypeSubstitutor,
    pub conditional_check_mode: ConditionalCheckMode,
}

impl<'a> GenericEvalEnv<'a> {
    pub fn new(db: &'a DbIndex, substitutor: &'a TypeSubstitutor) -> Self {
        Self {
            db,
            substitutor,
            conditional_check_mode: ConditionalCheckMode::Normal,
        }
    }

    pub fn with_conditional_check_mode(&self, mode: ConditionalCheckMode) -> Self {
        Self {
            db: self.db,
            substitutor: self.substitutor,
            conditional_check_mode: mode,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TypeSubstitutor {
    tpl_replace_map: HashMap<GenericTplId, SubstitutorValue>,
    conditional_tpl_replace_map: HashMap<GenericTplId, LuaType>,
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
            conditional_tpl_replace_map: HashMap::new(),
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
            conditional_tpl_replace_map: HashMap::new(),
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
            conditional_tpl_replace_map: HashMap::new(),
            alias_type_id: Some(alias_type_id),
            self_type: None,
        }
    }

    pub fn add_need_infer_tpls(&mut self, tpl_ids: HashSet<GenericTplId>) {
        for tpl_id in tpl_ids {
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
        self.insert_type_value(tpl_id, SubstitutorTypeValue::new(replace_type, decay));
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
        if !self.can_insert_type(tpl_id) {
            return;
        }

        self.tpl_replace_map
            .insert(tpl_id, SubstitutorValue::MultiTypes(types));
    }

    pub fn insert_multi_base(&mut self, tpl_id: GenericTplId, type_base: LuaType) {
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

    pub fn insert_conditional_type(&mut self, tpl_id: GenericTplId, replace_type: LuaType) {
        self.conditional_tpl_replace_map
            .insert(tpl_id, into_ref_type(replace_type));
    }

    pub fn get_conditional_raw_type(&self, tpl_id: GenericTplId) -> Option<&LuaType> {
        self.conditional_tpl_replace_map.get(&tpl_id)
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
