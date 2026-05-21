use std::{cell::RefCell, rc::Rc, sync::Arc};

use emmylua_parser::LuaCallExpr;
use hashbrown::{HashMap, HashSet};

use crate::{
    DbIndex, GenericTpl, GenericTplId, LuaInferCache, LuaType, check_type_compact,
    instantiate_type_generic, semantic::generic::regularize_tpl_candidate_type,
};

use crate::semantic::generic::{TypeMapper, TypeMapperValue};

use super::{
    InferenceCandidate, InferenceCandidateView, InferencePriority, InferenceResult,
    InferenceVariance, is_tpl_at_top_level, resolve::resolve_info,
};

#[derive(Debug, Clone)]
pub(super) struct InferenceRecord {
    pub(super) candidate: InferenceCandidate,
    pub(super) priority: InferencePriority,
}

#[derive(Debug, Clone, Default)]
pub(super) struct InferenceInfo {
    // 协变候选, 例如参数位置或返回值位置的正向推断.
    pub(super) covariant: Vec<InferenceRecord>,
    // 逆变候选, 例如函数参数约束的反向传播.
    pub(super) contravariant: Vec<InferenceRecord>,
    // 多返回值或变参展开时收集的候选.
    pub(super) multi: Vec<InferenceRecord>,
    // 已经固定下来的结果, 例如显式泛型或变参参数表.
    pub(super) fixed: Option<InferenceResult>,
    // 最近一次写入的推断结果, 便于在后续阶段判断是否需要重算.
    pub(super) inferred: Option<InferenceResult>,
    // 是否已经被 fixing mapper 读取并固定.
    pub(super) is_fixed: bool,
    // 当前信息是否始终处于顶层位置.
    pub(super) top_level: bool,
    // 该模板收到的最高优先级.
    pub(super) priority: Option<InferencePriority>,
}

impl InferenceInfo {
    fn new() -> Self {
        Self {
            top_level: true,
            ..Self::default()
        }
    }

    fn add_candidate(
        &mut self,
        variance: InferenceVariance,
        candidate: InferenceCandidate,
        top_level: bool,
        priority: InferencePriority,
    ) {
        if self.is_fixed {
            return;
        }

        self.top_level &= top_level;
        self.priority = Some(
            self.priority
                .map_or(priority, |current| current.max(priority)),
        );
        self.inferred = None;
        let record = InferenceRecord {
            candidate,
            priority,
        };
        match variance {
            InferenceVariance::Covariant => self.covariant.push(record),
            InferenceVariance::Contravariant => self.contravariant.push(record),
        }
    }
}

#[derive(Debug)]
pub(in crate::semantic::generic) struct InferenceContext<'a> {
    pub db: &'a DbIndex,
    pub cache: &'a mut LuaInferCache,
    pub call_expr: Option<LuaCallExpr>,
    infos: HashMap<GenericTplId, InferenceInfo>,
}

impl<'a> InferenceContext<'a> {
    pub fn new(
        db: &'a DbIndex,
        cache: &'a mut LuaInferCache,
        call_expr: Option<LuaCallExpr>,
    ) -> Self {
        Self {
            db,
            cache,
            call_expr,
            infos: HashMap::new(),
        }
    }

    pub fn prepare_inference_slots(&mut self, tpl_ids: HashSet<GenericTplId>) {
        for tpl_id in tpl_ids {
            if tpl_id.is_conditional_infer() {
                continue;
            }

            self.infos.entry(tpl_id).or_insert_with(InferenceInfo::new);
        }
    }

    pub fn has_unresolved_inference_slots(&self) -> bool {
        self.infos.values().any(|info| {
            info.fixed.is_none()
                && info.covariant.is_empty()
                && info.contravariant.is_empty()
                && info.multi.is_empty()
        })
    }

    pub fn fix_type(&mut self, tpl_id: GenericTplId, ty: LuaType) {
        if tpl_id.is_conditional_infer() {
            return;
        }

        let info = self.infos.entry(tpl_id).or_default();
        info.fixed = Some(InferenceResult::Type(ty));
        info.inferred = info.fixed.clone();
        info.is_fixed = true;
    }

    pub fn add_variadic_params(
        &mut self,
        tpl_id: GenericTplId,
        params: Vec<(String, Option<LuaType>)>,
    ) {
        if !self.can_bind(tpl_id) {
            return;
        }

        let info = self.infos.entry(tpl_id).or_default();
        if info.fixed.is_some()
            || !info.multi.is_empty()
            || !info.covariant.is_empty()
            || !info.contravariant.is_empty()
        {
            return;
        }

        info.fixed = Some(InferenceResult::VariadicParams(params));
        info.inferred = info.fixed.clone();
        info.is_fixed = true;
    }

