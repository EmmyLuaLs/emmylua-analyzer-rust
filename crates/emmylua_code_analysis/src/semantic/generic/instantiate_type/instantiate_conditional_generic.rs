use hashbrown::{HashMap, HashSet};

use crate::{
    DbIndex, GenericTplId, LuaConditionalType, LuaTypeDeclId, LuaTypeNode, TypeOps,
    check_type_compact,
    db_index::{LuaObjectType, LuaTupleType, LuaType},
    semantic::{member::find_members_with_key, type_check::check_type_compact_with_level},
};

use super::{get_default_constructor, instantiate_type_generic_inner};
use crate::semantic::generic::type_substitutor::{
    GenericInstantiateContext, GenericInstantiateFrame, TplBinding,
};

#[derive(Debug, Clone, Copy)]
enum InferVariance {
    Covariant,
    Contravariant,
}

impl InferVariance {
    fn flip(self) -> Self {
        match self {
            InferVariance::Covariant => InferVariance::Contravariant,
            InferVariance::Contravariant => InferVariance::Covariant,
        }
    }
}

#[derive(Debug, Default)]
struct InferCandidateSet {
    covariant: Option<LuaType>,
    contravariant: Option<LuaType>,
}

pub(super) fn instantiate_conditional(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    conditional: &LuaConditionalType,
) -> LuaType {
    let Some(frame) = frame.enter() else {
        return instantiate_conditional_residual(context, frame, conditional, None, None);
    };

    if let Some(distributed) = instantiate_distributed_conditional(context, frame, conditional) {
        return distributed;
    }

    instantiate_conditional_once(context, frame, conditional)
}

fn instantiate_conditional_once(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    conditional: &LuaConditionalType,
) -> LuaType {
    let left_type = instantiate_conditional_operand(
        context,
        frame,
        conditional.get_checked_type(),
        true,
        conditional.has_new,
    );
    let right_type = instantiate_conditional_operand(
        context,
        frame,
        conditional.get_extends_type(),
        false,
        conditional.has_new,
    );

    // right_has_infer 表示右侧 pattern 里还带 infer.
    let right_has_infer = contains_conditional_infer(&right_type);
    if right_has_infer {
        // infer pattern 直接对已实例化后的实际类型做结构匹配.
        let mut infer_assignments = HashMap::new();
        return if collect_infer_assignments(
            context.db,
            &left_type,
            &right_type,
            &mut infer_assignments,
            InferVariance::Covariant,
        ) {
            instantiate_true_branch(
                context,
                frame,
                conditional,
                finalize_infer_assignments(infer_assignments),
            )
        } else if is_deferred_conditional_operand(&left_type)
            || right_type.any_type(|inner| match inner {
                LuaType::TplRef(tpl) | LuaType::ConstTplRef(tpl) => {
                    !tpl.get_tpl_id().is_conditional_infer()
                }
                LuaType::StrTplRef(_)
                | LuaType::SelfInfer
                | LuaType::Conditional(_)
                | LuaType::Mapped(_)
                | LuaType::Call(_) => true,
                _ => false,
            })
        {
            instantiate_conditional_residual(
                context,
                frame,
                conditional,
                Some(left_type),
                Some(right_type),
            )
        } else {
            instantiate_type_generic_inner(context, frame, conditional.get_false_type())
        };
    }

    match check_conditional_extends(context.db, &left_type, &right_type) {
        ConditionalCheck::True => {
            instantiate_true_branch(context, frame, conditional, HashMap::new())
        }
        ConditionalCheck::False => {
            instantiate_type_generic_inner(context, frame, conditional.get_false_type())
        }
        ConditionalCheck::Both => {
            if is_deferred_conditional_operand(&left_type)
                || is_deferred_conditional_operand(&right_type)
            {
                return instantiate_conditional_residual(
                    context,
                    frame,
                    conditional,
                    Some(left_type),
                    Some(right_type),
                );
            }
            let true_type = instantiate_true_branch(context, frame, conditional, HashMap::new());
            let false_type =
                instantiate_type_generic_inner(context, frame, conditional.get_false_type());
            TypeOps::Union.apply(context.db, &true_type, &false_type)
        }
    }
}

