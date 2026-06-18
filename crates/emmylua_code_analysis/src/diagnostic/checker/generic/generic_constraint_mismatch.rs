use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallExpr, LuaClosureExpr, LuaComment, LuaDocGenericDeclList,
    LuaDocTagAlias, LuaDocTagClass, LuaDocTagGeneric, LuaDocTagType, LuaDocType,
};
use rowan::TextRange;
use smol_str::SmolStr;

use super::call_constraint::{
    CallConstraintArg, CallConstraintContext, build_call_constraint_context,
    normalize_constraint_type,
};
use crate::diagnostic::{checker::Checker, lua_diagnostic::DiagnosticContext};
use crate::{
    DiagnosticCode, DocTypeInferContext, GenericParam, GenericTplId, LuaArrayType, LuaGenericType,
    LuaIntersectionType, LuaObjectType, LuaSignatureId, LuaStringTplType, LuaTupleType, LuaType,
    LuaUnionType, RenderLevel, SemanticModel, TypeCheckFailReason, TypeCheckResult,
    TypeSubstitutor, VariadicType, humanize_type, infer_doc_type, instantiate_type_generic,
};

pub struct GenericConstraintMismatchChecker;

impl Checker for GenericConstraintMismatchChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::GenericConstraintMismatch];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for node in root.descendants::<LuaAst>() {
            match node {
                LuaAst::LuaCallExpr(call_expr) => {
                    check_call_expr(context, semantic_model, call_expr);
                }
                LuaAst::LuaDocTagClass(doc_tag_class) => {
                    check_doc_tag_class(context, semantic_model, doc_tag_class);
                }
                LuaAst::LuaDocTagAlias(doc_tag_alias) => {
                    check_doc_tag_alias(context, semantic_model, doc_tag_alias);
                }
                LuaAst::LuaDocTagGeneric(doc_tag_generic) => {
                    check_doc_tag_generic(context, semantic_model, doc_tag_generic);
                }
                LuaAst::LuaDocTagType(doc_tag_type) => {
                    check_doc_tag_type(context, semantic_model, doc_tag_type);
                }
                _ => {}
            }
        }
    }
}

fn check_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let Some(CallConstraintContext {
        params,
        args,
        substitutor,
    }) = build_call_constraint_context(semantic_model, &call_expr)
    else {
        return Some(());
    };

    for (i, (_, param_type)) in params.iter().enumerate() {
        let param_type = if let Some(param_type) = param_type {
            param_type
        } else {
            continue;
        };

        check_call_arg(
            context,
            semantic_model,
            i,
            param_type,
            &args,
            false,
            &substitutor,
        );
    }

    Some(())
}

fn check_doc_tag_class(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    doc_tag_class: LuaDocTagClass,
) -> Option<()> {
    let generic_decl_list = doc_tag_class.get_generic_decl()?;
    let name = doc_tag_class.get_name_token()?.get_name_text().to_string();
    let type_decl = semantic_model.get_db().get_type_index().find_type_decl(
        semantic_model.get_file_id(),
        &name,
        semantic_model
            .get_db()
            .resolve_workspace_id(semantic_model.get_file_id()),
    )?;
    let generic_params = semantic_model
        .get_db()
        .get_type_index()
        .get_generic_params(&type_decl.get_id())?;
    check_generic_decl_defaults(context, semantic_model, generic_decl_list, generic_params)
}

fn check_doc_tag_alias(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    doc_tag_alias: LuaDocTagAlias,
) -> Option<()> {
    let generic_decl_list = doc_tag_alias.get_generic_decl_list()?;
    let name = doc_tag_alias.get_name_token()?.get_name_text().to_string();
    let type_decl = semantic_model.get_db().get_type_index().find_type_decl(
        semantic_model.get_file_id(),
        &name,
        semantic_model
            .get_db()
            .resolve_workspace_id(semantic_model.get_file_id()),
    )?;
    let generic_params = semantic_model
        .get_db()
        .get_type_index()
        .get_generic_params(&type_decl.get_id())?;
    check_generic_decl_defaults(context, semantic_model, generic_decl_list, generic_params)
}