    pub fn add_variadic_base(&mut self, tpl_id: GenericTplId, base: LuaType) {
        if !self.can_bind(tpl_id) {
            return;
        }

        let info = self.infos.entry(tpl_id).or_default();
        if info.fixed.is_some()
            || !info.multi.is_empty()
            || !info.covariant.is_empty()
            || !info.contravariant.is_empty()
        {
            return;
        }

        info.fixed = Some(InferenceResult::VariadicBase(base));
        info.inferred = info.fixed.clone();
        info.is_fixed = true;
    }

    pub fn insert_type(
        &mut self,
        tpl_id: GenericTplId,
        candidate: InferenceCandidate,
        variance: InferenceVariance,
        top_level: bool,
        priority: InferencePriority,
    ) {
        if !self.can_bind(tpl_id) {
            return;
        }

        self.infos
            .entry(tpl_id)
            .or_default()
            .add_candidate(variance, candidate, top_level, priority);
    }

    pub fn insert_multi_types(
        &mut self,
        tpl_id: GenericTplId,
        types: Vec<LuaType>,
        view: InferenceCandidateView,
        top_level: bool,
        priority: InferencePriority,
    ) {
        if !self.can_bind(tpl_id) {
            return;
        }

        let info = self.infos.entry(tpl_id).or_default();
        let fixed = if types.len() == 1 {
            Some(InferenceResult::VariadicParams(vec![(
                "var0".to_string(),
                Some(types[0].clone()),
            )]))
        } else {
            Some(InferenceResult::MultiTypes(types.clone()))
        };
        if info.fixed.is_some()
            || !info.multi.is_empty()
            || !info.covariant.is_empty()
            || !info.contravariant.is_empty()
        {
            if top_level {
                info.fixed = fixed;
                info.inferred = info.fixed.clone();
                info.is_fixed = true;
            }
            return;
        }

        info.multi = types
            .into_iter()
            .map(|ty| InferenceRecord {
                candidate: InferenceCandidate { ty, view },
                priority,
            })
            .collect();
        info.top_level &= top_level;
        info.priority = Some(
            info.priority
                .map_or(priority, |current| current.max(priority)),
        );
        info.inferred = None;
    }

    pub(super) fn inferred_variadic_len(&self, tpl_id: GenericTplId) -> Option<usize> {
        let info = self.infos.get(&tpl_id)?;
        if let Some(fixed) = &info.fixed {
            return match fixed {
                InferenceResult::Type(_) => Some(1),
                InferenceResult::MultiTypes(types) => Some(types.len().max(1)),
                InferenceResult::VariadicParams(params) => Some(params.len().max(1)),
                InferenceResult::VariadicBase(_) => None,
            };
        }

        if !info.multi.is_empty() {
            return Some(info.multi.len().max(1));
        }

        if !info.covariant.is_empty() || !info.contravariant.is_empty() {
            return Some(1);
        }

        None
    }

