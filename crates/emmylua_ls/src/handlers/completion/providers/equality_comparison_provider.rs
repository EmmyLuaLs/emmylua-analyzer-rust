use emmylua_code_analysis::{InferGuard, LuaType};
use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaBinaryExpr, LuaBlock, LuaLiteralExpr, LuaSyntaxKind,
};

use crate::handlers::completion::{
    completion_builder::CompletionBuilder, providers::function_provider::dispatch_type,
};

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }
    if !check_can_add_completion(builder) {
        return None;
    }
    let types = get_token_should_type(builder)?;
    for typ in &types {
        dispatch_type(builder, typ.clone(), &mut InferGuard::new());
    }
    if types.len() > 0 && !builder.is_invoked() {
        builder.stop_here();
    }
    Some(())
}

fn check_can_add_completion(builder: &CompletionBuilder) -> bool {
    // 允许空格字符触发补全
    if builder.is_space_trigger_character {
        return true;
    }

    return true;
}

fn get_token_should_type(builder: &mut CompletionBuilder) -> Option<Vec<LuaType>> {
    let token = builder.trigger_token.clone();
    let mut parent_node = token.parent()?;
    // 如果父节点是块, 则可能是输入未完全, 语法树缺失
    if let Some(_) = LuaBlock::cast(parent_node.clone()) {
        if let Some(node) = token.prev_token()?.parent() {
            if LuaBinaryExpr::can_cast(node.kind().into()) {
                parent_node = node;
            }
        }
    } else {
        // 输入`""`时允许往上找
        if LuaLiteralExpr::can_cast(parent_node.kind().into()) {
            parent_node = parent_node.parent()?;
        }
    }

    match parent_node.kind().into() {
        LuaSyntaxKind::BinaryExpr => {
            let binary_expr = LuaBinaryExpr::cast(parent_node)?;
            let op_token = binary_expr.get_op_token()?;
            let op = op_token.get_op();
            if op == BinaryOperator::OpEq || op == BinaryOperator::OpNe {
                let left = binary_expr.get_left_expr()?;
                let left_type = builder.semantic_model.infer_expr(left);

                match left_type {
                    Ok(typ) => {
                        return Some(vec![typ]);
                    }
                    Err(_) => {}
                }
            }
        }
        _ => {}
    }

    None
}