fn check_doc_tag_generic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    doc_tag_generic: LuaDocTagGeneric,
) -> Option<()> {
    let comment = doc_tag_generic.get_parent::<LuaComment>()?;
    let closure = match comment.get_owner()? {
        LuaAst::LuaFuncStat(func) => func.get_closure(),
        LuaAst::LuaLocalFuncStat(local_func) => local_func.get_closure(),
        owner => owner.descendants::<LuaClosureExpr>().next(),
    }?;
    let signature_id = LuaSignatureId::from_closure(semantic_model.get_file_id(), &closure);
    let signature = semantic_model
        .get_db()
        .get_signature_index()
        .get(&signature_id)?;
    let generic_decl_list = doc_tag_generic.get_generic_decl_list()?;
    check_generic_decl_defaults(
        context,
        semantic_model,
        generic_decl_list,
        &signature.generic_params,
    )
}

fn check_generic_decl_defaults(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    generic_decl_list: LuaDocGenericDeclList,
    generic_params: &[GenericParam],
) -> Option<()> {
    for (idx, generic_decl) in generic_decl_list.get_generic_decl().enumerate() {
        let Some(generic_param) = generic_params.get(idx) else {
            continue;
        };
        let (Some(constraint), Some(default_type), Some(default_doc_type)) = (
            generic_param.constraint.as_ref(),
            generic_param.default.as_ref(),
            generic_decl.get_default_type(),
        ) else {
            continue;
        };

        let result =
            check_generic_default_satisfies_constraint(semantic_model, constraint, default_type);
        if result.is_err() {
            let display_constraint =
                normalize_constraint_type(semantic_model.get_db(), constraint.clone());
            let display_default_type =
                normalize_constraint_type(semantic_model.get_db(), default_type.clone());
            add_type_check_diagnostic(
                context,
                semantic_model,
                default_doc_type.get_range(),
                &display_constraint,
                &display_default_type,
                result,
            );
        }
    }

    Some(())
}

fn check_generic_default_satisfies_constraint(
    semantic_model: &SemanticModel,
    constraint: &LuaType,
    default_type: &LuaType,
) -> TypeCheckResult {
    check_generic_default_satisfies_constraint_inner(semantic_model, constraint, default_type, 0)
}

