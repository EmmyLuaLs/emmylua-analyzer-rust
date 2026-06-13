use std::{collections::HashSet, sync::Arc};

use emmylua_code_analysis::{
    DbIndex, LuaDocReturnOverloadInfo, LuaFunctionType, LuaSignature, LuaType, LuaTypeDeclId,
    VariadicType, infer_call_generic,
};
use emmylua_parser::LuaCallExpr;

use crate::handlers::hover::{HoverBuilder, HoverDeclContext, HoverDeclInfo};

use super::{
    define_hover::{HoverFunctionInfo, set_builder_contents},
    extract_function_member,
    generic::generic_type_substitutor,
    get_function_description,
    render::{FunctionRenderContext, build_function_return_overload_rows, process_function_type},
};

pub(super) fn build_function_call_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    decl_context: &HoverDeclContext,
    call_expr: &LuaCallExpr,
) -> Option<()> {
    let ordered_decls = decl_context.ordered_decl_refs();
    let call_arg_types = infer_call_arg_types(builder, call_expr);
    let mut function_infos = Vec::new();

    let matched_decls =
        find_decls_for_call(builder, db, &ordered_decls, &call_arg_types, call_expr);
    if matched_decls.is_empty() {
        for matched_decl in ordered_decls {
            if let Some(info) = build_decl_hover_function_info(builder, db, matched_decl) {
                function_infos.push(info);
            }
        }

        return set_builder_contents(builder, &mut function_infos);
    }

    for matched_decl in matched_decls {
        let info = build_call_hover_function_info(builder, db, matched_decl, call_expr);
        if let Some(info) = info {
            function_infos.push(info);
        }
    }

    set_builder_contents(builder, &mut function_infos)
}

fn infer_call_arg_types(builder: &HoverBuilder, call_expr: &LuaCallExpr) -> Vec<LuaType> {
    let Some(args) = call_expr.get_args_list() else {
        return Vec::new();
    };
    let args = args.get_args().collect::<Vec<_>>();
    builder
        .semantic_model
        .infer_expr_list_types(&args, None)
        .into_iter()
        .map(|(typ, _)| typ)
        .collect()
}

fn build_decl_hover_function_info(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    matched_decl: &HoverDeclInfo,
) -> Option<HoverFunctionInfo> {
    let match_semantic_decl = matched_decl.id();
    let function_member = extract_function_member(db, match_semantic_decl);
    let contents = process_function_type(
        builder,
        db,
        matched_decl.typ(),
        match_semantic_decl,
        function_member,
    )?;
    if contents.is_empty() {
        return None;
    }

    let description = get_function_description(builder, db, match_semantic_decl);
    HoverFunctionInfo::from_contents(contents, description)
}

fn build_call_hover_function_info(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    matched_decl: MatchedCallDecl<'_>,
    call_expr: &LuaCallExpr,
) -> Option<HoverFunctionInfo> {
    let match_semantic_decl = matched_decl.decl.id();
    let match_typ = matched_decl.decl.typ();
    let function_member = extract_function_member(db, match_semantic_decl);
    let call_func = matched_decl.func;

    let contents = if let LuaType::Signature(signature_id) = match_typ {
        let signature = db.get_signature_index().get(signature_id)?;
        let base_function = signature.to_doc_func_type();
        let instantiated_signature = infer_call_generic(
            db,
            &mut builder.semantic_model.get_cache().borrow_mut(),
            &base_function,
            call_expr.clone(),
        )
        .ok()?;

        if !signature.return_overloads.is_empty()
            && call_func.get_async_state() == instantiated_signature.get_async_state()
            && call_func.is_colon_define() == instantiated_signature.is_colon_define()
            && call_func.is_variadic() == instantiated_signature.is_variadic()
            && call_func.get_params() == instantiated_signature.get_params()
        {
            let return_overloads =
                instantiate_call_return_overloads(builder, db, call_expr, signature);
            let ret_detail = build_function_return_overload_rows(builder, &return_overloads);
            let ctx = FunctionRenderContext {
                func: call_func.as_ref(),
                semantic_decl: match_semantic_decl,
                owner_member: function_member,
                return_docs: Vec::new(),
                ret_detail: Some(ret_detail),
            };
            vec![super::render::render_function(builder, db, ctx)?]
        } else {
            process_function_type(
                builder,
                db,
                &LuaType::DocFunction(call_func.clone()),
                match_semantic_decl,
                function_member,
            )?
        }
    } else {
        process_function_type(
            builder,
            db,
            &LuaType::DocFunction(call_func.clone()),
            match_semantic_decl,
            function_member,
        )?
    };

    let description = get_function_description(builder, db, match_semantic_decl);
    HoverFunctionInfo::from_contents(contents, description)
}

