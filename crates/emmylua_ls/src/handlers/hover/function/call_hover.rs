use std::sync::Arc;

use emmylua_code_analysis::{DbIndex, LuaFunctionType, LuaType, find_callable_overload};
use emmylua_parser::LuaCallExpr;

use crate::handlers::hover::{HoverBuilder, HoverDeclContext, HoverDeclInfo};

use super::{
    define_hover::{HoverFunctionInfo, set_function_info_to_builder},
    extract_function_member, get_function_description,
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

        return set_function_info_to_builder(builder, &mut function_infos);
    }

    for matched_decl in matched_decls {
        let info = build_call_hover_function_info(builder, db, matched_decl, call_expr);
        if let Some(info) = info {
            function_infos.push(info);
        }
    }

    set_function_info_to_builder(builder, &mut function_infos)
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
    call_expr: &LuaCallExpr,
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
        Some(call_expr),
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
    find_callable_overload(
        db,
        &mut builder.semantic_model.get_cache().borrow_mut(),
        decl_type,
        call_arg_types,
        call_expr,
        None,
        false,
    )
    .ok()
    .flatten()
}
