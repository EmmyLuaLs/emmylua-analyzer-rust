use std::sync::Arc;

use emmylua_parser::{LuaAstToken, LuaForRangeStat};

use crate::{
    compilation::analyzer::unresolve::UnResolveIterVar, DbIndex, InferFailReason, LuaDeclId,
    LuaFunctionType, LuaOperatorMetaMethod, LuaType, LuaTypeCache, TypeOps,
};

use super::LuaAnalyzer;

pub fn analyze_for_range_stat(
    analyzer: &mut LuaAnalyzer,
    for_range_stat: LuaForRangeStat,
) -> Option<()> {
    let var_name_list = for_range_stat.get_var_name_list();
    let first_iter_expr = for_range_stat.get_expr_list().next()?;
    let first_iter_type = analyzer.infer_expr(&first_iter_expr);

    match first_iter_type {
        Ok(first_iter_type) => {
            let iter_doc_func = infer_for_range_iter_expr_func(analyzer.db, first_iter_type);

            if let Some(doc_func) = iter_doc_func {
                let multi_return = doc_func.get_multi_return();
                let mut idx = 0;
                for var_name in var_name_list {
                    let position = var_name.get_position();
                    let decl_id = LuaDeclId::new(analyzer.file_id, position);
                    let ret_type = multi_return
                        .get_type(idx)
                        .cloned()
                        .unwrap_or(LuaType::Unknown);
                    let ret_type = TypeOps::Remove.apply(analyzer.db, &ret_type, &LuaType::Nil);
                    analyzer
                        .db
                        .get_type_index_mut()
                        .bind_type(decl_id.into(), LuaTypeCache::InferType(ret_type));
                    idx += 1;
                }
                return Some(());
            } else {
                for var_name in var_name_list {
                    let position = var_name.get_position();
                    let decl_id = LuaDeclId::new(analyzer.file_id, position);
                    analyzer
                        .db
                        .get_type_index_mut()
                        .bind_type(decl_id.into(), LuaTypeCache::InferType(LuaType::Unknown));
                }
                return Some(());
            }
        }
        Err(InferFailReason::None) => {
            for var_name in var_name_list {
                let position = var_name.get_position();
                let decl_id = LuaDeclId::new(analyzer.file_id, position);
                analyzer
                    .db
                    .get_type_index_mut()
                    .bind_type(decl_id.into(), LuaTypeCache::InferType(LuaType::Unknown));
            }
            return Some(());
        }
        Err(reason) => {
            let mut idx = 0;
            for var_name in var_name_list {
                let position = var_name.get_position();
                let decl_id = LuaDeclId::new(analyzer.file_id, position);
                let unresolved = UnResolveIterVar {
                    file_id: analyzer.file_id,
                    decl_id,
                    iter_expr: first_iter_expr.clone(),
                    ret_idx: idx,
                    reason: reason.clone(),
                };
                analyzer.add_unresolved(unresolved.into());
                idx += 1;
            }
        }
    }

    Some(())
}

pub fn infer_for_range_iter_expr_func(
    db: &mut DbIndex,
    iter_expr_type: LuaType,
) -> Option<Arc<LuaFunctionType>> {
    match iter_expr_type {
        LuaType::DocFunction(func) => Some(func),
        LuaType::Ref(type_decl_id) => {
            let type_decl = db.get_type_index().get_type_decl(&type_decl_id)?;
            if type_decl.is_alias() {
                let alias_origin = type_decl.get_alias_origin(db, None)?;
                match alias_origin {
                    LuaType::DocFunction(doc_func) => Some(doc_func),
                    _ => None,
                }
            } else if type_decl.is_class() {
                let operator_index = db.get_operator_index();
                let operator_ids = operator_index
                    .get_operators(&type_decl_id.into(), LuaOperatorMetaMethod::Call)?;
                operator_ids
                    .iter()
                    .filter_map(|overload_id| {
                        let operator = operator_index.get_operator(overload_id)?;
                        let func = operator.get_operator_func();
                        match func {
                            LuaType::DocFunction(f) => {
                                return Some(f.clone());
                            }
                            _ => None,
                        }
                    })
                    .nth(0)
            } else {
                None
            }
        }
        _ => None,
    }
}