fn check_generic_default_satisfies_constraint_inner(
    semantic_model: &SemanticModel,
    constraint: &LuaType,
    default_type: &LuaType,
    depth: usize,
) -> TypeCheckResult {
    if depth > 64 {
        return Err(TypeCheckFailReason::TypeRecursion);
    }

    if constraint == default_type {
        return Ok(());
    }

    if let Some(constraint_tpl_id) = generic_tpl_id(constraint) {
        if generic_tpl_id(default_type) == Some(constraint_tpl_id) {
            return Ok(());
        }

        if let Some(default_constraint) = generic_tpl_constraint(default_type) {
            return check_generic_default_satisfies_constraint_inner(
                semantic_model,
                constraint,
                default_constraint,
                depth + 1,
            );
        }

        if let LuaType::Intersection(intersection) = default_type
            && intersection.get_types().iter().any(|ty| {
                check_generic_default_satisfies_constraint_inner(
                    semantic_model,
                    constraint,
                    ty,
                    depth + 1,
                )
                .is_ok()
            })
        {
            return Ok(());
        }

        return Err(TypeCheckFailReason::TypeNotMatch);
    }

    if let Some(default_constraint) = generic_tpl_constraint(default_type) {
        return check_generic_default_satisfies_constraint_inner(
            semantic_model,
            constraint,
            default_constraint,
            depth + 1,
        );
    }

    match (constraint, default_type) {
        (LuaType::Array(constraint_array), LuaType::Array(default_array)) => {
            return check_generic_default_satisfies_constraint_inner(
                semantic_model,
                constraint_array.get_base(),
                default_array.get_base(),
                depth + 1,
            );
        }
        (LuaType::Generic(constraint_generic), LuaType::Generic(default_generic))
            if constraint_generic.get_base_type_id_ref()
                == default_generic.get_base_type_id_ref()
                && constraint_generic.get_params().len() == default_generic.get_params().len() =>
        {
            for (constraint_param, default_param) in constraint_generic
                .get_params()
                .iter()
                .zip(default_generic.get_params())
            {
                check_generic_default_satisfies_constraint_inner(
                    semantic_model,
                    constraint_param,
                    default_param,
                    depth + 1,
                )?;
            }
            return Ok(());
        }
        (LuaType::Object(constraint_object), LuaType::Object(default_object)) => {
            for (key, constraint_field) in constraint_object.get_fields() {
                let Some(default_field) = default_object.get_fields().get(key) else {
                    if constraint_field.is_nullable() || constraint_field.is_any() {
                        continue;
                    }
                    return Err(TypeCheckFailReason::TypeNotMatch);
                };
                check_generic_default_satisfies_constraint_inner(
                    semantic_model,
                    constraint_field,
                    default_field,
                    depth + 1,
                )?;
            }
            return Ok(());
        }
        (LuaType::Tuple(constraint_tuple), LuaType::Tuple(default_tuple))
            if constraint_tuple.len() == default_tuple.len() =>
        {
            for (constraint_ty, default_ty) in constraint_tuple
                .get_types()
                .iter()
                .zip(default_tuple.get_types())
            {
                check_generic_default_satisfies_constraint_inner(
                    semantic_model,
                    constraint_ty,
                    default_ty,
                    depth + 1,
                )?;
            }
            return Ok(());
        }
        (LuaType::TableGeneric(constraint_params), LuaType::TableGeneric(default_params))
            if constraint_params.len() == default_params.len() =>
        {
            for (constraint_ty, default_ty) in constraint_params.iter().zip(default_params.iter()) {
                check_generic_default_satisfies_constraint_inner(
                    semantic_model,
                    constraint_ty,
                    default_ty,
                    depth + 1,
                )?;
            }
            return Ok(());
        }
        (LuaType::Variadic(constraint_variadic), LuaType::Variadic(default_variadic)) => {
            return check_variadic_default_satisfies_constraint(
                semantic_model,
                constraint_variadic,
                default_variadic,
                depth + 1,
            );
        }
        (LuaType::Union(union), _) => {
            for member in union.into_vec() {
                if check_generic_default_satisfies_constraint_inner(
                    semantic_model,
                    &member,
                    default_type,
                    depth + 1,
                )
                .is_ok()
                {
                    return Ok(());
                }
            }
            return Err(TypeCheckFailReason::TypeNotMatch);
        }
        (_, LuaType::Union(union)) => {
            for member in union.into_vec() {
                check_generic_default_satisfies_constraint_inner(
                    semantic_model,
                    constraint,
                    &member,
                    depth + 1,
                )?;
            }
            return Ok(());
        }
        (LuaType::Intersection(intersection), _) => {
            for member in intersection.get_types() {
                check_generic_default_satisfies_constraint_inner(
                    semantic_model,
                    member,
                    default_type,
                    depth + 1,
                )?;
            }
            return Ok(());
        }
        (_, LuaType::Intersection(intersection)) => {
            for member in intersection.get_types() {
                if check_generic_default_satisfies_constraint_inner(
                    semantic_model,
                    constraint,
                    member,
                    depth + 1,
                )
                .is_ok()
                {
                    return Ok(());
                }
            }
            return Err(TypeCheckFailReason::TypeNotMatch);
        }
        _ => {}
    }

    let check_constraint = instantiate_decl_type_for_check(constraint, false);
    let check_default = instantiate_decl_type_for_check(default_type, true);
    semantic_model.type_check_detail(&check_constraint, &check_default)
}

