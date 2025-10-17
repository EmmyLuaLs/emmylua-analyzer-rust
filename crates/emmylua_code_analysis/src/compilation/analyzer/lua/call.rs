use emmylua_parser::LuaCallExpr;

use crate::{
    InferFailReason, LuaType,
    compilation::analyzer::{lua::LuaAnalyzer, unresolve::UnResolveConstructor},
};

pub fn analyze_call(analyzer: &mut LuaAnalyzer, call_expr: LuaCallExpr) -> Option<()> {
    let prefix_expr = call_expr.clone().get_prefix_expr()?;
    if let Ok(expr_type) = analyzer.infer_expr(&prefix_expr) {
        let LuaType::Signature(signature_id) = expr_type else {
            return Some(());
        };
        let signature = analyzer.db.get_signature_index().get(&signature_id)?;
        for (idx, param_info) in signature.param_docs.iter() {
            if param_info.get_attribute_by_name("constructor").is_some() {
                let unresolve = UnResolveConstructor {
                    file_id: analyzer.file_id,
                    call_expr: call_expr.clone(),
                    signature_id,
                    param_idx: *idx,
                };
                analyzer
                    .context
                    .add_unresolve(unresolve.into(), InferFailReason::None);
                return Some(());
            }
        }
    }
    Some(())
}
