use std::ops::Deref;

use emmylua_parser::{LuaAstNode, LuaNameExpr};

use crate::{
    db_index::{DbIndex, LuaDeclOrMemberId, LuaMemberKey},
    LuaDecl, LuaDeclExtra, LuaFlowId, LuaMember, LuaMemberId, LuaType, TypeOps,
};

use super::{InferResult, LuaInferConfig};

pub fn infer_name_expr(
    db: &DbIndex,
    config: &mut LuaInferConfig,
    name_expr: LuaNameExpr,
) -> InferResult {
    let name_token = name_expr.get_name_token()?;
    let name = name_token.get_name_text();
    if name == "self" {
        return infer_self(db, config, name_expr);
    }

    let file_id = config.get_file_id();
    let references_index = db.get_reference_index();
    let range = name_expr.get_range();
    let file_ref = references_index.get_local_reference(&file_id)?;
    let decl_id = file_ref.get_decl_id(&range);
    if let Some(decl_id) = decl_id {
        let decl = db.get_decl_index().get_decl(&decl_id)?;
        let mut decl_type = if decl.is_global() {
            db.get_decl_index()
                .get_global_decl_type(&LuaMemberKey::Name(name.into()))?
                .clone()
        } else if let Some(typ) = decl.get_type() {
            typ.clone()
        } else if decl.is_param() {
            infer_param(db, decl).unwrap_or(LuaType::Unknown)
        } else {
            LuaType::Unknown
        };
        let flow_id = LuaFlowId::from_node(name_expr.syntax());
        let flow_chain = db.get_flow_index().get_flow_chain(file_id, flow_id);
        let root = name_expr.get_root();
        if let Some(flow_chain) = flow_chain {
            for type_assert in flow_chain.get_type_asserts(name, name_expr.get_position()) {
                decl_type = type_assert.tighten_type(db, config, &root, decl_type)?;
            }
        }

        if decl_type.is_unknown() {
            return None;
        }

        Some(decl_type)
    } else {
        let decl_type = db
            .get_decl_index()
            .get_global_decl_type(&LuaMemberKey::Name(name.into()))?
            .clone();
        Some(decl_type)
    }
}

fn infer_self(db: &DbIndex, config: &mut LuaInferConfig, name_expr: LuaNameExpr) -> InferResult {
    let file_id = config.get_file_id();
    let tree = db.get_decl_index().get_decl_tree(&file_id)?;
    let id = tree.find_self_decl(db, name_expr.clone())?;
    match id {
        LuaDeclOrMemberId::Decl(decl_id) => {
            let decl = db.get_decl_index().get_decl(&decl_id)?;
            let name = decl.get_name();
            let mut decl_type = if decl.is_global() {
                db.get_decl_index()
                    .get_global_decl_type(&LuaMemberKey::Name(name.into()))?
                    .clone()
            } else if let Some(typ) = decl.get_type() {
                typ.clone()
            } else if decl.is_param() {
                infer_param(db, decl).unwrap_or(LuaType::Unknown)
            } else {
                LuaType::Unknown
            };

            if let LuaType::Ref(id) = decl_type {
                decl_type = LuaType::Def(id);
            }

            let flow_id = LuaFlowId::from_node(name_expr.syntax());
            let flow_chain = db.get_flow_index().get_flow_chain(file_id, flow_id);
            let root = name_expr.get_root();
            if let Some(flow_chain) = flow_chain {
                for type_assert in flow_chain.get_type_asserts("self", name_expr.get_position()) {
                    decl_type = type_assert.tighten_type(db, config, &root, decl_type)?;
                }
            }

            Some(decl_type)
        }
        LuaDeclOrMemberId::Member(member_id) => {
            let member = db.get_member_index().get_member(&member_id)?;
            let ty = member.get_decl_type();
            if ty.is_unknown() {
                None
            } else {
                Some(ty.clone())
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
        _ => return None,
    };

    // find local annotation
    if let Some(signature) = db.get_signature_index().get(&signature_id) {
        if let Some(param_info) = signature.get_param_info_by_id(param_idx) {
            let mut typ = param_info.type_ref.clone();
            if param_info.nullable && !typ.is_nullable() {
                typ = LuaType::Nullable(typ.into());
            }

            return Some(typ);
        }
    }

    let current_member_id = member_id?;
    let member = find_decl_member_type(db, current_member_id)?;
    let member_decl_type = member.get_decl_type();
    let param_type = find_param_type_from_type(db, member_decl_type, param_idx);
    if let Some(param_type) = param_type {
        return Some(param_type);
    }

    // todo inherit from super
    None
}

fn find_decl_member_type(db: &DbIndex, member_id: LuaMemberId) -> Option<&LuaMember> {
    let member = db.get_member_index().get_member(&member_id)?;
    let owner = member.get_owner();
    let member = db
        .get_member_index()
        .get_member_from_owner(&owner, member.get_key())?;

    Some(member)
}

fn find_param_type_from_type(
    db: &DbIndex,
    source_type: LuaType,
    param_idx: usize,
) -> Option<LuaType> {
    match source_type {
        LuaType::Signature(signature_id) => {
            let signature = db.get_signature_index().get(&signature_id)?;
            if let Some(param_info) = signature.get_param_info_by_id(param_idx) {
                let mut typ = param_info.type_ref.clone();
                if param_info.nullable && !typ.is_nullable() {
                    typ = TypeOps::Union.apply(&typ, &LuaType::Nil);
                }

                return Some(typ);
            }
        }
        LuaType::DocFunction(f) => {
            if let Some((_, typ)) = f.get_params().get(param_idx) {
                return typ.clone();
            }
        }
        LuaType::Nullable(base) => {
            return find_param_type_from_type(db, base.deref().clone(), param_idx);
        }
        LuaType::Union(union_types) => {
            for ty in union_types.get_types() {
                if let Some(ty) = find_param_type_from_type(db, ty.clone(), param_idx) {
                    return Some(ty);
                }
            }
        }
        _ => {
            eprintln!("find_param_type_from_type: {:?}", source_type);
        }
    }

    None
}
