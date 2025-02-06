use emmylua_parser::{LuaAstNode, LuaTableExpr};

use crate::db_index::{DbIndex, LuaType, LuaTupleType};

use super::{InferResult, LuaInferConfig, infer_expr};

pub fn infer_table_expr(db: &DbIndex, config: &mut LuaInferConfig, table: LuaTableExpr) -> InferResult {
    if table.is_array() {
        infer_table_array_expr(db, config, table)
    } else {
        Some(LuaType::TableConst(crate::InFiled {
            file_id: config.get_file_id(),
            value: table.get_range(),
        }))
    }
}

fn infer_table_array_expr(db: &DbIndex, config: &mut LuaInferConfig, table: LuaTableExpr) -> InferResult {
    let field_types = table.get_fields()
        .map(|field| infer_expr(db, config, field.get_value_expr()?))
        .map(|field_type| field_type.unwrap_or(LuaType::Unknown))
        .collect::<Vec<_>>();
    Some(LuaTupleType::new(field_types).into())
}
