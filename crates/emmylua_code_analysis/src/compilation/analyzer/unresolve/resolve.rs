use crate::{
    compilation::analyzer::lua::LuaReturnPoint,
    db_index::{DbIndex, LuaDocReturnInfo, LuaMemberOwner, LuaType},
    semantic::{infer_expr, LuaInferConfig},
    SignatureReturnStatus,
};

use super::{
    merge_decl_expr_type, merge_member_type, UnResolveDecl, UnResolveIterVar, UnResolveMember,
    UnResolveModule, UnResolveReturn,
};

pub fn try_resolve_decl(
    db: &mut DbIndex,
    config: &mut LuaInferConfig,
    decl: &UnResolveDecl,
) -> Option<bool> {
    let expr = decl.expr.clone();
    let expr_type = infer_expr(db, config, expr)?;
    let decl_id = decl.decl_id;
    let expr_type = match &expr_type {
        LuaType::MuliReturn(multi) => multi
            .get_type(decl.ret_idx)
            .cloned()
            .unwrap_or(LuaType::Unknown),
        _ => expr_type,
    };

    merge_decl_expr_type(db, decl_id, expr_type);
    Some(true)
}

pub fn try_resolve_member(
    db: &mut DbIndex,
    config: &mut LuaInferConfig,
    unresolve_member: &mut UnResolveMember,
) -> Option<bool> {
    if let Some(prefix_expr) = &unresolve_member.prefix {
        let prefix_type = infer_expr(db, config, prefix_expr.clone())?;
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
        db.get_member_index_mut()
            .add_member_owner(member_owner.clone(), member_id);
        db.get_member_index_mut()
            .add_member_to_owner(member_owner, member_id);
        unresolve_member.prefix = None;
    }

    let expr = unresolve_member.expr.clone();
    let expr_type = infer_expr(db, config, expr)?;
    let expr_type = match &expr_type {
        LuaType::MuliReturn(multi) => multi
            .get_type(unresolve_member.ret_idx)
            .cloned()
            .unwrap_or(LuaType::Unknown),
        _ => expr_type,
    };

    let member_id = unresolve_member.member_id;
    merge_member_type(db, member_id, expr_type);
    Some(true)
}

pub fn try_resolve_module(
    db: &mut DbIndex,
    config: &mut LuaInferConfig,
    module: &UnResolveModule,
) -> Option<bool> {
    let expr = module.expr.clone();
    let expr_type = infer_expr(db, config, expr)?;
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
    config: &mut LuaInferConfig,
    return_: &UnResolveReturn,
) -> Option<bool> {
    let mut is_nullable = false;
    let mut return_docs = Vec::new();
    for return_point in &return_.return_points {
        match return_point {
            LuaReturnPoint::Expr(expr) => {
                let expr_type = infer_expr(db, config, expr.clone())?;
                if return_docs.is_empty() {
                    return_docs.push(LuaDocReturnInfo {
                        name: None,
                        type_ref: expr_type,
                        description: None,
                    });
                } else {
                    let last = return_docs.first_mut()?;
                    last.type_ref = expr_type;
                }
            }
            LuaReturnPoint::MuliExpr(exprs) => {
                if return_docs.is_empty() {
                    for expr in exprs {
                        let expr_type = infer_expr(db, config, expr.clone())?;

                        return_docs.push(LuaDocReturnInfo {
                            name: None,
                            type_ref: expr_type,
                            description: None,
                        });
                    }
                }
            }
            LuaReturnPoint::Nil => {
                is_nullable = true;
            }
            LuaReturnPoint::Error => {}
        }
    }

    if is_nullable {
        for doc in &mut return_docs {
            if !doc.type_ref.is_nullable() {
                doc.type_ref = LuaType::Nullable(doc.type_ref.clone().into());
            }
        }
    }

    let signature = db
        .get_signature_index_mut()
        .get_mut(&return_.signature_id)?;
    signature.resolve_return = SignatureReturnStatus::InferResolve;
    signature.return_docs = return_docs;
    Some(true)
}

pub fn try_resolve_iter_var(
    db: &mut DbIndex,
    config: &mut LuaInferConfig,
    iter_var: &UnResolveIterVar,
) -> Option<bool> {
    let expr_type = infer_expr(db, config, iter_var.iter_expr.clone())?;
    let func = match expr_type {
        LuaType::DocFunction(func) => func,
        _ => return Some(true),
    };

    let iter_type = func
        .get_ret()
        .get(iter_var.ret_idx)
        .unwrap_or(&LuaType::Nil);
    let decl_id = iter_var.decl_id;
    merge_decl_expr_type(db, decl_id, iter_type.clone());
    Some(true)
}
