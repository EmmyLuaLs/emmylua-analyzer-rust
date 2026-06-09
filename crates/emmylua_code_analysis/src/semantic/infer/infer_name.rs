use super::{InferFailReason, InferResult};
use crate::{
    CompilationDeclTree, LuaDecl, LuaDeclExtra, LuaInferCache, LuaMemberId, LuaSemanticDeclId,
    LuaType, LuaTypeNode, SemanticDeclLevel, TypeOps,
    db_index::{DbIndex, LuaDeclOrMemberId},
    find_compilation_decl_by_position, find_decl_by_id, find_signature_by_id, get_file_decl_tree,
    get_member_item_by_member_id, global_type, infer_compilation_decl_type,
    infer_node_semantic_decl,
    semantic::{
        infer::narrow::{VarRefId, get_var_ref_type, infer_expr_narrow_type},
        semantic_info::resolve_global_decl_id,
    },
};
use emmylua_parser::{LuaAstNode, LuaExpr, LuaIndexExpr, LuaNameExpr};

pub fn infer_name_expr(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    name_expr: LuaNameExpr,
) -> InferResult {
    let name_token = name_expr.get_name_token().ok_or(InferFailReason::None)?;
    let name = name_token.get_name_text();
    match name {
        "self" => return infer_self(db, cache, name_expr),
        "_G" => return Ok(LuaType::Global),
        _ => {}
    }

    let file_id = cache.get_file_id();
    let decl_id = db
        .get_reference_index()
        .get_local_reference(&file_id)
        .and_then(|file_ref| file_ref.get_decl_id(&name_expr.get_range()))
        .or_else(|| find_summary_local_decl_id(db, file_id, name, name_expr.get_position()));
    if let Some(decl_id) = decl_id {
        let result = infer_var_ref_type(
            db,
            cache,
            LuaExpr::NameExpr(name_expr),
            VarRefId::VarRef(decl_id),
        );
        match result {
            Ok(typ) if !typ.is_unknown() => Ok(typ),
            Ok(_) | Err(InferFailReason::UnResolveDeclType(_) | InferFailReason::None) => {
                if let Some(summary_decl) =
                    find_compilation_decl_by_position(db, decl_id.file_id, decl_id.position)
                    && let Some(summary_type) = infer_compilation_decl_type(db, &summary_decl)
                {
                    return Ok(summary_type);
                }

                result
            }
            Err(_) => result,
        }
    } else {
        if let Some(summary_type) =
            infer_summary_local_decl_type(db, file_id, name, name_expr.get_position())
        {
            return Ok(summary_type);
        }

        infer_global_type(db, name)
    }
}

fn infer_summary_local_decl_type(
    db: &DbIndex,
    file_id: crate::FileId,
    name: &str,
    position: rowan::TextSize,
) -> Option<LuaType> {
    let decl_tree = CompilationDeclTree::new(db.get_summary_db().file().decl_tree(file_id)?);
    let decl = decl_tree.find_local_decl(name, position)?;
    find_compilation_decl_by_position(db, file_id, decl.id.as_position())
        .and_then(|summary_decl| infer_compilation_decl_type(db, &summary_decl))
}

fn infer_self(db: &DbIndex, cache: &mut LuaInferCache, name_expr: LuaNameExpr) -> InferResult {
    let decl_or_member_id =
        find_self_decl_or_member_id(db, cache, &name_expr).ok_or(InferFailReason::None)?;
    infer_var_ref_type(
        db,
        cache,
        LuaExpr::NameExpr(name_expr),
        VarRefId::SelfRef(decl_or_member_id),
    )
}

fn infer_var_ref_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    expr: LuaExpr,
    var_ref_id: VarRefId,
) -> InferResult {
    if cache.is_no_flow() {
        get_var_ref_type(db, cache, &var_ref_id)
    } else {
        infer_expr_narrow_type(db, cache, expr, var_ref_id)
    }
}

pub fn get_name_expr_var_ref_id(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    name_expr: &LuaNameExpr,
) -> Option<VarRefId> {
    let name_token = name_expr.get_name_token()?;
    let name = name_token.get_name_text();
    match name {
        "self" => {
            let decl_or_id = find_self_decl_or_member_id(db, cache, name_expr)?;
            Some(VarRefId::SelfRef(decl_or_id))
        }
        _ => {
            let file_id = cache.get_file_id();
            if let Some(decl_id) = db
                .get_reference_index()
                .get_local_reference(&file_id)
                .and_then(|file_ref| file_ref.get_decl_id(&name_expr.get_range()))
                .or_else(|| find_summary_local_decl_id(db, file_id, name, name_expr.get_position()))
            {
                return Some(VarRefId::VarRef(decl_id));
            }

            let global_decl_id = resolve_global_decl_id(db, cache, name, Some(name_expr))?;
            Some(VarRefId::VarRef(global_decl_id))
        }
    }
}

