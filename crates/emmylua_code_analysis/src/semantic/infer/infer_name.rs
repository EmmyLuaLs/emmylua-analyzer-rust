use emmylua_parser::{LuaAstNode, LuaNameExpr};
use smol_str::SmolStr;

use crate::{
    db_index::{DbIndex, LuaDeclOrMemberId},
    LuaDecl, LuaDeclExtra, LuaFlowId, LuaInferCache, LuaMemberId, LuaType, TypeOps, VarRefId,
};

use super::{InferFailReason, InferResult};

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
    let references_index = db.get_reference_index();
    let range = name_expr.get_range();
    let file_ref = references_index
        .get_local_reference(&file_id)
        .ok_or(InferFailReason::None)?;
    let decl_id = file_ref.get_decl_id(&range);
    if let Some(decl_id) = decl_id {
        let decl = db
            .get_decl_index()
            .get_decl(&decl_id)
            .ok_or(InferFailReason::None)?;
        let mut decl_type = get_decl_type(db, decl)?;
        let var_ref_id = VarRefId::DeclId(decl_id);
        let flow_chain = db.get_flow_index().get_flow_chain(file_id, var_ref_id);
        let root = name_expr.get_root();
        if let Some(flow_chain) = flow_chain {
            let flow_id = LuaFlowId::from_node(name_expr.syntax());
            for type_assert in flow_chain.get_type_asserts(name_expr.get_position(), flow_id) {
                decl_type = type_assert.tighten_type(db, cache, &root, decl_type)?;
            }
        }
        Ok(decl_type)
    } else {
        infer_global_type(db, name)
    }
}

fn get_decl_type(db: &DbIndex, decl: &LuaDecl) -> InferResult {
    if decl.is_global() {
        let name = decl.get_name();
        return infer_global_type(db, name);
    }

    if let Some(type_cache) = db.get_type_index().get_type_cache(&decl.get_id().into()) {
        return Ok(type_cache.as_type().clone());
    }

    if decl.is_param() {
        return infer_param(db, decl);
    }

    Err(InferFailReason::UnResolveDeclType(decl.get_id()))
}

