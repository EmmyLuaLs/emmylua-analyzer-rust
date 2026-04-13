use hashbrown::{HashMap, HashSet};

use crate::{
    DbIndex, GenericTplId, LuaAliasCallKind, LuaConditionalType, LuaTypeDeclId, LuaTypeNode,
    TypeOps, check_type_compact,
    db_index::{LuaObjectType, LuaTupleType, LuaType},
    semantic::{
        generic::type_substitutor::SubstitutorValue, member::find_members_with_key,
        type_check::check_type_compact_with_level,
    },
};

use super::{get_default_constructor, instantiate_type_generic, instantiate_type_generic_with_env};
use crate::semantic::generic::type_substitutor::{
    ConditionalCheckMode, GenericEvalEnv, TypeSubstitutor,
};

enum ConditionalResolution {
    ExactTrue {
        infer_assignments: HashMap<String, LuaType>,
    },
    ExactFalse,
    Constraint(LuaType),
    Deferred,
}

pub(super) fn instantiate_conditional(
    env: &GenericEvalEnv,
    conditional: &LuaConditionalType,
) -> LuaType {
    match resolve_conditional(env, conditional) {
        ConditionalResolution::ExactTrue { infer_assignments } => {
            let mut true_substitutor = env.substitutor.clone();
            if !infer_assignments.is_empty() {
                // infer 绑定只在 true 分支提交, 这样 false 和 constraint 路径不会污染外层 substitutor.
                let infer_names: HashSet<String> = conditional
                    .get_infer_params()
                    .iter()
                    .map(|param| param.name.to_string())
                    .collect();

                if !infer_names.is_empty() {
                    let tpl_id_map =
                        resolve_infer_tpl_ids(conditional, env.substitutor, &infer_names);
                    for (name, ty) in infer_assignments.iter() {
                        if let Some(tpl_id) = tpl_id_map.get(name.as_str()) {
                            true_substitutor.insert_type(*tpl_id, ty.clone(), true);
                        }
                    }
                }
            }

            instantiate_type_generic(env.db, conditional.get_true_type(), &true_substitutor)
        }
        ConditionalResolution::ExactFalse => {
            instantiate_type_generic(env.db, conditional.get_false_type(), env.substitutor)
        }
        ConditionalResolution::Constraint(result) => result,
        ConditionalResolution::Deferred => {
            // truly deferred 时只做局部实例化, 保留 conditional 结构等待后续求值.
            let new_condition =
                instantiate_type_generic(env.db, conditional.get_condition(), env.substitutor);
            let new_true =
                instantiate_type_generic(env.db, conditional.get_true_type(), env.substitutor);
            let new_false =
                instantiate_type_generic(env.db, conditional.get_false_type(), env.substitutor);

            LuaType::Conditional(
                LuaConditionalType::new(
                    new_condition,
                    new_true,
                    new_false,
                    conditional.get_infer_params().to_vec(),
                    conditional.has_new,
                )
                .into(),
            )
        }
    }
}

