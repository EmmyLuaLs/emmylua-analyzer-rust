use emmylua_parser::{LuaAstNode, LuaNameExpr};

use crate::{
    db_index::{DbIndex, LuaDeclOrMemberId, LuaMemberKey},
    LuaDeclExtra, LuaFlowId, LuaType,
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
            match &decl.extra {
                LuaDeclExtra::Param { idx, signature_id } => {
                    let signature = db.get_signature_index().get(&signature_id)?;
                    if let Some(param_info) = signature.get_param_info_by_id(*idx) {
                        let mut typ = param_info.type_ref.clone();
                        if param_info.nullable && !typ.is_nullable() {
                            typ = LuaType::Nullable(typ.into());
                        }

                        typ
                    } else {
                        LuaType::Unknown
                    }
                }
                _ => unreachable!(),
            }
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
                match &decl.extra {
                    LuaDeclExtra::Param { idx, signature_id } => {
                        let signature = db.get_signature_index().get(&signature_id)?;
                        if let Some(param_info) = signature.get_param_info_by_id(*idx) {
                            let mut typ = param_info.type_ref.clone();
                            if param_info.nullable && !typ.is_nullable() {
                                typ = LuaType::Nullable(typ.into());
                            }

                            typ
                        } else {
                            LuaType::Unknown
                        }
                    }
                    _ => unreachable!(),
                }
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