fn infer_self(db: &DbIndex, cache: &mut LuaInferCache, name_expr: LuaNameExpr) -> InferResult {
    let file_id = cache.get_file_id();
    let semantic_id =
        find_self_decl_or_member_id(db, &cache, &name_expr).ok_or(InferFailReason::None)?;
    match semantic_id {
        LuaDeclOrMemberId::Decl(decl_id) => {
            let decl = db
                .get_decl_index()
                .get_decl(&decl_id)
                .ok_or(InferFailReason::None)?;
            let mut decl_type = get_decl_type(db, decl)?;
            if let LuaType::Ref(id) = decl_type {
                decl_type = LuaType::Def(id);
            }

            // let flow_id = LuaFlowId::from_node(name_expr.syntax());
            let var_ref_id = VarRefId::Name(SmolStr::new("self"));
            let flow_chain = db.get_flow_index().get_flow_chain(file_id, var_ref_id);
            let root = name_expr.get_root();
            if let Some(flow_chain) = flow_chain {
                let flow_id = LuaFlowId::from_node(name_expr.syntax());
                for type_assert in flow_chain.get_type_asserts(name_expr.get_position(), flow_id) {
                    decl_type = type_assert.tighten_type(db, cache, &root, decl_type)?;
                }
            }

            Ok(decl_type)
        }
        LuaDeclOrMemberId::Member(member_id) => find_decl_member_type(db, member_id),
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
    if let Some(signature) = db.get_signature_index().get(&signature_id) {
        colon_define = signature.is_colon_define;
        if let Some(param_info) = signature.get_param_info_by_id(param_idx) {
            let mut typ = param_info.type_ref.clone();
            if param_info.nullable && !typ.is_nullable() {
                typ = TypeOps::Union.apply(&typ, &LuaType::Nil);
            }

            return Ok(typ);
        }
    }

    if let Some(current_member_id) = member_id {
        let member_decl_type = find_decl_member_type(db, current_member_id)?;
        let param_type = find_param_type_from_type(db, member_decl_type, param_idx, colon_define);
        if let Some(param_type) = param_type {
            return Ok(param_type);
        }
    }

    Err(InferFailReason::UnResolveDeclType(decl.get_id()))
}

fn find_decl_member_type(db: &DbIndex, member_id: LuaMemberId) -> InferResult {
    let item = db
        .get_member_index()
        .get_member_item_by_member_id(member_id)
        .ok_or(InferFailReason::None)?;
    item.resolve_type(db)
}

fn find_param_type_from_type(
    db: &DbIndex,
    source_type: LuaType,
    param_idx: usize,
    current_colon_define: bool,
) -> Option<LuaType> {
    match source_type {
        LuaType::Signature(signature_id) => {
            let signature = db.get_signature_index().get(&signature_id)?;
            let decl_colon_defined = signature.is_colon_define;
            let mut param_idx = param_idx;
            match (current_colon_define, decl_colon_defined) {
                (true, false) => {
                    param_idx += 1;
                }
                (false, true) => {
                    if param_idx > 0 {
                        param_idx -= 1;
                    }
                }
                _ => {}
            }

            if let Some(param_info) = signature.get_param_info_by_id(param_idx) {
                let mut typ = param_info.type_ref.clone();
                if param_info.nullable && !typ.is_nullable() {
                    typ = TypeOps::Union.apply(&typ, &LuaType::Nil);
                }

                return Some(typ);
            }
        }
        LuaType::DocFunction(f) => {
            let mut param_idx = param_idx;
            let decl_colon_defined = f.is_colon_define();
            match (current_colon_define, decl_colon_defined) {
                (true, false) => {
                    param_idx += 1;
                }
                (false, true) => {
                    if param_idx > 0 {
                        param_idx -= 1;
                    }
                }
                _ => {}
            }

            if let Some((_, typ)) = f.get_params().get(param_idx) {
                return typ.clone();
            }
        }
        LuaType::Union(_) => {
            return find_param_type_from_union(db, source_type, param_idx, current_colon_define)
        }
        _ => {}
    }

    None
}

fn find_param_type_from_union(
    db: &DbIndex,
    source_type: LuaType,
    param_idx: usize,
    origin_colon_define: bool,
) -> Option<LuaType> {
    match source_type {
        LuaType::Signature(signature_id) => {
            let signature = db.get_signature_index().get(&signature_id)?;
            if !signature.param_docs.is_empty() {
                return None;
            }
            let mut final_type = None;
            for overload in &signature.overloads {
                let mut param_idx = param_idx;
                match (origin_colon_define, overload.is_colon_define()) {
                    (true, false) => {
                        param_idx += 1;
                    }
                    (false, true) => {
                        if param_idx > 0 {
                            param_idx -= 1;
                        }
                    }
                    _ => {}
                }

                if let Some((_, typ)) = overload.get_params().get(param_idx) {
                    if let Some(typ) = typ {
                        final_type = match final_type {
                            Some(existing) => Some(TypeOps::Union.apply(&existing, typ)),
                            None => Some(typ.clone()),
                        };
                    }
                }
            }
            final_type
        }
        LuaType::DocFunction(f) => {
            let mut param_idx = param_idx;
            match (origin_colon_define, f.is_colon_define()) {
                (true, false) => {
                    param_idx += 1;
                }
                (false, true) => {
                    if param_idx > 0 {
                        param_idx -= 1;
                    }
                }
                _ => {}
            }

            if let Some((_, typ)) = f.get_params().get(param_idx) {
                return typ.clone();
            }
            None
        }
        LuaType::Union(union_types) => {
            let mut final_type = None;
            for ty in union_types.get_types() {
                if let Some(ty) =
                    find_param_type_from_union(db, ty.clone(), param_idx, origin_colon_define)
                {
                    final_type = match final_type {
                        Some(existing) => Some(TypeOps::Union.apply(&existing, &ty)),
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
    let decl_ids = db
        .get_global_index()
        .get_global_decl_ids(name)
        .ok_or(InferFailReason::None)?;
    if decl_ids.len() == 1 {
        let id = decl_ids[0];
        return match db.get_type_index().get_type_cache(&id.into()) {
            Some(type_cache) => Ok(type_cache.as_type().clone()),
            None => Err(InferFailReason::UnResolveDeclType(id)),
        };
    }

    let mut valid_type = LuaType::Unknown;
    let mut last_resolve_reason = InferFailReason::None;
    for decl_id in decl_ids {
        let decl_type_cache = db.get_type_index().get_type_cache(&decl_id.clone().into());
        match decl_type_cache {
            Some(type_cache) => {
                let typ = type_cache.as_type();
                if typ.is_def() || typ.is_ref() || typ.is_function() {
                    return Ok(typ.clone());
                }

                if type_cache.is_table() {
                    valid_type = typ.clone();
                }
            }
            None => {
                last_resolve_reason = InferFailReason::UnResolveDeclType(*decl_id);
            }
        }
    }

    if !valid_type.is_unknown() {
        return Ok(valid_type);
    }

    Err(last_resolve_reason)
}

pub fn find_self_decl_or_member_id(
    db: &DbIndex,
    cache: &LuaInferCache,
    name_expr: &LuaNameExpr,
) -> Option<LuaDeclOrMemberId> {
    let file_id = cache.get_file_id();
    let tree = db.get_decl_index().get_decl_tree(&file_id)?;

    tree.find_self_decl(db, name_expr.clone())
}
