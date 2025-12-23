mod instantiate_func_generic;
mod instantiate_special_generic;

use std::{
    collections::{HashMap, HashSet},
    ops::Deref,
};

use crate::{
    DbIndex, GenericTpl, GenericTplId, LuaAliasCallKind, LuaArrayType, LuaConditionalType,
    LuaMappedType, LuaMemberKey, LuaOperatorMetaMethod, LuaSignatureId, LuaTupleStatus,
    LuaTypeDeclId, TypeOps, check_type_compact,
    db_index::{
        LuaFunctionType, LuaGenericType, LuaIntersectionType, LuaObjectType, LuaTupleType, LuaType,
        LuaUnionType, VariadicType,
    },
    semantic::type_check::{TypeCheckCheckLevel, check_type_compact_with_level},
};

use super::type_substitutor::{SubstitutorValue, TypeSubstitutor};
use crate::TypeVisitTrait;
pub use instantiate_func_generic::{build_self_type, infer_self_type, instantiate_func_generic};
pub use instantiate_special_generic::get_keyof_members;
pub use instantiate_special_generic::instantiate_alias_call;

pub fn instantiate_type_generic(
    db: &DbIndex,
    ty: &LuaType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    match ty {
        LuaType::Array(array_type) => instantiate_array(db, array_type.get_base(), substitutor),
        LuaType::Tuple(tuple) => instantiate_tuple(db, tuple, substitutor),
        LuaType::DocFunction(doc_func) => instantiate_doc_function(db, doc_func, substitutor),
        LuaType::Object(object) => instantiate_object(db, object, substitutor),
        LuaType::Union(union) => instantiate_union(db, union, substitutor),
        LuaType::Intersection(intersection) => {
            instantiate_intersection(db, intersection, substitutor)
        }
        LuaType::Generic(generic) => instantiate_generic(db, generic, substitutor),
        LuaType::TableGeneric(table_params) => {
            instantiate_table_generic(db, table_params, substitutor)
        }
        LuaType::TplRef(tpl) => instantiate_tpl_ref(db, tpl, substitutor),
        LuaType::Signature(sig_id) => instantiate_signature(db, sig_id, substitutor),
        LuaType::Call(alias_call) => instantiate_alias_call(db, alias_call, substitutor),
        LuaType::Variadic(variadic) => instantiate_variadic_type(db, variadic, substitutor),
        LuaType::SelfInfer => {
            if let Some(typ) = substitutor.get_self_type() {
                typ.clone()
            } else {
                LuaType::SelfInfer
            }
        }
        LuaType::TypeGuard(guard) => {
            let inner = instantiate_type_generic(db, guard.deref(), substitutor);
            LuaType::TypeGuard(inner.into())
        }
        LuaType::Conditional(conditional) => instantiate_conditional(db, conditional, substitutor),
        LuaType::Mapped(mapped) => instantiate_mapped_type(db, mapped.deref(), substitutor),
        _ => ty.clone(),
    }
}

fn instantiate_array(db: &DbIndex, base: &LuaType, substitutor: &TypeSubstitutor) -> LuaType {
    let base = instantiate_type_generic(db, base, substitutor);
    LuaType::Array(LuaArrayType::from_base_type(base).into())
}

fn instantiate_tuple(db: &DbIndex, tuple: &LuaTupleType, substitutor: &TypeSubstitutor) -> LuaType {
    let tuple_types = tuple.get_types();
    let mut new_types = Vec::new();
    for t in tuple_types {
        if let LuaType::Variadic(inner) = t {
            match inner.deref() {
                VariadicType::Base(base) => {
                    if let LuaType::TplRef(tpl) = base {
                        if let Some(value) = substitutor.get(tpl.get_tpl_id()) {
                            match value {
                                SubstitutorValue::None => {}
                                SubstitutorValue::MultiTypes(types) => {
                                    for typ in types {
                                        new_types.push(typ.clone());
                                    }
                                }
                                SubstitutorValue::Params(params) => {
                                    for (_, ty) in params {
                                        new_types.push(ty.clone().unwrap_or(LuaType::Unknown));
                                    }
                                }
                                SubstitutorValue::Type(ty) => new_types.push(ty.default().clone()),
                                SubstitutorValue::MultiBase(base) => new_types.push(base.clone()),
                            }
                        }
                    }
                }
                VariadicType::Multi(_) => (),
            }

            break;
        }

        let t = instantiate_type_generic(db, t, substitutor);
        new_types.push(t);
    }
    LuaType::Tuple(LuaTupleType::new(new_types, tuple.status).into())
}

