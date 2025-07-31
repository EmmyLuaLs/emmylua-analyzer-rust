use std::collections::HashMap;

use emmylua_parser::LuaAstNode;

use crate::{
    DbIndex, InFiled, InferFailReason, LuaDocReturnInfo, LuaSemanticDeclId, LuaType, LuaTypeCache,
    SignatureReturnStatus, compilation::analyzer::infer_cache_manager::InferCacheManager,
    infer_expr, infer_param,
};

use super::UnResolve;

pub fn check_reach_reason(
    db: &DbIndex,
    infer_manager: &mut InferCacheManager,
    reason: &InferFailReason,
) -> Option<bool> {
    match reason {
        InferFailReason::None
        | InferFailReason::FieldNotFound
        | InferFailReason::RecursiveInfer => Some(true),
        InferFailReason::UnResolveDeclType(decl_id) => {
            let decl = db.get_decl_index().get_decl(decl_id)?;
            let typ = db.get_type_index().get_type_cache(&decl_id.clone().into());
            if typ.is_none() && decl.is_param() {
                return Some(infer_param(db, decl).is_ok());
            }

            Some(typ.is_some())
        }
        InferFailReason::UnResolveMemberType(member_id) => {
            let member = db.get_member_index().get_member(member_id)?;
            let key = member.get_key();
            let owner = db.get_member_index().get_current_owner(member_id)?;
            let member_item = db.get_member_index().get_member_item(&owner, key)?;
            Some(member_item.resolve_type(db).is_ok())
        }
        InferFailReason::UnResolveExpr(expr) => {
            let cache = infer_manager.get_infer_cache(expr.file_id);
            Some(infer_expr(db, cache, expr.value.clone()).is_ok())
        }
        InferFailReason::UnResolveSignatureReturn(signature_id) => {
            let signature = db.get_signature_index().get(signature_id)?;
            Some(signature.is_resolve_return())
        }
    }
}

pub fn resolve_all_reason(
    db: &mut DbIndex,
    reason_unresolves: &mut HashMap<InferFailReason, Vec<UnResolve>>,
    loop_count: usize,
) {
    for (reason, _) in reason_unresolves.iter_mut() {
        resolve_as_any(db, reason, loop_count);
    }
}

pub fn resolve_as_any(db: &mut DbIndex, reason: &InferFailReason, loop_count: usize) -> Option<()> {
    match reason {
        InferFailReason::None
        | InferFailReason::FieldNotFound
        | InferFailReason::RecursiveInfer => {
            return Some(());
        }
        InferFailReason::UnResolveDeclType(decl_id) => {
            db.get_type_index_mut().bind_type(
                decl_id.clone().into(),
                LuaTypeCache::InferType(LuaType::Any),
            );
        }
        InferFailReason::UnResolveMemberType(member_id) => {
            // 第一次循环不处理, 或许需要判断`unresolves`是否全为取值再跳过?
            if loop_count == 0 {
                return Some(());
            }
            let member = db.get_member_index().get_member(member_id)?;
            let key = member.get_key();
            let owner = db.get_member_index().get_current_owner(&member_id)?;
            let member_item = db.get_member_index().get_member_item(&owner, key)?;
            let opt_type = member_item.resolve_type(db).ok();
            if opt_type.is_none() {
                let semantic_member_id = member_item.resolve_semantic_decl(db)?;
                if let LuaSemanticDeclId::Member(member_id) = semantic_member_id {
                    db.get_type_index_mut()
                        .bind_type(member_id.into(), LuaTypeCache::InferType(LuaType::Any));
                }
            }
        }
        InferFailReason::UnResolveExpr(expr) => {
            let key = InFiled::new(expr.file_id, expr.value.get_syntax_id());
            db.get_type_index_mut()
                .bind_type(key.into(), LuaTypeCache::InferType(LuaType::Any));
        }
        InferFailReason::UnResolveSignatureReturn(signature_id) => {
            let signature = db.get_signature_index_mut().get_mut(signature_id)?;
            if !signature.is_resolve_return() {
                signature.return_docs = vec![LuaDocReturnInfo {
                    name: None,
                    type_ref: LuaType::Any,
                    description: None,
                }];
                signature.resolve_return = SignatureReturnStatus::InferResolve;
            }
        }
    }

    Some(())
}
