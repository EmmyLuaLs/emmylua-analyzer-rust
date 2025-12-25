use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallExpr, LuaClosureExpr, LuaExpr, LuaIndexExpr, LuaLiteralToken,
    LuaNameExpr, LuaStat, LuaSyntaxKind,
};

use crate::{
    DbIndex, LuaDeclId, LuaDeclOrMemberId, LuaInferCache, LuaInstanceType, LuaIntersectionType,
    LuaMemberId, LuaMemberKey, LuaMemberOwner, LuaSemanticDeclId, LuaType, LuaTypeCache,
    LuaTypeDeclId, LuaUnionType, TypeOps,
    semantic::{
        infer::find_self_decl_or_member_id, member::get_buildin_type_map_type_id,
        semantic_info::resolve_global_decl_id,
    },
};

use super::{
    SemanticDeclLevel, infer_expr, infer_token_semantic_decl, semantic_guard::SemanticDeclGuard,
};

pub fn infer_expr_semantic_decl(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    expr: LuaExpr,
    semantic_guard: SemanticDeclGuard,
    level: SemanticDeclLevel,
) -> Option<LuaSemanticDeclId> {
    let file_id = cache.get_file_id();
    let maybe_decl_id = LuaDeclId::new(file_id, expr.get_position());
    if db.get_decl_index().get_decl(&maybe_decl_id).is_some() {
        return Some(LuaSemanticDeclId::LuaDecl(maybe_decl_id));
    };

    match expr {
        LuaExpr::NameExpr(name_expr) => {
            infer_name_expr_semantic_decl(db, cache, name_expr, semantic_guard.next_level()?, level)
        }
        LuaExpr::IndexExpr(index_expr) => {
            infer_index_expr_semantic_decl(db, cache, index_expr, semantic_guard.next_level()?)
        }
        LuaExpr::ClosureExpr(closure_expr) => infer_closure_expr_semantic_decl(
            db,
            cache,
            closure_expr,
            semantic_guard.next_level()?,
            level,
        ),
        _ => {
            let member_id = LuaMemberId::new(expr.get_syntax_id(), file_id);
            if db.get_member_index().get_member(&member_id).is_some() {
                return Some(LuaSemanticDeclId::Member(member_id));
            };

            None
        }
    }
}

fn infer_name_expr_semantic_decl(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    name_expr: LuaNameExpr,
    semantic_guard: SemanticDeclGuard,
    level: SemanticDeclLevel,
) -> Option<LuaSemanticDeclId> {
    let name_token = name_expr.get_name_token()?;
    let name = name_token.get_name_text().to_string();
    if name == "self" {
        return infer_self_semantic_decl(db, cache, name_expr);
    }

    let decl_id = get_name_decl_id(db, cache, &name, name_expr.clone())?;
    let decl = db.get_decl_index().get_decl(&decl_id)?;
    if semantic_guard.reached_limit() {
        return Some(LuaSemanticDeclId::LuaDecl(decl_id));
    }

    let decl_type = db
        .get_type_index()
        .get_type_cache(&decl_id.into())
        .unwrap_or(&LuaTypeCache::InferType(LuaType::Unknown));
    let remove_nil_type = TypeOps::Remove.apply(db, &decl_type, &LuaType::Nil);
    let is_ref_object = remove_nil_type.is_function() || remove_nil_type.is_table();
    if decl.is_local() && !is_ref_object {
        return Some(LuaSemanticDeclId::LuaDecl(decl_id));
    }

    if level.reached_limit() {
        return Some(LuaSemanticDeclId::LuaDecl(decl_id));
    }

    if let Some(value_expr_id) = decl.get_value_syntax_id() {
        match value_expr_id.get_kind() {
            LuaSyntaxKind::NameExpr | LuaSyntaxKind::IndexExpr if remove_nil_type.is_function() => {
                let file_id = decl.get_file_id();
                let tree = db.get_vfs().get_syntax_tree(&file_id)?;
                // second infer
                let value_expr = LuaExpr::cast(value_expr_id.to_node(tree)?)?;
                if let Some(semantic_id) = infer_expr_semantic_decl(
                    db,
                    cache,
                    value_expr,
                    semantic_guard.next_level()?,
                    level.next_level()?,
                ) {
                    return Some(semantic_id);
                }
            }
            LuaSyntaxKind::RequireCallExpr => {
                let file_id = decl.get_file_id();
                let tree = db.get_vfs().get_syntax_tree(&file_id)?;
                let call_expr = LuaCallExpr::cast(value_expr_id.to_node(tree)?)?;
                if call_expr.is_require() {
                    if let Some(semantic_id) = infer_require_module_semantic_decl(db, call_expr) {
                        return Some(semantic_id);
                    }
                }
            }
            _ => {}
        }
    }

    Some(LuaSemanticDeclId::LuaDecl(decl_id))
}