pub fn infer_param(db: &DbIndex, decl: &LuaDecl) -> InferResult {
    let (param_idx, signature_id, member_id) = match &decl.extra {
        LuaDeclExtra::Param {
            idx,
            signature_id,
            owner_member_id: closure_owner_syntax_id,
        } => (*idx, *signature_id, *closure_owner_syntax_id),
        _ => unreachable!(),
    };

    let mut colon_define = false;
    // find local annotation
    if let Some(signature) = find_signature_by_id(db, &signature_id) {
        colon_define = signature.is_colon_define;
        if let Some(param_info) = signature.get_param_info_by_id(param_idx) {
            let mut typ = param_info.type_ref.clone();
            if param_info.nullable && !typ.is_nullable() {
                typ = TypeOps::Union.apply(db, &typ, &LuaType::Nil);
            }

            return Ok(typ);
        }
    }

    if let Some(current_member_id) = member_id {
        let member_decl_type = find_decl_member_type(db, current_member_id)?;
        let param_type = find_param_type_from_type(
            db,
            member_decl_type,
            param_idx,
            colon_define,
            decl.get_name() == "...",
        );
        if let Some(param_type) = param_type {
            return Ok(param_type);
        }
    }

    Err(InferFailReason::UnResolveDeclType(decl.get_id()))
}

pub fn find_decl_member_type(db: &DbIndex, member_id: LuaMemberId) -> InferResult {
    let item = get_member_item_by_member_id(db, member_id).ok_or(InferFailReason::None)?;
    item.resolve_type(db)
}

fn adjust_param_idx(
    param_idx: usize,
    current_colon_define: bool,
    decl_colon_defined: bool,
) -> usize {
    let mut adjusted_idx = param_idx;
    match (current_colon_define, decl_colon_defined) {
        (true, false) => {
            adjusted_idx += 1;
        }
        (false, true) => adjusted_idx = adjusted_idx.saturating_sub(1),
        _ => {}
    }
    adjusted_idx
}

fn check_dots_param_types(
    params: &[(String, Option<LuaType>)],
    param_idx: usize,
    cur_type: &Option<LuaType>,
) -> Option<LuaType> {
    for (_, typ) in params.iter().skip(param_idx) {
        if let Some(typ) = typ
            && let Some(cur_type) = cur_type
            && cur_type != typ
        {
            return Some(LuaType::Any);
        }
    }
    None
}

fn find_param_type_from_type(
    db: &DbIndex,
    source_type: LuaType,
    param_idx: usize,
    current_colon_define: bool,
    is_dots: bool,
) -> Option<LuaType> {
    match source_type {
        LuaType::Signature(signature_id) => {
            let signature = find_signature_by_id(db, &signature_id)?;
            let adjusted_idx =
                adjust_param_idx(param_idx, current_colon_define, signature.is_colon_define);

            match signature.get_param_info_by_id(adjusted_idx) {
                Some(param_info) => {
                    let mut typ = param_info.type_ref.clone();
                    if param_info.nullable && !typ.is_nullable() {
                        typ = TypeOps::Union.apply(db, &typ, &LuaType::Nil);
                    }
                    Some(typ)
                }
                None => {
                    if !signature.param_docs.is_empty() {
                        return None;
                    }

                    let mut final_type = None;
                    for overload in &signature.overloads {
                        let adjusted_idx = adjust_param_idx(
                            param_idx,
                            current_colon_define,
                            overload.is_colon_define(),
                        );

                        let cur_type =
                            if let Some((_, typ)) = overload.get_params().get(adjusted_idx) {
                                typ.clone()
                            } else {
                                return None;
                            };

                        if is_dots
                            && let Some(any_type) = check_dots_param_types(
                                overload.get_params(),
                                adjusted_idx,
                                &cur_type,
                            )
                        {
                            return Some(any_type);
                        }

                        if let Some(typ) = cur_type {
                            final_type = match final_type {
                                Some(existing) => Some(TypeOps::Union.apply(db, &existing, &typ)),
                                None => Some(typ.clone()),
                            };
                        }
                    }
                    final_type
                }
            }
        }
        LuaType::DocFunction(f) => {
            let adjusted_idx =
                adjust_param_idx(param_idx, current_colon_define, f.is_colon_define());
            if let Some((_, typ)) = f.get_params().get(adjusted_idx) {
                let cur_type = typ.clone();
                if is_dots
                    && let Some(any_type) =
                        check_dots_param_types(f.get_params(), adjusted_idx, &cur_type)
                {
                    return Some(any_type);
                }
                cur_type
            } else {
                None
            }
        }
        LuaType::Union(_) => {
            find_param_type_from_union(db, source_type, param_idx, current_colon_define, is_dots)
        }
        _ => None,
    }
}