fn resolve_conditional(
    env: &GenericEvalEnv,
    conditional: &LuaConditionalType,
) -> ConditionalResolution {
    let LuaType::Call(alias_call) = conditional.get_condition() else {
        return ConditionalResolution::Deferred;
    };
    if alias_call.get_call_kind() != LuaAliasCallKind::Extends
        || alias_call.get_operands().len() != 2
    {
        return ConditionalResolution::Deferred;
    }

    // `T extends U and true_type or false_type`, T 为被检查的类型, U 为约束类型
    // left_operand 为 T, right_operand 为 U
    let left_operand = &alias_call.get_operands()[0];
    let right_operand = &alias_call.get_operands()[1];

    let instantiate_operand = |operand: &LuaType, mode: ConditionalCheckMode, checked: bool| {
        // conditional 求值会同时构造 permissive 和 rigid 两种视图.
        // permissive 用于回答 "是否必不成立", rigid 用于回答 "是否必成立".
        // checked operand 需要尽量看到 raw type, 否则像 T extends Foo 这类判断会被包装后的模板形态干扰.
        let scoped_env = env.with_conditional_check_mode(mode);
        let mut result = instantiate_type_generic_with_env(&scoped_env, operand);
        if checked && let LuaType::TplRef(tpl_ref) | LuaType::ConstTplRef(tpl_ref) = operand {
            let tpl_id = tpl_ref.get_tpl_id();
            if let Some(raw) = env.substitutor.get_conditional_raw_type(tpl_id) {
                result = raw.clone();
            } else if let Some(raw) = env.substitutor.get_raw_type(tpl_id) {
                result = raw.clone();
            }
        }
        if conditional.has_new
            && let LuaType::Ref(id) | LuaType::Def(id) = &result
            && let Some(decl) = env.db.get_type_index().get_type_decl(id)
            && decl.is_class()
            && let Some(constructor) = get_default_constructor(env.db, id)
        {
            return constructor;
        }

        result
    };

    // permissive 宽容模式下未解析模版会被尽量放宽为 `any`.
    // rigid 严格模式下未解析模版会被保留.
    // 这 4 个值分别用于 false proof, true proof, infer 匹配和后续 constraint fallback.
    let left_permissive = instantiate_operand(left_operand, ConditionalCheckMode::Permissive, true);
    let left_rigid = instantiate_operand(left_operand, ConditionalCheckMode::Rigid, true);
    let right_permissive =
        instantiate_operand(right_operand, ConditionalCheckMode::Permissive, false);
    let right_rigid = instantiate_operand(right_operand, ConditionalCheckMode::Rigid, false);

    // right_has_infer 表示右侧 pattern 里还带 infer.
    let right_has_infer = contains_conditional_infer(&right_rigid);
    // rigid_has_tpl 表示严格视图下至少一侧还依赖未解模板.
    let rigid_has_tpl = left_rigid.contains_tpl_node() || right_rigid.contains_tpl_node();
    if !rigid_has_tpl && right_has_infer {
        // 左右两侧都已经具体化时, infer pattern 可以直接做精确匹配, 成功就是 true, 失败就是 false.
        let mut infer_assignments = HashMap::new();
        return if collect_infer_assignments(
            env.db,
            &left_rigid,
            &right_rigid,
            &mut infer_assignments,
        ) {
            ConditionalResolution::ExactTrue { infer_assignments }
        } else {
            ConditionalResolution::ExactFalse
        };
    }

    // permissive false proof 负责回答 "在最宽松视图下, 是否仍然必不成立".
    // 这里禁止 infer 参与, 因为 infer pattern 不能拿来证明 false.
    if !right_has_infer
        && check_type_compact_with_level(
            env.db,
            &left_permissive,
            &right_permissive,
            crate::semantic::type_check::TypeCheckCheckLevel::GenericConditional,
        )
        .is_err()
    {
        // permissive false proof 只回答 "是否必不成立", 因此这里禁止 infer 参与.
        return ConditionalResolution::ExactFalse;
    }

    // rigid true proof 负责回答 "在最保守视图下, 是否已经足够确定地成立".
    // 只有两侧都稳定, 且右侧不含 infer 时, 才能把结果折叠成 ExactTrue.
    if !rigid_has_tpl
        && !right_has_infer
        && check_type_compact_with_level(
            env.db,
            &left_rigid,
            &right_rigid,
            crate::semantic::type_check::TypeCheckCheckLevel::GenericConditional,
        )
        .is_ok()
    {
        // rigid true proof 只在两侧都稳定时成立, 避免把仍依赖模板的信息过早折叠.
        return ConditionalResolution::ExactTrue {
            infer_assignments: HashMap::new(),
        };
    }

    // infer 右侧如果仍依赖未解模板, 当前作用域就没有资格为它固定绑定结果.
    // 这时必须 defer, 否则会把占位结论错误地提前提交.
    if right_has_infer && rigid_has_tpl {
        // infer 右侧仍依赖未解模板时必须 defer, 否则会把占位结果错误固定到当前作用域.
        return ConditionalResolution::Deferred;
    }

    // 走到这里说明:
    // 1. 还不能精确证明 true 或 false.
    // 2. 也不需要把整个 conditional 原样 defer.
    // 因此回退到 constraint 求值, 用 true 和 false 两个分支的保守结果合成近似类型.
    let true_type = instantiate_constraint_true_type(env, conditional, left_operand, &right_rigid);
    let false_type =
        instantiate_type_generic(env.db, conditional.get_false_type(), env.substitutor);

    // 两个分支如果已经收敛为同一类型, 直接返回该类型.
    // 否则返回它们的 union, 作为当前 conditional 的约束结果.
    if true_type == false_type {
        ConditionalResolution::Constraint(true_type)
    } else {
        ConditionalResolution::Constraint(TypeOps::Union.apply(env.db, &true_type, &false_type))
    }
}

// 在 conditional 无法证明恒真或恒假, 只能退化为 constraint union 时,
// true 分支仍然应该看到 "T 已满足 extends 右侧约束" 这一局部事实.
// 这里会优先构造一个只对 true 分支生效的 substitutor:
// - 未绑定模板直接注入窄化后的 checked type;
// - 已有 raw 绑定的模板则只增加 checked overlay, 保留原始部分实例化信息.
fn instantiate_constraint_true_type(
    env: &GenericEvalEnv,
    conditional: &LuaConditionalType,
    left_operand: &LuaType,
    narrowed_checked: &LuaType,
) -> LuaType {
    if let Some(true_substitutor) =
        build_true_constraint_substitutor(env, left_operand, narrowed_checked)
    {
        return instantiate_type_generic(env.db, conditional.get_true_type(), &true_substitutor);
    }

    instantiate_type_generic(env.db, conditional.get_true_type(), env.substitutor)
}

