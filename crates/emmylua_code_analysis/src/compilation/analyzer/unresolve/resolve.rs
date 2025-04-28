use std::ops::Deref;

use emmylua_parser::{LuaAstNode, LuaExpr, LuaLocalStat, LuaTableExpr};

use crate::{
    compilation::analyzer::{
        bind_type::{add_member, bind_type},
        lua::{analyze_return_point, infer_for_range_iter_expr_func},
    },
    db_index::{DbIndex, LuaMemberOwner, LuaType},
    semantic::{infer_expr, LuaInferCache},
    InFiled, InferFailReason, LuaDeclId, LuaMember, LuaMemberId, LuaMemberKey, LuaSemanticDeclId,
    LuaTypeCache, SignatureReturnStatus, TypeOps,
};

use super::{
    check_reason::check_reach_reason, UnResolveDecl, UnResolveIterVar, UnResolveMember,
    UnResolveModule, UnResolveModuleRef, UnResolveReturn, UnResolveTableField,
};

pub fn try_resolve_decl(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    decl: &mut UnResolveDecl,
) -> Option<bool> {
    if !check_reach_reason(db, cache, &decl.reason).unwrap_or(false) {
        return None;
    }

    let expr = decl.expr.clone();
    let expr_type = match infer_expr(db, cache, expr) {
        Ok(t) => t,
        Err(reason) => {
            decl.reason = reason;
            return None;
        }
    };
    let decl_id = decl.decl_id;
    let expr_type = match &expr_type {
        LuaType::MuliReturn(multi) => multi
            .get_type(decl.ret_idx)
            .cloned()
            .unwrap_or(LuaType::Unknown),
        _ => expr_type,
    };

    bind_type(db, decl_id.into(), LuaTypeCache::InferType(expr_type));
    Some(true)
}

pub fn try_resolve_member(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    unresolve_member: &mut UnResolveMember,
) -> Option<bool> {
    if !check_reach_reason(db, cache, &unresolve_member.reason).unwrap_or(false) {
        return None;
    }

    if let Some(prefix_expr) = &unresolve_member.prefix {
        let prefix_type = match infer_expr(db, cache, prefix_expr.clone()) {
            Ok(t) => t,
            Err(reason) => {
                unresolve_member.reason = reason;
                return None;
            }
        };
        let member_owner = match prefix_type {
            LuaType::TableConst(in_file_range) => LuaMemberOwner::Element(in_file_range),
            LuaType::Def(def_id) => {
                let type_decl = db.get_type_index().get_type_decl(&def_id)?;
                // if is exact type, no need to extend field
                if type_decl.is_exact() {
                    return None;
                }
                LuaMemberOwner::Type(def_id)
            }
            LuaType::Instance(instance) => LuaMemberOwner::Element(instance.get_range().clone()),
            // is ref need extend field?
            _ => {
                return None;
            }
        };
        let member_id = unresolve_member.member_id.clone();
        add_member(db, member_owner, member_id);
        unresolve_member.prefix = None;
    }

    if let Some(expr) = unresolve_member.expr.clone() {
        let expr_type = match infer_expr(db, cache, expr) {
            Ok(t) => t,
            Err(reason) => {
                unresolve_member.reason = reason;
                return None;
            }
        };

        let expr_type = match &expr_type {
            LuaType::MuliReturn(multi) => multi
                .get_type(unresolve_member.ret_idx)
                .cloned()
                .unwrap_or(LuaType::Unknown),
            _ => expr_type,
        };

        let member_id = unresolve_member.member_id;
        bind_type(db, member_id.into(), LuaTypeCache::InferType(expr_type));
    }
    Some(true)
}

pub fn try_resolve_table_field(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    unresolve_table_field: &mut UnResolveTableField,
) -> Option<bool> {
    if !check_reach_reason(db, cache, &unresolve_table_field.reason).unwrap_or(false) {
        return None;
    }
    if !matches!(
        unresolve_table_field.reason,
        InferFailReason::UnResolveExpr(_)
    ) {
        return Some(true);
    }
    let field = unresolve_table_field.field.clone();
    let field_key = field.get_field_key()?;
    let field_expr = field_key.get_expr()?;
    let field_type = match infer_expr(db, cache, field_expr.clone()) {
        Ok(t) => t,
        Err(reason) => {
            unresolve_table_field.reason = reason;
            return None;
        }
    };
    let member_key: LuaMemberKey = match field_type {
        LuaType::StringConst(s) => LuaMemberKey::Name((*s).clone()),
        LuaType::IntegerConst(i) => LuaMemberKey::Integer(i),
        _ => {
            if field_type.is_table() {
                LuaMemberKey::Expr(field_type)
            } else {
                return None;
            }
        }
    };
    let file_id = unresolve_table_field.file_id;
    let table_expr = unresolve_table_field.table_expr.clone();
    let owner_id = LuaMemberOwner::Element(InFiled {
        file_id,
        value: table_expr.get_range(),
    });

    db.get_reference_index_mut().add_index_reference(
        member_key.clone(),
        file_id,
        field.get_syntax_id(),
    );

    let decl_type = field.get_value_expr().map_or(None, |expr| {
        let expr_type = infer_expr(db, cache, expr).ok()?;
        Some(expr_type)
    })?;

    let member_id = LuaMemberId::new(field.get_syntax_id(), file_id);
    let member = LuaMember::new(
        member_id,
        member_key,
        unresolve_table_field.decl_feature,
        None,
    );
    db.get_member_index_mut().add_member(owner_id, member);
    db.get_type_index_mut().bind_type(
        member_id.clone().into(),
        LuaTypeCache::InferType(decl_type.clone()),
    );

    merge_table_field_to_def(db, cache, table_expr, member_id);
    Some(true)
}

