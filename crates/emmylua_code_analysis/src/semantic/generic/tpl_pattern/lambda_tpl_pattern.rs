use crate::{
    InferFailReason, LuaSignatureId, LuaType, TplContext, infer_expr,
    semantic::generic::tpl_pattern::TplPatternMatchResult,
};

pub fn check_lambda_tpl_pattern(
    context: &mut TplContext,
    signature_id: LuaSignatureId,
) -> TplPatternMatchResult {
    let call_expr = context.call_expr.clone().ok_or(InferFailReason::None)?;
    let call_arg_list = call_expr.get_args_list().ok_or(InferFailReason::None)?;
    for arg in call_arg_list.get_args() {
        if let Ok(LuaType::Signature(arg_signature_id)) =
            infer_expr(context.db, context.cache, arg.clone())
            && arg_signature_id == signature_id
        {
            return Ok(());
        }
    }

    Err(InferFailReason::UnResolveSignatureReturn(signature_id))
}
