use std::sync::Arc;

use crate::{
    DbIndex, LuaType, LuaUnionType,
    semantic::type_check::{
        error_chain::{
            ChainMessage, ErrorChain, OverflowKind, not_assignable_message, push_message,
        },
        fast_eq_check, normalize_type,
        structured::relate_array_to_array,
    },
};

use super::{
    callable::relate_callable,
    intersection::relate_intersection,
    is_circular_tpl_constraint,
    simple::relate_simple,
    structured::{relate_structured, relate_to_declared_target_members, relate_to_object_target},
    union::relate_union,
};

pub(crate) type RelationResult = Result<(), RelationFailure>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelationFailure {
    Unrelated,
    Indeterminate(OverflowKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelationOutcome {
    Related,
    Unrelated,
    Indeterminate(OverflowKind),
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub(crate) struct IntersectionState(u32);

impl IntersectionState {
    pub(crate) const NONE: Self = Self(0);
    pub(crate) const SOURCE: Self = Self(1 << 0);
    pub(crate) const TARGET: Self = Self(1 << 1);

    pub(crate) fn contains(self, state: Self) -> bool {
        self.0 & state.0 != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvidenceMode {
    Silent,
    Explain,
}

fn relation_type_eq(source: &LuaType, target: &LuaType) -> bool {
    if let LuaType::FloatConst(left) = source {
        return matches!(target, LuaType::FloatConst(right) if left.to_bits() == right.to_bits());
    }
    if let LuaType::Object(left) = source {
        return matches!(target, LuaType::Object(right) if Arc::ptr_eq(left, right));
    }
    source == target
}

struct ActiveRelation<'active> {
    source: &'active LuaType,
    target: &'active LuaType,
    intersection_state: IntersectionState,
    parent: Option<&'active ActiveRelation<'active>>,
}

pub(crate) struct RelationSession<'db> {
    db: &'db DbIndex,
    evidence: EvidenceMode,
    relation_budget: u32,
    recursion_depth: u16,
    error_chain: Option<ErrorChain>,
}

pub(crate) struct Relater<'session, 'active, 'db> {
    session: &'session mut RelationSession<'db>,
    active_relation: Option<&'active ActiveRelation<'active>>,
}

impl<'db> RelationSession<'db> {
    fn new(db: &'db DbIndex, evidence: EvidenceMode) -> Self {
        Self {
            db,
            evidence,
            relation_budget: 20_000,
            recursion_depth: 0,
            error_chain: None,
        }
    }

    fn relate(
        &mut self,
        source: &LuaType,
        target: &LuaType,
        intersection_state: IntersectionState,
    ) -> RelationResult {
        let mut relater = Relater {
            session: self,
            active_relation: None,
        };
        relater.relate(source, target, intersection_state)
    }

    pub(crate) fn probe(db: &'db DbIndex, source: &LuaType, target: &LuaType) -> RelationOutcome {
        let mut session = Self::new(db, EvidenceMode::Silent);
        match session.relate(source, target, IntersectionState::NONE) {
            Ok(()) => RelationOutcome::Related,
            Err(RelationFailure::Unrelated) => RelationOutcome::Unrelated,
            Err(RelationFailure::Indeterminate(kind)) => RelationOutcome::Indeterminate(kind),
        }
    }

    pub(crate) fn explain(
        db: &'db DbIndex,
        source: &LuaType,
        target: &LuaType,
    ) -> super::AssignabilityResult {
        let mut session = Self::new(db, EvidenceMode::Explain);
        match session.relate(source, target, IntersectionState::NONE) {
            Ok(()) => super::AssignabilityResult::Assignable,
            Err(RelationFailure::Unrelated) => {
                super::AssignabilityResult::NotAssignable(session.error_chain)
            }
            Err(RelationFailure::Indeterminate(kind)) => {
                super::AssignabilityResult::Indeterminate(kind)
            }
        }
    }
}

impl<'session, 'active, 'db> Relater<'session, 'active, 'db> {
    pub(super) fn db(&self) -> &'db DbIndex {
        self.session.db
    }

    pub(super) fn is_explain(&self) -> bool {
        matches!(self.session.evidence, EvidenceMode::Explain)
    }

    pub(super) fn remaining_relation_budget(&self) -> usize {
        self.session.relation_budget as usize
    }

    pub(super) fn consume_relation_budget(&mut self) -> RelationResult {
        if self.session.relation_budget == 0 {
            return Err(RelationFailure::Indeterminate(OverflowKind::Budget));
        }
        self.session.relation_budget -= 1;
        Ok(())
    }

    pub(crate) fn relate(
        &mut self,
        source: &LuaType,
        target: &LuaType,
        intersection_state: IntersectionState,
    ) -> RelationResult {
        self.relate_with::<false>(source, target, intersection_state, false)
    }

    /// `FIELD` 控制字段是否走快速路径
    #[inline(always)]
    fn relate_with<const FIELD: bool>(
        &mut self,
        source: &LuaType,
        target: &LuaType,
        intersection_state: IntersectionState,
        guarded: bool,
    ) -> RelationResult {
        if guarded {
            self.relate_guarded::<FIELD>(source, target, intersection_state)
        } else {
            self.relate_unguarded::<FIELD>(source, target, intersection_state)
        }
    }

    fn relate_unguarded<const FIELD: bool>(
        &mut self,
        source: &LuaType,
        target: &LuaType,
        intersection_state: IntersectionState,
    ) -> RelationResult {
        if relate_semantic_accept(source, target) {
            return Ok(());
        }

        // 一些高频结构需要在此提前处理以提高性能
        // source_requires_scope 表示是否创建作用域
        // allow_structured_fast_path 表示是否允许结构化类型的快速路径
        let (source_requires_scope, allow_structured_fast_path) = match source {
            LuaType::Array(source_array) => {
                if let LuaType::Array(target_array) = target {
                    return self.run_fast_path(|relater| {
                        relate_array_to_array(
                            relater,
                            source_array,
                            target_array,
                            intersection_state,
                        )
                    });
                }
                (false, FIELD)
            }
            LuaType::TableConst(_) | LuaType::Object(_) => {
                if let LuaType::Object(target_object) = target {
                    return self.run_fast_path(|relater| {
                        relate_to_object_target(
                            relater,
                            source,
                            target,
                            target_object,
                            intersection_state,
                        )
                    });
                }
                (false, true)
            }
            LuaType::Ref(source_id) | LuaType::Def(source_id) => (
                true,
                FIELD
                    && self
                        .session
                        .db
                        .get_type_index()
                        .get_type_decl(source_id)
                        .is_some_and(|decl| decl.is_class()),
            ),
            LuaType::Generic(source_generic) => (
                true,
                FIELD
                    && self
                        .session
                        .db
                        .get_type_index()
                        .get_type_decl(source_generic.get_base_type_id_ref())
                        .is_some_and(|decl| decl.is_class()),
            ),
            LuaType::Tuple(_) | LuaType::TableGeneric(_) | LuaType::Intersection(_) => {
                (false, FIELD)
            }
            LuaType::Signature(_)
            | LuaType::Instance(_)
            | LuaType::Call(_)
            | LuaType::Conditional(_)
            | LuaType::Mapped(_)
            | LuaType::MultiLineUnion(_)
            | LuaType::TypeGuard(_)
            | LuaType::ModuleRef(_) => (true, false),
            LuaType::TplRef(tpl) => (tpl.get_constraint().is_some(), false),
            LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => {
                // 游戏开发可能会为巨型配置表创建 ID 集合, 在此提前匹配
                let target_origin = match target {
                    LuaType::Union(_) => Some(target),
                    LuaType::Ref(target_id) => self
                        .session
                        .db
                        .get_type_index()
                        .get_type_decl(target_id)
                        .and_then(|target_decl| target_decl.get_alias_ref()),
                    _ => None,
                };
                let exact_member = match target_origin {
                    Some(LuaType::MultiLineUnion(union)) => union
                        .get_unions()
                        .iter()
                        .any(|(candidate, _)| fast_eq_check(source, candidate)),
                    Some(LuaType::Union(union)) => match union.as_ref() {
                        LuaUnionType::Basic(_) => false,
                        LuaUnionType::Nullable(candidate) => fast_eq_check(source, candidate),
                        LuaUnionType::Multi(candidates) => candidates
                            .iter()
                            .any(|candidate| fast_eq_check(source, candidate)),
                    },
                    _ => false,
                };
                if exact_member {
                    return Ok(());
                }
                (false, false)
            }
            _ => (false, false),
        };

        // 一些高频结构需要在此提前处理以提高性能
        if allow_structured_fast_path {
            if let LuaType::Object(target_object) = target {
                return self.run_fast_path(|relater| {
                    relate_to_object_target(
                        relater,
                        source,
                        target,
                        target_object,
                        intersection_state,
                    )
                });
            }

            if matches!(
                source,
                LuaType::TableConst(_)
                    | LuaType::Object(_)
                    | LuaType::Tuple(_)
                    | LuaType::Array(_)
                    | LuaType::TableGeneric(_)
                    | LuaType::Intersection(_)
            ) {
                let target_decl = match target {
                    LuaType::Ref(target_id) | LuaType::Def(target_id) => {
                        self.session.db.get_type_index().get_type_decl(target_id)
                    }
                    LuaType::Generic(target_generic) => self
                        .session
                        .db
                        .get_type_index()
                        .get_type_decl(target_generic.get_base_type_id_ref()),
                    _ => None,
                };
                if target_decl.is_some_and(|decl| !decl.is_alias() && !decl.is_enum()) {
                    return self.run_fast_path(|relater| {
                        relate_to_declared_target_members(
                            relater,
                            source,
                            target,
                            intersection_state,
                        )
                    });
                }
            }
        }

        if let Some(result) = relate_simple::<true>(self, source, target, intersection_state) {
            return result;
        }

        if self.session.recursion_depth >= 100 {
            return Err(RelationFailure::Indeterminate(OverflowKind::Recursion));
        }

        let target_requires_scope = match target {
            LuaType::Ref(_)
            | LuaType::Def(_)
            | LuaType::Generic(_)
            | LuaType::Signature(_)
            | LuaType::Instance(_)
            | LuaType::Call(_)
            | LuaType::Conditional(_)
            | LuaType::Mapped(_)
            | LuaType::MultiLineUnion(_)
            | LuaType::TypeGuard(_)
            | LuaType::ModuleRef(_) => true,
            LuaType::TplRef(tpl) => tpl.get_constraint().is_some(),
            _ => false,
        };

        if source_requires_scope || target_requires_scope {
            // 重复进入活动关系链中的同一关系时, 当前递归边直接判定为成功
            let mut active = self.active_relation;
            while let Some(relation) = active {
                if relation.intersection_state == intersection_state
                    && relation_type_eq(relation.source, source)
                    && relation_type_eq(relation.target, target)
                {
                    return Ok(());
                }
                active = relation.parent;
            }
            self.consume_relation_budget()?;
            self.with_relation_scope(source, target, intersection_state, |relater| {
                relater.relate_with::<FIELD>(source, target, intersection_state, true)
            })
        } else {
            self.run_fast_path(|relater| {
                relater.relate_with::<FIELD>(source, target, intersection_state, true)
            })
        }
    }

    fn relate_guarded<const FIELD: bool>(
        &mut self,
        source: &LuaType,
        target: &LuaType,
        intersection_state: IntersectionState,
    ) -> RelationResult {
        if let Some(normalized) = normalize_type(self.session.db, source)
            && normalized != *source
        {
            if matches!(source, LuaType::Ref(_)) && normalized == *target {
                return Ok(());
            }
            return self.relate(&normalized, target, intersection_state);
        }
        if let Some(normalized) = normalize_type(self.session.db, target)
            && normalized != *target
        {
            if matches!(target, LuaType::Ref(_)) && *source == normalized {
                return Ok(());
            }
            return self.relate(source, &normalized, intersection_state);
        }

        if matches!(target, LuaType::ModuleRef(_)) {
            return Ok(());
        }

        // 复合类型需要提前拆分
        if let Some(result) = relate_union(self, source, target, intersection_state) {
            return result;
        }
        if let Some(result) = relate_intersection(self, source, target, intersection_state) {
            return result;
        }

        // 同 id 的 constraint 是循环约束, 不能降级成预注册的无约束占位符.
        if let LuaType::TplRef(target_tpl) = target
            && let Some(constraint) = target_tpl.get_constraint()
        {
            if is_circular_tpl_constraint(target_tpl) {
                return Err(RelationFailure::Indeterminate(OverflowKind::Recursion));
            }
            return self.relate(source, constraint, intersection_state);
        }
        if let LuaType::TplRef(source_tpl) = source
            && let Some(constraint) = source_tpl.get_constraint()
        {
            if is_circular_tpl_constraint(source_tpl) {
                return Err(RelationFailure::Indeterminate(OverflowKind::Recursion));
            }
            return self.relate(constraint, target, intersection_state);
        }

        if matches!(source, LuaType::Unknown) {
            return Ok(());
        }

        if matches!(source, LuaType::Never) {
            if matches!(target, LuaType::Never) {
                return Ok(());
            }
            return self.fail(|db| not_assignable_message(db, source, target));
        }
        if matches!(target, LuaType::Never) {
            return self.fail(|db| not_assignable_message(db, source, target));
        }

        // 声明类型和带元表的常量表可能同时具有结构约束和调用能力, 必须先保留结构关系的确定结论.
        let source_relation = match source {
            LuaType::Function | LuaType::DocFunction(_) | LuaType::Signature(_) => {
                relate_callable(self, source, target, intersection_state)
            }
            LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => {
                relate_structured(self, source, target, intersection_state)
                    .or_else(|| relate_callable(self, source, target, intersection_state))
            }
            LuaType::TableConst(table)
                if self.session.db.get_metatable_index().get(table).is_some() =>
            {
                relate_structured(self, source, target, intersection_state)
                    .or_else(|| relate_callable(self, source, target, intersection_state))
            }
            _ => relate_structured(self, source, target, intersection_state),
        };
        if let Some(result) = source_relation
            .or_else(|| relate_simple::<false>(self, source, target, intersection_state))
        {
            return result;
        }

        self.fail(|db| not_assignable_message(db, source, target))
    }

    /// 快速路径, 用在确定无复杂行为的类型检查
    fn run_fast_path(
        &mut self,
        body: impl FnOnce(&mut Relater<'_, '_, 'db>) -> RelationResult,
    ) -> RelationResult {
        if self.session.recursion_depth >= 100 {
            return Err(RelationFailure::Indeterminate(OverflowKind::Recursion));
        }
        self.session.recursion_depth += 1;
        let result = body(self);
        self.session.recursion_depth -= 1;
        result
    }

    // 字段成员需要走快速类型检查通道以提高性能
    pub(super) fn relate_field_types(
        &mut self,
        source_member: &LuaType,
        target_member: &LuaType,
        intersection_state: IntersectionState,
    ) -> RelationResult {
        self.relate_with::<true>(source_member, target_member, intersection_state, false)
    }

    /// 创建完整的活动关系作用域, 用于处理复杂类型.
    fn with_relation_scope(
        &mut self,
        source: &LuaType,
        target: &LuaType,
        intersection_state: IntersectionState,
        body: impl FnOnce(&mut Relater<'_, '_, 'db>) -> RelationResult,
    ) -> RelationResult {
        self.session.recursion_depth += 1;
        let active_relation = ActiveRelation {
            source,
            target,
            intersection_state,
            parent: self.active_relation,
        };
        let mut relater = Relater {
            session: &mut *self.session,
            active_relation: Some(&active_relation),
        };
        let result = body(&mut relater);
        self.session.recursion_depth -= 1;
        result
    }

    /// 关系探测, 用于选出候选结果
    pub(super) fn probe_relation(
        &mut self,
        source: &LuaType,
        target: &LuaType,
        intersection_state: IntersectionState,
    ) -> RelationOutcome {
        let evidence = self.session.evidence;
        self.session.evidence = EvidenceMode::Silent;
        let result = self.relate(source, target, intersection_state);
        self.session.evidence = evidence;
        match result {
            Ok(()) => RelationOutcome::Related,
            Err(RelationFailure::Unrelated) => RelationOutcome::Unrelated,
            Err(RelationFailure::Indeterminate(kind)) => RelationOutcome::Indeterminate(kind),
        }
    }

    /// 在叶子失败时调用, 用于构建具体的错误
    pub(crate) fn fail(
        &mut self,
        message: impl FnOnce(&DbIndex) -> ChainMessage,
    ) -> RelationResult {
        if self.is_explain() {
            let message = message(self.session.db);
            self.push_error_message(message);
        }
        Err(RelationFailure::Unrelated)
    }

    /// 在中间层调用, 如果发生错误时将构建错误链
    pub(crate) fn on_unrelated(
        &mut self,
        result: RelationResult,
        message: impl FnOnce(&DbIndex) -> ChainMessage,
    ) -> RelationResult {
        if let Err(RelationFailure::Unrelated) = &result
            && self.is_explain()
        {
            let message = message(self.session.db);
            self.push_error_message(message);
        }
        result
    }

    fn push_error_message(&mut self, message: ChainMessage) {
        let head = self.session.error_chain.take();
        self.session.error_chain = Some(push_message(head, message));
    }

    pub(super) fn error_chain_snapshot(&self) -> Option<ErrorChain> {
        self.session.error_chain.clone()
    }

    pub(super) fn restore_error_chain(&mut self, snapshot: Option<ErrorChain>) {
        self.session.error_chain = snapshot;
    }
}

// 这里应该只写确定是高频或必须的类型匹配检查, 不要使用完整的 fast_eq_check.
fn relate_semantic_accept(source: &LuaType, target: &LuaType) -> bool {
    matches!(source, LuaType::Any | LuaType::SelfInfer)
        || matches!(target, LuaType::Any | LuaType::Unknown | LuaType::SelfInfer)
        || matches!(source, LuaType::TplRef(tpl) if tpl.get_constraint().is_none())
        || matches!(target, LuaType::TplRef(tpl) if tpl.get_constraint().is_none())
}