fn infer_require_module_semantic_decl(
    db: &DbIndex,
    call_expr: LuaCallExpr,
) -> Option<LuaSemanticDeclId> {
    let first_arg = call_expr.get_args_list()?.get_args().next()?;
    let module_path = match first_arg {
        LuaExpr::LiteralExpr(literal_expr) => {
            if let Some(literal_token) = literal_expr.get_literal() {
                match literal_token {
                    LuaLiteralToken::String(string_token) => string_token.get_value(),
                    _ => return None,
                }
            } else {
                return None;
            }
        }
        _ => return None,
    };

    let module_info = db.get_module_index().find_module(&module_path)?;
    module_info.semantic_id.clone()
}

fn get_name_decl_id(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    name: &str,
    name_expr: LuaNameExpr,
) -> Option<LuaDeclId> {
    let file_id = cache.get_file_id();
    let references_index = db.get_reference_index();
    let range = name_expr.get_range();
    let local_ref = references_index.get_local_reference(&file_id)?;
    let decl_id = local_ref.get_decl_id(&range);

    if let Some(decl_id) = decl_id {
        let decl = db.get_decl_index().get_decl(&decl_id)?;
        if decl.is_local() {
            return Some(decl_id);
        }
    }

    resolve_global_decl_id(db, cache, name, Some(&name_expr))
}

fn infer_self_semantic_decl(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    name_expr: LuaNameExpr,
) -> Option<LuaSemanticDeclId> {
    let id = find_self_decl_or_member_id(db, cache, &name_expr)?;
    match id {
        LuaDeclOrMemberId::Decl(decl_id) => Some(LuaSemanticDeclId::LuaDecl(decl_id)),
        LuaDeclOrMemberId::Member(member_id) => Some(LuaSemanticDeclId::Member(member_id)),
    }
}

fn infer_index_expr_semantic_decl(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    index_expr: LuaIndexExpr,
    semantic_guard: SemanticDeclGuard,
) -> Option<LuaSemanticDeclId> {
    let prefix_expr = index_expr.get_prefix_expr()?;
    let prefix_type = infer_expr(db, cache, prefix_expr).ok()?;
    let index_key = index_expr.get_index_key()?;
    let member_key = LuaMemberKey::from_index_key(db, cache, &index_key).ok()?;
    infer_member_semantic_decl_by_member_key(
        db,
        cache,
        &prefix_type,
        &member_key,
        semantic_guard.next_level()?,
    )
}

fn infer_closure_expr_semantic_decl(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    closure_expr: LuaClosureExpr,
    semantic_guard: SemanticDeclGuard,
    level: SemanticDeclLevel,
) -> Option<LuaSemanticDeclId> {
    let parent = closure_expr.get_parent::<LuaStat>()?;
    match parent {
        LuaStat::LocalFuncStat(local_func_stat) => {
            let local_name = local_func_stat.get_local_name()?;
            let name_token = local_name.get_name_token()?;
            infer_token_semantic_decl(db, cache, name_token.syntax().clone(), level)
        }
        LuaStat::FuncStat(func_stat) => {
            let func_name = func_stat.get_func_name()?;
            infer_expr_semantic_decl(
                db,
                cache,
                func_name.into(),
                semantic_guard.next_level()?,
                level,
            )
        }
        _ => None,
    }
}

fn infer_member_semantic_decl_by_member_key(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type: &LuaType,
    member_key: &LuaMemberKey,
    semantic_guard: SemanticDeclGuard,
) -> Option<LuaSemanticDeclId> {
    match &prefix_type {
        LuaType::TableConst(id) => {
            let owner = LuaMemberOwner::Element(id.clone());
            infer_table_member_semantic_decl(db, owner, member_key)
        }
        LuaType::String | LuaType::Io | LuaType::StringConst(_) | LuaType::DocStringConst(_) => {
            let decl_id = get_buildin_type_map_type_id(prefix_type)?;
            infer_custom_type_member_semantic_decl(
                db,
                cache,
                decl_id,
                member_key,
                semantic_guard.next_level()?,
            )
        }
        LuaType::Ref(decl_id) => infer_custom_type_member_semantic_decl(
            db,
            cache,
            decl_id.clone(),
            member_key,
            semantic_guard.next_level()?,
        ),
        LuaType::Def(decl_id) => infer_custom_type_member_semantic_decl(
            db,
            cache,
            decl_id.clone(),
            member_key,
            semantic_guard.next_level()?,
        ),
        LuaType::Union(union_type) => infer_union_member_semantic_info(
            db,
            cache,
            union_type,
            member_key,
            semantic_guard.next_level()?,
        ),
        LuaType::Generic(generic_type) => infer_custom_type_member_semantic_decl(
            db,
            cache,
            generic_type.get_base_type_id(),
            member_key,
            semantic_guard.next_level()?,
        ),
        LuaType::Instance(inst) => infer_instance_member_semantic_decl_by_member_key(
            db,
            cache,
            inst,
            member_key,
            semantic_guard.next_level()?,
        ),
        LuaType::Global => infer_global_member_semantic_decl_by_member_key(db, cache, member_key),
        LuaType::ModuleRef(file_id) => {
            let module_info = db.get_module_index().get_module(*file_id)?;
            if let Some(export_type) = &module_info.export_type {
                infer_member_semantic_decl_by_member_key(
                    db,
                    cache,
                    export_type,
                    member_key,
                    semantic_guard.next_level()?,
                )
            } else {
                None
            }
        }
        LuaType::Intersection(intersection_type) => infer_intersection_member_semantic_info(
            db,
            cache,
            intersection_type,
            member_key,
            semantic_guard.next_level()?,
        ),
        _ => None,
    }
}