fn check_variadic_default_satisfies_constraint(
    semantic_model: &SemanticModel,
    constraint_variadic: &VariadicType,
    default_variadic: &VariadicType,
    depth: usize,
) -> TypeCheckResult {
    match (constraint_variadic, default_variadic) {
        (VariadicType::Base(constraint_base), VariadicType::Base(default_base)) => {
            check_generic_default_satisfies_constraint_inner(
                semantic_model,
                constraint_base,
                default_base,
                depth + 1,
            )
        }
        (VariadicType::Multi(constraint_types), VariadicType::Multi(default_types))
            if constraint_types.len() == default_types.len() =>
        {
            for (constraint_ty, default_ty) in constraint_types.iter().zip(default_types.iter()) {
                check_generic_default_satisfies_constraint_inner(
                    semantic_model,
                    constraint_ty,
                    default_ty,
                    depth + 1,
                )?;
            }
            Ok(())
        }
        _ => Err(TypeCheckFailReason::TypeNotMatch),
    }
}

fn generic_tpl_id(ty: &LuaType) -> Option<GenericTplId> {
    match ty {
        LuaType::TplRef(tpl) => Some(tpl.get_tpl_id()),
        LuaType::StrTplRef(str_tpl) => Some(str_tpl.get_tpl_id()),
        _ => None,
    }
}

fn generic_tpl_constraint(ty: &LuaType) -> Option<&LuaType> {
    match ty {
        LuaType::TplRef(tpl) => tpl.get_constraint(),
        LuaType::StrTplRef(str_tpl) => str_tpl.get_constraint(),
        _ => None,
    }
}

/// 将泛型声明中的约束/默认值转换成普通类型检查可比较的形态.
///
/// 对于默认值中的泛型, 我们需要回退到自身声明约束上进行检查.
fn instantiate_decl_type_for_check(ty: &LuaType, is_default: bool) -> LuaType {
    match ty {
        LuaType::TplRef(tpl) => {
            if is_default && let Some(constraint) = tpl.get_constraint() {
                return instantiate_decl_type_for_check(constraint, true);
            }
            rigid_generic_placeholder(tpl.get_tpl_id())
        }
        LuaType::StrTplRef(str_tpl) => {
            if is_default && let Some(constraint) = str_tpl.get_constraint() {
                return instantiate_decl_type_for_check(constraint, true);
            }
            rigid_generic_placeholder(str_tpl.get_tpl_id())
        }
        LuaType::Array(array) => {
            let base = instantiate_decl_type_for_check(array.get_base(), is_default);
            LuaType::Array(LuaArrayType::new(base, array.get_len().clone()).into())
        }
        LuaType::Tuple(tuple) => LuaType::Tuple(
            LuaTupleType::new(
                tuple
                    .get_types()
                    .iter()
                    .map(|ty| instantiate_decl_type_for_check(ty, is_default))
                    .collect(),
                tuple.status,
            )
            .into(),
        ),
        LuaType::Object(object) => {
            let fields = object
                .get_fields()
                .iter()
                .map(|(key, ty)| (key.clone(), instantiate_decl_type_for_check(ty, is_default)))
                .collect();
            let index_access = object
                .get_index_access()
                .iter()
                .map(|(key, value)| {
                    (
                        instantiate_decl_type_for_check(key, is_default),
                        instantiate_decl_type_for_check(value, is_default),
                    )
                })
                .collect();
            LuaType::Object(LuaObjectType::new_with_fields(fields, index_access).into())
        }
        LuaType::Union(union) => LuaType::Union(
            LuaUnionType::from_vec(
                union
                    .into_vec()
                    .iter()
                    .map(|ty| instantiate_decl_type_for_check(ty, is_default))
                    .collect(),
            )
            .into(),
        ),
        LuaType::Intersection(intersection) => LuaType::Intersection(
            LuaIntersectionType::new(
                intersection
                    .get_types()
                    .iter()
                    .map(|ty| instantiate_decl_type_for_check(ty, is_default))
                    .collect(),
            )
            .into(),
        ),
        LuaType::Generic(generic) => LuaType::Generic(
            LuaGenericType::new(
                generic.get_base_type_id(),
                generic
                    .get_params()
                    .iter()
                    .map(|ty| instantiate_decl_type_for_check(ty, is_default))
                    .collect(),
            )
            .into(),
        ),
        LuaType::TableGeneric(params) => LuaType::TableGeneric(
            params
                .iter()
                .map(|ty| instantiate_decl_type_for_check(ty, is_default))
                .collect::<Vec<_>>()
                .into(),
        ),
        LuaType::Variadic(variadic) => LuaType::Variadic(
            match variadic.as_ref() {
                VariadicType::Base(base) => {
                    VariadicType::Base(instantiate_decl_type_for_check(base, is_default))
                }
                VariadicType::Multi(types) => VariadicType::Multi(
                    types
                        .iter()
                        .map(|ty| instantiate_decl_type_for_check(ty, is_default))
                        .collect(),
                ),
            }
            .into(),
        ),
        _ => ty.clone(),
    }
}

