use std::sync::Arc;

use crate::{DbIndex, LuaType, LuaTypeDeclId};

use super::super::TypeMapper;

const MAX_INSTANTIATION_DEPTH: usize = 128;
const MAX_ALIAS_STACK: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::semantic::generic) enum UninferredTplPolicy {
    /// 未推断模板按 `default -> constraint -> unknown` 推断成实际类型.
    Fallback,
    /// 没有默认值的未推断模板仍保留为 `TplRef`, 让后续调用点继续参与参数推导.
    PreserveTplRef,
}

#[derive(Debug)]
pub(in crate::semantic::generic) struct GenericInstantiateContext<'a> {
    pub db: &'a DbIndex,
    pub mapper: TypeMapper,
    self_type: Option<&'a LuaType>,
    alias_stack: Arc<[LuaTypeDeclId]>,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::semantic::generic) struct GenericInstantiateFrame {
    policy: UninferredTplPolicy,
    depth: usize,
}

impl<'a> GenericInstantiateContext<'a> {
    pub(super) fn new(
        db: &'a DbIndex,
        mapper: &TypeMapper,
        self_type: Option<&'a LuaType>,
    ) -> Self {
        Self {
            db,
            mapper: mapper.clone(),
            self_type,
            alias_stack: Arc::from([]),
        }
    }

    pub(super) fn root_frame(&self) -> GenericInstantiateFrame {
        GenericInstantiateFrame {
            policy: UninferredTplPolicy::Fallback,
            depth: 0,
        }
    }

    pub(super) fn self_type(&self) -> Option<&LuaType> {
        self.self_type
    }

    pub(super) fn with_mapper(&self, mapper: TypeMapper) -> GenericInstantiateContext<'a> {
        GenericInstantiateContext {
            db: self.db,
            mapper,
            self_type: self.self_type,
            alias_stack: self.alias_stack.clone(),
        }
    }

    pub(super) fn enter_alias_stack(
        &self,
        alias_type_id: &LuaTypeDeclId,
    ) -> Option<Arc<[LuaTypeDeclId]>> {
        if self.alias_stack.len() >= MAX_ALIAS_STACK
            || self.alias_stack.iter().any(|id| id == alias_type_id)
        {
            return None;
        }

        let mut alias_stack = Vec::with_capacity(self.alias_stack.len() + 1);
        alias_stack.extend(self.alias_stack.iter().cloned());
        alias_stack.push(alias_type_id.clone());
        Some(Arc::from(alias_stack))
    }

    pub(super) fn with_alias_stack(
        &self,
        alias_stack: Arc<[LuaTypeDeclId]>,
        mapper: TypeMapper,
    ) -> GenericInstantiateContext<'a> {
        GenericInstantiateContext {
            db: self.db,
            mapper,
            self_type: self.self_type,
            alias_stack,
        }
    }
}

impl GenericInstantiateFrame {
    pub(super) fn with_policy(self, policy: UninferredTplPolicy) -> Self {
        Self { policy, ..self }
    }

    pub(super) fn should_preserve_tpl_ref(&self) -> bool {
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