fn infer_table_member_semantic_decl(
    db: &DbIndex,
    owner: LuaMemberOwner,
    member_key: &LuaMemberKey,
) -> Option<LuaSemanticDeclId> {
    let member_item = db.get_member_index().get_member_item(&owner, member_key)?;
    member_item.resolve_semantic_decl(db)
}

fn infer_custom_type_member_semantic_decl(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    prefix_type_id: LuaTypeDeclId,
    member_key: &LuaMemberKey,
    semantic_guard: SemanticDeclGuard,
) -> Option<LuaSemanticDeclId> {
    let type_index = db.get_type_index();
    let type_decl = type_index.get_type_decl(&prefix_type_id)?;
    if type_decl.is_alias() {
        if let Some(origin_type) = type_decl.get_alias_origin(db, None) {
            return infer_member_semantic_decl_by_member_key(
                db,
                cache,
                &origin_type,
                member_key,
                semantic_guard.next_level()?,
            );
        } else {
            return infer_member_semantic_decl_by_member_key(
                db,
                cache,
                &LuaType::String,
                member_key,
                semantic_guard.next_level()?,
            );
        }
    }

    let owner = LuaMemberOwner::Type(prefix_type_id.clone());
    if let Some(member_item) = db.get_member_index().get_member_item(&owner, member_key) {
        return member_item.resolve_semantic_decl(db);
    }

    if type_decl.is_class() {
        let super_types = type_index.get_super_types(&prefix_type_id)?;
        for super_type in super_types {
            if let Some(property) = infer_member_semantic_decl_by_member_key(
                db,
                cache,
                &super_type,
                member_key,
                semantic_guard.next_level()?,
            ) {
                return Some(property);
            }
        }
    }

    None
}

fn infer_union_member_semantic_info(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    union_type: &LuaUnionType,
    member_key: &LuaMemberKey,
    semantic_guard: SemanticDeclGuard,
) -> Option<LuaSemanticDeclId> {
    for typ in union_type.into_vec() {
        if let Some(property_owner_id) = infer_member_semantic_decl_by_member_key(
            db,
            cache,
            &typ,
            member_key,
            semantic_guard.next_level()?,
        ) {
            return Some(property_owner_id);
        }
    }

    None
}

fn infer_instance_member_semantic_decl_by_member_key(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    inst: &LuaInstanceType,
    member_key: &LuaMemberKey,
    semantic_guard: SemanticDeclGuard,
) -> Option<LuaSemanticDeclId> {
    let range = inst.get_range();

    let origin_type = inst.get_base();
    if let Some(result) = infer_member_semantic_decl_by_member_key(
        db,
        cache,
        origin_type,
        member_key,
        semantic_guard.next_level()?,
    ) {
        return Some(result);
    }

    let owner = LuaMemberOwner::Element(range.clone());
    infer_table_member_semantic_decl(db, owner, member_key)
}

fn infer_global_member_semantic_decl_by_member_key(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    member_key: &LuaMemberKey,
) -> Option<LuaSemanticDeclId> {
    let name = member_key.get_name()?;
    resolve_global_decl_id(db, cache, name, None).map(LuaSemanticDeclId::LuaDecl)
}

fn infer_intersection_member_semantic_info(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    intersection_type: &LuaIntersectionType,
    member_key: &LuaMemberKey,
    semantic_guard: SemanticDeclGuard,
) -> Option<LuaSemanticDeclId> {
    for typ in intersection_type.get_types() {
        if let Some(property_owner_id) = infer_member_semantic_decl_by_member_key(
            db,
            cache,
            typ,
            member_key,
            semantic_guard.next_level()?,
        ) {
            return Some(property_owner_id);
        }
    }

    None
}
