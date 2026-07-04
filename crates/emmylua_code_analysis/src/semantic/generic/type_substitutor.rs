use hashbrown::{HashMap, HashSet};
use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
};

use super::widening::widen_literal_type;
use crate::{DbIndex, GenericTplId, LuaSignatureId, LuaType, LuaTypeDeclId};

#[derive(Debug)]
pub struct GenericInstantiateContext<'a> {
    pub db: &'a DbIndex,
    pub substitutor: &'a TypeSubstitutor,
    pub resolve_mode: GenericResolveMode,
    instantiating_signatures: Rc<RefCell<HashSet<LuaSignatureId>>>,
}

impl<'a> GenericInstantiateContext<'a> {
    pub fn new(db: &'a DbIndex, substitutor: &'a TypeSubstitutor) -> Self {
        Self {
            db,
            substitutor,
            resolve_mode: GenericResolveMode::Value,
            instantiating_signatures: Rc::new(RefCell::new(HashSet::new())),
        }
    }

    pub fn with_substitutor<'b>(
        &'b self,
        substitutor: &'b TypeSubstitutor,
    ) -> GenericInstantiateContext<'b> {
        GenericInstantiateContext {
            db: self.db,
            substitutor,
            resolve_mode: self.resolve_mode,
            instantiating_signatures: self.instantiating_signatures.clone(),
        }
    }

    pub fn with_resolve_mode(
        &self,
        resolve_mode: GenericResolveMode,
    ) -> GenericInstantiateContext<'a> {
        GenericInstantiateContext {
            db: self.db,
            substitutor: self.substitutor,
            resolve_mode,
            instantiating_signatures: self.instantiating_signatures.clone(),
        }
    }

    pub(super) fn enter_signature(
        &self,
        signature_id: LuaSignatureId,
    ) -> Option<InstantiatingSignatureGuard> {
        if !self
            .instantiating_signatures
            .borrow_mut()
            .insert(signature_id)
        {
            return None;
        }

        Some(InstantiatingSignatureGuard {
            signatures: self.instantiating_signatures.clone(),
            signature_id,
        })
    }
}

#[derive(Debug)]
pub(super) struct InstantiatingSignatureGuard {
    signatures: Rc<RefCell<HashSet<LuaSignatureId>>>,
    signature_id: LuaSignatureId,
}

impl Drop for InstantiatingSignatureGuard {
    fn drop(&mut self) {
        self.signatures.borrow_mut().remove(&self.signature_id);
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
                SubstitutorValue::Type(GenericCandidate::new(ty, LiteralPolicy::Preserve)),
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
                SubstitutorValue::Type(GenericCandidate::new(ty, LiteralPolicy::Preserve)),
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

    pub fn insert_value(&mut self, tpl_id: GenericTplId, value: SubstitutorValue) {
        if tpl_id.is_conditional_infer()
            || self
                .tpl_replace_map
                .get(&tpl_id)
                .is_some_and(|value| !value.is_none())
        {
            return;
        }

        self.tpl_replace_map.insert(tpl_id, value.normalize());
    }

    pub fn infer_value(&mut self, tpl_id: GenericTplId, value: SubstitutorValue) {
        if tpl_id.is_conditional_infer()
            || !self
                .tpl_replace_map
                .get(&tpl_id)
                .is_some_and(SubstitutorValue::is_none)
        {
            return;
        }

        self.tpl_replace_map.insert(tpl_id, value.normalize());
    }

    pub(super) fn replace_value(&mut self, tpl_id: GenericTplId, value: SubstitutorValue) {
        self.tpl_replace_map.insert(tpl_id, value.normalize());
    }

    pub fn get(&self, tpl_id: GenericTplId) -> Option<&SubstitutorValue> {
        self.tpl_replace_map.get(&tpl_id)
    }

    pub fn without_pending_tpls(
        &self,
        mut should_remove: impl FnMut(GenericTplId) -> bool,
    ) -> Self {
        let mut substitutor = self.clone();
        substitutor
            .tpl_replace_map
            .retain(|tpl_id, value| !(value.is_none() && should_remove(*tpl_id)));

        substitutor
    }

    pub fn resolve_type(
        &self,
        tpl_id: GenericTplId,
        resolve_mode: GenericResolveMode,
        is_const: bool,
    ) -> Option<&LuaType> {
        match self.tpl_replace_map.get(&tpl_id) {
            Some(SubstitutorValue::Type(candidate)) => {
                Some(candidate.resolve(resolve_mode, is_const))
            }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericResolveMode {
    Value,
    Literal,
}

impl GenericResolveMode {
    fn preserves_literal(self) -> bool {
        matches!(self, GenericResolveMode::Literal)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LiteralPolicy {
    Preserve,
    Widen,
    FreshWidening,
}

#[derive(Debug, Clone)]
pub struct GenericCandidate {
    original: LuaType,
    widened: OnceCell<Option<LuaType>>,
    literal_policy: LiteralPolicy,
}

impl GenericCandidate {
    pub(super) fn new(original: LuaType, literal_policy: LiteralPolicy) -> Self {
        Self {
            original: into_ref_type(original),
            widened: OnceCell::new(),
            literal_policy,
        }
    }

    pub(super) fn resolve(&self, resolve_mode: GenericResolveMode, is_const: bool) -> &LuaType {
        if is_const || self.literal_policy == LiteralPolicy::Preserve {
            return &self.original;
        }

        if self.literal_policy == LiteralPolicy::FreshWidening && resolve_mode.preserves_literal() {
            return &self.original;
        }

        self.widened
            .get_or_init(|| {
                let widened = into_ref_type(widen_literal_type(self.original.clone()));
                if widened == self.original {
                    None
                } else {
                    Some(widened)
                }
            })
            .as_ref()
            .unwrap_or(&self.original)
    }
}

#[derive(Debug, Clone)]
pub enum SubstitutorValue {
    None,
    Type(GenericCandidate),
    Params(Vec<(String, Option<LuaType>)>),
    MultiTypes(Vec<LuaType>),
    MultiBase(LuaType),
}

impl SubstitutorValue {
    pub fn is_none(&self) -> bool {
        matches!(self, SubstitutorValue::None)
    }

    fn normalize(self) -> Self {
        match self {
            SubstitutorValue::Params(params) => SubstitutorValue::Params(
                params
                    .into_iter()
                    .map(|(name, ty)| (name, ty.map(into_ref_type)))
                    .collect(),
            ),
            value => value,
        }
    }
}

fn into_ref_type(ty: LuaType) -> LuaType {
    match ty {
        LuaType::Def(type_decl_id) => LuaType::Ref(type_decl_id),
        _ => ty,
    }
}
