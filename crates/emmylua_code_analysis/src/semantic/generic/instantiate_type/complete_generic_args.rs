use hashbrown::HashSet;

use crate::{
    DbIndex, GenericParam, GenericTplId, LuaAliasCallType, LuaArrayType, LuaAttributeType,
    LuaConditionalType, LuaMappedType, LuaMultiLineUnion, LuaTypeDeclId,
    db_index::{
        LuaFunctionType, LuaGenericType, LuaIntersectionType, LuaObjectType, LuaTupleType, LuaType,
        LuaUnionType, VariadicType,
    },
    semantic::generic::type_substitutor::TypeSubstitutor,
};

use super::instantiate_type_generic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericArgumentCompletion {
    /// 补齐后的泛型实参列表.
    pub completed_args: Option<Vec<LuaType>>,
    /// 仍然缺失且没有默认值的必填泛型参数数量.
    pub missing_required_count: usize,
    /// 补齐默认实参时是否遇到循环引用.
    pub cycled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompletedType {
    ty: LuaType,
    cycled: bool,
}

impl CompletedType {
    fn new(ty: LuaType, cycled: bool) -> Self {
        Self { ty, cycled }
    }

    fn unchanged(ty: &LuaType) -> Self {
        Self::new(ty.clone(), false)
    }
}

struct CompletedTypeList {
    types: Vec<LuaType>,
    cycled: bool,
}

/// 根据已提供的类型泛型实参补齐默认实参.
pub fn complete_type_generic_args(
    db: &DbIndex,
    type_decl_id: &LuaTypeDeclId,
    provided_args: Vec<LuaType>,
) -> GenericArgumentCompletion {
    let mut visiting = HashSet::new();
    complete_type_generic_args_inner(db, type_decl_id, provided_args, &mut visiting)
}

/// 在任意类型表达式内补齐类型泛型实参.
pub fn complete_type_generic_args_in_type(db: &DbIndex, ty: &LuaType) -> LuaType {
    let mut visiting = HashSet::new();
    complete_type_generic_args_in_type_inner(db, ty, &mut visiting).ty
}

fn complete_type_generic_args_inner(
    db: &DbIndex,
    type_decl_id: &LuaTypeDeclId,
    provided_args: Vec<LuaType>,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> GenericArgumentCompletion {
    let Some(generic_params) = db.get_type_index().get_generic_params(type_decl_id) else {
        return GenericArgumentCompletion {
            completed_args: Some(provided_args),
            missing_required_count: 0,
            cycled: false,
        };
    };

    if generic_params.is_empty() || provided_args.len() >= generic_params.len() {
        return GenericArgumentCompletion {
            completed_args: Some(provided_args),
            missing_required_count: 0,
            cycled: false,
        };
    }

    if !visiting.insert(type_decl_id.clone()) {
        return GenericArgumentCompletion {
            completed_args: Some(provided_args),
            missing_required_count: 0,
            cycled: true,
        };
    }

    let mut params = Vec::with_capacity(generic_params.len().max(provided_args.len()));
    let mut substitutor = TypeSubstitutor::new();
    let mut missing_required_count = 0;
    let mut cycled = false;
    for (idx, generic_param) in generic_params.iter().enumerate() {
        if let Some(provided_arg) = provided_args.get(idx) {
            let provided_arg = provided_arg.clone();
            substitutor.insert_type(GenericTplId::Type(idx as u32), provided_arg.clone(), true);
            params.push(provided_arg);
            continue;
        }

        if let Some(default_type) = &generic_param.default_type {
            if missing_required_count != 0 {
                continue;
            }

            let completed_type =
                complete_type_generic_args_in_type_inner(db, default_type, visiting);
            cycled |= completed_type.cycled;
            let default_type = if completed_type.cycled {
                default_type.clone()
            } else {
                completed_type.ty
            };
            let instantiated = instantiate_type_generic(db, &default_type, &substitutor);
            substitutor.insert_type(GenericTplId::Type(idx as u32), instantiated.clone(), true);
            params.push(instantiated);
        } else {
            missing_required_count += 1;
        }
    }

    if missing_required_count == 0 && provided_args.len() > generic_params.len() {
        params.extend(provided_args[generic_params.len()..].iter().cloned());
    }

    visiting.remove(type_decl_id);
    GenericArgumentCompletion {
        completed_args: (missing_required_count == 0).then_some(params),
        missing_required_count,
        cycled,
    }
}

fn complete_type_generic_args_in_type_inner(
    db: &DbIndex,
    ty: &LuaType,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> CompletedType {
    match ty {
        LuaType::Ref(type_decl_id) | LuaType::Def(type_decl_id) => {
            complete_type_decl_ref(db, ty, type_decl_id, visiting)
        }
        LuaType::Generic(generic) => complete_generic_type(db, ty, generic, visiting),
        LuaType::Array(array) => {
            let base = complete_type_generic_args_in_type_inner(db, array.get_base(), visiting);
            CompletedType::new(
                LuaType::Array(LuaArrayType::new(base.ty, array.get_len().clone()).into()),
                base.cycled,
            )
        }
        LuaType::Tuple(tuple) => {
            let types = complete_type_list(db, tuple.get_types(), visiting);
            CompletedType::new(
                LuaType::Tuple(LuaTupleType::new(types.types, tuple.status).into()),
                types.cycled,
            )
        }
        LuaType::DocFunction(func) => complete_doc_function(db, func, visiting),
        LuaType::Object(object) => complete_object_type(db, object, visiting),
        LuaType::Union(union) => {
            let types = complete_type_list(db, union.into_vec().iter(), visiting);
            CompletedType::new(
                LuaType::Union(LuaUnionType::from_vec(types.types).into()),
                types.cycled,
            )
        }
        LuaType::Intersection(intersection) => {
            let types = complete_type_list(db, intersection.get_types().iter(), visiting);
            CompletedType::new(
                LuaType::Intersection(LuaIntersectionType::new(types.types).into()),
                types.cycled,
            )
        }
        LuaType::TableGeneric(params) => {
            let types = complete_type_list(db, params.iter(), visiting);
            CompletedType::new(LuaType::TableGeneric(types.types.into()), types.cycled)
        }
        LuaType::StrTplRef(str_tpl) => {
            let constraint = str_tpl
                .get_constraint()
                .map(|ty| complete_type_generic_args_in_type_inner(db, ty, visiting));
            let cycled = constraint
                .as_ref()
                .is_some_and(|constraint| constraint.cycled);
            CompletedType::new(
                LuaType::StrTplRef(
                    crate::LuaStringTplType::new(
                        str_tpl.get_prefix(),
                        str_tpl.get_name(),
                        str_tpl.get_tpl_id(),
                        str_tpl.get_suffix(),
                        constraint.map(|constraint| constraint.ty),
                    )
                    .into(),
                ),
                cycled,
            )
        }
        LuaType::Variadic(variadic) => complete_variadic_type(db, variadic, visiting),
        LuaType::Instance(instance) => {
            let base = complete_type_generic_args_in_type_inner(db, instance.get_base(), visiting);
            CompletedType::new(
                LuaType::Instance(
                    crate::LuaInstanceType::new(base.ty, instance.get_range().clone()).into(),
                ),
                base.cycled,
            )
        }
        LuaType::Call(alias_call) => {
            let operands = complete_type_list(db, alias_call.get_operands().iter(), visiting);
            CompletedType::new(
                LuaType::Call(
                    LuaAliasCallType::new(alias_call.get_call_kind(), operands.types).into(),
                ),
                operands.cycled,
            )
        }
        LuaType::MultiLineUnion(multi) => complete_multi_line_union(db, multi, visiting),
        LuaType::TypeGuard(guard) => {
            let guard = complete_type_generic_args_in_type_inner(db, guard, visiting);
            CompletedType::new(LuaType::TypeGuard(guard.ty.into()), guard.cycled)
        }
        LuaType::DocAttribute(attribute) => complete_attribute_type(db, attribute, visiting),
        LuaType::Conditional(conditional) => complete_conditional_type(db, conditional, visiting),
        LuaType::Mapped(mapped) => complete_mapped_type(db, mapped, visiting),
        _ => CompletedType::unchanged(ty),
    }
}

fn complete_type_decl_ref(
    db: &DbIndex,
    ty: &LuaType,
    type_decl_id: &LuaTypeDeclId,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> CompletedType {
    if visiting.contains(type_decl_id) {
        return CompletedType::new(ty.clone(), true);
    }

    let completion = complete_type_generic_args_inner(db, type_decl_id, Vec::new(), visiting);
    if completion.cycled {
        return CompletedType::new(ty.clone(), true);
    }

    if let Some(completed_args) = completion
        .completed_args
        .filter(|completed_args| !completed_args.is_empty())
    {
        CompletedType::new(
            LuaType::Generic(LuaGenericType::new(type_decl_id.clone(), completed_args).into()),
            false,
        )
    } else {
        CompletedType::unchanged(ty)
    }
}

fn complete_generic_type(
    db: &DbIndex,
    ty: &LuaType,
    generic: &LuaGenericType,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> CompletedType {
    let base_type_id = generic.get_base_type_id();
    let provided_args = complete_type_list(db, generic.get_params(), visiting);
    if visiting.contains(&base_type_id) {
        return CompletedType::new(
            LuaType::Generic(LuaGenericType::new(base_type_id, provided_args.types).into()),
            true,
        );
    }

    let completion =
        complete_type_generic_args_inner(db, &base_type_id, provided_args.types, visiting);
    if completion.cycled {
        return CompletedType::new(ty.clone(), true);
    }

    let Some(completed_args) = completion.completed_args else {
        return CompletedType::new(ty.clone(), provided_args.cycled);
    };

    CompletedType::new(
        LuaType::Generic(LuaGenericType::new(base_type_id, completed_args).into()),
        provided_args.cycled,
    )
}

fn complete_doc_function(
    db: &DbIndex,
    func: &LuaFunctionType,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> CompletedType {
    let mut cycled = false;
    let params = func
        .get_params()
        .iter()
        .map(|(name, ty)| {
            let completed = ty
                .as_ref()
                .map(|ty| complete_type_generic_args_in_type_inner(db, ty, visiting));
            cycled |= completed.as_ref().is_some_and(|completed| completed.cycled);
            (name.clone(), completed.map(|completed| completed.ty))
        })
        .collect();
    let ret = complete_type_generic_args_in_type_inner(db, func.get_ret(), visiting);
    CompletedType::new(
        LuaType::DocFunction(
            LuaFunctionType::new(
                func.get_async_state(),
                func.is_colon_define(),
                func.is_variadic(),
                params,
                ret.ty,
            )
            .into(),
        ),
        cycled || ret.cycled,
    )
}

fn complete_object_type(
    db: &DbIndex,
    object: &LuaObjectType,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> CompletedType {
    let mut cycled = false;
    let fields = object
        .get_fields()
        .iter()
        .map(|(key, ty)| {
            let completed = complete_type_generic_args_in_type_inner(db, ty, visiting);
            cycled |= completed.cycled;
            (key.clone(), completed.ty)
        })
        .collect();
    let index_access = object
        .get_index_access()
        .iter()
        .map(|(key, value)| {
            let key = complete_type_generic_args_in_type_inner(db, key, visiting);
            let value = complete_type_generic_args_in_type_inner(db, value, visiting);
            cycled |= key.cycled || value.cycled;
            (key.ty, value.ty)
        })
        .collect();
    CompletedType::new(
        LuaType::Object(LuaObjectType::new_with_fields(fields, index_access).into()),
        cycled,
    )
}

fn complete_variadic_type(
    db: &DbIndex,
    variadic: &VariadicType,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> CompletedType {
    match variadic {
        VariadicType::Multi(types) => {
            let types = complete_type_list(db, types, visiting);
            CompletedType::new(
                LuaType::Variadic(VariadicType::Multi(types.types).into()),
                types.cycled,
            )
        }
        VariadicType::Base(base) => {
            let base = complete_type_generic_args_in_type_inner(db, base, visiting);
            CompletedType::new(
                LuaType::Variadic(VariadicType::Base(base.ty).into()),
                base.cycled,
            )
        }
    }
}

fn complete_multi_line_union(
    db: &DbIndex,
    multi: &LuaMultiLineUnion,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> CompletedType {
    let mut cycled = false;
    let unions = multi
        .get_unions()
        .iter()
        .map(|(ty, description)| {
            let completed = complete_type_generic_args_in_type_inner(db, ty, visiting);
            cycled |= completed.cycled;
            (completed.ty, description.clone())
        })
        .collect();
    CompletedType::new(
        LuaType::MultiLineUnion(LuaMultiLineUnion::new(unions).into()),
        cycled,
    )
}

fn complete_attribute_type(
    db: &DbIndex,
    attribute: &LuaAttributeType,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> CompletedType {
    let mut cycled = false;
    let params = attribute
        .get_params()
        .iter()
        .map(|(name, ty)| {
            let completed = ty
                .as_ref()
                .map(|ty| complete_type_generic_args_in_type_inner(db, ty, visiting));
            cycled |= completed.as_ref().is_some_and(|completed| completed.cycled);
            (name.clone(), completed.map(|completed| completed.ty))
        })
        .collect();
    CompletedType::new(
        LuaType::DocAttribute(LuaAttributeType::new(params).into()),
        cycled,
    )
}

fn complete_conditional_type(
    db: &DbIndex,
    conditional: &LuaConditionalType,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> CompletedType {
    let checked_type =
        complete_type_generic_args_in_type_inner(db, conditional.get_checked_type(), visiting);
    let extends_type =
        complete_type_generic_args_in_type_inner(db, conditional.get_extends_type(), visiting);
    let true_type =
        complete_type_generic_args_in_type_inner(db, conditional.get_true_type(), visiting);
    let false_type =
        complete_type_generic_args_in_type_inner(db, conditional.get_false_type(), visiting);
    let infer_params = complete_generic_param_list(db, conditional.get_infer_params(), visiting);
    let cycled = checked_type.cycled
        || extends_type.cycled
        || true_type.cycled
        || false_type.cycled
        || infer_params.cycled;
    CompletedType::new(
        LuaType::Conditional(
            LuaConditionalType::new(
                checked_type.ty,
                extends_type.ty,
                true_type.ty,
                false_type.ty,
                infer_params.params,
                conditional.has_new,
            )
            .into(),
        ),
        cycled,
    )
}

fn complete_mapped_type(
    db: &DbIndex,
    mapped: &LuaMappedType,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> CompletedType {
    let param = complete_generic_param(db, &mapped.param.1, visiting);
    let value = complete_type_generic_args_in_type_inner(db, &mapped.value, visiting);
    CompletedType::new(
        LuaType::Mapped(
            LuaMappedType::new(
                (mapped.param.0, param.param),
                value.ty,
                mapped.is_readonly,
                mapped.is_optional,
            )
            .into(),
        ),
        param.cycled || value.cycled,
    )
}

fn complete_type_list<'a, I>(
    db: &DbIndex,
    types: I,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> CompletedTypeList
where
    I: IntoIterator<Item = &'a LuaType>,
{
    let mut cycled = false;
    let types = types
        .into_iter()
        .map(|ty| {
            let completed = complete_type_generic_args_in_type_inner(db, ty, visiting);
            cycled |= completed.cycled;
            completed.ty
        })
        .collect();
    CompletedTypeList { types, cycled }
}

struct CompletedGenericParam {
    param: GenericParam,
    cycled: bool,
}

struct CompletedGenericParamList {
    params: Vec<GenericParam>,
    cycled: bool,
}

fn complete_generic_param_list(
    db: &DbIndex,
    params: &[GenericParam],
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> CompletedGenericParamList {
    let mut cycled = false;
    let params = params
        .iter()
        .map(|param| {
            let completed = complete_generic_param(db, param, visiting);
            cycled |= completed.cycled;
            completed.param
        })
        .collect();
    CompletedGenericParamList { params, cycled }
}

fn complete_generic_param(
    db: &DbIndex,
    param: &GenericParam,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> CompletedGenericParam {
    let constraint = param
        .type_constraint
        .as_ref()
        .map(|ty| complete_type_generic_args_in_type_inner(db, ty, visiting));
    let default_type = param
        .default_type
        .as_ref()
        .map(|ty| complete_type_generic_args_in_type_inner(db, ty, visiting));
    let cycled = constraint.as_ref().is_some_and(|ty| ty.cycled)
        || default_type.as_ref().is_some_and(|ty| ty.cycled);
    CompletedGenericParam {
        param: GenericParam::new(
            param.name.clone(),
            constraint.map(|ty| ty.ty),
            default_type.map(|ty| ty.ty),
            param.attributes.clone(),
        ),
        cycled,
    }
}
