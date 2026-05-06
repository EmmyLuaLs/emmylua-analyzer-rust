use emmylua_parser::LuaCallExpr;

use crate::{
    DbIndex, InFiled, InferFailReason, LuaInferCache, LuaType,
    find_compilation_module_by_require_path, infer_expr, resolve_projected_module_export_type,
    semantic::infer::InferResult,
};

pub fn infer_require_call(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: LuaCallExpr,
) -> InferResult {
    let arg_list = call_expr.get_args_list().ok_or(InferFailReason::None)?;
    let first_arg = arg_list.get_args().next().ok_or(InferFailReason::None)?;
    let require_path_type = infer_expr(db, cache, first_arg)?;
    let module_path: String = match &require_path_type {
        LuaType::StringConst(module_path) => module_path.as_ref().to_string(),
        _ => {
            return Ok(LuaType::Any);
        }
    };

    let module_info =
        find_compilation_module_by_require_path(db, &module_path).ok_or(InferFailReason::None)?;
    let export_type =
        resolve_projected_module_export_type(db, module_info.file_id).ok_or_else(|| {
            InferFailReason::UnResolveExpr(InFiled::new(cache.get_file_id(), call_expr.into()))
        })?;

    match export_type {
        LuaType::Def(id) => Ok(LuaType::Ref(id)),
        other => Ok(other),
    }
}
