use emmylua_parser::{LuaAstNode, LuaNameExpr};

use crate::{
    db_index::{DbIndex, LuaDeclOrMemberId},
    LuaDecl, LuaDeclExtra, LuaFlowId, LuaInferCache, LuaMemberId, LuaType, TypeOps,
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
        let flow_id = LuaFlowId::from_node(name_expr.syntax());
        let flow_chain = db.get_flow_index().get_flow_chain(file_id, flow_id);
        let root = name_expr.get_root();
        if let Some(flow_chain) = flow_chain {
            for type_assert in
                flow_chain.get_type_asserts(name, name_expr.get_position(), Some(decl_id.position))
            {
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

    if let Some(typ) = decl.get_type() {
        return Ok(typ.clone());
    }

    if decl.is_param() {
        return infer_param(db, decl);
    }

    Err(InferFailReason::UnResolveDeclType(decl.get_id()))
}

fn infer_self(db: &DbIndex, cache: &mut LuaInferCache, name_expr: LuaNameExpr) -> InferResult {
    let file_id = cache.get_file_id();
    let tree = db
        .get_decl_index()
        .get_decl_tree(&file_id)
        .ok_or(InferFailReason::None)?;
    let id = tree
        .find_self_decl(db, name_expr.clone())
        .ok_or(InferFailReason::None)?;
    match id {
        LuaDeclOrMemberId::Decl(decl_id) => {
            let decl = db
                .get_decl_index()
                .get_decl(&decl_id)
                .ok_or(InferFailReason::None)?;
            let mut decl_type = get_decl_type(db, decl)?;
            if let LuaType::Ref(id) = decl_type {
                decl_type = LuaType::Def(id);
            }

            let flow_id = LuaFlowId::from_node(name_expr.syntax());
            let flow_chain = db.get_flow_index().get_flow_chain(file_id, flow_id);
            let root = name_expr.get_root();
            if let Some(flow_chain) = flow_chain {
                for type_assert in flow_chain.get_type_asserts(
                    "self",
                    name_expr.get_position(),
                    Some(decl_id.position),
                ) {
                    decl_type = type_assert.tighten_type(db, cache, &root, decl_type)?;
                }
            }

            Ok(decl_type)
        }
        LuaDeclOrMemberId::Member(member_id) => {
            let member = db
                .get_member_index()
                .get_member(&member_id)
                .ok_or(InferFailReason::None)?;
            let typ = member.get_option_decl_type();
            match typ {
                Some(typ) => Ok(typ),
                None => Err(InferFailReason::UnResolveMemberType(member.get_id())),
            }
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
    let member = db
        .get_member_index()
        .get_member(&member_id)
        .ok_or(InferFailReason::None)?;
    let key = member.get_key();
    let owner = member.get_owner();
    let member_item = db
        .get_member_index()
        .get_member_item(&owner, key)
        .ok_or(InferFailReason::None)?;
    member_item.resolve_type(db)
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
        LuaType::Union(union_types) => {
            for ty in union_types.get_types() {
                if let Some(ty) =
                    find_param_type_from_type(db, ty.clone(), param_idx, current_colon_define)
                {
                    return Some(ty);
                }
            }
        }
        _ => {}
    }

    None
}

fn infer_global_type(db: &DbIndex, name: &str) -> InferResult {
    let decl_index = db.get_decl_index();
    let decls = decl_index.get_global_decls_by_name(name);
    if decls.len() == 1 {
        let decl = decl_index
            .get_decl(&decls[0])
            .ok_or(InferFailReason::None)?;
        return match decl.get_type() {
            Some(typ) => Ok(typ.clone()),
            None => Err(InferFailReason::UnResolveDeclType(decl.get_id())),
        };
    }

    let mut valid_type = LuaType::Unknown;
    let mut last_resolve_reason = InferFailReason::None;
    for decl_id in decls {
        let decl = decl_index.get_decl(&decl_id).ok_or(InferFailReason::None)?;
        match decl.get_type() {
            Some(typ) => {
                if typ.is_def() || typ.is_ref() || typ.is_function() {
                    return Ok(typ.clone());
                }

                if typ.is_table() {
                    valid_type = typ.clone();
                }
            }
            None => {
                last_resolve_reason = InferFailReason::UnResolveDeclType(decl.get_id());
            }
        }
    }

    if valid_type.is_unknown() && last_resolve_reason != InferFailReason::None {
        return Ok(valid_type);
    }

    Err(last_resolve_reason)
}
