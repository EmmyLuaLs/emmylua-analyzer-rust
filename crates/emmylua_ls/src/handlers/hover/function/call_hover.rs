use emmylua_code_analysis::{
    DbIndex, InferGuard, LuaSemanticDeclId, LuaType, infer_call_expr_func, infer_call_generic,
};
use emmylua_parser::LuaCallExpr;

use crate::handlers::hover::HoverBuilder;

use super::{
    extract_function_member, get_function_description, instantiate_call_return_overloads,
    render::{FunctionRenderContext, build_function_return_overload_rows, process_function_type},
};

pub(super) fn build_function_call_hover(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    semantic_decls: &[(LuaSemanticDeclId, LuaType)],
    call_expr: &LuaCallExpr,
) -> Option<()> {
    let final_type = infer_call_expr_func(
        db,
        &mut builder.semantic_model.get_cache().borrow_mut(),
        call_expr.clone(),
        semantic_decls.last()?.1.clone(),
        &InferGuard::new(),
        None,
    )
    .ok()?;

    // 根据推断出来的类型确定哪个 semantic_decl 是匹配的
    let mut matched_decl = semantic_decls.last()?;
    for semantic_decl in semantic_decls.iter() {
        let (_, typ) = semantic_decl;
        if let LuaType::DocFunction(f) = typ {
            if f == &final_type {
                matched_decl = semantic_decl;
                break;
            }
        }
    }
    let (match_semantic_decl, match_typ) = matched_decl;

    let function_member = extract_function_member(db, match_semantic_decl);

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
            && final_type.get_async_state() == instantiated_signature.get_async_state()
            && final_type.is_colon_define() == instantiated_signature.is_colon_define()
            && final_type.is_variadic() == instantiated_signature.is_variadic()
            && final_type.get_params() == instantiated_signature.get_params()
        {
            let return_overloads =
                instantiate_call_return_overloads(builder, db, call_expr, signature);
            let ret_detail = build_function_return_overload_rows(builder, &return_overloads);
            let ctx = FunctionRenderContext {
                func: final_type.as_ref(),
                semantic_decl: match_semantic_decl,
                owner_member: function_member,
                return_docs: Vec::new(),
                ret_detail: Some(ret_detail),
            };
            vec![super::render::render_function_signature(builder, db, ctx)?]
        } else {
            process_function_type(
                builder,
                db,
                &LuaType::DocFunction(final_type),
                match_semantic_decl,
                function_member,
            )?
        }
    } else {
        process_function_type(
            builder,
            db,
            &LuaType::DocFunction(final_type),
            match_semantic_decl,
            function_member,
        )?
    };
    let description = get_function_description(builder, db, &match_semantic_decl);
    builder.set_type_description(contents.first()?.clone());
    builder.add_description_from_info(description);

    Some(())
}