pub fn instantiate_doc_function(
    db: &DbIndex,
    doc_func: &LuaFunctionType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let tpl_func_params = doc_func.get_params();
    let tpl_ret = doc_func.get_ret();
    let async_state = doc_func.get_async_state();
    let colon_define = doc_func.is_colon_define();
    let mut is_variadic = doc_func.is_variadic();

    let mut new_params = Vec::new();
    for origin_param in tpl_func_params.iter() {
        let origin_param_type = if let Some(ty) = &origin_param.1 {
            ty
        } else {
            new_params.push((origin_param.0.clone(), None));
            continue;
        };
        match origin_param_type {
            LuaType::Variadic(variadic) => match variadic.deref() {
                VariadicType::Base(base) => match base {
                    LuaType::TplRef(tpl) => {
                        if let Some(value) = substitutor.get(tpl.get_tpl_id()) {
                            match value {
                                SubstitutorValue::Type(ty) => {
                                    let resolved_type = ty.default();
                                    // 如果参数是 `...: T...` 且类型是 tuple, 那么我们将展开 tuple
                                    if origin_param.0 == "..."
                                        && let LuaType::Tuple(tuple) = resolved_type
                                    {
                                        for (i, typ) in tuple.get_types().iter().enumerate() {
                                            let param_name = format!("var{}", i);
                                            new_params.push((param_name, Some(typ.clone())));
                                        }
                                        continue;
                                    }
                                    is_variadic = true;
                                    new_params.push((
                                        "...".to_string(),
                                        Some(LuaType::Variadic(
                                            VariadicType::Base(LuaType::Any).into(),
                                        )),
                                    ));
                                }
                                SubstitutorValue::Params(params) => {
                                    for (i, param) in params.iter().enumerate() {
                                        is_variadic = i + 1 == params.len() && param.0 == "...";
                                        new_params.push(param.clone());
                                    }
                                }
                                SubstitutorValue::MultiTypes(types) => {
                                    for (i, typ) in types.iter().enumerate() {
                                        let param_name = format!("var{}", i);
                                        new_params.push((param_name, Some(typ.clone())));
                                    }
                                }
                                _ => {
                                    is_variadic = true;
                                    new_params.push((
                                        "...".to_string(),
                                        Some(LuaType::Variadic(
                                            VariadicType::Base(LuaType::Any).into(),
                                        )),
                                    ));
                                }
                            }
                        }
                    }
                    LuaType::Generic(generic) => {
                        let new_type = instantiate_generic(db, generic, substitutor);
                        // 如果是 rest 参数且实例化后的类型是 tuple, 那么我们将展开 tuple
                        if let LuaType::Tuple(tuple_type) = &new_type {
                            let base_index = new_params.len();
                            for (offset, tuple_element) in tuple_type.get_types().iter().enumerate()
                            {
                                let param_name = format!("var{}", base_index + offset);
                                new_params.push((param_name, Some(tuple_element.clone())));
                            }
                            continue;
                        }
                        new_params.push((origin_param.0.clone(), Some(new_type)));
                    }
                    _ => {}
                },
                VariadicType::Multi(_) => (),
            },
            _ => {
                let new_type = instantiate_type_generic(db, origin_param_type, substitutor);
                new_params.push((origin_param.0.clone(), Some(new_type)));
            }
        }
    }

    let mut inst_ret_type = instantiate_type_generic(db, tpl_ret, substitutor);
    // 对于可变返回值, 如果实例化是 tuple, 那么我们将展开 tuple
    if let LuaType::Variadic(_) = &&tpl_ret
        && let LuaType::Tuple(tuple) = &inst_ret_type
    {
        match tuple.len() {
            0 => {}
            1 => inst_ret_type = tuple.get_types()[0].clone(),
            _ => {
                inst_ret_type =
                    LuaType::Variadic(VariadicType::Multi(tuple.get_types().to_vec()).into())
            }
        }
    }

    LuaType::DocFunction(
        LuaFunctionType::new(
            async_state,
            colon_define,
            is_variadic,
            new_params,
            inst_ret_type,
        )
        .into(),
    )
}