// true 分支需要两种不同处理:
// 1. 模板完全未绑定时, 可以直接把它临时绑定为 extends 右侧约束.
// 2. 模板已经带着 raw 绑定进入当前 conditional, 且 raw 里还含外层模板时,
//    只能给 checked operand 增加一层局部 constraint overlay, 不能覆盖原始 raw 绑定.
fn build_true_constraint_substitutor(
    env: &GenericEvalEnv,
    left_operand: &LuaType,
    narrowed_checked: &LuaType,
) -> Option<TypeSubstitutor> {
    let tpl = match left_operand {
        LuaType::TplRef(tpl) | LuaType::ConstTplRef(tpl) => tpl,
        _ => return None,
    };

    let tpl_id = tpl.get_tpl_id();
    match env.substitutor.get(tpl_id) {
        None | Some(SubstitutorValue::None) => {
            let mut true_substitutor = env.substitutor.clone();
            true_substitutor.insert_type(tpl_id, narrowed_checked.clone(), true);
            Some(true_substitutor)
        }
        Some(_) => {
            let raw = env.substitutor.get_raw_type(tpl_id)?;
            if !raw.contain_tpl() {
                return None;
            }

            let mut true_substitutor = env.substitutor.clone();
            true_substitutor.insert_conditional_type(tpl_id, narrowed_checked.clone());
            Some(true_substitutor)
        }
    }
}

fn contains_conditional_infer(ty: &LuaType) -> bool {
    ty.any_type(|inner| matches!(inner, LuaType::ConditionalInfer(_)))
}

fn collect_infer_assignments(
    db: &DbIndex,
    source: &LuaType,
    pattern: &LuaType,
    assignments: &mut HashMap<String, LuaType>,
) -> bool {
    match pattern {
        LuaType::ConditionalInfer(name) => {
            insert_infer_assignment(assignments, name.as_str(), source)
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
                    if !collect_infer_assignments(db, source_param, pattern_param, assignments) {
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

                    if !collect_infer_assignments(db, &ty, pattern_ty, assignments) {
                        return false;
                    }
                }

                let pattern_ret = pattern_func.get_ret();
                if contains_conditional_infer(pattern_ret) {
                    collect_infer_assignments(db, source_func.get_ret(), pattern_ret, assignments)
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
                    return collect_infer_assignments(db, &origin, pattern, assignments);
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
                )
            } else {
                false
            }
        }
        LuaType::Object(pattern_object) => match source {
            LuaType::Object(source_object) => {
                collect_infer_from_object_to_object(db, source_object, pattern_object, assignments)
            }
            LuaType::Ref(type_id) | LuaType::Def(type_id) => {
                collect_infer_from_class_to_object(db, type_id, pattern_object, assignments)
            }
            LuaType::TableConst(table_id) => {
                collect_infer_from_table_to_object(db, table_id, pattern_object, assignments)
            }
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
    assignments: &mut HashMap<String, LuaType>,
) -> bool {
    let source_fields = source_object.get_fields();
    let pattern_fields = pattern_object.get_fields();

    for (key, pattern_field_ty) in pattern_fields {
        if let Some(source_field_ty) = source_fields.get(key) {
            if !collect_infer_assignments(db, source_field_ty, pattern_field_ty, assignments) {
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
    assignments: &mut HashMap<String, LuaType>,
) -> bool {
    let pattern_fields = pattern_object.get_fields();
    let source_type = LuaType::Ref(type_id.clone());

    for (key, pattern_field_ty) in pattern_fields {
        if let Some(member_infos) = find_members_with_key(db, &source_type, key.clone(), false) {
            if let Some(member_info) = member_infos.first() {
                if !collect_infer_assignments(db, &member_info.typ, pattern_field_ty, assignments) {
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
    assignments: &mut HashMap<String, LuaType>,
) -> bool {
    let pattern_fields = pattern_object.get_fields();
    let source_type = LuaType::TableConst(table_id.clone());

    for (key, pattern_field_ty) in pattern_fields {
        if let Some(member_infos) = find_members_with_key(db, &source_type, key.clone(), false) {
            if let Some(member_info) = member_infos.first() {
                if !collect_infer_assignments(db, &member_info.typ, pattern_field_ty, assignments) {
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
    assignments: &mut HashMap<String, LuaType>,
    name: &str,
    ty: &LuaType,
) -> bool {
    if let Some(existing) = assignments.get(name) {
        existing == ty
    } else {
        assignments.insert(name.to_string(), ty.clone());
        true
    }
}

fn resolve_infer_tpl_ids(
    conditional: &LuaConditionalType,
    substitutor: &TypeSubstitutor,
    infer_names: &HashSet<String>,
) -> HashMap<String, GenericTplId> {
    let mut map = HashMap::new();
    conditional.visit_nested_types(&mut |ty: &LuaType| {
        if let LuaType::TplRef(tpl) = ty {
            if substitutor.get(tpl.get_tpl_id()).is_none() {
                let name = tpl.get_name();
                if infer_names.contains(name) && !map.contains_key(name) {
                    map.insert(name.to_string(), tpl.get_tpl_id());
                }
            }
        }
    });

    map
}