fn instantiate_call_return_overloads(
    builder: &HoverBuilder,
    db: &DbIndex,
    call_expr: &LuaCallExpr,
    signature: &LuaSignature,
) -> Vec<LuaDocReturnOverloadInfo> {
    let mut cache = builder.semantic_model.get_cache().borrow_mut();

    signature
        .return_overloads
        .iter()
        .map(|row| {
            let row_return_type = match row.type_refs.len() {
                0 => LuaType::Nil,
                1 => row.type_refs[0].clone(),
                _ => LuaType::Variadic(VariadicType::Multi(row.type_refs.clone()).into()),
            };
            let row_function = LuaFunctionType::new(
                signature.async_state,
                signature.is_colon_define,
                signature.is_vararg,
                signature.get_type_params(),
                row_return_type,
                Some(signature.get_function_generic_params()),
            );
            let type_refs = infer_call_generic(db, &mut cache, &row_function, call_expr.clone())
                .ok()
                .map(|func| match func.get_ret() {
                    LuaType::Variadic(variadic) => match variadic.as_ref() {
                        VariadicType::Multi(types) => types.clone(),
                        VariadicType::Base(_) => vec![LuaType::Variadic(variadic.clone())],
                    },
                    typ => vec![typ.clone()],
                })
                .unwrap_or_else(|| row.type_refs.clone());

            LuaDocReturnOverloadInfo {
                type_refs,
                description: row.description.clone(),
            }
        })
        .collect()
}

struct MatchedCallDecl<'a> {
    decl: &'a HoverDeclInfo,
    func: Arc<LuaFunctionType>,
}

fn find_decls_for_call<'a>(
    builder: &HoverBuilder,
    db: &DbIndex,
    ordered_decls: &[&'a HoverDeclInfo],
    call_arg_types: &[LuaType],
    call_expr: &LuaCallExpr,
) -> Vec<MatchedCallDecl<'a>> {
    let mut matched_decls = Vec::new();

    for decl in ordered_decls.iter().copied() {
        if let Some(func) =
            find_callable_for_call(builder, db, decl.typ(), call_arg_types, call_expr)
        {
            matched_decls.push(MatchedCallDecl { decl, func });
        }
    }

    matched_decls
}

fn find_callable_for_call(
    builder: &HoverBuilder,
    db: &DbIndex,
    decl_type: &LuaType,
    call_arg_types: &[LuaType],
    call_expr: &LuaCallExpr,
) -> Option<Arc<LuaFunctionType>> {
    let mut overloads = Vec::new();
    let mut visiting_aliases = HashSet::new();
    collect_callable_functions(db, decl_type, &mut overloads, &mut visiting_aliases);

    overloads.iter().find_map(|func| {
        let func = if func.contain_tpl() {
            infer_call_generic(
                db,
                &mut builder.semantic_model.get_cache().borrow_mut(),
                func.as_ref(),
                call_expr.clone(),
            )
            .map(Arc::new)
            .unwrap_or_else(|_| func.clone())
        } else {
            func.clone()
        };

        builder
            .semantic_model
            .callable_accepts_args(
                func.as_ref(),
                call_arg_types,
                call_expr.is_colon_call(),
                None,
            )
            .then_some(func)
    })
}

fn collect_callable_functions(
    db: &DbIndex,
    typ: &LuaType,
    overloads: &mut Vec<Arc<LuaFunctionType>>,
    visiting_aliases: &mut HashSet<LuaTypeDeclId>,
) {
    match typ {
        LuaType::Ref(type_id) | LuaType::Def(type_id) => {
            let Some(type_decl) = db.get_type_index().get_type_decl(type_id) else {
                return;
            };
            if !visiting_aliases.insert(type_id.clone()) {
                return;
            }

            if let Some(origin_type) = type_decl.get_alias_origin(db, None) {
                collect_callable_functions(db, &origin_type, overloads, visiting_aliases);
            }
            visiting_aliases.remove(type_id);
        }
        LuaType::Generic(generic) => {
            let type_id = generic.get_base_type_id();
            if !visiting_aliases.insert(type_id.clone()) {
                return;
            }

            let Some(substitutor) = generic_type_substitutor(typ) else {
                return;
            };
            if let Some(type_decl) = db.get_type_index().get_type_decl(&type_id)
                && let Some(origin_type) = type_decl.get_alias_origin(db, Some(&substitutor))
            {
                collect_callable_functions(db, &origin_type, overloads, visiting_aliases);
            }
            visiting_aliases.remove(&type_id);
        }
        LuaType::Union(union) => {
            for member in union.into_vec() {
                collect_callable_functions(db, &member, overloads, visiting_aliases);
            }
        }
        LuaType::Intersection(intersection) => {
            for member in intersection.get_types() {
                collect_callable_functions(db, member, overloads, visiting_aliases);
            }
        }
        LuaType::DocFunction(func) => overloads.push(func.clone()),
        LuaType::Signature(signature_id) => {
            if let Some(signature) = db.get_signature_index().get(signature_id) {
                overloads.extend(signature.overloads.iter().cloned());
                overloads.push(signature.to_doc_func_type());
            }
        }
        _ => {}
    }
}
