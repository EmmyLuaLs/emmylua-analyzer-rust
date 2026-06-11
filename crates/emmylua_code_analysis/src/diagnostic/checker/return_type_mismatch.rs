//! Return type mismatch — pure salsa.

use emmylua_parser::{
    LuaAstNode, LuaClosureExpr, LuaExpr, LuaFuncStat, LuaReturnStat, LuaSyntaxKind, LuaVarExpr,
};
use rowan::TextRange;

use crate::semantic_model::SemanticModel;
use crate::semantic_model::signature::SignatureReturnStatus;
use crate::{DiagnosticCode, LuaSignatureId, LuaType};

use super::{DiagnosticContext, humanize_lint_type, get_return_stats};

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for closure in root.descendants::<LuaClosureExpr>() {
        check_closure(context, model, &closure);
    }
}

fn check_closure(context: &mut DiagnosticContext, model: &SemanticModel, closure: &LuaClosureExpr) {
    let file_id = model.get_file_id();
    let sig_id = LuaSignatureId::from_closure(file_id, closure);
    let Some(sig) = model.get_signature(file_id, sig_id.get_position()) else { return };
    if sig.resolve_return() != SignatureReturnStatus::DocResolve { return }

    let return_type = sig.return_type();
    let self_type = get_self_type(model, closure);

    for ret in get_return_stats(closure) {
        let exprs: Vec<_> = ret.get_expr_list().collect();
        let Ok(infos) = model.infer_expr_list_types(&exprs, None) else { continue };
        let types: Vec<LuaType> = infos.iter().map(|(t, _)| t.clone()).collect();
        check_return(context, model, &self_type, &return_type, &types, &exprs);
    }
}

fn get_self_type(model: &SemanticModel, closure: &LuaClosureExpr) -> Option<LuaType> {
    let func = closure.get_parent::<LuaFuncStat>()?;
    let name = func.get_func_name()?;
    match name {
        LuaVarExpr::IndexExpr(ix) => {
            let prefix = ix.get_prefix_expr()?;
            model.infer_expr(prefix).ok()
        }
        _ => None,
    }
}

fn check_return(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    self_type: &Option<LuaType>,
    return_type: &LuaType,
    types: &[LuaType],
    exprs: &[LuaExpr],
) {
    if types.is_empty() { return }
    let db = context.db;

    match return_type {
        LuaType::Variadic(variadic) => {
            for (i, t) in types.iter().enumerate() {
                let expected = variadic.get_type(i).cloned().unwrap_or(LuaType::Unknown);
                if expected.is_unknown() || expected.is_any() { continue }
                if let Err(_) = crate::check_type_compact(db, t, &expected) {
                    let range = exprs.get(i).map(|e| e.get_range()).unwrap_or(TextRange::default());
                    context.add_diagnostic(DiagnosticCode::ReturnTypeMismatch, range,
                        t!("expected `%{a}` but found `%{b}`",
                            a = humanize_lint_type(db, &expected),
                            b = humanize_lint_type(db, t)).to_string(), None);
                }
            }
        }
        _ if return_type.is_nil() => {
            // returning nothing is fine for nil return type
        }
        _ => {
            let expected = match self_type {
                Some(st) if st.is_custom_type() => st.clone(),
                _ => return_type.clone(),
            };
            if let Some(t) = types.first() {
                if crate::check_type_compact(db, t, &expected).is_err() {
                    let range = exprs.first().map(|e| e.get_range()).unwrap_or(TextRange::default());
                    context.add_diagnostic(DiagnosticCode::ReturnTypeMismatch, range,
                        t!("expected `%{a}` but found `%{b}`",
                            a = humanize_lint_type(db, &expected),
                            b = humanize_lint_type(db, t)).to_string(), None);
                }
            }
        }
    }
}
