use emmylua_parser::LuaCallExpr;

use crate::{
    InferFailReason, LuaBuiltinAttributeKind, LuaType,
    compilation::analyzer::{
        lua::LuaAnalyzer,
        unresolve::{UnResolveCall, UnResolveConstructor},
    },
};

pub fn analyze_call(analyzer: &mut LuaAnalyzer, call_expr: LuaCallExpr) -> Option<()> {
    let prefix_expr = call_expr.clone().get_prefix_expr()?;
    // Constructor discovery only needs the callee's declared signature. Full
    // flow inference here replays narrowing for every call in call-dense files.
    match analyzer.infer_expr_no_flow(&prefix_expr) {
        Ok(Some(LuaType::Signature(signature_id))) => {
            let signature = analyzer.db.get_signature_index().get(&signature_id)?;
            for (idx, param_info) in signature.param_docs.iter() {
                if param_info
                    .get_builtin_attribute(LuaBuiltinAttributeKind::Constructor)
                    .is_some()
                {
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
        Err(InferFailReason::UnResolveDeclType(id)) => {
            let unresolve = UnResolveCall {
                file_id: analyzer.file_id,
                call_expr: call_expr.clone(),
            };
            analyzer
                .context
                .add_unresolve(unresolve.into(), InferFailReason::UnResolveDeclType(id));
        }
        _ => {}
    }
    Some(())
}
