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
    let call_expr = name_expr.and_then(|name_expr| name_expr.get_parent::<LuaCallExpr>());
    let globals = globals(db, name);
    let first_decl = globals
        .decls
        .first()
        .map(|candidate| candidate.decl.decl_id);
    if globals.decls.len() == 1 {
        return Some(globals.decls[0].decl.decl_id);
    }

    if let Some(call_expr) = call_expr
        && let Some(decl_id) = resolve_global_call(db, cache, &globals, call_expr)
    {
        return Some(decl_id);
    }

    let mut last_table = None;
    for candidate in globals.decls {
        let Some(typ) = candidate.typ else {
            continue;
        };

        if typ.is_def() || typ.is_ref() || typ.is_function() {
            return Some(candidate.decl.decl_id);
        }

        if matches!(
            typ,
            LuaType::Table | LuaType::Object(_) | LuaType::TableConst(_)
        ) {
            last_table = Some(candidate.decl.decl_id);
        }
    }

    last_table.or(first_decl)
}

fn resolve_global_call(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    globals: &crate::compilation::CompilationGlobals,
    call_expr: LuaCallExpr,
) -> Option<LuaDeclId> {
    let mut overloads = Vec::new();

    for candidate in &globals.decls {
        let Some(typ) = &candidate.typ else {
            continue;
        };

        if typ.is_def()
            || typ.is_ref()
            || matches!(
                typ,
                LuaType::Table | LuaType::Object(_) | LuaType::TableConst(_)
            )
        {
            return Some(candidate.decl.decl_id);
        }

        if let LuaType::DocFunction(doc_func) = typ {
            overloads.push((candidate.decl.decl_id, doc_func.clone()));
        }
    }

    for function in &globals.functions {
        if let (Some(decl_id), Some(doc_func)) = (function.decl_id, function.typ.clone()) {
            overloads.push((decl_id, doc_func));
        }
    }

    let signature = resolve_signature(
        db,
        cache,
        overloads
            .iter()
            .map(|(_, doc_func)| doc_func.clone())
            .collect(),
        call_expr,
        false,
        None,
    )
    .ok()?;

    overloads
        .into_iter()
        .find_map(|(decl_id, doc_func)| Arc::ptr_eq(&signature, &doc_func).then_some(decl_id))
}
