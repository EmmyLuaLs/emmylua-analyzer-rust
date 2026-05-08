use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallArgList, LuaCallExpr, LuaClosureExpr, LuaExpr, LuaFuncStat,
    LuaVarExpr,
};

use crate::{
    DbIndex, InferFailReason, LuaInferCache, LuaType, SignatureReturnStatus, TypeOps,
    compilation::analyzer::unresolve::{
        UnResolveCallClosureParams, UnResolveClosureReturn, UnResolveParentAst,
        UnResolveParentClosureParams, UnResolveReturn,
    },
    db_index::{LuaDocReturnInfo, LuaSignatureId, return_row::merge_return_rows_with},
    infer_expr,
    semantic::infer_return_expr_list_types,
};

use super::{LuaAnalyzer, LuaReturnPoint, analyze_func_body_returns_with};

pub fn analyze_closure(analyzer: &mut LuaAnalyzer, closure: LuaClosureExpr) -> Option<()> {
    let signature_id = LuaSignatureId::from_closure(analyzer.file_id, &closure);

    analyze_colon_define(analyzer, &signature_id, &closure);
    analyze_lambda_params(analyzer, &signature_id, &closure);
    analyze_return(analyzer, &signature_id, &closure);
    Some(())
}

fn analyze_colon_define(
    analyzer: &mut LuaAnalyzer,
    signature_id: &LuaSignatureId,
    closure: &LuaClosureExpr,
) -> Option<()> {
    let signature = analyzer
        .db
        .get_signature_index_mut()
        .get_or_create(*signature_id);

    let func_stat = closure.get_parent::<LuaFuncStat>()?;
    let func_name = func_stat.get_func_name()?;
    if let LuaVarExpr::IndexExpr(index_expr) = func_name {
        let index_token = index_expr.get_index_token()?;
        signature.is_colon_define = index_token.is_colon();
    }

    Some(())
}

fn analyze_lambda_params(
    analyzer: &mut LuaAnalyzer,
    signature_id: &LuaSignatureId,
    closure: &LuaClosureExpr,
) -> Option<()> {
    let ast_node = closure.get_parent::<LuaAst>()?;
    match ast_node {
        LuaAst::LuaCallArgList(call_arg_list) => {
            let call_expr = call_arg_list.get_parent::<LuaCallExpr>()?;
            let pos = closure.get_position();
            let founded_idx = call_arg_list
                .get_args()
                .position(|arg| arg.get_position() == pos)?;

            let unresolved = UnResolveCallClosureParams {
                file_id: analyzer.file_id,
                signature_id: *signature_id,
                call_expr,
                param_idx: founded_idx,
            };

            analyzer
                .context
                .add_unresolve(unresolved.into(), InferFailReason::None);
        }
        LuaAst::LuaFuncStat(func_stat) => {
            let unresolved = UnResolveParentClosureParams {
                file_id: analyzer.file_id,
                signature_id: *signature_id,
                parent_ast: UnResolveParentAst::LuaFuncStat(func_stat.clone()),
            };

            analyzer
                .context
                .add_unresolve(unresolved.into(), InferFailReason::None);
        }
        LuaAst::LuaTableField(table_field) => {
            let unresolved = UnResolveParentClosureParams {
                file_id: analyzer.file_id,
                signature_id: *signature_id,
                parent_ast: UnResolveParentAst::LuaTableField(table_field.clone()),
            };

            analyzer
                .context
                .add_unresolve(unresolved.into(), InferFailReason::None);
        }
        LuaAst::LuaAssignStat(assign_stat) => {
            let unresolved = UnResolveParentClosureParams {
                file_id: analyzer.file_id,
                signature_id: *signature_id,
                parent_ast: UnResolveParentAst::LuaAssignStat(assign_stat.clone()),
            };

            analyzer
                .context
                .add_unresolve(unresolved.into(), InferFailReason::None);
        }
        _ => {}
    }

    Some(())
}

