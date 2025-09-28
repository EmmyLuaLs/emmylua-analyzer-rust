use emmylua_parser::LuaCallExpr;

use crate::{
    InferFailReason, LuaType,
    compilation::analyzer::{lua::LuaAnalyzer, unresolve::UnResolveClassCtor},
};

pub fn analyze_call(analyzer: &mut LuaAnalyzer, call_expr: LuaCallExpr) -> Option<()> {
    let prefix_expr = call_expr.clone().get_prefix_expr()?;
    match analyzer.infer_expr(&prefix_expr) {
        Ok(expr_type) => {
            let LuaType::Signature(signature_id) = expr_type else {
                return Some(());
            };
            let signature = analyzer.db.get_signature_index().get(&signature_id)?;
            for (idx, param_info) in signature.param_docs.iter() {
                if let Some(ref attrs) = param_info.attributes {
                    for attr in attrs.iter() {
                        if attr.id.get_name() == "class_ctor" {
                            let unresolve = UnResolveClassCtor {
                                file_id: analyzer.file_id,
                                call_expr: call_expr.clone(),
                                signature_id: signature_id,
                                param_idx: *idx,
                            };
                            analyzer
                                .context
                                .add_unresolve(unresolve.into(), InferFailReason::None);
                            return Some(());
                        }
                    }
                }
            }
        }
        Err(_) => {}
    }
    Some(())
}