fn instantiate_object(
    db: &DbIndex,
    object: &LuaObjectType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let fields = object.get_fields();
    let index_access = object.get_index_access();

    let mut new_fields = HashMap::new();
    for (key, field) in fields {
        let new_field = instantiate_type_generic(db, field, substitutor);
        new_fields.insert(key.clone(), new_field);
    }

    let mut new_index_access = Vec::new();
    for (key, value) in index_access {
        let key = instantiate_type_generic(db, key, substitutor);
        let value = instantiate_type_generic(db, value, substitutor);
        new_index_access.push((key, value));
    }

    LuaType::Object(LuaObjectType::new_with_fields(new_fields, new_index_access).into())
}

fn instantiate_union(db: &DbIndex, union: &LuaUnionType, substitutor: &TypeSubstitutor) -> LuaType {
    let types = union.into_vec();
    let mut result_types = Vec::new();
    for t in types {
        let t = instantiate_type_generic(db, &t, substitutor);
        result_types.push(t);
    }

    LuaType::from_vec(result_types)
}

fn instantiate_intersection(
    db: &DbIndex,
    intersection: &LuaIntersectionType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let types = intersection.get_types();
    let mut new_types = Vec::new();
    for t in types {
        let t = instantiate_type_generic(db, t, substitutor);
        new_types.push(t);
    }

    LuaType::Intersection(LuaIntersectionType::new(new_types).into())
}

pub fn instantiate_generic(
    db: &DbIndex,
    generic: &LuaGenericType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let generic_params = generic.get_params();
    let mut new_params = Vec::new();
    for param in generic_params {
        let new_param = instantiate_type_generic(db, param, substitutor);
        new_params.push(new_param);
    }

    let base = generic.get_base_type();
    let type_decl_id = if let LuaType::Ref(id) = base {
        id
    } else {
        return LuaType::Unknown;
    };

    if !substitutor.check_recursion(&type_decl_id)
        && let Some(type_decl) = db.get_type_index().get_type_decl(&type_decl_id)
        && type_decl.is_alias()
    {
        let new_substitutor = TypeSubstitutor::from_alias(new_params.clone(), type_decl_id.clone());
        if let Some(origin) = type_decl.get_alias_origin(db, Some(&new_substitutor)) {
            return origin;
        }
    }

    LuaType::Generic(LuaGenericType::new(type_decl_id, new_params).into())
}

fn instantiate_table_generic(
    db: &DbIndex,
    table_params: &Vec<LuaType>,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let mut new_params = Vec::new();
    for param in table_params {
        let new_param = instantiate_type_generic(db, param, substitutor);
        new_params.push(new_param);
    }

    LuaType::TableGeneric(new_params.into())
}

