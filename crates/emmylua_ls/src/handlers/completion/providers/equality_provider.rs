use emmylua_code_analysis::{
    DbIndex, InferGuard, LuaDeclId, LuaType, get_real_type, infer_table_field_value_should_be,
    infer_table_should_be,
};
use emmylua_parser::{
    BinaryOperator, LuaAst, LuaAstNode, LuaAstToken, LuaLiteralExpr, LuaTokenKind,
};

use crate::handlers::completion::{
    completion_builder::CompletionBuilder, providers::function_provider::dispatch_type,
};

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let types = get_token_should_type(builder)?;
    for typ in &types {
        dispatch_type(builder, typ.clone(), &InferGuard::new());
    }

    if !types.is_empty() && !builder.is_invoked() {
        builder.stop_here();
    }
    Some(())
}

fn get_token_should_type(builder: &mut CompletionBuilder) -> Option<Vec<LuaType>> {
    let token = builder.trigger_token.clone();
    let mut parent_node = token.parent()?;
    if let Some(node) = token.prev_token()?.parent() {
        parent_node = node;
    } else {
        // 输入`""`时允许往上找
        if LuaLiteralExpr::can_cast(parent_node.kind().into()) {
            parent_node = parent_node.parent()?;
        }
    }

    match LuaAst::cast(parent_node)? {
        // == 和 ~= 操作符
        LuaAst::LuaBinaryExpr(binary_expr) => {
            let op_token = binary_expr.get_op_token()?;
            let op = op_token.get_op();
            if op == BinaryOperator::OpEq || op == BinaryOperator::OpNe {
                let left = binary_expr.get_left_expr()?;
                let left_type = builder.semantic_model.infer_expr(left);

                if let Ok(typ) = left_type {
                    return Some(vec![typ]);
                }
            }
        }
        LuaAst::LuaLocalStat(local_stat) => {
            let locals = local_stat.get_local_name_list().collect::<Vec<_>>();
            if locals.len() != 1 {
                return None;
            }

            let position = builder.trigger_token.text_range().start();
            let eq = local_stat.token_by_kind(LuaTokenKind::TkAssign)?;
            if position < eq.get_position() {
                return None;
            }
            let local = locals.first()?;
            let decl_id =
                LuaDeclId::new(builder.semantic_model.get_file_id(), local.get_position());
            let decl_type = builder
                .semantic_model
                .get_db()
                .get_type_index()
                .get_type_cache(&decl_id.into())?;
            let typ = decl_type.as_type().clone();
            if contain_function_types(builder.semantic_model.get_db(), &typ).is_none() {
                return Some(vec![typ]);
            }
        }
        LuaAst::LuaAssignStat(assign_stat) => {
            let (vars, _) = assign_stat.get_var_and_expr_list();

            if vars.len() != 1 {
                return None;
            }

            let position = builder.trigger_token.text_range().start();
            let eq = assign_stat.token_by_kind(LuaTokenKind::TkAssign)?;
            if position < eq.get_position() {
                return None;
            }

            let var = vars.first()?;
            let var_type = builder.semantic_model.infer_expr(var.to_expr());
            if let Ok(typ) = var_type
            // this is to avoid repeating function types in completion
                && contain_function_types(builder.semantic_model.get_db(), &typ).is_none()
            {
                return Some(vec![typ]);
            }
        }
        LuaAst::LuaTableExpr(table_expr) => {
            let table_type = infer_table_should_be(
                builder.semantic_model.get_db(),
                &mut builder.semantic_model.get_cache().borrow_mut(),
                table_expr,
            );
            if let Ok(typ) = table_type
                && let LuaType::Array(array_type) = typ
            {
                return Some(vec![array_type.get_base().clone()]);
            }
        }
        LuaAst::LuaTableField(table_field) => {
            if table_field.is_value_field() {
                return None;
            }

            let typ = infer_table_field_value_should_be(
                builder.semantic_model.get_db(),
                &mut builder.semantic_model.get_cache().borrow_mut(),
                table_field,
            )
            .ok()?;
            return Some(vec![typ]);
        }
        _ => {}
    }

    None
}

pub fn contain_function_types(db: &DbIndex, typ: &LuaType) -> Option<()> {
    match typ {
        LuaType::Union(union_typ) => {
            for member in union_typ.into_vec().iter() {
                match member {
                    _ if member.is_function() => {
                        return Some(());
                    }
                    _ if member.is_custom_type() => {
                        let real_type = get_real_type(db, member)?;
                        if real_type.is_function() {
                            return Some(());
                        }
                    }
                    _ => {
                        continue;
                    }
                }
            }

            None
        }
        _ if typ.is_function() => Some(()),
        _ => None,
    }
}