fn find_param_type_from_union(
    db: &DbIndex,
    source_type: LuaType,
    param_idx: usize,
    origin_colon_define: bool,
    is_dots: bool,
) -> Option<LuaType> {
    match source_type {
        LuaType::Signature(signature_id) => {
            let signature = find_signature_by_id(db, &signature_id)?;
            if !signature.param_docs.is_empty() {
                return None;
            }
            let mut final_type = None;
            for overload in &signature.overloads {
                let adjusted_idx =
                    adjust_param_idx(param_idx, origin_colon_define, overload.is_colon_define());

                let cur_type = if let Some((_, typ)) = overload.get_params().get(adjusted_idx) {
                    typ.clone()
                } else {
                    return None;
                };

                if is_dots
                    && let Some(any_type) =
                        check_dots_param_types(overload.get_params(), adjusted_idx, &cur_type)
                {
                    return Some(any_type);
                }

                if let Some(typ) = cur_type {
                    final_type = match final_type {
                        Some(existing) => Some(TypeOps::Union.apply(db, &existing, &typ)),
                        None => Some(typ.clone()),
                    };
                }
            }
            final_type
        }
        LuaType::DocFunction(f) => {
            let adjusted_idx =
                adjust_param_idx(param_idx, origin_colon_define, f.is_colon_define());
            let cur_type = if let Some((_, typ)) = f.get_params().get(adjusted_idx) {
                typ.clone()
            } else {
                return None;
            };

            if is_dots
                && let Some(any_type) =
                    check_dots_param_types(f.get_params(), adjusted_idx, &cur_type)
            {
                return Some(any_type);
            }

            cur_type
        }
        LuaType::Union(union_types) => {
            let mut final_type = None;
            for ty in union_types.into_vec() {
                if let Some(ty) = find_param_type_from_union(
                    db,
                    ty.clone(),
                    param_idx,
                    origin_colon_define,
                    is_dots,
                ) {
                    if is_dots && ty.is_any() {
                        return Some(ty);
                    }
                    final_type = match final_type {
                        Some(existing) => Some(TypeOps::Union.apply(db, &existing, &ty)),
                        None => Some(ty),
                    };
                }
            }
            final_type
        }
        _ => None,
    }
}

pub fn infer_global_type(db: &DbIndex, name: &str) -> InferResult {
    let typ = global_type(db, name).ok_or(InferFailReason::None)?;
    if matches!(typ, LuaType::DocFunction(_) | LuaType::Signature(_))
        || typ.any_type(LuaType::is_function)
    {
        Ok(typ)
    } else if !typ.is_generic() && typ.contain_tpl() {
        Ok(LuaType::Unknown)
    } else {
        Ok(typ)
    }
}

fn find_summary_local_decl_id(
    db: &DbIndex,
    file_id: crate::FileId,
    name: &str,
    position: rowan::TextSize,
) -> Option<crate::LuaDeclId> {
    let decl_tree = CompilationDeclTree::new(db.get_summary_db().file().decl_tree(file_id)?);
    let decl = decl_tree.find_local_decl(name, position)?;
    let decl_id = crate::LuaDeclId::new(file_id, decl.id.as_position());
    let decl = find_decl_by_id(db, &decl_id)?;
    if decl.is_global() {
        return None;
    }

    Some(decl_id)
}

pub fn find_self_decl_or_member_id(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    name_expr: &LuaNameExpr,
) -> Option<LuaDeclOrMemberId> {
    let file_id = cache.get_file_id();
    let tree = get_file_decl_tree(db, file_id)?;
    todo!()
    // let self_decl = tree.find_local_decl("self", name_expr.get_position())?;
    // if !self_decl.is_implicit_self() {
    //     return Some(LuaDeclOrMemberId::Decl(self_decl.get_id()));
    // }

    // let root = name_expr.get_root();
    // let syntax_id = self_decl.get_syntax_id();
    // let index_token = syntax_id.to_token_from_root(&root)?;
    // let index_expr = LuaIndexExpr::cast(index_token.parent()?)?;
    // let prefix_expr = index_expr.get_prefix_expr()?;

    // match prefix_expr {
    //     LuaExpr::NameExpr(prefix_name) => {
    //         let name = prefix_name.get_name_text()?;
    //         let decl = tree.find_local_decl(&name, prefix_name.get_position());
    //         if let Some(decl) = decl {
    //             return Some(LuaDeclOrMemberId::Decl(decl.get_id()));
    //         }

    //         let id = resolve_global_decl_id(db, cache, &name, Some(&prefix_name))?;
    //         Some(LuaDeclOrMemberId::Decl(id))
    //     }
    //     LuaExpr::IndexExpr(prefix_index) => {
    //         let semantic_id = infer_node_semantic_decl(
    //             db,
    //             cache,
    //             prefix_index.syntax().clone(),
    //             SemanticDeclLevel::NoTrace,
    //         )?;

    //         match semantic_id {
    //             LuaSemanticDeclId::Member(member_id) => Some(LuaDeclOrMemberId::Member(member_id)),
    //             LuaSemanticDeclId::LuaDecl(decl_id) => Some(LuaDeclOrMemberId::Decl(decl_id)),
    //             _ => None,
    //         }
    //     }
    //     _ => None,
    // }
}
