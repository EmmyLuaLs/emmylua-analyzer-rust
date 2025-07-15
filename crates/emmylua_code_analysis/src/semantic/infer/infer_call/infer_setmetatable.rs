use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaIndexKey};

use crate::{
    infer_expr,
    semantic::{infer::InferResult, member::find_members_with_key},
    DbIndex, InFiled, InferFailReason, LuaInferCache, LuaInstanceType, LuaMemberKey, LuaType,
};

pub fn infer_setmetatable_call(
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

            return Ok(LuaType::TableConst(InFiled::new(
                cache.get_file_id(),
                table_expr.get_range(),
            )));
        }
        _ => {
            if meta_type.is_unknown() {
                return infer_expr(db, cache, basic_table);
            }

            return Ok(meta_type);
        }
    }
}

// wrong implementation, should be removed
// fn meta_type_contain_table(
//     db: &DbIndex,
//     cache: &mut LuaInferCache,
//     meta_type: LuaType,
//     table_expr: LuaTableExpr,
// ) -> Option<LuaType> {
//     let meta_members =
//         find_members_with_key(db, &meta_type, LuaMemberKey::Name("__index".into()), true)?;
//     for member in meta_members {
//         let index_members = find_members(db, &member.typ)?;
//         let table_type = infer_expr(db, cache, LuaExpr::TableExpr(table_expr.clone())).ok()?;
//         let table_members = find_members(db, &table_type)?;
//         // 如果 index_members 包含了 table_members 中的所有成员，则返回 meta_type
//         if table_members.iter().all(|table_member| {
//             index_members
//                 .iter()
//                 .any(|index_member| index_member.key.to_path() == table_member.key.to_path())
//         }) {
//             return Some(meta_type);
//         }
//     }
//     None
// }

fn infer_metatable_index_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    metatable: LuaExpr,
) -> Result<(LuaType, bool /*__index type*/), InferFailReason> {
    match &metatable {
        LuaExpr::TableExpr(table) => {
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
        }
        _ => {}
    };

    let meta_type = infer_expr(db, cache, metatable)?;
    if let Some(meta_members) =
        find_members_with_key(db, &meta_type, LuaMemberKey::Name("__index".into()), false)
    {
        if let Some(meta_member) = meta_members.first() {
            if meta_member.typ.is_custom_type() {
                return Ok((meta_member.typ.clone(), true));
            }
        }
    }

    Ok((meta_type, false))
}