fn analyze_return(
    analyzer: &mut LuaAnalyzer,
    signature_id: &LuaSignatureId,
    closure: &LuaClosureExpr,
) -> Option<()> {
    let signature = analyzer.db.get_signature_index().get(signature_id)?;
    if signature.resolve_return == SignatureReturnStatus::DocResolve {
        return None;
    }

    let parent = closure.get_parent::<LuaAst>()?;
    if let LuaAst::LuaCallArgList(_) = &parent {
        analyze_lambda_returns(analyzer, signature_id, closure);
    };

    let block = match closure.get_block() {
        Some(block) => block,
        None => {
            let signature = analyzer
                .db
                .get_signature_index_mut()
                .get_or_create(*signature_id);
            signature.resolve_return = SignatureReturnStatus::InferResolve;
            return Some(());
        }
    };

    let cache = analyzer
        .context
        .infer_manager
        .get_infer_cache(analyzer.file_id);
    let return_points = match analyze_func_body_returns_with(block.clone(), &mut |expr| {
        infer_expr(analyzer.db, cache, expr.clone())
    }) {
        Ok(return_points) => return_points,
        Err(reason) => {
            let unresolve = UnResolveReturn {
                file_id: analyzer.file_id,
                signature_id: *signature_id,
                body: block,
            };

            analyzer.context.add_unresolve(unresolve.into(), reason);
            return None;
        }
    };
    let returns = match analyze_return_point(analyzer.db, cache, &return_points) {
        Ok(returns) => returns,
        Err(InferFailReason::None) => {
            vec![LuaDocReturnInfo {
                type_ref: LuaType::Unknown,
                description: None,
                name: None,
                attributes: None,
            }]
        }
        Err(reason) => {
            let unresolve = UnResolveReturn {
                file_id: analyzer.file_id,
                signature_id: *signature_id,
                body: block,
            };

            analyzer.context.add_unresolve(unresolve.into(), reason);
            return None;
        }
    };
    let signature = analyzer
        .db
        .get_signature_index_mut()
        .get_or_create(*signature_id);

    signature.resolve_return = SignatureReturnStatus::InferResolve;

    signature.return_docs = returns;

    Some(())
}

fn analyze_lambda_returns(
    analyzer: &mut LuaAnalyzer,
    signature_id: &LuaSignatureId,
    closure: &LuaClosureExpr,
) -> Option<()> {
    let call_arg_list = closure.get_parent::<LuaCallArgList>()?;
    let call_expr = call_arg_list.get_parent::<LuaCallExpr>()?;
    let pos = closure.get_position();
    let founded_idx = call_arg_list
        .get_args()
        .position(|arg| arg.get_position() == pos)?;
    let block = closure.get_block()?;
    let unresolved = UnResolveClosureReturn {
        file_id: analyzer.file_id,
        signature_id: *signature_id,
        call_expr,
        param_idx: founded_idx,
        body: block,
    };

    analyzer
        .context
        .add_unresolve(unresolved.into(), InferFailReason::None);

    Some(())
}

pub fn analyze_return_point(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    return_points: &[LuaReturnPoint],
) -> Result<Vec<LuaDocReturnInfo>, InferFailReason> {
    let mut return_row: Option<Vec<LuaType>> = None;
    for point in return_points {
        let point_row = match point {
            LuaReturnPoint::Expr(expr) => {
                Some(infer_return_row(db, cache, std::slice::from_ref(expr))?)
            }
            LuaReturnPoint::MuliExpr(exprs) => Some(infer_return_row(db, cache, exprs)?),
            LuaReturnPoint::Empty => Some(Vec::new()),
            _ => None,
        };

        if let Some(point_row) = point_row {
            return_row = Some(match return_row {
                Some(return_row) => {
                    let rows = [return_row.as_slice(), point_row.as_slice()];
                    merge_return_rows_with(&rows, |types| {
                        types
                            .into_iter()
                            .reduce(|left, right| TypeOps::Union.apply(db, &left, &right))
                            .unwrap_or(LuaType::Never)
                    })
                }
                None => point_row,
            });
        }
    }

    let return_row = return_row.unwrap_or_else(|| vec![LuaType::Unknown]);

    Ok(return_row
        .into_iter()
        .map(|type_ref| LuaDocReturnInfo {
            type_ref,
            description: None,
            name: None,
            attributes: None,
        })
        .collect())
}

fn infer_return_row(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    exprs: &[LuaExpr],
) -> Result<Vec<LuaType>, InferFailReason> {
    Ok(infer_return_expr_list_types(db, cache, exprs, infer_expr)?
        .into_iter()
        .map(|(ty, _)| ty)
        .collect())
}
