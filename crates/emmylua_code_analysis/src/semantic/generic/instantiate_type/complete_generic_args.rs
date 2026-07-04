use hashbrown::HashSet;

use crate::{
    DbIndex, GenericParam, GenericTpl, GenericTplId, LuaAliasCallType, LuaArrayType,
    LuaConditionalType, LuaMappedType, LuaMultiLineUnion, LuaTypeDeclId, TypeVisitTrait,
    db_index::{
        LuaFunctionType, LuaGenericType, LuaIntersectionType, LuaObjectType, LuaTupleType, LuaType,
        LuaUnionType, VariadicType,
    },
    semantic::generic::type_substitutor::{
        GenericCandidate, LiteralPolicy, SubstitutorValue, TypeSubstitutor,
    },
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum GenericDefaultSlot {
    Pending,
    Visiting,
    Resolved(LuaType),
}

struct GenericDefaultContext<'a> {
    generic_params: &'a [GenericParam],
    slots: Vec<GenericDefaultSlot>,
    substitutor: TypeSubstitutor,
}

impl<'a> GenericDefaultContext<'a> {
    fn new(generic_params: &'a [GenericParam], provided_args: &[LuaType]) -> Self {
        // 先把调用方显式传入的实参固定下来, 后续 default 求值不能覆盖这些位置.
        let mut slots = vec![GenericDefaultSlot::Pending; generic_params.len()];
        let mut substitutor = TypeSubstitutor::new();
        for (idx, provided_arg) in provided_args
            .iter()
            .take(generic_params.len())
            .cloned()
            .enumerate()
        {
            slots[idx] = GenericDefaultSlot::Resolved(provided_arg.clone());
            substitutor.insert_value(
                GenericTplId::Type(idx as u32),
                SubstitutorValue::Type(GenericCandidate::new(
                    provided_arg,
                    LiteralPolicy::Preserve,
                )),
            );
        }

        Self {
            generic_params,
            slots,
            substitutor,
        }
    }

    fn set_resolved(&mut self, idx: usize, ty: LuaType) {
        self.slots[idx] = GenericDefaultSlot::Resolved(ty.clone());
        self.substitutor.insert_value(
            GenericTplId::Type(idx as u32),
            SubstitutorValue::Type(GenericCandidate::new(ty, LiteralPolicy::Preserve)),
        );
    }

    fn into_completed_args(self, provided_args: &[LuaType]) -> Vec<LuaType> {
        let mut completed_args = self
            .slots
            .into_iter()
            .map(|slot| match slot {
                GenericDefaultSlot::Resolved(ty) => ty,
                GenericDefaultSlot::Pending | GenericDefaultSlot::Visiting => LuaType::Unknown,
            })
            .collect::<Vec<_>>();
        if provided_args.len() > self.generic_params.len() {
            completed_args.extend(provided_args[self.generic_params.len()..].iter().cloned());
        }

        completed_args
    }
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

    let mut default_context = GenericDefaultContext::new(generic_params, &provided_args);

    // 逐个具化缺失实参. default 可以依赖同一声明列表里的任意参数,
    // 所以这里不能再按 left-to-right 简单替换.
    let mut missing_required_count = 0;
    let mut cycled = false;
    for idx in 0..default_context.generic_params.len() {
        if matches!(&default_context.slots[idx], GenericDefaultSlot::Resolved(_)) {
            continue;
        }

        match resolve_generic_default_arg(db, &mut default_context, idx, visiting) {
            Some(default_cycled) => cycled |= default_cycled,
            None => missing_required_count += 1,
        }
    }

    // 只有所有必填参数都有结果时才返回完整实参列表; 多余实参沿用旧行为追加回结果.
    let completed_args = if missing_required_count == 0 {
        Some(default_context.into_completed_args(&provided_args))
    } else {
        None
    };

    visiting.remove(type_decl_id);
    GenericArgumentCompletion {
        completed_args,
        missing_required_count,
        cycled,
    }
}