fn instantiate_tpl_ref(_: &DbIndex, tpl: &GenericTpl, substitutor: &TypeSubstitutor) -> LuaType {
    if let Some(value) = substitutor.get(tpl.get_tpl_id()) {
        match value {
            SubstitutorValue::None => {
                // 如果存在泛型约束, 那么返回约束
                if let Some(constraint) = tpl.get_constraint() {
                    return constraint.clone();
                }
            }
            SubstitutorValue::Type(ty) => return ty.default().clone(),
            SubstitutorValue::MultiTypes(types) => {
                return LuaType::Variadic(VariadicType::Multi(types.clone()).into());
            }
            SubstitutorValue::Params(params) => {
                return params
                    .first()
                    .unwrap_or(&(String::new(), None))
                    .1
                    .clone()
                    .unwrap_or(LuaType::Unknown);
            }
            SubstitutorValue::MultiBase(base) => return base.clone(),
        }
    }

    LuaType::TplRef(tpl.clone().into())
}

fn instantiate_signature(
    db: &DbIndex,
    signature_id: &LuaSignatureId,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    if let Some(signature) = db.get_signature_index().get(signature_id) {
        let origin_type = {
            let fake_doc_function = signature.to_doc_func_type();
            instantiate_doc_function(db, &fake_doc_function, substitutor)
        };
        if signature.overloads.is_empty() {
            return origin_type;
        } else {
            let mut result = Vec::new();
            for overload in signature.overloads.iter() {
                result.push(instantiate_doc_function(
                    db,
                    &(*overload).clone(),
                    substitutor,
                ));
            }
            result.push(origin_type); // 我们需要将原始类型放到最后
            return LuaType::from_vec(result);
        }
    }

    LuaType::Signature(*signature_id)
}

fn instantiate_variadic_type(
    db: &DbIndex,
    variadic: &VariadicType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    match variadic {
        VariadicType::Base(base) => match base {
            LuaType::TplRef(tpl) => {
                if let Some(value) = substitutor.get(tpl.get_tpl_id()) {
                    match value {
                        SubstitutorValue::None => {
                            return LuaType::Never;
                        }
                        SubstitutorValue::Type(ty) => {
                            let resolved_type = ty.default();
                            if matches!(
                                resolved_type,
                                LuaType::Nil | LuaType::Any | LuaType::Unknown | LuaType::Never
                            ) {
                                return resolved_type.clone();
                            }
                            return LuaType::Variadic(
                                VariadicType::Base(resolved_type.clone()).into(),
                            );
                        }
                        SubstitutorValue::MultiTypes(types) => {
                            return LuaType::Variadic(VariadicType::Multi(types.clone()).into());
                        }
                        SubstitutorValue::Params(params) => {
                            let types = params
                                .iter()
                                .filter_map(|(_, ty)| ty.clone())
                                .collect::<Vec<_>>();
                            return LuaType::Variadic(VariadicType::Multi(types).into());
                        }
                        SubstitutorValue::MultiBase(base) => {
                            return LuaType::Variadic(VariadicType::Base(base.clone()).into());
                        }
                    }
                } else {
                    return LuaType::Never;
                }
            }
            LuaType::Generic(generic) => {
                return instantiate_generic(db, generic, substitutor);
            }
            _ => {}
        },
        VariadicType::Multi(types) => {
            if types.iter().any(|it| it.contain_tpl()) {
                let mut new_types = Vec::new();
                for t in types {
                    let t = instantiate_type_generic(db, t, substitutor);
                    match t {
                        LuaType::Never => {}
                        LuaType::Variadic(variadic) => match variadic.deref() {
                            VariadicType::Base(base) => new_types.push(base.clone()),
                            VariadicType::Multi(multi) => {
                                for mt in multi {
                                    new_types.push(mt.clone());
                                }
                            }
                        },
                        _ => new_types.push(t),
                    }
                }
                return LuaType::Variadic(VariadicType::Multi(new_types).into());
            }
        }
    }

    LuaType::Variadic(variadic.clone().into())
}

