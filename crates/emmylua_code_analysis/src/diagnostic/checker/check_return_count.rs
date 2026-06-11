//! Check return count — hybrid (salsa model + DbIndex bridge for signatures).

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaBlock, LuaClosureExpr, LuaExpr, LuaGeneralToken,
    LuaReturnStat, LuaTokenKind,
};

use crate::semantic_model::SemanticModel;
use crate::{
    DiagnosticCode, LuaSignatureId, LuaType, SignatureReturnStatus,
    compilation::analyze_func_body_missing_return_flags_with,
};

use super::{DiagnosticContext, get_return_stats};

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for closure in root.descendants::<LuaClosureExpr>() {
        check_missing(context, model, &closure);
    }
}

fn get_return_info(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    closure: &LuaClosureExpr,
) -> Option<(bool, LuaType)> {
    let typ = model.infer_bind_value_type(closure.clone().into()).unwrap_or(LuaType::Unknown);
    match typ {
        LuaType::DocFunction(f) => return Some((true, f.get_ret().clone())),
        LuaType::Signature(sig) => {
            let signature = context.db.get_signature_index().get(&sig)?;
            return Some((
                signature.resolve_return == SignatureReturnStatus::DocResolve,
                signature.get_return_type(),
            ));
        }
        _ => {}
    }
    let sig_id = LuaSignatureId::from_closure(model.get_file_id(), closure);
    let signature = context.db.get_signature_index().get(&sig_id)?;
    Some((
        signature.resolve_return == SignatureReturnStatus::DocResolve,
        signature.get_return_type(),
    ))
}

fn check_missing(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    closure: &LuaClosureExpr,
) {
    let Some((is_doc_resolved, return_type)) = get_return_info(context, model, closure) else {
        return;
    };
    if !is_doc_resolved { return }

    let min_expected = match &return_type {
        LuaType::Variadic(variadic) => {
            let Some(min_len) = variadic.get_min_len() else { return };
            let mut real = min_len;
            if min_len > 0 {
                for i in (0..min_len).rev() {
                    if variadic.get_type(i).is_some_and(|t| t.is_optional()) {
                        real -= 1;
                    } else {
                        break;
                    }
                }
            }
            real
        }
        LuaType::Nil | LuaType::Any | LuaType::Unknown => 0,
        _ if return_type.is_nullable() => 0,
        _ => 1,
    };

    for ret in get_return_stats(closure) {
        check_return(context, model, &ret, &return_type, min_expected);
    }

    if min_expected > 0 {
        let range = if let Some(block) = closure.get_block() {
            let fall_result = analyze_func_body_missing_return_flags_with(
                block.clone(),
                &mut |expr: &LuaExpr| Ok(model.infer_expr(expr.clone()).unwrap_or(LuaType::Unknown)),
            ).ok();
            let Some((can_fall, can_break)) = fall_result else { return };
            if !can_fall && !can_break { return }
            let Some(token) = get_block_end(&block)
                .or_else(|| block.tokens::<LuaGeneralToken>().last())
            else { return };
            Some(token.get_range())
        } else {
            let Some(tk) = closure.token_by_kind(LuaTokenKind::TkEnd) else { return };
            Some(tk.get_range())
        };
        if let Some(r) = range {
            context.add_diagnostic(
                DiagnosticCode::MissingReturn, r,
                t!("Annotations specify that a return value is required here.").to_string(),
                None,
            );
        }
    }
}

fn get_block_end(block: &LuaBlock) -> Option<LuaGeneralToken> {
    let p = LuaAst::cast(block.syntax().parent()?)?;
    p.token_by_kind(LuaTokenKind::TkEnd)
}

fn check_return(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    ret: &LuaReturnStat,
    return_type: &LuaType,
    min_expected: usize,
) {
    let max_expected = match return_type {
        LuaType::Variadic(v) => v.get_max_len(),
        LuaType::Any | LuaType::Unknown => Some(1),
        LuaType::Nil => Some(0),
        _ => Some(1),
    };

    let exprs: Vec<_> = ret.get_expr_list().collect();
    let mut count = 0;
    let mut tail_nil = false;
    let mut redundant = Vec::new();

    for (idx, expr) in exprs.iter().enumerate() {
        let ty = model.infer_expr(expr.clone()).unwrap_or(LuaType::Unknown);
        match ty {
            LuaType::Variadic(v) => count += v.get_max_len().unwrap_or(0),
            LuaType::Nil => { tail_nil = idx == exprs.len() - 1; count += 1; }
            _ => count += 1,
        }
        if let Some(max) = max_expected {
            if count > max {
                if tail_nil && count - 1 == max { continue }
                redundant.push(expr.get_range());
            }
        }
    }

    if count < min_expected {
        context.add_diagnostic(
            DiagnosticCode::MissingReturnValue, ret.get_range(),
            t!("Annotations specify that at least %{min} return value(s) are required, found %{rmin} returned here instead.",
                min = min_expected, rmin = count).to_string(),
            None,
        );
    }
    for range in redundant {
        context.add_diagnostic(
            DiagnosticCode::RedundantReturnValue, range,
            t!("Annotations specify that at most %{max} return value(s) are required, found %{rmax} returned here instead.",
                max = max_expected.unwrap_or(0), rmax = count).to_string(),
            None,
        );
    }
}
