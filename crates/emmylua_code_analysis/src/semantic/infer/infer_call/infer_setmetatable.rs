use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaIndexKey};

use crate::{
    DbIndex, InFiled, InferFailReason, LuaInferCache, LuaInstanceType, LuaMemberKey, LuaType,
    infer_expr,
    semantic::{infer::InferResult, member::find_members_with_key},
};

pub(super) fn infer_setmetatable_call(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    call_expr: LuaCallExpr,
) -> InferResult {
    let arg_list = call_expr.get_args_list().ok_or(InferFailReason::None)?;
    let args = arg_list.get_args().collect::<Vec<LuaExpr>>();

    if args.len() != 2 {
        return Ok(LuaType::Any);
    }

    let basic_table = args[0].clone();
    let metatable = args[1].clone();

    let (meta_type, is_index) = infer_metatable_index_type(db, cache, metatable)?;
    if cache.is_no_flow() && !is_index && !meta_type.is_custom_type() {
        // No-flow setmetatable inference is only used as a conservative fallback.
        // If the metatable does not resolve to an actual metatable shape, decline
        // instead of treating arbitrary static expressions as the result type.
        return Err(InferFailReason::None);
    }
    match &basic_table {
        LuaExpr::TableExpr(table_expr) => {
            if table_expr.is_empty() && is_index {
                return Ok(meta_type);
            }

            if is_index {
                return Ok(LuaType::Instance(
                    LuaInstanceType::new(
                        meta_type,
                        InFiled::new(cache.get_file_id(), table_expr.get_range()),
                    )
                    .into(),
                ));
            }

            Ok(LuaType::TableConst(InFiled::new(
                cache.get_file_id(),
                table_expr.get_range(),
            )))
        }
        _ => {
            if !is_index
                && let Some(basic_type) = infer_local_basic_table_type(db, cache, &basic_table)
            {
                return Ok(basic_type);
            }

            if meta_type.is_unknown() {
                return infer_expr(db, cache, basic_table);
            }

            Ok(meta_type)
        }
    }
}

fn infer_local_basic_table_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    basic_table: &LuaExpr,
) -> Option<LuaType> {
    let LuaExpr::NameExpr(name_expr) = basic_table else {
        return None;
    };

    let file_id = cache.get_file_id();
    let decl_id = db
        .get_reference_index()
        .get_local_reference(&file_id)?
        .get_decl_id(&name_expr.get_range())?;
    let decl = db.get_decl_index().get_decl(&decl_id)?;
    if !decl.is_local() {
        return None;
    }

    // 第一个变量如果是 local 定义的表变量, 那么我们使用他作为 setmetatable 的返回值
    let basic_type = infer_expr(db, cache, basic_table.clone()).ok()?;
    basic_type.is_table().then_some(basic_type)
}

fn infer_metatable_index_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    metatable: LuaExpr,
) -> Result<(LuaType, bool /*__index type*/), InferFailReason> {
    if let LuaExpr::TableExpr(table) = &metatable {
        let fields = table.get_fields();
        for field in fields {
            let field_name = match field.get_field_key() {
                Some(key) => match key {
                    LuaIndexKey::Name(n) => n.get_name_text().to_string(),
                    LuaIndexKey::String(s) => s.get_value(),
                    _ => continue,
                },
                None => continue,
            };

            if field_name == "__index" {
                let field_value = field.get_value_expr().ok_or(InferFailReason::None)?;
                if matches!(
                    field_value,
                    LuaExpr::TableExpr(_)
                        | LuaExpr::CallExpr(_)
                        | LuaExpr::IndexExpr(_)
                        | LuaExpr::NameExpr(_)
                ) {
                    let meta_type = infer_expr(db, cache, field_value)?;
                    return Ok((meta_type, true));
                }
            }
        }
    };

    let meta_type = infer_expr(db, cache, metatable)?;
    if let Some(meta_members) =
        find_members_with_key(db, &meta_type, LuaMemberKey::Name("__index".into()), false)
        && let Some(meta_member) = meta_members.first()
        && meta_member.typ.is_custom_type()
    {
        return Ok((meta_member.typ.clone(), true));
    }

    Ok((meta_type, false))
}