fn instantiate_conditional(
    db: &DbIndex,
    conditional: &LuaConditionalType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    // 记录右侧出现的每个 infer 名称对应的具体类型
    let mut infer_assignments: HashMap<String, LuaType> = HashMap::new();
    let mut condition_result: Option<bool> = None;

    // 仅当条件形如 T extends ... 时才尝试提前求值, 否则返回原始结构
    if let LuaType::Call(alias_call) = conditional.get_condition()
        && alias_call.get_call_kind() == LuaAliasCallKind::Extends
        && alias_call.get_operands().len() == 2
    {
        let left_operand = &alias_call.get_operands()[0];
        let mut left = instantiate_type_generic(db, left_operand, substitutor);
        // 如果左侧是泛型, 那么我们取字面量类型
        if let LuaType::TplRef(tpl_ref) | LuaType::ConstTplRef(tpl_ref) = left_operand {
            if let Some(raw) = substitutor.get_raw_type(tpl_ref.get_tpl_id()) {
                left = raw.clone();
            }
        }
        let right_origin = &alias_call.get_operands()[1];
        let right = instantiate_type_generic(db, right_origin, substitutor);
        // 如果存在 new 标记与左侧为类定义, 那么我们需要的是他的构造函数签名
        if conditional.has_new
            && let LuaType::Ref(id) | LuaType::Def(id) = &left
        {
            if let Some(decl) = db.get_type_index().get_type_decl(id) {
                // 我们取第一个构造函数签名
                if decl.is_class()
                    && let Some(constructor) = get_default_constructor(db, id)
                {
                    left = constructor;
                }
            }
        }

        // infer 必须位于条件语句中(right), 判断是否包含并收集
        if contains_conditional_infer(&right)
            && collect_infer_assignments(db, &left, &right, &mut infer_assignments)
        {
            condition_result = Some(true);
        } else {
            condition_result = Some(
                check_type_compact_with_level(
                    db,
                    &left,
                    &right,
                    TypeCheckCheckLevel::GenericConditional,
                )
                .is_ok(),
            );
        }
    }

    if let Some(result) = condition_result {
        if result {
            let mut true_substitutor = substitutor.clone();
            if !infer_assignments.is_empty() {
                // 克隆替换器, 确保只有 true 分支可见这些推断结果
                let infer_names: HashSet<String> = conditional
                    .get_infer_params()
                    .iter()
                    .map(|param| param.name.to_string())
                    .collect();

                if !infer_names.is_empty() {
                    let tpl_id_map = resolve_infer_tpl_ids(conditional, substitutor, &infer_names);
                    for (name, ty) in infer_assignments.iter() {
                        if let Some(tpl_id) = tpl_id_map.get(name.as_str()) {
                            true_substitutor.insert_type(*tpl_id, ty.clone(), true);
                        }
                    }
                }
            }

            return instantiate_type_generic(db, conditional.get_true_type(), &true_substitutor);
        } else {
            return instantiate_type_generic(db, conditional.get_false_type(), substitutor);
        }
    }

    let new_condition = instantiate_type_generic(db, conditional.get_condition(), substitutor);
    let new_true = instantiate_type_generic(db, conditional.get_true_type(), substitutor);
    let new_false = instantiate_type_generic(db, conditional.get_false_type(), substitutor);

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

// 遍历类型树判断是否仍存在 ConditionalInfer 占位符
fn contains_conditional_infer(ty: &LuaType) -> bool {
    let mut found = false;
    ty.visit_type(&mut |inner| {
        if matches!(inner, LuaType::ConditionalInfer(_)) {
            found = true;
        }
    });
    found
}

// 尝试将`pattern`中的每个`infer`名称映射到`source`内部对应的类型, 当结构不兼容或发现冲突的赋值时, 返回`false`
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
        LuaType::DocFunction(pattern_func) => {
            match source {
                LuaType::DocFunction(source_func) => {
                    // 匹配函数参数
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
                        } else if let Some(pattern_ty) = pattern_param {
                            if contains_conditional_infer(pattern_ty)
                                || !is_optional_param_type(db, pattern_ty)
                            {
                                return false;
                            }
                        }
                    }

                    if has_variadic && let Some((_, variadic_param)) = pattern_params.last() {
                        if let Some(pattern_ty) = variadic_param {
                            if contains_conditional_infer(pattern_ty) {
                                let rest = if normal_param_len < source_params.len() {
                                    &source_params[normal_param_len..]
                                } else {
                                    &[]
                                };
                                let mut rest_types = Vec::with_capacity(rest.len());
                                for (_, source_param) in rest {
                                    // 如果来源没有类型, 那么将其设为 Any 而不是 Never
                                    rest_types.push(
                                        source_param.as_ref().unwrap_or(&LuaType::Any).clone(),
                                    );
                                }
                                let ty = match rest_types.len() {
                                    0 => LuaType::Never,
                                    1 => rest_types[0].clone(),
                                    _ => LuaType::Tuple(
                                        LuaTupleType::new(rest_types, LuaTupleStatus::InferResolve)
                                            .into(),
                                    ),
                                };

                                if !collect_infer_assignments(db, &ty, pattern_ty, assignments) {
                                    return false;
                                }
                            }
                        }
                    }

                    // 匹配函数返回值
                    let pattern_ret = pattern_func.get_ret();
                    if contains_conditional_infer(pattern_ret) {
                        // 如果返回值也包含 infer, 继续与来源返回值进行匹配
                        collect_infer_assignments(
                            db,
                            source_func.get_ret(),
                            pattern_ret,
                            assignments,
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
                        )
                    } else {
                        false
                    }
                }
                LuaType::Ref(type_decl_id) => {
                    if let Some(type_decl) = db.get_type_index().get_type_decl(type_decl_id) {
                        if type_decl.is_alias()
                            && let Some(origin) = type_decl.get_alias_origin(db, None)
                        {
                            return collect_infer_assignments(db, &origin, &pattern, assignments);
                        }
                    }
                    false
                }
                _ => false,
            }
        }
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
        _ => {
            if contains_conditional_infer(pattern) {
                false
            } else {
                strict_type_match(db, source, pattern)
            }
        }
    }
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

