//! Check return count — pure salsa.

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaBlock, LuaClosureExpr, LuaExpr, LuaGeneralToken,
    LuaReturnStat, LuaTokenKind,
};
use rowan::TextRange;

use crate::semantic_model::{SemanticModel, signature::SignatureReturnStatus};
use crate::{
    DiagnosticCode, LuaSignatureId, LuaType,
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
    _context: &mut DiagnosticContext,
    model: &SemanticModel,
    closure: &LuaClosureExpr,
) -> Option<(bool, LuaType)> {
    let file_id = model.get_file_id();
    let typ = model
        .infer_bind_value_type(closure.clone().into())
        .unwrap_or(LuaType::Unknown);
    match typ {
        LuaType::DocFunction(f) => return Some((true, f.get_ret().clone())),
        LuaType::Signature(sig) => {
            let info = model.get_signature(file_id, sig.get_position())?;
            return Some((
                info.resolve_return() == SignatureReturnStatus::DocResolve,
                info.return_type(),
            ));
        }
        // Function type from @field annotation (degraded from specific fun(...) type):
        // treat as doc-defined function with nil return (no @return annotated).
        LuaType::Function => return Some((true, LuaType::Nil)),
        _ => {}
    }
    let sig_id = LuaSignatureId::from_closure(file_id, closure);
    let info = model.get_signature(file_id, sig_id.get_position())?;
    Some((
        info.resolve_return() == SignatureReturnStatus::DocResolve,
        info.return_type(),
    ))
}

fn check_missing(context: &mut DiagnosticContext, model: &SemanticModel, closure: &LuaClosureExpr) {
    let Some((is_doc, return_type)) = get_return_info(context, model, closure) else {
        return;
    };
    if !is_doc {
        return;
    }

    let min_expected = match &return_type {
        LuaType::Variadic(v) => {
            let Some(min) = v.get_min_len() else { return };
            let mut real = min;
            if min > 0 {
                for i in (0..min).rev() {
                    if v.get_type(i).is_some_and(|t| t.is_optional()) {
                        real -= 1
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
        check_return_exprs(context, model, &ret, &return_type, min_expected);
    }

    if min_expected > 0 {
        let range = if let Some(block) = closure.get_block() {
            let Ok((can_fall, can_break)) = analyze_func_body_missing_return_flags_with(
                block.clone(),
                &mut |expr: &LuaExpr| model.infer_expr(expr.clone()),
            ) else {
                return;
            };
            if !can_fall && !can_break {
                return;
            }
            let token = get_block_end(&block).or_else(|| block.tokens::<LuaGeneralToken>().last());
            let Some(token) = token else { return };
            Some(token.get_range())
        } else {
            let Some(end) = closure.token_by_kind(LuaTokenKind::TkEnd) else {
                return;
            };
            Some(end.get_range())
        };
        if let Some(r) = range {
            context.add_diagnostic(
                DiagnosticCode::MissingReturn,
                r,
                t!("Annotations specify that a return value is required here.").to_string(),
                None,
            );
        }
    }
}

fn get_block_end(block: &LuaBlock) -> Option<LuaGeneralToken> {
    if let Some(tk) = block.token_by_kind(LuaTokenKind::TkEnd) {
        return Some(tk);
    }
    let parent = LuaAst::cast(block.syntax().parent()?)?;
    parent.token_by_kind(LuaTokenKind::TkEnd)
}

fn check_return_exprs(
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
    let mut count = 0usize;
    let mut tail_nil = false;
    let mut redundant: Vec<TextRange> = Vec::new();

    for (i, expr) in exprs.iter().enumerate() {
        let ty = model.infer_expr(expr.clone()).unwrap_or(LuaType::Unknown);
        match ty {
            LuaType::Variadic(v) => count += v.get_max_len().unwrap_or(0),
            LuaType::Nil => {
                if i == exprs.len() - 1 {
                    tail_nil = true
                }
                count += 1;
            }
            _ => count += 1,
        }
        if let Some(max) = max_expected
            && count > max
            && !(tail_nil && count - 1 == max)
        {
            redundant.push(expr.get_range());
        }
    }

    if count < min_expected {
        context.add_diagnostic(DiagnosticCode::MissingReturnValue, ret.get_range(),
            t!("Annotations specify that at least %{min} return value(s) are required, found %{rmin} returned here instead.",
                min = min_expected, rmin = count).to_string(), None);
    }
    for r in redundant {
        context.add_diagnostic(DiagnosticCode::RedundantReturnValue, r,
            t!("Annotations specify that at most %{max} return value(s) are required, found %{rmax} returned here instead.",
                max = max_expected.unwrap_or(0), rmax = count).to_string(), None);
    }
}
