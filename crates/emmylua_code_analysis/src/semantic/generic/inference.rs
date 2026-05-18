use std::{collections::HashMap as StdHashMap, ops::Deref, sync::Arc};

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr};
use hashbrown::{HashMap, HashSet};
use itertools::Itertools;
use rowan::NodeOrToken;
use smol_str::SmolStr;

use crate::{
    DbIndex, GenericTpl, GenericTplId, InferFailReason, InferGuard, InferGuardRef, LuaFunctionType,
    LuaGenericType, LuaInferCache, LuaMemberInfo, LuaMemberKey, LuaMemberOwner, LuaSemanticDeclId,
    LuaTupleType, LuaType, LuaTypeDeclId, LuaTypeNode, LuaUnionType, SemanticDeclLevel, TypeOps,
    TypeSubstitutor, VariadicType, check_type_compact, infer_node_semantic_decl,
    instantiate_type_generic,
    semantic::{
        generic::{
            is_primitive_or_literal_type, regularize_tpl_candidate_type, widen_tpl_candidate_type,
        },
        member::{find_index_operations, get_member_map},
    },
};

use super::type_substitutor::TplBinding;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::semantic::generic) enum InferenceCandidateView {
    FreshExpression,
    RegularType,
    ConstPreserving,
    Ordinary,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::semantic::generic) struct InferenceCandidate {
    ty: LuaType,
    view: InferenceCandidateView,
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

    fn candidate_type(&self) -> LuaType {
        self.ty.clone()
    }

    fn is_const_preserving(&self) -> bool {
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
    fn flip(self) -> Self {
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

#[derive(Debug, Clone)]
struct InferenceRecord {
    candidate: InferenceCandidate,
    priority: InferencePriority,
}

#[derive(Debug, Clone, Default)]
pub(in crate::semantic::generic) struct InferenceInfo {
    // 协变候选, 例如参数位置或返回值位置的正向推断.
    covariant: Vec<InferenceRecord>,
    // 逆变候选, 例如函数参数约束的反向传播.
    contravariant: Vec<InferenceRecord>,
    // 多返回值或变参展开时收集的候选.
    multi: Vec<InferenceRecord>,
    // 已经固定下来的结果, 例如显式泛型或变参参数表.
    fixed: Option<InferenceResult>,
    // 最近一次写入的推断结果, 便于在后续阶段判断是否需要重算.
    inferred: Option<InferenceResult>,
    // 当前信息是否始终处于顶层位置.
    top_level: bool,
    // 该模板收到的最高优先级.
    priority: Option<InferencePriority>,
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

    fn inferred_variadic_len(&self, tpl_id: GenericTplId) -> Option<usize> {
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

    pub fn bridge_to_substitutor<'b>(
        &mut self,
        substitutor: &mut TypeSubstitutor,
        generic_tpls: impl IntoIterator<Item = &'b Arc<GenericTpl>>,
        return_type: &LuaType,
    ) {
        let generic_tpls = generic_tpls.into_iter().collect::<Vec<_>>();
        let tpl_ids = generic_tpls.iter().map(|tpl| tpl.get_tpl_id()).collect();
        substitutor.prepare_inference_slots(tpl_ids);
        self.bridge_to_substitutor_inner(substitutor, generic_tpls, return_type);
    }

    pub fn bridge_resolved_to_substitutor<'b>(
        &mut self,
        substitutor: &mut TypeSubstitutor,
        generic_tpls: impl IntoIterator<Item = &'b Arc<GenericTpl>>,
        return_type: &LuaType,
    ) {
        self.bridge_to_substitutor_inner(substitutor, generic_tpls, return_type);
    }

    fn bridge_to_substitutor_inner<'b>(
        &mut self,
        substitutor: &mut TypeSubstitutor,
        generic_tpls: impl IntoIterator<Item = &'b Arc<GenericTpl>>,
        return_type: &LuaType,
    ) {
        for tpl in generic_tpls {
            let tpl_id = tpl.get_tpl_id();
            let return_top_level = is_tpl_at_top_level(self.db, return_type, tpl_id);
            let result = self
                .infos
                .get(&tpl_id)
                .and_then(|info| resolve_info(self.db, tpl, info, return_top_level, substitutor));
            if let Some(result) = result {
                write_result_to_substitutor(substitutor, tpl_id, result);
            }
        }
    }

    fn can_bind(&self, tpl_id: GenericTplId) -> bool {
        !tpl_id.is_conditional_infer()
    }
}

pub(in crate::semantic::generic) fn infer_types(
    context: &mut InferenceContext,
    source: &LuaType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
) -> Result<(), InferFailReason> {
    infer_types_inner(
        context,
        source,
        target,
        original_target,
        variance,
        priority,
        None,
        &InferGuard::new(),
    )
}

pub(in crate::semantic::generic) fn infer_types_from_expr(
    context: &mut InferenceContext,
    source: &LuaType,
    target: &LuaType,
    original_target: &LuaType,
    arg_expr: &LuaExpr,
) -> Result<(), InferFailReason> {
    infer_types_inner(
        context,
        source,
        target,
        original_target,
        InferenceVariance::Covariant,
        InferencePriority::Normal,
        Some(arg_expr),
        &InferGuard::new(),
    )
}

fn infer_types_inner(
    context: &mut InferenceContext,
    source: &LuaType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    let target = escape_alias(context.db, target);
    if !source.contains_tpl_node() {
        return Ok(());
    }

    let top_level = target == *original_target;
    match source {
        LuaType::TplRef(tpl) => {
            if tpl.get_tpl_id().is_func() {
                let candidate = match variance {
                    InferenceVariance::Covariant => {
                        InferenceCandidate::from_expr_arg(arg_expr, target.clone())
                    }
                    InferenceVariance::Contravariant => InferenceCandidate::ordinary(target),
                };
                context.insert_type(tpl.get_tpl_id(), candidate, variance, top_level, priority);
            }
        }
        LuaType::ConstTplRef(tpl) => {
            if tpl.get_tpl_id().is_func() {
                context.insert_type(
                    tpl.get_tpl_id(),
                    InferenceCandidate::const_preserving(target),
                    variance,
                    top_level,
                    priority,
                );
            }
        }
        LuaType::StrTplRef(str_tpl) => {
            if let LuaType::StringConst(s) = target {
                let type_name = SmolStr::new(format!(
                    "{}{}{}",
                    str_tpl.get_prefix(),
                    s,
                    str_tpl.get_suffix()
                ));
                context.insert_type(
                    str_tpl.get_tpl_id(),
                    InferenceCandidate::regular_type(get_str_tpl_infer_type(&type_name)),
                    variance,
                    top_level,
                    priority,
                );
            }
        }
        LuaType::Array(array_type) => {
            array_infer_types(
                context,
                array_type.get_base(),
                &target,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::TableGeneric(params) => {
            table_generic_infer_types(
                context,
                params,
                &target,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Generic(generic) => {
            generic_infer_types(
                context,
                generic,
                &target,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Union(union) => {
            let members = union.into_vec();
            let mut error_count = 0;
            let mut last_error = InferFailReason::None;
            for member in &members {
                match infer_types_inner(
                    context,
                    member,
                    &target,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    &infer_guard.fork(),
                ) {
                    Ok(_) => {}
                    Err(err) => {
                        error_count += 1;
                        last_error = err;
                    }
                }
            }
            if error_count == members.len() {
                return Err(last_error);
            }
        }
        LuaType::DocFunction(func) => {
            function_infer_types(
                context,
                func,
                &target,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Tuple(tuple) => {
            tuple_infer_types(
                context,
                tuple,
                &target,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Object(object) => {
            object_infer_types(
                context,
                object,
                &target,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        _ => {}
    }

    Ok(())
}

fn array_infer_types(
    context: &mut InferenceContext,
    base: &LuaType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    match target {
        LuaType::Array(target_array) => infer_types_inner(
            context,
            base,
            target_array.get_base(),
            original_target,
            variance,
            priority,
            arg_expr,
            infer_guard,
        )?,
        LuaType::Tuple(target_tuple) => {
            let target_base = target_tuple.cast_down_array_base(context.db);
            infer_types_inner(
                context,
                base,
                &target_base,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Object(target_object) => {
            let target_base = target_object
                .cast_down_array_base(context.db)
                .ok_or(InferFailReason::None)?;
            infer_types_inner(
                context,
                base,
                &target_base,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        _ => {}
    }

    Ok(())
}

fn table_generic_infer_types(
    context: &mut InferenceContext,
    table_params: &[LuaType],
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    if table_params.len() != 2 {
        return Err(InferFailReason::None);
    }

    match target {
        LuaType::TableGeneric(target_params) => {
            let min_len = table_params.len().min(target_params.len());
            for i in 0..min_len {
                infer_types_inner(
                    context,
                    &table_params[i],
                    &target_params[i],
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    infer_guard,
                )?;
            }
        }
        LuaType::Array(target_array) => {
            infer_types_inner(
                context,
                &table_params[0],
                &LuaType::Integer,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
            infer_types_inner(
                context,
                &table_params[1],
                target_array.get_base(),
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Tuple(target_tuple) => {
            let keys = (0..target_tuple.get_types().len())
                .map(|i| LuaType::IntegerConst((i as i64) + 1))
                .collect::<Vec<_>>();
            let key_type = LuaType::Union(LuaUnionType::from_vec(keys).into());
            let target_base = target_tuple.cast_down_array_base(context.db);
            infer_types_inner(
                context,
                &table_params[0],
                &key_type,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
            infer_types_inner(
                context,
                &table_params[1],
                &target_base,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::TableConst(inst) => {
            table_generic_member_owner_infer_types(
                context,
                table_params,
                LuaMemberOwner::Element(inst.clone()),
                &[],
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Ref(type_id) | LuaType::Def(type_id) => {
            table_generic_member_owner_infer_types(
                context,
                table_params,
                LuaMemberOwner::Type(type_id.clone()),
                &[],
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Generic(generic) => {
            table_generic_member_owner_infer_types(
                context,
                table_params,
                LuaMemberOwner::Type(generic.get_base_type_id()),
                generic.get_params(),
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Object(obj) => {
            let mut keys =
                Vec::with_capacity(obj.get_fields().len() + obj.get_index_access().len());
            let mut values =
                Vec::with_capacity(obj.get_fields().len() + obj.get_index_access().len());
            for (key, value) in obj.get_fields() {
                match key {
                    LuaMemberKey::Integer(i) => keys.push(LuaType::IntegerConst(*i)),
                    LuaMemberKey::Name(name) => {
                        keys.push(LuaType::StringConst(name.clone().into()))
                    }
                    _ => {}
                }
                values.push(value.clone());
            }
            for (key, value) in obj.get_index_access() {
                keys.push(key.clone());
                values.push(value.clone());
            }
            let key_type = LuaType::Union(LuaUnionType::from_vec(keys).into());
            let value_type = LuaType::Union(LuaUnionType::from_vec(values).into());
            infer_types_inner(
                context,
                &table_params[0],
                &key_type,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
            infer_types_inner(
                context,
                &table_params[1],
                &value_type,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Global | LuaType::Any | LuaType::Table | LuaType::Userdata => {
            infer_types_inner(
                context,
                &table_params[0],
                &LuaType::Any,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
            infer_types_inner(
                context,
                &table_params[1],
                &LuaType::Any,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        _ => {}
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn table_generic_member_owner_infer_types(
    context: &mut InferenceContext,
    table_params: &[LuaType],
    owner: LuaMemberOwner,
    target_params: &[LuaType],
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    if table_params.len() != 2 {
        return Err(InferFailReason::None);
    }

    let owner_type = match &owner {
        LuaMemberOwner::Element(inst) => LuaType::TableConst(inst.clone()),
        LuaMemberOwner::Type(type_id) => match target_params.len() {
            0 => LuaType::Ref(type_id.clone()),
            _ => LuaType::Generic(Arc::new(LuaGenericType::new(
                type_id.clone(),
                target_params.to_vec(),
            ))),
        },
        _ => return Err(InferFailReason::None),
    };

    let members = get_member_map(context.db, &owner_type).ok_or(InferFailReason::None)?;
    if is_pairs_call(context).unwrap_or(false)
        && try_handle_pairs_metamethod(
            context,
            table_params,
            &members,
            original_target,
            variance,
            priority,
            arg_expr,
            infer_guard,
        )
        .is_ok()
    {
        return Ok(());
    }

    let target_key_type = table_params[0].clone();
    let mut keys = Vec::with_capacity(members.len());
    let mut values = Vec::with_capacity(members.len());
    for (key, members) in members {
        let key_type = match key {
            LuaMemberKey::Integer(i) => LuaType::IntegerConst(i),
            LuaMemberKey::Name(name) => LuaType::StringConst(name.clone().into()),
            LuaMemberKey::ExprType(ty) => ty,
            _ => continue,
        };

        if !target_key_type.is_generic()
            && check_type_compact(context.db, &target_key_type, &key_type).is_err()
        {
            continue;
        }

        keys.push(key_type);
        values.push(member_infos_type(members));
    }

    if keys.is_empty() {
        find_index_operations(context.db, &owner_type)
            .ok_or(InferFailReason::None)?
            .iter()
            .for_each(|member| {
                if target_key_type.is_generic() {
                    return;
                }
                let LuaMemberKey::ExprType(key_type) = &member.key else {
                    return;
                };
                if check_type_compact(context.db, &target_key_type, key_type).is_ok() {
                    keys.push(key_type.clone());
                    values.push(member.typ.clone());
                }
            });
    }

    let key_type = match &keys[..] {
        [] => return Err(InferFailReason::None),
        [first] => first.clone(),
        _ => LuaType::Union(LuaUnionType::from_vec(keys).into()),
    };
    let value_type = match &values[..] {
        [first] => first.clone(),
        _ => LuaType::Union(LuaUnionType::from_vec(values).into()),
    };

    infer_types_inner(
        context,
        &table_params[0],
        &key_type,
        original_target,
        variance,
        priority,
        arg_expr,
        infer_guard,
    )?;
    infer_types_inner(
        context,
        &table_params[1],
        &value_type,
        original_target,
        variance,
        priority,
        arg_expr,
        infer_guard,
    )
}

fn generic_infer_types(
    context: &mut InferenceContext,
    source_generic: &LuaGenericType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    match target {
        LuaType::Generic(target_generic) => {
            let source_base = source_generic.get_base_type_id_ref();
            let target_base = target_generic.get_base_type_id_ref();
            if source_base == target_base {
                for (start, (source_param, target_param)) in source_generic
                    .get_params()
                    .iter()
                    .zip(target_generic.get_params())
                    .enumerate()
                {
                    match source_param {
                        LuaType::Variadic(variadic) => {
                            variadic_infer_types(
                                context,
                                variadic,
                                &target_generic.get_params()[start..],
                                original_target,
                                variance,
                                priority,
                            )?;
                            break;
                        }
                        _ => infer_types_inner(
                            context,
                            source_param,
                            target_param,
                            original_target,
                            variance,
                            priority,
                            arg_expr,
                            infer_guard,
                        )?,
                    }
                }
                return Ok(());
            }

            let target_decl = context
                .db
                .get_type_index()
                .get_type_decl(target_base)
                .ok_or(InferFailReason::None)?;
            if target_decl.is_alias() {
                let substitutor = TypeSubstitutor::from_alias(
                    context.db,
                    target_generic.get_params().clone(),
                    target_base.clone(),
                );
                if let Some(origin_type) =
                    target_decl.get_alias_origin(context.db, Some(&substitutor))
                {
                    return generic_infer_types(
                        context,
                        source_generic,
                        &origin_type,
                        original_target,
                        variance,
                        priority,
                        arg_expr,
                        infer_guard,
                    );
                }
            } else if let Some(super_types) =
                context.db.get_type_index().get_super_types(target_base)
            {
                for mut super_type in super_types {
                    if super_type.contains_tpl_node() {
                        let substitutor =
                            TypeSubstitutor::from_type_array(target_generic.get_params().clone());
                        super_type =
                            instantiate_type_generic(context.db, &super_type, &substitutor);
                    }
                    generic_infer_types(
                        context,
                        source_generic,
                        &super_type,
                        original_target,
                        variance,
                        priority,
                        arg_expr,
                        &infer_guard.fork(),
                    )?;
                }
            }
        }
        LuaType::Ref(type_id) | LuaType::Def(type_id) => {
            infer_guard.check(type_id)?;
            let type_decl = context
                .db
                .get_type_index()
                .get_type_decl(type_id)
                .ok_or(InferFailReason::None)?;
            if let Some(origin_type) = type_decl.get_alias_origin(context.db, None) {
                return generic_infer_types(
                    context,
                    source_generic,
                    &origin_type,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    infer_guard,
                );
            }

            for super_type in context
                .db
                .get_type_index()
                .get_super_types(type_id)
                .unwrap_or_default()
            {
                generic_infer_types(
                    context,
                    source_generic,
                    &super_type,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    &infer_guard.fork(),
                )?;
            }
        }
        LuaType::Union(union) => {
            for member in union.into_vec() {
                generic_infer_types(
                    context,
                    source_generic,
                    &member,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    &infer_guard.fork(),
                )?;
            }
        }
        _ => {
            let substitutor = TypeSubstitutor::new();
            let generic_ty = LuaType::Generic(source_generic.clone().into());
            let ty = instantiate_type_generic(context.db, &generic_ty, &substitutor);
            if LuaType::from(source_generic.clone()) != ty {
                infer_types_inner(
                    context,
                    &ty,
                    target,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    infer_guard,
                )?;
            }
        }
    }

    Ok(())
}

fn function_infer_types(
    context: &mut InferenceContext,
    source_func: &LuaFunctionType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    match target {
        LuaType::DocFunction(target_func) => {
            function_doc_infer_types(
                context,
                source_func,
                target_func,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        LuaType::Signature(signature_id) => {
            let signature = context
                .db
                .get_signature_index()
                .get(signature_id)
                .ok_or(InferFailReason::None)?;
            if !signature.is_resolve_return() {
                return check_lambda_inference(context, *signature_id);
            }

            let fake_func = signature.to_doc_func_type();
            function_doc_infer_types(
                context,
                source_func,
                &fake_func,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        _ => {}
    }

    Ok(())
}

fn function_doc_infer_types(
    context: &mut InferenceContext,
    source_func: &LuaFunctionType,
    target_func: &LuaFunctionType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    let mut source_params = source_func.get_params().to_vec();
    if source_func.is_colon_define() {
        source_params.insert(0, ("self".to_string(), Some(LuaType::Any)));
    }

    let mut target_params = target_func.get_params().to_vec();
    if target_func.is_colon_define() {
        target_params.insert(0, ("self".to_string(), Some(LuaType::Any)));
    }

    param_list_infer_types(
        context,
        &source_params,
        &target_params,
        original_target,
        variance.flip(),
        priority,
        arg_expr,
        infer_guard,
    )?;
    return_type_infer_types(
        context,
        source_func.get_ret(),
        target_func.get_ret(),
        original_target,
        variance,
        priority,
        arg_expr,
        infer_guard,
    )
}

#[allow(clippy::too_many_arguments)]
fn param_list_infer_types(
    context: &mut InferenceContext,
    sources: &[(String, Option<LuaType>)],
    targets: &[(String, Option<LuaType>)],
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    let mut target_offset = 0;
    for i in 0..sources.len() {
        let source = match sources.get(i) {
            Some((_, ty)) => ty.clone().unwrap_or(LuaType::Any),
            None => break,
        };

        match &source {
            LuaType::Variadic(inner) => {
                let i = i + target_offset;
                if i >= targets.len() {
                    if let VariadicType::Base(LuaType::TplRef(tpl_ref)) = inner.deref() {
                        context.insert_type(
                            tpl_ref.get_tpl_id(),
                            InferenceCandidate::ordinary(LuaType::Nil),
                            variance,
                            false,
                            priority,
                        );
                    }
                    break;
                }

                if let VariadicType::Base(LuaType::TplRef(tpl_ref)) = inner.deref()
                    && let Some(len) = context.inferred_variadic_len(tpl_ref.get_tpl_id())
                {
                    target_offset += len - 1;
                    continue;
                }

                let mut target_rest_params = &targets[i..];
                if i + 1 < sources.len() {
                    let source_rest_len = sources.len() - i - 1;
                    if source_rest_len >= target_rest_params.len() {
                        continue;
                    }
                    let target_rest_len = target_rest_params.len() - source_rest_len;
                    target_rest_params = &target_rest_params[..target_rest_len];
                    if target_rest_len > 1 {
                        target_offset += target_rest_len - 1;
                    }
                }

                function_varargs_infer_types(context, inner, target_rest_params)?;
            }
            _ => {
                let target = match targets.get(i + target_offset) {
                    Some((_, ty)) => ty.clone().unwrap_or(LuaType::Any),
                    None => break,
                };
                infer_types_inner(
                    context,
                    &source,
                    &target,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    infer_guard,
                )?;
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(in crate::semantic::generic) fn return_type_infer_types(
    context: &mut InferenceContext,
    source: &LuaType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    match (source, target) {
        (LuaType::Variadic(source_variadic), LuaType::Variadic(target_variadic)) => {
            match target_variadic.deref() {
                VariadicType::Base(target_base) => match source_variadic.deref() {
                    VariadicType::Base(source_base) => {
                        if let LuaType::TplRef(tpl_ref) = source_base {
                            context.insert_type(
                                tpl_ref.get_tpl_id(),
                                InferenceCandidate::ordinary(target_base.clone()),
                                variance,
                                false,
                                priority,
                            );
                        }
                    }
                    VariadicType::Multi(source_multi) => {
                        for ret_type in source_multi {
                            match ret_type {
                                LuaType::Variadic(inner) => {
                                    if let VariadicType::Base(base) = inner.deref()
                                        && let LuaType::TplRef(tpl_ref) = base
                                    {
                                        context.insert_type(
                                            tpl_ref.get_tpl_id(),
                                            InferenceCandidate::ordinary(target_base.clone()),
                                            variance,
                                            false,
                                            priority,
                                        );
                                    }
                                    break;
                                }
                                LuaType::TplRef(tpl_ref) => {
                                    context.insert_type(
                                        tpl_ref.get_tpl_id(),
                                        InferenceCandidate::ordinary(target_base.clone()),
                                        variance,
                                        false,
                                        priority,
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                },
                VariadicType::Multi(target_types) => {
                    variadic_infer_types(
                        context,
                        source_variadic,
                        target_types,
                        original_target,
                        variance,
                        priority,
                    )?;
                }
            }
        }
        (LuaType::Variadic(variadic), _) => {
            variadic_infer_types(
                context,
                variadic,
                std::slice::from_ref(target),
                original_target,
                variance,
                priority,
            )?;
        }
        (_, LuaType::Variadic(variadic)) => {
            multi_param_infer_multi_return(
                context,
                std::slice::from_ref(source),
                variadic,
                original_target,
                variance,
                priority,
            )?;
        }
        _ => infer_types_inner(
            context,
            source,
            target,
            original_target,
            variance,
            priority,
            arg_expr,
            infer_guard,
        )?,
    }

    Ok(())
}

fn function_varargs_infer_types(
    context: &mut InferenceContext,
    variadic: &VariadicType,
    target_rest_params: &[(String, Option<LuaType>)],
) -> Result<(), InferFailReason> {
    if let VariadicType::Base(LuaType::TplRef(tpl_ref)) = variadic {
        context.add_variadic_params(
            tpl_ref.get_tpl_id(),
            target_rest_params
                .iter()
                .map(|(name, ty)| (name.clone(), ty.clone()))
                .collect(),
        );
    }

    Ok(())
}

pub(in crate::semantic::generic) fn variadic_infer_types(
    context: &mut InferenceContext,
    source: &VariadicType,
    target_rest_types: &[LuaType],
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
) -> Result<(), InferFailReason> {
    match source {
        VariadicType::Base(base) => match base {
            LuaType::TplRef(tpl_ref) => {
                let tpl_id = tpl_ref.get_tpl_id();
                match target_rest_types.len() {
                    0 => context.insert_type(
                        tpl_id,
                        InferenceCandidate::ordinary(LuaType::Nil),
                        variance,
                        false,
                        priority,
                    ),
                    1 => match &target_rest_types[0] {
                        LuaType::Variadic(variadic) => match variadic.deref() {
                            VariadicType::Multi(types) => match types.len() {
                                0 => context.insert_type(
                                    tpl_id,
                                    InferenceCandidate::ordinary(LuaType::Nil),
                                    variance,
                                    false,
                                    priority,
                                ),
                                1 => context.insert_type(
                                    tpl_id,
                                    InferenceCandidate::ordinary(types[0].clone()),
                                    variance,
                                    false,
                                    priority,
                                ),
                                _ => context.insert_multi_types(
                                    tpl_id,
                                    types.to_vec(),
                                    InferenceCandidateView::Ordinary,
                                    false,
                                    priority,
                                ),
                            },
                            VariadicType::Base(base) => {
                                context.add_variadic_base(tpl_id, base.clone());
                            }
                        },
                        target => context.insert_type(
                            tpl_id,
                            InferenceCandidate::ordinary(target.clone()),
                            variance,
                            false,
                            priority,
                        ),
                    },
                    _ => context.insert_multi_types(
                        tpl_id,
                        target_rest_types.to_vec(),
                        InferenceCandidateView::Ordinary,
                        false,
                        priority,
                    ),
                }
            }
            LuaType::ConstTplRef(tpl_ref) => {
                let tpl_id = tpl_ref.get_tpl_id();
                match target_rest_types.len() {
                    0 => context.insert_type(
                        tpl_id,
                        InferenceCandidate::const_preserving(LuaType::Nil),
                        variance,
                        false,
                        priority,
                    ),
                    1 => context.insert_type(
                        tpl_id,
                        InferenceCandidate::const_preserving(target_rest_types[0].clone()),
                        variance,
                        false,
                        priority,
                    ),
                    _ => context.insert_multi_types(
                        tpl_id,
                        target_rest_types.to_vec(),
                        InferenceCandidateView::ConstPreserving,
                        false,
                        priority,
                    ),
                }
            }
            _ => {}
        },
        VariadicType::Multi(multi) => {
            for (i, ret_type) in multi.iter().enumerate() {
                match ret_type {
                    LuaType::Variadic(inner) => {
                        if i < target_rest_types.len() {
                            variadic_infer_types(
                                context,
                                inner,
                                &target_rest_types[i..],
                                original_target,
                                variance,
                                priority,
                            )?;
                        }
                        break;
                    }
                    LuaType::TplRef(tpl_ref) => {
                        let Some(target) = target_rest_types.get(i) else {
                            break;
                        };
                        context.insert_type(
                            tpl_ref.get_tpl_id(),
                            InferenceCandidate::ordinary(target.clone()),
                            variance,
                            target == original_target,
                            priority,
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

pub(in crate::semantic::generic) fn multi_param_infer_multi_return(
    context: &mut InferenceContext,
    source_params: &[LuaType],
    multi_return: &VariadicType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
) -> Result<(), InferFailReason> {
    match multi_return {
        VariadicType::Base(base) => {
            let mut target_types = Vec::with_capacity(source_params.len());
            for param in source_params {
                if param.is_variadic() {
                    target_types.push(LuaType::Variadic(multi_return.clone().into()));
                    break;
                } else {
                    target_types.push(base.clone());
                }
            }
            infer_type_list(
                context,
                source_params,
                &target_types,
                original_target,
                variance,
                priority,
            )?;
        }
        VariadicType::Multi(_) => {
            let mut target_types = Vec::with_capacity(source_params.len());
            for (i, param) in source_params.iter().enumerate() {
                let Some(return_type) = multi_return.get_type(i) else {
                    break;
                };
                if param.is_variadic() {
                    target_types.push(LuaType::Variadic(
                        multi_return.get_new_variadic_from(i).into(),
                    ));
                    break;
                } else {
                    target_types.push(return_type.clone());
                }
            }
            infer_type_list(
                context,
                source_params,
                &target_types,
                original_target,
                variance,
                priority,
            )?;
        }
    }

    Ok(())
}

pub(in crate::semantic::generic) fn infer_type_list(
    context: &mut InferenceContext,
    source_types: &[LuaType],
    target_types: &[LuaType],
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
) -> Result<(), InferFailReason> {
    for (start, (source, target)) in source_types.iter().zip(target_types).enumerate() {
        match (source, target) {
            (LuaType::Variadic(variadic), _) => {
                variadic_infer_types(
                    context,
                    variadic,
                    &target_types[start..],
                    original_target,
                    variance,
                    priority,
                )?;
                break;
            }
            (_, LuaType::Variadic(variadic)) => {
                multi_param_infer_multi_return(
                    context,
                    &source_types[start..],
                    variadic,
                    original_target,
                    variance,
                    priority,
                )?;
                break;
            }
            _ => infer_types(context, source, target, original_target, variance, priority)?,
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn tuple_infer_types(
    context: &mut InferenceContext,
    source_tuple: &LuaTupleType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    match target {
        LuaType::Tuple(target_tuple) => {
            for (i, source_type) in source_tuple.get_types().iter().enumerate() {
                if let LuaType::Variadic(inner) = source_type {
                    variadic_infer_types(
                        context,
                        inner,
                        &target_tuple.get_types()[i..],
                        original_target,
                        variance,
                        priority,
                    )?;
                    break;
                }
                let Some(target_type) = target_tuple.get_types().get(i) else {
                    break;
                };
                infer_types_inner(
                    context,
                    source_type,
                    target_type,
                    original_target,
                    variance,
                    priority,
                    arg_expr,
                    infer_guard,
                )?;
            }
        }
        LuaType::Array(target_array) => {
            let Some(last_type) = source_tuple.get_types().last() else {
                return Err(InferFailReason::None);
            };
            if let LuaType::Variadic(inner) = last_type
                && let VariadicType::Base(LuaType::TplRef(tpl_ref)) = inner.deref()
            {
                context.add_variadic_base(tpl_ref.get_tpl_id(), target_array.get_base().clone());
            }
        }
        _ => {}
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn object_infer_types(
    context: &mut InferenceContext,
    source_obj: &crate::LuaObjectType,
    target: &LuaType,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    match target {
        LuaType::Object(target_obj) => {
            for (key, value) in source_obj
                .get_fields()
                .iter()
                .sorted_by_key(|(key, _)| *key)
            {
                if let Some(target_value) = target_obj.get_fields().get(key) {
                    infer_types_inner(
                        context,
                        value,
                        target_value,
                        original_target,
                        variance,
                        priority,
                        arg_expr,
                        infer_guard,
                    )?;
                }
            }
            for (source_key, value) in source_obj.get_index_access() {
                let target_access = target_obj
                    .get_index_access()
                    .iter()
                    .find(|(target_key, _)| {
                        check_type_compact(context.db, source_key, target_key).is_ok()
                    });
                if let Some((target_key, target_value)) = target_access {
                    infer_types_inner(
                        context,
                        source_key,
                        target_key,
                        original_target,
                        variance,
                        priority,
                        arg_expr,
                        infer_guard,
                    )?;
                    infer_types_inner(
                        context,
                        value,
                        target_value,
                        original_target,
                        variance,
                        priority,
                        arg_expr,
                        infer_guard,
                    )?;
                }
            }
        }
        LuaType::TableConst(inst) => {
            object_member_owner_infer_types(
                context,
                source_obj,
                LuaMemberOwner::Element(inst.clone()),
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
        _ => {}
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn object_member_owner_infer_types(
    context: &mut InferenceContext,
    source_obj: &crate::LuaObjectType,
    owner: LuaMemberOwner,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    let owner_type = match &owner {
        LuaMemberOwner::Element(inst) => LuaType::TableConst(inst.clone()),
        LuaMemberOwner::Type(type_id) => LuaType::Ref(type_id.clone()),
        _ => return Err(InferFailReason::None),
    };

    let members = get_member_map(context.db, &owner_type).ok_or(InferFailReason::None)?;
    for (key, members) in members {
        let resolve_type = member_infos_type(members);
        if let Some(field_value) = source_obj.get_field(&key) {
            infer_types_inner(
                context,
                field_value,
                &resolve_type,
                original_target,
                variance,
                priority,
                arg_expr,
                infer_guard,
            )?;
        }
    }

    Ok(())
}

fn member_infos_type(members: Vec<LuaMemberInfo>) -> LuaType {
    match members.len() {
        0 => LuaType::Any,
        1 => members[0].typ.clone(),
        _ => LuaType::from_vec(members.into_iter().map(|member| member.typ).collect()),
    }
}

fn is_pairs_call(context: &mut InferenceContext) -> Option<bool> {
    let call_expr = context.call_expr.as_ref()?;
    let prefix_expr = call_expr.get_prefix_expr()?;
    let semantic_decl = match prefix_expr.syntax().clone().into() {
        NodeOrToken::Node(node) => infer_node_semantic_decl(
            context.db,
            context.cache,
            node,
            SemanticDeclLevel::default(),
        ),
        _ => None,
    }?;

    let LuaSemanticDeclId::LuaDecl(decl_id) = semantic_decl else {
        return None;
    };
    let decl = context.db.get_decl_index().get_decl(&decl_id)?;
    if !context.db.get_module_index().is_std(&decl.get_file_id()) {
        return None;
    }
    if decl.get_name() != "pairs" {
        return None;
    }

    Some(true)
}

#[allow(clippy::too_many_arguments)]
fn try_handle_pairs_metamethod(
    context: &mut InferenceContext,
    table_params: &[LuaType],
    members: &StdHashMap<LuaMemberKey, Vec<LuaMemberInfo>>,
    original_target: &LuaType,
    variance: InferenceVariance,
    priority: InferencePriority,
    arg_expr: Option<&LuaExpr>,
    infer_guard: &InferGuardRef,
) -> Result<(), InferFailReason> {
    let pairs_member = members
        .get(&LuaMemberKey::Name("__pairs".into()))
        .ok_or(InferFailReason::None)?
        .first()
        .ok_or(InferFailReason::None)?;

    let meta_return = match &pairs_member.typ {
        LuaType::Signature(signature_id) => context
            .db
            .get_signature_index()
            .get(signature_id)
            .map(|signature| signature.get_return_type()),
        LuaType::DocFunction(doc_func) => Some(doc_func.get_ret().clone()),
        _ => None,
    }
    .ok_or(InferFailReason::None)?;

    let final_return_type = match meta_return {
        LuaType::DocFunction(doc_func) => Some(doc_func.get_ret().clone()),
        LuaType::Signature(signature_id) => context
            .db
            .get_signature_index()
            .get(&signature_id)
            .map(|signature| signature.get_return_type()),
        _ => None,
    };

    if let Some(LuaType::Variadic(variadic)) = &final_return_type {
        let key_type = variadic.get_type(0).ok_or(InferFailReason::None)?;
        let value_type = variadic.get_type(1).ok_or(InferFailReason::None)?;
        infer_types_inner(
            context,
            &table_params[0],
            key_type,
            original_target,
            variance,
            priority,
            arg_expr,
            infer_guard,
        )?;
        infer_types_inner(
            context,
            &table_params[1],
            value_type,
            original_target,
            variance,
            priority,
            arg_expr,
            infer_guard,
        )?;
        return Ok(());
    }

    Err(InferFailReason::None)
}

fn check_lambda_inference(
    context: &mut InferenceContext,
    signature_id: crate::LuaSignatureId,
) -> Result<(), InferFailReason> {
    let call_expr = context.call_expr.as_ref().ok_or(InferFailReason::None)?;
    let call_arg_list = call_expr.get_args_list().ok_or(InferFailReason::None)?;
    for arg in call_arg_list.get_args() {
        if let Ok(LuaType::Signature(arg_signature_id)) =
            crate::semantic::infer_expr(context.db, context.cache, arg.clone())
            && arg_signature_id == signature_id
        {
            return Ok(());
        }
    }

    Err(InferFailReason::UnResolveSignatureReturn(signature_id))
}

fn resolve_info(
    db: &DbIndex,
    tpl: &GenericTpl,
    info: &InferenceInfo,
    return_top_level: bool,
    substitutor: &TypeSubstitutor,
) -> Option<InferenceResult> {
    if let Some(fixed) = &info.fixed {
        return Some(fixed.clone());
    }

    if !info.multi.is_empty() {
        let primitive_constraint = tpl.get_constraint().is_some_and(|constraint| {
            let constraint = instantiate_type_generic(db, constraint, substitutor);
            is_primitive_or_literal_type(&constraint)
        });
        let const_preserving = info
            .multi
            .iter()
            .any(|record| record.candidate.is_const_preserving());
        let preserve_root_literal_form =
            primitive_constraint || const_preserving || return_top_level;
        return Some(InferenceResult::MultiTypes(
            info.multi
                .iter()
                .map(|record| {
                    resolve_candidate_type(db, &record.candidate, preserve_root_literal_form)
                })
                .collect(),
        ));
    }

    if !info.covariant.is_empty() {
        return Some(InferenceResult::Type(resolve_covariant_candidates(
            db,
            tpl,
            info,
            return_top_level,
            substitutor,
        )));
    }

    if !info.contravariant.is_empty() {
        return Some(InferenceResult::Type(resolve_contravariant_candidates(
            db, info,
        )));
    }

    None
}

fn write_result_to_substitutor(
    substitutor: &mut TypeSubstitutor,
    tpl_id: GenericTplId,
    result: InferenceResult,
) {
    match result {
        InferenceResult::Type(ty) => substitutor.bind_type(tpl_id, ty),
        InferenceResult::MultiTypes(types) => {
            substitutor.bind(tpl_id, TplBinding::InferredMultiTypes(types));
        }
        InferenceResult::VariadicParams(params) => {
            substitutor.bind(tpl_id, TplBinding::VariadicParams(params));
        }
        InferenceResult::VariadicBase(base) => {
            substitutor.bind(tpl_id, TplBinding::VariadicBase(base));
        }
    }
}

fn resolve_covariant_candidates(
    db: &DbIndex,
    tpl: &GenericTpl,
    info: &InferenceInfo,
    return_top_level: bool,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let primitive_constraint = tpl
        .get_constraint()
        .map(|constraint| {
            let constraint = instantiate_type_generic(db, constraint, substitutor);
            is_primitive_or_literal_type(&constraint)
        })
        .unwrap_or(false);
    let const_preserving = info
        .covariant
        .iter()
        .any(|record| record.candidate.is_const_preserving());
    let preserve_root_literal_form =
        primitive_constraint || const_preserving || !info.top_level || return_top_level;

    combine_records(
        db,
        &info.covariant,
        info.priority.unwrap_or(InferencePriority::Normal),
        preserve_root_literal_form,
        TypeOps::Union,
    )
}

fn resolve_contravariant_candidates(db: &DbIndex, info: &InferenceInfo) -> LuaType {
    combine_records(
        db,
        &info.contravariant,
        info.priority.unwrap_or(InferencePriority::Normal),
        true,
        TypeOps::Intersect,
    )
}

fn combine_records(
    db: &DbIndex,
    records: &[InferenceRecord],
    max_priority: InferencePriority,
    preserve_root_literal_form: bool,
    op: TypeOps,
) -> LuaType {
    let mut selected = records
        .iter()
        .filter(|record| record.priority == max_priority)
        .map(|record| resolve_candidate_type(db, &record.candidate, preserve_root_literal_form));

    let Some(first) = selected.next() else {
        return LuaType::Unknown;
    };

    selected.fold(first, |acc, ty| op.apply(db, &acc, &ty))
}

fn resolve_candidate_type(
    db: &DbIndex,
    candidate: &InferenceCandidate,
    preserve_root_literal_form: bool,
) -> LuaType {
    if candidate.is_const_preserving() {
        // std.ConstTpl 需要保留结构字面量, 例如 tuple/object/table const.
        return candidate.candidate_type();
    }

    if preserve_root_literal_form {
        return regularize_tpl_candidate_type(db, candidate.candidate_type());
    }

    match candidate.view {
        InferenceCandidateView::FreshExpression | InferenceCandidateView::Ordinary => {
            widen_tpl_candidate_type(db, candidate.candidate_type())
        }
        _ => regularize_tpl_candidate_type(db, candidate.candidate_type()),
    }
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

fn get_str_tpl_infer_type(name: &str) -> LuaType {
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

fn escape_alias(db: &DbIndex, may_alias: &LuaType) -> LuaType {
    if let LuaType::Ref(type_id) = may_alias
        && let Some(type_decl) = db.get_type_index().get_type_decl(type_id)
        && type_decl.is_alias()
        && let Some(origin_type) = type_decl.get_alias_origin(db, None)
    {
        return origin_type.clone();
    }

    may_alias.clone()
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
