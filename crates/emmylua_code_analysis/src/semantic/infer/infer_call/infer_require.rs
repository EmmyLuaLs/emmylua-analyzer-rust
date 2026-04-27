use emmylua_parser::LuaCallExpr;

use crate::{
    DbIndex, InFiled, InferFailReason, LuaInferCache, LuaType, infer_expr_root,
    module_query::{export::infer_module_export_type, identity::find_db_module_file_id},
    semantic::infer::InferResult,
};

pub fn infer_require_call(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: LuaCallExpr,
) -> InferResult {
    let arg_list = call_expr.get_args_list().ok_or(InferFailReason::None)?;
    let first_arg = arg_list.get_args().next().ok_or(InferFailReason::None)?;
    let require_path_type = infer_expr_root(db, cache, first_arg)?;
    let module_path: String = match &require_path_type {
        LuaType::StringConst(module_path) => module_path.as_ref().to_string(),
        _ => {
            return Ok(LuaType::Any);
        }
    };

    let module_file_id = find_db_module_file_id(db, &module_path).ok_or(InferFailReason::None)?;
    infer_module_export_type(db, module_file_id).ok_or(InferFailReason::UnResolveExpr(
        InFiled::new(cache.get_file_id(), call_expr.into()),
    ))
}
