use std::{collections::HashSet, sync::Arc};

use emmylua_code_analysis::{DbIndex, LuaFunctionType, LuaType, LuaTypeDeclId, infer_call_generic};
use emmylua_parser::LuaCallExpr;

use crate::handlers::hover::{HoverBuilder, HoverDeclContext, HoverDeclInfo};

use super::{
    define_hover::{HoverFunctionInfo, set_builder_contents},
    extract_function_member,
    generic::generic_type_substitutor,
    get_function_description,
    render::process_function_type,
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
            if let Some(info) =
                build_unmatched_call_hover_function_info(builder, db, matched_decl, call_expr)
            {
                function_infos.push(info);
            }
        }

        return set_builder_contents(builder, &mut function_infos);
    }

    for matched_decl in matched_decls {
        let info = build_call_hover_function_info(builder, db, matched_decl);
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

fn build_unmatched_call_hover_function_info(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    matched_decl: &HoverDeclInfo,
    call_expr: &LuaCallExpr,
) -> Option<HoverFunctionInfo> {
    let match_semantic_decl = matched_decl.id();
    let function_member = extract_function_member(db, match_semantic_decl);
    let contents = process_function_type(
        builder,
        db,
        matched_decl.typ(),
        match_semantic_decl,
        function_member,
        Some(call_expr),
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
) -> Option<HoverFunctionInfo> {
    let match_semantic_decl = matched_decl.decl.id();
    let function_member = extract_function_member(db, match_semantic_decl);
    let call_type = LuaType::DocFunction(matched_decl.func);

    let contents = process_function_type(
        builder,
        db,
        &call_type,
        match_semantic_decl,
        function_member,
        None,
    )?;

    let description = get_function_description(builder, db, match_semantic_decl);
    HoverFunctionInfo::from_contents(contents, description)
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

    overloads.into_iter().find_map(|func| {
        let func = if func.contain_tpl() {
            infer_call_generic(
                db,
                &mut builder.semantic_model.get_cache().borrow_mut(),
                func.as_ref(),
                call_expr.clone(),
            )
            .map(Arc::new)
            .unwrap_or(func)
        } else {
            func
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
