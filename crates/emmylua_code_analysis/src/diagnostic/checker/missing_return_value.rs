use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaBlock, LuaClosureExpr, LuaReturnStat, LuaTokenKind,
};

use crate::{DiagnosticCode, LuaSignatureId, SemanticModel, SignatureReturnStatus};

use super::DiagnosticContext;

pub const CODES: &[DiagnosticCode] = &[DiagnosticCode::MissingReturnValue];

pub fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) -> Option<()> {
    let root = semantic_model.get_root().clone();
    for return_stat in root.descendants::<LuaReturnStat>() {
        check_return_stat(context, semantic_model, &return_stat);
    }
    Some(())
}

fn check_return_stat(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    return_stat: &LuaReturnStat,
) -> Option<()> {
    let closure_expr = return_stat
        .get_parent::<LuaBlock>()?
        .ancestors::<LuaClosureExpr>()
        .next()?;

    let signature_id = LuaSignatureId::from_closure(semantic_model.get_file_id(), &closure_expr);
    let signature = context.db.get_signature_index().get(&signature_id)?;
    let min_return_types = signature
        .get_return_types()
        .iter()
        .filter(|ty| !ty.is_optional())
        .cloned()
        .collect::<Vec<_>>();

    if signature.resolve_return != SignatureReturnStatus::DocResolve {
        return None;
    }

    let disable_return_count_check = min_return_types.iter().any(|ty| ty.is_variadic());

    let expr_return_len = return_stat.get_expr_list().collect::<Vec<_>>().len();
    let return_types_len = min_return_types.len();
    if !disable_return_count_check && expr_return_len < return_types_len {
        context.add_diagnostic(
            DiagnosticCode::MissingReturnValue,
            return_stat
                .token_by_kind(LuaTokenKind::TkReturn)?
                .get_range(),
            t!(
                "Annotations specify that at least %{min} return value(s) are required, found %{rmin} returned here instead.",
                min = return_types_len,
                rmin = expr_return_len
            )
            .to_string(),
            None,
        );
    }
    Some(())
}