// 过渡期小技巧, 用于泛型默认值约束检查.
//
// 用内部 namespace 名字承载声明处泛型的刚性占位, 避免普通 type_check 将 T 当作可兼容模板放宽.
fn rigid_generic_placeholder(tpl_id: GenericTplId) -> LuaType {
    let name = match tpl_id {
        GenericTplId::Type(idx) => format!("__generic_decl_type_param_{}", idx),
        GenericTplId::Func(idx) => format!("__generic_decl_func_param_{}", idx),
        GenericTplId::ConditionalInfer(idx) => {
            format!("__generic_decl_conditional_param_{}", idx)
        }
    };
    LuaType::Namespace(SmolStr::new(&name).into())
}

fn check_doc_tag_type(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    doc_tag_type: LuaDocTagType,
) -> Option<()> {
    let type_list = doc_tag_type.get_type_list();
    let doc_ctx = DocTypeInferContext::new(semantic_model.get_db(), semantic_model.get_file_id());
    for doc_type in type_list {
        let LuaDocType::Generic(generic_doc_type) = &doc_type else {
            continue;
        };
        let explicit_args = generic_doc_type
            .get_generic_types()
            .map(|type_list| type_list.get_types().collect::<Vec<_>>())
            .unwrap_or_default();
        if explicit_args.is_empty() {
            continue;
        }

        let type_ref = infer_doc_type(doc_ctx, &doc_type);
        let generic_type = match type_ref {
            LuaType::Generic(generic_type) => generic_type,
            _ => continue,
        };

        let generic_params = semantic_model
            .get_db()
            .get_type_index()
            .get_generic_params(&generic_type.get_base_type_id())?;
        for (i, param_type) in generic_type
            .get_params()
            .iter()
            .take(explicit_args.len())
            .enumerate()
        {
            let extend_type = generic_params.get(i)?.constraint.as_ref()?;
            let result = semantic_model.type_check_detail(extend_type, param_type);
            if result.is_err() {
                add_type_check_diagnostic(
                    context,
                    semantic_model,
                    explicit_args.get(i)?.get_range(),
                    extend_type,
                    param_type,
                    result,
                );
            }
        }
    }
    Some(())
}

