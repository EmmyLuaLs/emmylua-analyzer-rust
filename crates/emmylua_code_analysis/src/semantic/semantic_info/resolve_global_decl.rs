use std::sync::Arc;

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaNameExpr};

use crate::{
    DbIndex, LuaDeclId, LuaInferCache, LuaType, semantic::overload_resolve::resolve_signature,
};

pub fn resolve_global_decl_id(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    name: &str,
    name_expr: Option<&LuaNameExpr>,
) -> Option<LuaDeclId> {
    let decl_ids = db.get_global_index().get_global_decl_ids(name)?;
    if decl_ids.len() == 1 {
        return Some(decl_ids[0]);
    }

    if let Some(name_expr) = name_expr
        && let Some(call_expr) = name_expr.get_parent::<LuaCallExpr>()
    {
        return resolve_global_func_decl_id(db, cache, name, call_expr);
    }

    let mut last_valid_decl_id = None;
    for decl_id in decl_ids {
        let decl_type_cache = db.get_type_index().get_type_cache(&(*decl_id).into());
        if let Some(type_cache) = decl_type_cache {
            let typ = type_cache.as_type();
            if typ.is_def() || typ.is_ref() || typ.is_function() {
                return Some(*decl_id);
            }

            if type_cache.is_table() {
                last_valid_decl_id = Some(decl_id)
            }
        }
    }
    if last_valid_decl_id.is_none() && !decl_ids.is_empty() {
        return Some(decl_ids[0]);
    }

    last_valid_decl_id.cloned()
}

fn resolve_global_func_decl_id(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    name: &str,
    call_expr: LuaCallExpr,
) -> Option<LuaDeclId> {
    let decl_ids = db.get_global_index().get_global_decl_ids(name)?;
    let mut overload_signature = vec![];
    for decl_id in decl_ids {
        let decl_type_cache = db.get_type_index().get_type_cache(&(*decl_id).into());
        if let Some(type_cache) = decl_type_cache {
            let typ = type_cache.as_type();
            if typ.is_def() || typ.is_ref() || typ.is_table() {
                return Some(*decl_id);
            }

            if let LuaType::Signature(signature) = typ {
                let signature = db.get_signature_index().get(&signature)?;
                overload_signature.push((decl_id.clone(), signature.to_doc_func_type()));
            }
        }
    }

    let signature = resolve_signature(
        db,
        cache,
        overload_signature
            .iter()
            .map(|(_, doc_func)| doc_func.clone())
            .collect(),
        call_expr,
        false,
        None,
    );

    if let Ok(signature) = signature {
        for (decl_id, doc_func) in &overload_signature {
            if Arc::ptr_eq(&signature, doc_func) {
                return Some(decl_id.clone());
            }
        }
    }

    overload_signature.first().map(|(id, _)| id.clone())
}