// 记录某个 infer 名称推断出的类型, 并保证重复匹配时保持一致
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

// 定位与每个`infer`名称对应的具体模板标识符, 以便将推断出的绑定写回替换器中.
fn resolve_infer_tpl_ids(
    conditional: &LuaConditionalType,
    substitutor: &TypeSubstitutor,
    infer_names: &HashSet<String>,
) -> HashMap<String, GenericTplId> {
    let mut map = HashMap::new();
    let mut visit = |ty: &LuaType| {
        if let LuaType::TplRef(tpl) = ty {
            if substitutor.get(tpl.get_tpl_id()).is_none() {
                let name = tpl.get_name();
                if infer_names.contains(name) && !map.contains_key(name) {
                    map.insert(name.to_string(), tpl.get_tpl_id());
                }
            }
        }
    };

    conditional.get_true_type().visit_type(&mut visit);
    conditional.get_condition().visit_type(&mut visit);
    conditional.get_false_type().visit_type(&mut visit);

    map
}

fn instantiate_mapped_type(
    db: &DbIndex,
    mapped: &LuaMappedType,
    substitutor: &TypeSubstitutor,
) -> LuaType {
    let constraint = mapped
        .param
        .1
        .type_constraint
        .as_ref()
        .map(|ty| instantiate_type_generic(db, ty, substitutor));

    if let Some(constraint) = constraint {
        let mut key_types = Vec::new();
        collect_mapped_key_atoms(&constraint, &mut key_types);

        let mut visited = HashSet::new();
        let mut fields: Vec<(LuaMemberKey, LuaType)> = Vec::new();
        let mut index_access: Vec<(LuaType, LuaType)> = Vec::new();

        for key_ty in key_types {
            if !visited.insert(key_ty.clone()) {
                continue;
            }

            let value_ty =
                instantiate_mapped_value(db, substitutor, &mapped, mapped.param.0, &key_ty);

            if let Some(member_key) = key_type_to_member_key(&key_ty) {
                if let Some((_, existing)) = fields.iter_mut().find(|(key, _)| key == &member_key) {
                    let merged = LuaType::from_vec(vec![existing.clone(), value_ty]);
                    *existing = merged;
                } else {
                    fields.push((member_key, value_ty));
                }
            } else {
                index_access.push((key_ty, value_ty));
            }
        }

        if !fields.is_empty() || !index_access.is_empty() {
            // key 从 0 开始递增才被视为元组
            if constraint.is_tuple() {
                let mut index = 0;
                let mut is_tuple = true;
                for (key, _) in &fields {
                    if let LuaMemberKey::Integer(i) = key {
                        if *i != index {
                            is_tuple = false;
                            break;
                        }
                        index += 1;
                    } else {
                        is_tuple = false;
                        break;
                    }
                }
                if is_tuple {
                    let types = fields.into_iter().map(|(_, ty)| ty).collect();
                    return LuaType::Tuple(
                        LuaTupleType::new(types, LuaTupleStatus::InferResolve).into(),
                    );
                }
            }
            let field_map: HashMap<LuaMemberKey, LuaType> = fields.into_iter().collect();
            return LuaType::Object(LuaObjectType::new_with_fields(field_map, index_access).into());
        }
    }

    instantiate_type_generic(db, &mapped.value, substitutor)
}