    pub fn fixing_mapper<'b>(
        &mut self,
        generic_tpls: impl IntoIterator<Item = &'b Arc<GenericTpl>>,
        return_type: &LuaType,
    ) -> TypeMapper {
        self.mapper_inner(generic_tpls, return_type, true)
    }

    pub fn non_fixing_mapper<'b>(
        &mut self,
        generic_tpls: impl IntoIterator<Item = &'b Arc<GenericTpl>>,
        return_type: &LuaType,
    ) -> TypeMapper {
        self.mapper_inner(generic_tpls, return_type, false)
    }

    fn mapper_inner<'b>(
        &mut self,
        generic_tpls: impl IntoIterator<Item = &'b Arc<GenericTpl>>,
        return_type: &LuaType,
        fixing: bool,
    ) -> TypeMapper {
        let generic_tpls = generic_tpls.into_iter().collect::<Vec<_>>();
        let sources = generic_tpls
            .iter()
            .map(|tpl| tpl.get_tpl_id())
            .collect::<Vec<_>>();
        let mut fallback_indices = HashMap::with_capacity(generic_tpls.len());
        let fallback_targets = generic_tpls
            .iter()
            .enumerate()
            .map(|(index, tpl)| {
                fallback_indices.entry(tpl.get_tpl_id()).or_insert(index);
                self.infos
                    .get(&tpl.get_tpl_id())
                    .and_then(|info| info.inferred.clone().or_else(|| info.fixed.clone()))
                    .map(inference_result_to_mapper_value)
                    .unwrap_or(TypeMapperValue::None)
            })
            .collect::<Vec<_>>();
        let fallback_targets = Rc::new(RefCell::new(fallback_targets));
        let fallback_mapper = TypeMapper::from_inference_fallback(
            Rc::new(fallback_indices),
            fallback_targets.clone(),
        );
        let targets = (0..generic_tpls.len())
            .map(|index| {
                let value = self.get_inferred_mapper_value(
                    &generic_tpls,
                    index,
                    return_type,
                    &fallback_mapper,
                    fixing,
                );
                if value != TypeMapperValue::None
                    && let Some(target) = fallback_targets.borrow_mut().get_mut(index)
                {
                    *target = value.clone();
                }
                value
            })
            .collect();
        TypeMapper::from_values(sources, targets)
    }

    fn can_bind(&self, tpl_id: GenericTplId) -> bool {
        !tpl_id.is_conditional_infer()
    }

    fn get_inferred_mapper_value(
        &mut self,
        generic_tpls: &[&Arc<GenericTpl>],
        index: usize,
        return_type: &LuaType,
        fallback_mapper: &TypeMapper,
        fixing: bool,
    ) -> TypeMapperValue {
        let tpl = generic_tpls[index];
        let tpl_id = tpl.get_tpl_id();
        let return_top_level = is_tpl_at_top_level(self.db, return_type, tpl_id);

        if let Some(info) = self.infos.get_mut(&tpl_id)
            && let Some(inferred) = info.inferred.clone()
        {
            if fixing {
                info.is_fixed = true;
            }
            return inference_result_to_mapper_value(inferred);
        }

        let inferred = self.infos.get_mut(&tpl_id).and_then(|info| {
            if fixing {
                info.is_fixed = true;
            }

            if let Some(inferred) = &info.inferred {
                return Some(inferred.clone());
            }

            let result = resolve_info(self.db, tpl, info, return_top_level, fallback_mapper)?;
            info.inferred = Some(result.clone());
            Some(result)
        });
        let value = if let Some(inferred) = inferred {
            let value = inference_result_to_mapper_value(inferred);
            apply_inferred_constraint(self.db, tpl, value, fallback_mapper)
        } else {
            TypeMapperValue::None
        };

        if value != TypeMapperValue::None
            && let Some(info) = self.infos.get_mut(&tpl_id)
        {
            info.inferred = Some(mapper_value_to_inference_result(value.clone()));
        }

        value
    }
}

pub(super) fn inference_result_to_mapper_value(result: InferenceResult) -> TypeMapperValue {
    match result {
        InferenceResult::Type(ty) => TypeMapperValue::type_value(ty),
        InferenceResult::MultiTypes(types) => TypeMapperValue::MultiTypes(
            types
                .into_iter()
                .map(|ty| {
                    TypeMapperValue::type_value(ty)
                        .raw_type()
                        .unwrap_or(LuaType::Unknown)
                })
                .collect(),
        ),
        InferenceResult::VariadicParams(params) => TypeMapperValue::params_value(params),
        InferenceResult::VariadicBase(base) => TypeMapperValue::MultiBase(
            TypeMapperValue::type_value(base)
                .raw_type()
                .unwrap_or(LuaType::Unknown),
        ),
    }
}

fn mapper_value_to_inference_result(value: TypeMapperValue) -> InferenceResult {
    match value {
        TypeMapperValue::None => InferenceResult::Type(LuaType::Unknown),
        TypeMapperValue::Type(ty) => InferenceResult::Type(ty),
        TypeMapperValue::Params(params) => InferenceResult::VariadicParams(params),
        TypeMapperValue::MultiTypes(types) => InferenceResult::MultiTypes(types),
        TypeMapperValue::MultiBase(base) => InferenceResult::VariadicBase(base),
    }
}

fn apply_inferred_constraint(
    db: &DbIndex,
    tpl: &GenericTpl,
    value: TypeMapperValue,
    mapper: &TypeMapper,
) -> TypeMapperValue {
    let TypeMapperValue::Type(inferred_type) = &value else {
        return value;
    };

    let Some(constraint) = tpl.get_constraint() else {
        return value;
    };

    let instantiated_constraint = instantiate_type_generic(db, constraint, mapper);
    if inferred_satisfies_constraint(db, inferred_type, &instantiated_constraint) {
        return value;
    }

    value
}

fn inferred_satisfies_constraint(db: &DbIndex, inferred: &LuaType, constraint: &LuaType) -> bool {
    if inferred == constraint || check_type_compact(db, inferred, constraint).is_ok() {
        return true;
    }

    match constraint {
        LuaType::Union(union) => {
            return union
                .into_vec()
                .iter()
                .any(|member| inferred_satisfies_constraint(db, inferred, member));
        }
        LuaType::MultiLineUnion(union) => {
            return union
                .get_unions()
                .iter()
                .any(|(member, _)| inferred_satisfies_constraint(db, inferred, member));
        }
        _ => {}
    }

    let regular = regularize_tpl_candidate_type(db, inferred.clone());
    regular != *inferred && inferred_satisfies_constraint(db, &regular, constraint)
}
