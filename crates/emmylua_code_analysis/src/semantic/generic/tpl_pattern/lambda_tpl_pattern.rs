use emmylua_parser::LuaAstNode;

use crate::{
    InferFailReason, LuaFunctionType, LuaSignatureId, TplContext,
    semantic::generic::tpl_pattern::TplPatternMatchResult,
};

pub fn check_lambda_tpl_pattern(
    context: &mut TplContext,
    _tpl_func: &LuaFunctionType,
    signature_id: LuaSignatureId,
) -> TplPatternMatchResult {
    let call_expr = context.call_expr.clone().ok_or(InferFailReason::None)?;
    let call_arg_list = call_expr.get_args_list().ok_or(InferFailReason::None)?;
    let closure_position = signature_id.get_position();
    let closure_expr = call_arg_list
        .get_args()
        .find(|arg| arg.get_position() == closure_position);

    if closure_expr.is_none() {
        return Err(InferFailReason::UnResolveSignatureReturn(signature_id));
    }

    Ok(())
}