fn resolve_generic_default_arg(
    db: &DbIndex,
    context: &mut GenericDefaultContext<'_>,
    idx: usize,
    visiting: &mut HashSet<LuaTypeDeclId>,
) -> Option<bool> {
    // 显式实参或已经具化过的 default 都直接复用.
    if matches!(&context.slots[idx], GenericDefaultSlot::Resolved(_)) {
        return Some(false);
    }

    if matches!(&context.slots[idx], GenericDefaultSlot::Visiting) {
        // 重新遇到正在求值的参数, 说明本地 default 依赖成环.
        context.set_resolved(idx, LuaType::Unknown);
        return Some(true);
    }

    let default_type = context.generic_params[idx].default.clone()?;

    context.slots[idx] = GenericDefaultSlot::Visiting;
    let mut cycled = false;
    // 先具化当前 default 直接引用的本地泛型参数, 例如 `A = B[]`.
    for dep_idx in collect_local_default_deps(&default_type, context.generic_params.len()) {
        if dep_idx == idx {
            // `T = T` 是最短的本地 default 环, 直接落到 unknown.
            context.set_resolved(idx, LuaType::Unknown);
            return Some(true);
        }

        match resolve_generic_default_arg(db, context, dep_idx, visiting) {
            Some(dep_cycled) => cycled |= dep_cycled,
            None => {
                // 依赖的参数本身缺少 default, 当前 default 也无法安全具化.
                context.slots[idx] = GenericDefaultSlot::Pending;
                return None;
            }
        }
    }

    if cycled {
        // 依赖链中出现 default 环时, 当前参数也使用 unknown, 避免留下半解析的 TplRef.
        context.set_resolved(idx, LuaType::Unknown);
        return Some(true);
    }

    let completed_type = complete_type_generic_args_in_type_inner(db, &default_type, visiting);
    let default_type = if completed_type.cycled {
        default_type.clone()
    } else {
        completed_type.ty
    };
    // 本地依赖已经写入 substitutor, 这里直接把 default 里的 TplRef 替换成实际类型.
    let resolved = instantiate_type_generic(db, &default_type, &context.substitutor);
    context.set_resolved(idx, resolved);

    Some(completed_type.cycled)
}

fn collect_local_default_deps(ty: &LuaType, generic_count: usize) -> Vec<usize> {
    let mut deps = Vec::new();
    ty.visit_type(&mut |inner_ty| {
        if let LuaType::TplRef(tpl) = inner_ty
            && let GenericTplId::Type(idx) = tpl.get_tpl_id()
        {
            let idx = idx as usize;
            if idx < generic_count && !deps.contains(&idx) {
                deps.push(idx);
            }
        }
    });
    deps
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
    let generic_params = complete_function_generic_params(db, func, visiting, &mut cycled);
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
                Some(generic_params),
            )
            .into(),
        ),
        cycled || ret.cycled,
    )
}

fn complete_function_generic_params(
    db: &DbIndex,
    func: &LuaFunctionType,
    visiting: &mut HashSet<LuaTypeDeclId>,
    cycled: &mut bool,
) -> Vec<GenericTpl> {
    func.get_generic_params()
        .iter()
        .map(|generic_tpl| {
            let tpl_id = generic_tpl.get_tpl_id();
            let param = generic_tpl.get_param();
            let completed = complete_generic_param(db, param, visiting);
            *cycled |= completed.cycled;
            GenericTpl::new(
                tpl_id,
                completed.param.name,
                completed.param.constraint,
                completed.param.default,
                completed.param.is_const,
                completed.param.attributes,
            )
        })
        .collect()
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
        .constraint
        .as_ref()
        .map(|ty| complete_type_generic_args_in_type_inner(db, ty, visiting));
    let default_type = param
        .default
        .as_ref()
        .map(|ty| complete_type_generic_args_in_type_inner(db, ty, visiting));
    let cycled = constraint.as_ref().is_some_and(|ty| ty.cycled)
        || default_type.as_ref().is_some_and(|ty| ty.cycled);
    CompletedGenericParam {
        param: GenericParam::new(
            param.name.clone(),
            constraint.map(|ty| ty.ty),
            default_type.map(|ty| ty.ty),
            param.is_const,
            param.attributes.clone(),
        ),
        cycled,
    }
}