fn instantiate_conditional_residual(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    conditional: &LuaConditionalType,
    checked_type: Option<LuaType>,
    extends_type: Option<LuaType>,
) -> LuaType {
    let instantiate_branch = |branch: &LuaType| {
        if branch.any_type(|ty| match ty {
            LuaType::TplRef(tpl) | LuaType::ConstTplRef(tpl) => {
                context.substitutor.get(tpl.get_tpl_id()).is_some()
            }
            LuaType::SelfInfer => context.substitutor.get_self_type().is_some(),
            _ => false,
        }) {
            instantiate_type_generic_inner(context, frame, branch)
        } else {
            branch.clone()
        }
    };

    LuaType::Conditional(
        LuaConditionalType::new(
            checked_type.unwrap_or_else(|| {
                instantiate_type_generic_inner(context, frame, conditional.get_checked_type())
            }),
            extends_type.unwrap_or_else(|| {
                instantiate_type_generic_inner(context, frame, conditional.get_extends_type())
            }),
            instantiate_branch(conditional.get_true_type()),
            instantiate_branch(conditional.get_false_type()),
            conditional.get_infer_params().to_vec(),
            conditional.has_new,
        )
        .into(),
    )
}

/// 处理分布式条件类型, 与`TS`中的分布式条件类型处理方式相同, 只有裸模版参数才会被分布式.
fn instantiate_distributed_conditional(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    conditional: &LuaConditionalType,
) -> Option<LuaType> {
    let tpl_id = match conditional.get_checked_type() {
        LuaType::TplRef(tpl) | LuaType::ConstTplRef(tpl)
            if tpl.get_tpl_id().is_type() || tpl.get_tpl_id().is_func() =>
        {
            tpl.get_tpl_id()
        }
        _ => return None,
    };
    let raw_checked_type = context.substitutor.get_raw_type(tpl_id)?;

    if raw_checked_type.is_never() {
        return Some(LuaType::Never);
    }

    let members = match &raw_checked_type {
        LuaType::Union(union) => union.into_vec(),
        LuaType::MultiLineUnion(multi) => multi
            .get_unions()
            .iter()
            .map(|(member, _)| member.clone())
            .collect(),
        _ => return None,
    };
    let mut result = LuaType::Never;
    for member in members {
        let mut member_substitutor = context.substitutor.clone();
        member_substitutor.bind(tpl_id, TplBinding::ReplaceConstType(member));
        let member_context = context.with_substitutor(&member_substitutor);
        let member_result = instantiate_conditional_once(&member_context, frame, conditional);
        result = TypeOps::Union.apply(context.db, &result, &member_result);
    }

    Some(result)
}

fn instantiate_true_branch(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    conditional: &LuaConditionalType,
    infer_assignments: HashMap<GenericTplId, LuaType>,
) -> LuaType {
    if infer_assignments.is_empty() {
        return instantiate_type_generic_inner(context, frame, conditional.get_true_type());
    }

    let mut true_substitutor = context.substitutor.clone();
    for (tpl_id, ty) in infer_assignments {
        true_substitutor.bind(tpl_id, TplBinding::ConditionalInferType(ty));
    }
    let true_context = context.with_substitutor(&true_substitutor);
    instantiate_type_generic_inner(&true_context, frame, conditional.get_true_type())
}

