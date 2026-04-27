use emmylua_parser::{LuaAstNode, LuaChunk, LuaExpr};

use crate::{
    InferFailReason, LuaDeclId, LuaSemanticDeclId, LuaSignatureId,
    compilation::{
        LuaReturnPoint, analyze_func_body_returns_with, analyzer::unresolve::UnResolveModule,
    },
    db_index::LuaType,
    infer_expr_root,
};

use super::LuaAnalyzer;

pub fn analyze_chunk_return(analyzer: &mut LuaAnalyzer, chunk: LuaChunk) -> Option<()> {
    let block = chunk.get_block()?;
    let file_id = analyzer.file_id;
    let cache = analyzer.context.infer_manager.get_infer_cache(file_id);
    let return_exprs = analyze_func_body_returns_with(block, &mut |expr| {
        Ok(infer_expr_root(analyzer.db, cache, expr.clone()).unwrap_or(LuaType::Unknown))
    })
    .unwrap_or_default();
    for point in return_exprs {
        if let LuaReturnPoint::Expr(expr) = point {
            // Module export selection follows the first return candidate.
            // It does not refine `pred()`-style call conditions.
            let expr_type = match analyzer.infer_expr(&expr) {
                Ok(expr_type) => expr_type,
                Err(InferFailReason::None) => LuaType::Unknown,
                Err(reason) => {
                    let unresolve = UnResolveModule {
                        file_id: analyzer.file_id,
                        expr,
                    };
                    analyzer.context.add_unresolve(unresolve.into(), reason);
                    return None;
                }
            };

            let semantic_id = get_semantic_id(analyzer, expr.clone());

            let visibility = semantic_id.as_ref().and_then(|id| {
                analyzer
                    .db
                    .get_property_index()
                    .get_property(id)
                    .map(|p| p.visibility.clone())
            });

            let module_info = analyzer
                .db
                .get_module_index_mut()
                .get_module_mut(analyzer.file_id)?;
            module_info.export_type = Some(expr_type.get_result_slot_type(0).unwrap_or(expr_type));
            module_info.semantic_id = semantic_id;
            if let Some(visibility) = visibility {
                module_info.merge_visibility(visibility);
            }
            break;
        }
    }

    Some(())
}

fn get_semantic_id(analyzer: &LuaAnalyzer, expr: LuaExpr) -> Option<LuaSemanticDeclId> {
    match expr {
        LuaExpr::NameExpr(name_expr) => {
            let name = name_expr.get_name_text()?;
            let tree = analyzer
                .db
                .get_decl_index()
                .get_decl_tree(&analyzer.file_id)?;
            let decl = tree.find_local_decl(&name, name_expr.get_position())?;

            Some(LuaSemanticDeclId::LuaDecl(decl.get_id()))
        }
        LuaExpr::ClosureExpr(closure) => Some(LuaSemanticDeclId::Signature(
            LuaSignatureId::from_closure(analyzer.file_id, &closure),
        )),
        // `return {}`
        LuaExpr::TableExpr(table_expr) => Some(LuaSemanticDeclId::LuaDecl(LuaDeclId::new(
            analyzer.file_id,
            table_expr.get_position(),
        ))),
        _ => None,
    }
}
