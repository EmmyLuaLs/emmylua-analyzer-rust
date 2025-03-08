use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallArgList, LuaCallExpr, LuaClosureExpr, LuaFuncStat, LuaVarExpr,
};

use crate::{
    compilation::analyzer::unresolve::{
        UnResolveClosureParams, UnResolveClosureReturn, UnResolveReturn,
    },
    db_index::{LuaDocReturnInfo, LuaSignatureId},
    SignatureReturnStatus,
};

use super::{func_body::analyze_func_body_returns, LuaAnalyzer, LuaReturnPoint};

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
        .get_or_create(signature_id.clone());

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
    let call_arg_list = closure.get_parent::<LuaCallArgList>()?;
    let call_expr = call_arg_list.get_parent::<LuaCallExpr>()?;
    let pos = closure.get_position();
    let founded_idx = call_arg_list
        .get_args()
        .position(|arg| arg.get_position() == pos)?;

    let unresolved = UnResolveClosureParams {
        file_id: analyzer.file_id,
        signature_id: signature_id.clone(),
        call_expr,
        param_idx: founded_idx,
    };

    analyzer.add_unresolved(unresolved.into());

    Some(())
}

fn analyze_return(
    analyzer: &mut LuaAnalyzer,
    signature_id: &LuaSignatureId,
    closure: &LuaClosureExpr,
) -> Option<()> {
    let signature = analyzer.db.get_signature_index().get(&signature_id)?;
    if signature.resolve_return == SignatureReturnStatus::DocResolve {
        return None;
    }

    let parent = closure.get_parent::<LuaAst>()?;
    match &parent {
        LuaAst::LuaCallArgList(_) => {
            analyze_lambda_returns(analyzer, signature_id, closure)?;
            return Some(());
        }
        _ => {}
    };

    let block = closure.get_block()?;
    let return_points = analyze_func_body_returns(block);
    let returns = match analyze_return_point(analyzer, &return_points) {
        Some(returns) => returns,
        None => {
            let unresolve = UnResolveReturn {
                file_id: analyzer.file_id,
                signature_id: signature_id.clone(),
                return_points,
            };

            analyzer.add_unresolved(unresolve.into());
            return None;
        }
    };
    let signature = analyzer
        .db
        .get_signature_index_mut()
        .get_or_create(signature_id.clone());
    signature.return_docs = returns;
    signature.resolve_return = SignatureReturnStatus::InferResolve;
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
        signature_id: signature_id.clone(),
        call_expr,
        param_idx: founded_idx,
        return_points,
    };

    analyzer.add_unresolved(unresolved.into());

    Some(())
}

fn analyze_return_point(
    analyzer: &mut LuaAnalyzer,
    return_points: &Vec<LuaReturnPoint>,
) -> Option<Vec<LuaDocReturnInfo>> {
    let mut return_infos = Vec::new();
    for point in return_points {
        match point {
            LuaReturnPoint::Expr(expr) => {
                let expr_type = analyzer.infer_expr(&expr)?;
                if return_infos.is_empty() {
                    return_infos.push(LuaDocReturnInfo {
                        name: None,
                        type_ref: expr_type,
                        description: None,
                    });
                } else {
                    // todo merge two type
                    // let has_return = return_infos[0].type_ref.clone();
                    return_infos[0].type_ref = expr_type;
                }
            }
            LuaReturnPoint::MuliExpr(exprs) => {
                // todo merge type
                if return_infos.is_empty() {
                    for expr in exprs {
                        let expr_type = analyzer.infer_expr(&expr)?;
                        return_infos.push(LuaDocReturnInfo {
                            name: None,
                            type_ref: expr_type,
                            description: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    Some(return_infos)
}