fn instantiate_mapped_value(
    db: &DbIndex,
    substitutor: &TypeSubstitutor,
    mapped: &LuaMappedType,
    tpl_id: GenericTplId,
    replacement: &LuaType,
) -> LuaType {
    let mut local_substitutor = substitutor.clone();
    local_substitutor.insert_type(tpl_id, replacement.clone(), true);
    let mut result = instantiate_type_generic(db, &mapped.value, &local_substitutor);
    // 根据 readonly 和 optional 属性进行处理
    if mapped.is_optional {
        result = TypeOps::Union.apply(db, &result, &LuaType::Nil);
    }
    // TODO: 处理 readonly, 但目前 readonly 的实现存在问题, 这里我们先跳过

    result
}

pub(super) fn key_type_to_member_key(key_ty: &LuaType) -> Option<LuaMemberKey> {
    match key_ty {
        LuaType::DocStringConst(s) => Some(LuaMemberKey::Name(s.deref().clone())),
        LuaType::StringConst(s) => Some(LuaMemberKey::Name(s.deref().clone())),
        LuaType::DocIntegerConst(i) => Some(LuaMemberKey::Integer(*i)),
        LuaType::IntegerConst(i) => Some(LuaMemberKey::Integer(*i)),
        _ => None,
    }
}

fn collect_mapped_key_atoms(key_ty: &LuaType, acc: &mut Vec<LuaType>) {
    match key_ty {
        LuaType::Union(union) => {
            for member in union.into_vec() {
                collect_mapped_key_atoms(&member, acc);
            }
        }
        LuaType::MultiLineUnion(multi) => {
            for (member, _) in multi.get_unions() {
                collect_mapped_key_atoms(member, acc);
            }
        }
        LuaType::Variadic(variadic) => match variadic.deref() {
            VariadicType::Base(base) => collect_mapped_key_atoms(base, acc),
            VariadicType::Multi(types) => {
                for member in types {
                    collect_mapped_key_atoms(member, acc);
                }
            }
        },
        LuaType::Tuple(tuple) => {
            for member in tuple.get_types() {
                collect_mapped_key_atoms(member, acc);
            }
        }
        LuaType::Unknown | LuaType::Never => {}
        _ => acc.push(key_ty.clone()),
    }
}

fn get_default_constructor(db: &DbIndex, decl_id: &LuaTypeDeclId) -> Option<LuaType> {
    let ids = db
        .get_operator_index()
        .get_operators(&decl_id.clone().into(), LuaOperatorMetaMethod::Call)?;

    let id = ids.first()?;
    let operator = db.get_operator_index().get_operator(id)?;
    Some(operator.get_operator_func(db))
}
