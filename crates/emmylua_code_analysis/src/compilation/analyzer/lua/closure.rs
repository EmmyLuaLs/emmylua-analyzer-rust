use std::ops::Deref;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallArgList, LuaCallExpr, LuaClosureExpr, LuaFuncStat, LuaVarExpr,
};

use crate::{
    DbIndex, InferFailReason, LuaInferCache, LuaType, SignatureReturnStatus, TypeOps, VariadicType,
    compilation::analyzer::unresolve::{
        UnResolveCallClosureParams, UnResolveClosureReturn, UnResolveParentAst,
        UnResolveParentClosureParams, UnResolveReturn,
    },
    db_index::{LuaDocReturnInfo, LuaSignatureId},
    infer_expr,
};

use super::{LuaAnalyzer, LuaReturnPoint, func_body::analyze_func_body_returns};

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

    let return_points = analyze_func_body_returns(block);
    let returns = match analyze_return_point(
        analyzer.db,
        analyzer
            .context
            .infer_manager
            .get_infer_cache(analyzer.file_id),
        &return_points,
    ) {
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
                return_points,
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
    let return_points = analyze_func_body_returns(block);
    let unresolved = UnResolveClosureReturn {
        file_id: analyzer.file_id,
        signature_id: *signature_id,
        call_expr,
        param_idx: founded_idx,
        return_points,
    };

    analyzer
        .context
        .add_unresolve(unresolved.into(), InferFailReason::None);

    Some(())
}

pub fn analyze_return_point(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    return_points: &Vec<LuaReturnPoint>,
) -> Result<Vec<LuaDocReturnInfo>, InferFailReason> {
    let mut return_type = LuaType::Unknown;
    for point in return_points {
        match point {
            LuaReturnPoint::Expr(expr) => {
                let expr_type = infer_expr(db, cache, expr.clone())?;
                return_type = union_return_expr(db, return_type, expr_type);
            }
            LuaReturnPoint::MuliExpr(exprs) => {
                let mut multi_return = vec![];
                for expr in exprs {
                    let expr_type = infer_expr(db, cache, expr.clone())?;
                    multi_return.push(expr_type);
                }
                let typ = LuaType::Variadic(VariadicType::Multi(multi_return).into());
                return_type = union_return_expr(db, return_type, typ);
            }
            LuaReturnPoint::Nil => {
                return_type = union_return_expr(db, return_type, LuaType::Nil);
            }
            _ => {}
        }
    }

    Ok(vec![LuaDocReturnInfo {
        type_ref: return_type,
        description: None,
        name: None,
        attributes: None,
    }])
}

fn union_return_expr(db: &DbIndex, left: LuaType, right: LuaType) -> LuaType {
    if left == LuaType::Unknown {
        return right;
    }

    match (&left, &right) {
        (LuaType::Variadic(left_variadic), LuaType::Variadic(right_variadic)) => {
            match (&left_variadic.deref(), &right_variadic.deref()) {
                (VariadicType::Base(left_base), VariadicType::Base(right_base)) => {
                    let union_base = TypeOps::Union.apply(db, left_base, right_base);
                    LuaType::Variadic(VariadicType::Base(union_base).into())
                }
                (VariadicType::Multi(left_multi), VariadicType::Multi(right_multi)) => {
                    let mut new_multi = vec![];
                    let max_len = left_multi.len().max(right_multi.len());
                    for i in 0..max_len {
                        let left_type = left_multi.get(i).cloned().unwrap_or(LuaType::Nil);
                        let right_type = right_multi.get(i).cloned().unwrap_or(LuaType::Nil);
                        new_multi.push(TypeOps::Union.apply(db, &left_type, &right_type));
                    }
                    LuaType::Variadic(VariadicType::Multi(new_multi).into())
                }
                // difficult to merge the type, use let
                _ => left.clone(),
            }
        }
        (LuaType::Variadic(variadic), _) => {
            let first_type = variadic.get_type(0).cloned().unwrap_or(LuaType::Unknown);
            let first_union_type = TypeOps::Union.apply(db, &first_type, &right);

            match variadic.deref() {
                VariadicType::Base(base) => {
                    let union_base = TypeOps::Union.apply(db, base, &LuaType::Nil);
                    LuaType::Variadic(
                        VariadicType::Multi(vec![
                            first_union_type,
                            LuaType::Variadic(VariadicType::Base(union_base).into()),
                        ])
                        .into(),
                    )
                }
                VariadicType::Multi(multi) => {
                    let mut new_multi = multi.clone();
                    if !new_multi.is_empty() {
                        new_multi[0] = first_union_type;
                        for mult in new_multi.iter_mut().skip(1) {
                            *mult = TypeOps::Union.apply(db, mult, &LuaType::Nil);
                        }
                    } else {
                        new_multi.push(first_union_type);
                    }

                    LuaType::Variadic(VariadicType::Multi(new_multi).into())
                }
            }
        }
        (_, LuaType::Variadic(variadic)) => {
            let first_type = variadic.get_type(0).cloned().unwrap_or(LuaType::Unknown);
            let first_union_type = TypeOps::Union.apply(db, &left, &first_type);
            match variadic.deref() {
                VariadicType::Base(base) => {
                    let union_base = TypeOps::Union.apply(db, base, &LuaType::Nil);
                    LuaType::Variadic(
                        VariadicType::Multi(vec![
                            first_union_type,
                            LuaType::Variadic(VariadicType::Base(union_base).into()),
                        ])
                        .into(),
                    )
                }
                VariadicType::Multi(multi) => {
                    let mut new_multi = multi.clone();
                    if !new_multi.is_empty() {
                        new_multi[0] = first_union_type;
                        for mult in new_multi.iter_mut().skip(1) {
                            *mult = TypeOps::Union.apply(db, mult, &LuaType::Nil);
                        }
                    } else {
                        new_multi.push(first_union_type);
                    }

                    LuaType::Variadic(VariadicType::Multi(new_multi).into())
                }
            }
        }
        _ => TypeOps::Union.apply(db, &left, &right),
    }
}