fn contains_conditional_infer(ty: &LuaType) -> bool {
    ty.any_type(|inner| {
        matches!(
            inner,
            LuaType::TplRef(tpl) | LuaType::ConstTplRef(tpl)
                if tpl.get_tpl_id().is_conditional_infer()
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConditionalCheck {
    True,
    False,
    Both,
}

fn check_conditional_extends(db: &DbIndex, source: &LuaType, target: &LuaType) -> ConditionalCheck {
    if source.is_any() {
        return ConditionalCheck::Both;
    }

    if target.is_any() {
        return ConditionalCheck::True;
    }

    if matches!(target, LuaType::Unknown) {
        return ConditionalCheck::True;
    }

    if source.is_unknown() {
        return ConditionalCheck::False;
    }

    if source.is_never() {
        return ConditionalCheck::True;
    }

    if literal_extends_base_type(source, target) {
        return ConditionalCheck::True;
    }

    if let LuaType::Union(union) = source {
        let mut result = ConditionalCheck::False;
        for member in union.into_vec() {
            result =
                merge_conditional_check(result, check_conditional_extends(db, &member, target));
            if result == ConditionalCheck::Both {
                break;
            }
        }
        return result;
    }

    if let LuaType::Union(union) = target {
        for member in union.into_vec() {
            if matches!(
                check_conditional_extends(db, source, &member),
                ConditionalCheck::True | ConditionalCheck::Both
            ) {
                return ConditionalCheck::True;
            }
        }
        return ConditionalCheck::False;
    }

    if is_deferred_conditional_operand(source) || is_deferred_conditional_operand(target) {
        return ConditionalCheck::Both;
    }

    if check_type_compact_with_level(
        db,
        source,
        target,
        crate::semantic::type_check::TypeCheckCheckLevel::GenericConditional,
    )
    .is_ok()
    {
        ConditionalCheck::True
    } else {
        ConditionalCheck::False
    }
}

fn merge_conditional_check(left: ConditionalCheck, right: ConditionalCheck) -> ConditionalCheck {
    match (left, right) {
        (ConditionalCheck::True, ConditionalCheck::True) => ConditionalCheck::True,
        (ConditionalCheck::False, ConditionalCheck::False) => ConditionalCheck::False,
        _ => ConditionalCheck::Both,
    }
}

fn literal_extends_base_type(source: &LuaType, target: &LuaType) -> bool {
    matches!(
        (source, target),
        (
            LuaType::StringConst(_) | LuaType::DocStringConst(_),
            LuaType::String
        ) | (
            LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_),
            LuaType::Integer
        ) | (
            LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) | LuaType::FloatConst(_),
            LuaType::Number,
        ) | (
            LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_),
            LuaType::Boolean
        )
    )
}

fn collect_infer_assignments(
    db: &DbIndex,
    source: &LuaType,
    pattern: &LuaType,
    assignments: &mut HashMap<GenericTplId, InferCandidateSet>,
    variance: InferVariance,
) -> bool {
    match pattern {
        LuaType::TplRef(tpl) | LuaType::ConstTplRef(tpl)
            if tpl.get_tpl_id().is_conditional_infer() =>
        {
            insert_infer_assignment(db, assignments, tpl.get_tpl_id(), source, variance)
        }
        LuaType::Generic(pattern_generic) => {
            if let LuaType::Generic(source_generic) = source {
                if source_generic.get_base_type_id_ref() != pattern_generic.get_base_type_id_ref() {
                    return false;
                }

                let pattern_params = pattern_generic.get_params();
                let source_params = source_generic.get_params();
                if pattern_params.len() != source_params.len() {
                    return false;
                }
                for (pattern_param, source_param) in pattern_params.iter().zip(source_params) {
                    if !collect_infer_assignments(
                        db,
                        source_param,
                        pattern_param,
                        assignments,
                        variance,
                    ) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
        LuaType::DocFunction(pattern_func) => match source {
            LuaType::DocFunction(source_func) => {
                let pattern_params = pattern_func.get_params();
                let source_params = source_func.get_params();
                let has_variadic = pattern_params.last().is_some_and(|(name, ty)| {
                    name == "..." || ty.as_ref().is_some_and(|ty| ty.is_variadic())
                });
                let normal_param_len = if has_variadic {
                    pattern_params.len().saturating_sub(1)
                } else {
                    pattern_params.len()
                };

                if !has_variadic && source_params.len() > normal_param_len {
                    return false;
                }

                for (i, (_, pattern_param)) in
                    pattern_params.iter().take(normal_param_len).enumerate()
                {
                    if let Some((_, source_param)) = source_params.get(i) {
                        match (source_param, pattern_param) {
                            (Some(source_ty), Some(pattern_ty)) => {
                                if !collect_infer_assignments(
                                    db,
                                    source_ty,
                                    pattern_ty,
                                    assignments,
                                    variance.flip(),
                                ) {
                                    return false;
                                }
                            }
                            (Some(_), None) => continue,
                            (None, Some(pattern_ty)) => {
                                if contains_conditional_infer(pattern_ty) {
                                    return false;
                                }
                            }
                            (None, None) => continue,
                        }
                    } else if let Some(pattern_ty) = pattern_param
                        && (contains_conditional_infer(pattern_ty)
                            || !is_optional_param_type(db, pattern_ty))
                    {
                        return false;
                    }
                }

                if has_variadic
                    && let Some((_, variadic_param)) = pattern_params.last()
                    && let Some(pattern_ty) = variadic_param
                    && contains_conditional_infer(pattern_ty)
                {
                    let rest = if normal_param_len < source_params.len() {
                        &source_params[normal_param_len..]
                    } else {
                        &[]
                    };
                    let mut rest_types = Vec::with_capacity(rest.len());
                    for (_, source_param) in rest {
                        rest_types.push(source_param.as_ref().unwrap_or(&LuaType::Any).clone());
                    }
                    // 真 variadic 保持 base type, 命名尾参数则包装成 tuple, 这样后续展开语义才一致.
                    let ty = match rest_types.len() {
                        0 => LuaType::Never,
                        1 => {
                            if source_func.is_variadic() {
                                rest_types[0].clone()
                            } else {
                                LuaType::Tuple(
                                    LuaTupleType::new(
                                        rest_types,
                                        crate::LuaTupleStatus::InferResolve,
                                    )
                                    .into(),
                                )
                            }
                        }
                        _ => LuaType::Tuple(
                            LuaTupleType::new(rest_types, crate::LuaTupleStatus::InferResolve)
                                .into(),
                        ),
                    };

                    if !collect_infer_assignments(db, &ty, pattern_ty, assignments, variance.flip())
                    {
                        return false;
                    }
                }

                let pattern_ret = pattern_func.get_ret();
                if contains_conditional_infer(pattern_ret) {
                    collect_infer_assignments(
                        db,
                        source_func.get_ret(),
                        pattern_ret,
                        assignments,
                        variance,
                    )
                } else {
                    true
                }
            }
            LuaType::Signature(id) => {
                if let Some(signature) = db.get_signature_index().get(id) {
                    let source_func = signature.to_doc_func_type();
                    collect_infer_assignments(
                        db,
                        &LuaType::DocFunction(source_func),
                        pattern,
                        assignments,
                        variance,
                    )
                } else {
                    false
                }
            }
            LuaType::Ref(type_decl_id) => {
                if let Some(type_decl) = db.get_type_index().get_type_decl(type_decl_id)
                    && type_decl.is_alias()
                    && let Some(origin) = type_decl.get_alias_origin(db, None)
                {
                    return collect_infer_assignments(db, &origin, pattern, assignments, variance);
                }
                false
            }
            _ => false,
        },
        LuaType::Array(array) => {
            if let LuaType::Array(source_array) = source {
                collect_infer_assignments(
                    db,
                    source_array.get_base(),
                    array.get_base(),
                    assignments,
                    variance,
                )
            } else {
                false
            }
        }
        LuaType::Object(pattern_object) => match source {
            LuaType::Object(source_object) => collect_infer_from_object_to_object(
                db,
                source_object,
                pattern_object,
                assignments,
                variance,
            ),
            LuaType::Ref(type_id) | LuaType::Def(type_id) => collect_infer_from_class_to_object(
                db,
                type_id,
                pattern_object,
                assignments,
                variance,
            ),
            LuaType::TableConst(table_id) => collect_infer_from_table_to_object(
                db,
                table_id,
                pattern_object,
                assignments,
                variance,
            ),
            _ => false,
        },
        _ => {
            if contains_conditional_infer(pattern) {
                false
            } else {
                strict_type_match(db, source, pattern)
            }
        }
    }
}

fn collect_infer_from_object_to_object(
    db: &DbIndex,
    source_object: &LuaObjectType,
    pattern_object: &LuaObjectType,
    assignments: &mut HashMap<GenericTplId, InferCandidateSet>,
    variance: InferVariance,
) -> bool {
    let source_fields = source_object.get_fields();
    let pattern_fields = pattern_object.get_fields();

    for (key, pattern_field_ty) in pattern_fields {
        if let Some(source_field_ty) = source_fields.get(key) {
            if !collect_infer_assignments(
                db,
                source_field_ty,
                pattern_field_ty,
                assignments,
                variance,
            ) {
                return false;
            }
        } else if contains_conditional_infer(pattern_field_ty) {
            return false;
        }
    }

    true
}

fn collect_infer_from_class_to_object(
    db: &DbIndex,
    type_id: &LuaTypeDeclId,
    pattern_object: &LuaObjectType,
    assignments: &mut HashMap<GenericTplId, InferCandidateSet>,
    variance: InferVariance,
) -> bool {
    let pattern_fields = pattern_object.get_fields();
    let source_type = LuaType::Ref(type_id.clone());

    for (key, pattern_field_ty) in pattern_fields {
        if let Some(member_infos) = find_members_with_key(db, &source_type, key.clone(), false) {
            if let Some(member_info) = member_infos.first() {
                if !collect_infer_assignments(
                    db,
                    &member_info.typ,
                    pattern_field_ty,
                    assignments,
                    variance,
                ) {
                    return false;
                }
            } else if contains_conditional_infer(pattern_field_ty) {
                return false;
            }
        } else if contains_conditional_infer(pattern_field_ty) {
            return false;
        }
    }

    true
}

fn collect_infer_from_table_to_object(
    db: &DbIndex,
    table_id: &crate::InFiled<rowan::TextRange>,
    pattern_object: &LuaObjectType,
    assignments: &mut HashMap<GenericTplId, InferCandidateSet>,
    variance: InferVariance,
) -> bool {
    let pattern_fields = pattern_object.get_fields();
    let source_type = LuaType::TableConst(table_id.clone());

    for (key, pattern_field_ty) in pattern_fields {
        if let Some(member_infos) = find_members_with_key(db, &source_type, key.clone(), false) {
            if let Some(member_info) = member_infos.first() {
                if !collect_infer_assignments(
                    db,
                    &member_info.typ,
                    pattern_field_ty,
                    assignments,
                    variance,
                ) {
                    return false;
                }
            } else if contains_conditional_infer(pattern_field_ty) {
                return false;
            }
        } else if contains_conditional_infer(pattern_field_ty) {
            return false;
        }
    }

    true
}

fn strict_type_match(db: &DbIndex, source: &LuaType, pattern: &LuaType) -> bool {
    if source == pattern {
        return true;
    }

    check_type_compact(db, pattern, source).is_ok()
}

fn is_optional_param_type(db: &DbIndex, ty: &LuaType) -> bool {
    let mut stack = vec![ty.clone()];
    let mut visited = HashSet::new();

    while let Some(current) = stack.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }

        match current {
            LuaType::Any | LuaType::Unknown | LuaType::Nil | LuaType::Variadic(_) => {
                return true;
            }
            LuaType::Ref(decl_id) => {
                if let Some(decl) = db.get_type_index().get_type_decl(&decl_id)
                    && decl.is_alias()
                    && let Some(alias_origin) = decl.get_alias_ref()
                {
                    stack.push(alias_origin.clone());
                }
            }
            LuaType::Union(union) => {
                for t in union.into_vec() {
                    stack.push(t);
                }
            }
            LuaType::MultiLineUnion(multi) => {
                for (t, _) in multi.get_unions() {
                    stack.push(t.clone());
                }
            }
            _ => {}
        }
    }
    false
}

fn insert_infer_assignment(
    db: &DbIndex,
    assignments: &mut HashMap<GenericTplId, InferCandidateSet>,
    infer_id: GenericTplId,
    ty: &LuaType,
    variance: InferVariance,
) -> bool {
    let candidates = assignments.entry(infer_id).or_default();
    match variance {
        InferVariance::Covariant => {
            candidates.covariant = Some(match &candidates.covariant {
                Some(existing) => TypeOps::Union.apply(db, existing, ty),
                None => ty.clone(),
            });
        }
        InferVariance::Contravariant => {
            candidates.contravariant = Some(match &candidates.contravariant {
                Some(existing) => TypeOps::Intersect.apply(db, existing, ty),
                None => ty.clone(),
            });
        }
    }
    true
}

fn finalize_infer_assignments(
    assignments: HashMap<GenericTplId, InferCandidateSet>,
) -> HashMap<GenericTplId, LuaType> {
    assignments
        .into_iter()
        .filter_map(|(tpl_id, candidates)| {
            candidates
                .covariant
                .or(candidates.contravariant)
                .map(|raw_candidate| (tpl_id, raw_candidate))
        })
        .collect()
}

fn instantiate_conditional_operand(
    context: &GenericInstantiateContext,
    frame: GenericInstantiateFrame,
    operand: &LuaType,
    checked: bool,
    has_new: bool,
) -> LuaType {
    let mut result = instantiate_type_generic_inner(context, frame, operand);
    if let LuaType::TplRef(tpl_ref) | LuaType::ConstTplRef(tpl_ref) = operand {
        let tpl_id = tpl_ref.get_tpl_id();
        if let Some(raw) = context.substitutor.get_raw_type(tpl_id) {
            result = raw.clone();
        } else if checked && result.is_never() {
            result = LuaType::Never;
        }
    }

    if has_new
        && let LuaType::Ref(id) | LuaType::Def(id) = &result
        && let Some(decl) = context.db.get_type_index().get_type_decl(id)
        && decl.is_class()
        && let Some(constructor) = get_default_constructor(context.db, id)
    {
        return constructor;
    }

    result
}

fn is_deferred_conditional_operand(ty: &LuaType) -> bool {
    ty.any_type(|inner| {
        matches!(
            inner,
            LuaType::TplRef(_)
                | LuaType::ConstTplRef(_)
                | LuaType::StrTplRef(_)
                | LuaType::SelfInfer
                | LuaType::Conditional(_)
                | LuaType::Mapped(_)
                | LuaType::Call(_)
        )
    })
}