fn check_call_arg(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    param_index: usize,
    param_type: &LuaType,
    args: &[CallConstraintArg],
    from_union: bool,
    substitutor: &TypeSubstitutor,
) -> Option<()> {
    // 应该先通过泛型体操约束到唯一类型再进行检查
    match param_type {
        LuaType::StrTplRef(str_tpl_ref) => {
            let arg = args.get(param_index)?;
            let arg_type = &arg.raw_type;

            if from_union && !arg_type.is_string() {
                return None;
            }

            let extend_type = str_tpl_ref.get_constraint().map(|ty| {
                normalize_constraint_type(
                    semantic_model.get_db(),
                    instantiate_type_generic(semantic_model.get_db(), ty, substitutor),
                )
            });
            check_str_tpl_ref(
                context,
                semantic_model,
                str_tpl_ref,
                arg_type,
                arg.range,
                extend_type,
            );
        }
        LuaType::TplRef(tpl_ref) => {
            let arg = args.get(param_index)?;
            if let Some(extend_type) = tpl_ref.get_constraint().map(|ty| {
                normalize_constraint_type(
                    semantic_model.get_db(),
                    instantiate_type_generic(semantic_model.get_db(), ty, substitutor),
                )
            }) {
                let result = check_generic_default_satisfies_constraint(
                    semantic_model,
                    &extend_type,
                    &arg.raw_type,
                );
                if result.is_err() {
                    add_type_check_diagnostic(
                        context,
                        semantic_model,
                        arg.range,
                        &extend_type,
                        &arg.raw_type,
                        result,
                    );
                }
            }
        }
        LuaType::Union(union_type) => {
            // 如果不是来自 union, 才展开 union 中的每个类型进行检查
            if !from_union {
                for union_member_type in union_type.into_vec().iter() {
                    check_call_arg(
                        context,
                        semantic_model,
                        param_index,
                        union_member_type,
                        args,
                        true,
                        substitutor,
                    );
                }
            }
        }
        _ => {}
    }
    Some(())
}

fn check_str_tpl_ref(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    str_tpl_ref: &LuaStringTplType,
    arg_type: &LuaType,
    range: TextRange,
    extend_type: Option<LuaType>,
) -> Option<()> {
    match arg_type {
        LuaType::StringConst(str) | LuaType::DocStringConst(str) => {
            let full_type_name = format!(
                "{}{}{}",
                str_tpl_ref.get_prefix(),
                str,
                str_tpl_ref.get_suffix()
            );
            let founded_type_decl = semantic_model.get_db().get_type_index().find_type_decl(
                semantic_model.get_file_id(),
                &full_type_name,
                semantic_model
                    .get_db()
                    .resolve_workspace_id(semantic_model.get_file_id()),
            );
            if founded_type_decl.is_none() {
                context.add_diagnostic(
                    DiagnosticCode::GenericConstraintMismatch,
                    range,
                    t!("the string template type does not match any type declaration").to_string(),
                    None,
                );
            }

            if let Some(extend_type) = extend_type
                && let Some(type_decl) = founded_type_decl
            {
                let type_id = type_decl.get_id();
                let ref_type = LuaType::Ref(type_id);
                let result = semantic_model.type_check_detail(&extend_type, &ref_type);
                if result.is_err() {
                    add_type_check_diagnostic(
                        context,
                        semantic_model,
                        range,
                        &extend_type,
                        &ref_type,
                        result,
                    );
                }
            }
        }
        LuaType::String | LuaType::Any | LuaType::Unknown | LuaType::StrTplRef(_) => {}
        _ => {
            context.add_diagnostic(
                DiagnosticCode::GenericConstraintMismatch,
                range,
                t!("the string template type must be a string constant").to_string(),
                None,
            );
        }
    }
    Some(())
}

fn add_type_check_diagnostic(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    range: TextRange,
    extend_type: &LuaType,
    expr_type: &LuaType,
    result: TypeCheckResult,
) {
    let db = semantic_model.get_db();
    match result {
        Ok(_) => (),
        Err(reason) => {
            let reason_message = match reason {
                TypeCheckFailReason::TypeNotMatchWithReason(reason) => reason,
                TypeCheckFailReason::TypeNotMatch | TypeCheckFailReason::DonotCheck => {
                    "".to_string()
                }
                TypeCheckFailReason::TypeRecursion => "type recursion".to_string(),
            };
            context.add_diagnostic(
                DiagnosticCode::GenericConstraintMismatch,
                range,
                t!(
                    "type `%{found}` does not satisfy the constraint `%{source}`. %{reason}",
                    source = humanize_type(db, extend_type, RenderLevel::Simple),
                    found = humanize_type(db, expr_type, RenderLevel::Simple),
                    reason = reason_message
                )
                .to_string(),
                None,
            );
        }
    }
}