fn merge_table_field_to_def(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    table_expr: LuaTableExpr,
    member_id: LuaMemberId,
) -> Option<()> {
    let file_id = cache.get_file_id();
    let local_name = table_expr
        .get_parent::<LuaLocalStat>()?
        .get_value_local_name(LuaExpr::TableExpr(table_expr.clone()))?;
    let decl_id = LuaDeclId::new(file_id, local_name.get_position());
    let type_cache = db.get_type_index().get_type_cache(&decl_id.into())?;
    if let LuaType::Def(id) = type_cache.deref() {
        let owner = LuaMemberOwner::Type(id.clone());
        db.get_member_index_mut()
            .set_member_owner(owner.clone(), member_id.file_id, member_id);
        db.get_member_index_mut()
            .add_member_to_owner(owner.clone(), member_id);
    }

    Some(())
}

pub fn try_resolve_module(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    module: &mut UnResolveModule,
) -> Option<bool> {
    if !check_reach_reason(db, cache, &module.reason).unwrap_or(false) {
        return None;
    }

    let expr = module.expr.clone();
    let expr_type = match infer_expr(db, cache, expr) {
        Ok(t) => t,
        Err(reason) => {
            module.reason = reason;
            return None;
        }
    };

    let expr_type = match &expr_type {
        LuaType::MuliReturn(multi) => multi.get_type(0).cloned().unwrap_or(LuaType::Unknown),
        _ => expr_type,
    };
    let module_info = db.get_module_index_mut().get_module_mut(module.file_id)?;
    module_info.export_type = Some(expr_type);
    Some(true)
}

pub fn try_resolve_return_point(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    return_: &mut UnResolveReturn,
) -> Option<bool> {
    if !check_reach_reason(db, cache, &return_.reason).unwrap_or(false) {
        return None;
    }

    let return_docs = match analyze_return_point(&db, cache, &return_.return_points) {
        Ok(docs) => docs,
        Err(reason) => {
            return_.reason = reason;
            return None;
        }
    };

    let signature = db
        .get_signature_index_mut()
        .get_mut(&return_.signature_id)?;

    if signature.resolve_return == SignatureReturnStatus::UnResolve {
        signature.resolve_return = SignatureReturnStatus::InferResolve;
        signature.return_docs = return_docs;
    }
    Some(true)
}

pub fn try_resolve_iter_var(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    iter_var: &mut UnResolveIterVar,
) -> Option<bool> {
    if !check_reach_reason(db, cache, &iter_var.reason).unwrap_or(false) {
        return None;
    }

    let expr_type = match infer_expr(db, cache, iter_var.iter_expr.clone()) {
        Ok(t) => t,
        Err(InferFailReason::None) => return Some(true),
        Err(reason) => {
            iter_var.reason = reason;
            return None;
        }
    };
    let func = match infer_for_range_iter_expr_func(db, expr_type.clone()) {
        Some(func) => func,
        None => {
            return Some(true);
        }
    };

    let multi_return = func.get_multi_return();
    let mut iter_type = multi_return
        .get_type(iter_var.ret_idx)
        .cloned()
        .unwrap_or(LuaType::Unknown);
    iter_type = TypeOps::Remove.apply(db, &iter_type, &LuaType::Nil);
    let decl_id = iter_var.decl_id;
    bind_type(
        db,
        decl_id.into(),
        LuaTypeCache::InferType(iter_type.clone()),
    );
    Some(true)
}

pub fn try_resolve_module_ref(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    module_ref: &UnResolveModuleRef,
) -> Option<bool> {
    if !check_reach_reason(db, cache, &module_ref.reason).unwrap_or(false) {
        return None;
    }

    let module_index = db.get_module_index();
    let module = module_index.get_module(module_ref.module_file_id)?;
    let export_type = module.export_type.clone()?;
    match &module_ref.owner_id {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            db.get_type_index_mut()
                .bind_type(decl_id.clone().into(), LuaTypeCache::InferType(export_type));
        }
        LuaSemanticDeclId::Member(member_id) => {
            db.get_type_index_mut().bind_type(
                member_id.clone().into(),
                LuaTypeCache::InferType(export_type),
            );
        }
        _ => {}
    }

    Some(true)
}
