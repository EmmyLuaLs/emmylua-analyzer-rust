use emmylua_parser::{
    BinaryOperator, LuaAssignStat, LuaAst, LuaAstNode, LuaBinaryExpr, LuaBlock, LuaCallArgList,
    LuaCallExpr, LuaExpr, LuaLiteralToken, LuaStat, LuaVarExpr, UnaryOperator,
};
use rowan::TextRange;
use smol_str::SmolStr;

use crate::{
    db_index::{LuaType, TypeAssertion},
    DbIndex, LuaFlowChain, LuaTypeDeclId,
};

pub fn analyze_ref_expr(
    db: &mut DbIndex,
    flow_chain: &mut LuaFlowChain,
    expr: &LuaExpr,
    path: &str,
) -> Option<()> {
    let parent = expr.get_parent::<LuaAst>()?;
    broadcast_up(
        db,
        flow_chain,
        &path,
        parent,
        LuaAst::cast(expr.syntax().clone())?,
        TypeAssertion::Exist,
    );

    Some(())
}

pub fn analyze_ref_assign(
    db: &mut DbIndex,
    flow_chain: &mut LuaFlowChain,
    var_expr: LuaVarExpr,
    path: &str,
) -> Option<()> {
    let assign_stat = var_expr.get_parent::<LuaAssignStat>()?;
    let (var_exprs, value_exprs) = assign_stat.get_var_and_expr_list();
    let index = var_exprs
        .iter()
        .position(|it| it.get_position() == var_expr.get_position())?;

    if value_exprs.len() == 0 {
        return None;
    }

    let (value_expr, idx) = if let Some(expr) = value_exprs.get(index) {
        (expr.clone(), 0)
    } else {
        (
            value_exprs.last()?.clone(),
            (index - value_exprs.len()) as i32,
        )
    };

    let type_assert = TypeAssertion::Reassign((value_expr.get_syntax_id(), idx));
    broadcast_down(
        db,
        flow_chain,
        path,
        LuaAst::LuaAssignStat(assign_stat),
        type_assert,
        true,
    );

    Some(())
}

fn broadcast_up(
    db: &mut DbIndex,
    flow_chain: &mut LuaFlowChain,
    path: &str,
    parent: LuaAst,
    origin: LuaAst,
    type_assert: TypeAssertion,
) -> Option<()> {
    match parent {
        LuaAst::LuaIfStat(if_stat) => {
            // this mean the name_expr is a condition and the name_expr is not nil and is not false
            if let Some(block) = if_stat.get_block() {
                flow_chain.add_type_assert(path, type_assert.clone(), block.get_range());
            }

            if let Some(ne_type_assert) = type_assert.get_negation() {
                if let Some(else_stat) = if_stat.get_else_clause() {
                    let range = else_stat.get_range();
                    flow_chain.add_type_assert(path, ne_type_assert, range);
                } else if is_block_has_return(if_stat.get_block()?).unwrap_or(false) {
                    let parent_block = if_stat.get_parent::<LuaBlock>()?;
                    let parent_range = parent_block.get_range();
                    let if_range = if_stat.get_range();
                    if if_range.end() < parent_range.end() {
                        let range = TextRange::new(if_range.end(), parent_range.end());
                        flow_chain.add_type_assert(path, ne_type_assert, range);
                    }
                }
            }
        }
        LuaAst::LuaWhileStat(while_stat) => {
            // this mean the name_expr is a condition and the name_expr is not nil and is not false
            let block = while_stat.get_block()?;
            flow_chain.add_type_assert(path, type_assert, block.get_range());
        }
        LuaAst::LuaElseIfClauseStat(else_if_clause_stat) => {
            // this mean the name_expr is a condition and the name_expr is not nil and is not false
            let block = else_if_clause_stat.get_block()?;
            flow_chain.add_type_assert(path, type_assert, block.get_range());
        }
        LuaAst::LuaBinaryExpr(binary_expr) => {
            let op = binary_expr.get_op_token()?;
            match op.get_op() {
                BinaryOperator::OpAnd => {
                    let (left, right) = binary_expr.get_exprs()?;
                    if left.get_position() == origin.get_position() {
                        flow_chain.add_type_assert(path, type_assert.clone(), right.get_range());
                    }

                    broadcast_up(
                        db,
                        flow_chain,
                        path,
                        binary_expr.get_parent::<LuaAst>()?,
                        LuaAst::LuaBinaryExpr(binary_expr),
                        type_assert,
                    );
                }
                BinaryOperator::OpOr => {
                    let (left, right) = binary_expr.get_exprs()?;
                    if left.get_position() == origin.get_position() {
                        if let Some(ne) = type_assert.get_negation() {
                            flow_chain.add_type_assert(path, ne, right.get_range());
                        }
                    }
                }
                BinaryOperator::OpEq => {
                    let (left, right) = binary_expr.get_exprs()?;
                    let expr = if left.get_position() == origin.get_position() {
                        right
                    } else {
                        left
                    };

                    if let LuaExpr::LiteralExpr(literal) = expr {
                        let type_assert = match literal.get_literal()? {
                            LuaLiteralToken::Nil(_) => TypeAssertion::NotExist,
                            LuaLiteralToken::Bool(b) => {
                                if b.is_true() {
                                    TypeAssertion::Exist
                                } else {
                                    TypeAssertion::NotExist
                                }
                            }
                            LuaLiteralToken::Number(i) => {
                                if i.is_int() {
                                    TypeAssertion::Narrow(LuaType::IntegerConst(i.get_int_value()))
                                } else {
                                    TypeAssertion::Narrow(LuaType::Number)
                                }
                            }
                            LuaLiteralToken::String(s) => TypeAssertion::Narrow(
                                LuaType::StringConst(SmolStr::new(s.get_value()).into()),
                            ),
                            _ => return None,
                        };

                        broadcast_up(
                            db,
                            flow_chain,
                            path,
                            binary_expr.get_parent::<LuaAst>()?,
                            LuaAst::LuaBinaryExpr(binary_expr),
                            type_assert,
                        );
                    }
                }
                BinaryOperator::OpNe => {
                    let (left, right) = binary_expr.get_exprs()?;
                    let expr = if left.get_position() == origin.get_position() {
                        right
                    } else {
                        left
                    };

                    if let LuaExpr::LiteralExpr(literal) = expr {
                        let type_assert = match literal.get_literal()? {
                            LuaLiteralToken::Nil(_) => TypeAssertion::Exist,
                            LuaLiteralToken::Bool(b) => {
                                if b.is_true() {
                                    TypeAssertion::NotExist
                                } else {
                                    TypeAssertion::Exist
                                }
                            }
                            LuaLiteralToken::Number(i) => {
                                if i.is_int() {
                                    TypeAssertion::Remove(LuaType::IntegerConst(i.get_int_value()))
                                } else {
                                    TypeAssertion::Remove(LuaType::Number)
                                }
                            }
                            LuaLiteralToken::String(s) => TypeAssertion::Remove(
                                LuaType::StringConst(SmolStr::new(s.get_value()).into()),
                            ),
                            _ => return None,
                        };

                        broadcast_up(
                            db,
                            flow_chain,
                            path,
                            binary_expr.get_parent::<LuaAst>()?,
                            LuaAst::LuaBinaryExpr(binary_expr),
                            type_assert,
                        );
                    }
                }

                _ => {}
            }
        }
        LuaAst::LuaCallArgList(call_args_list) => {
            infer_call_arg_list(db, flow_chain, type_assert, path, call_args_list)?;
        }
        LuaAst::LuaUnaryExpr(unary_expr) => {
            let op = unary_expr.get_op_token()?;
            match op.get_op() {
                UnaryOperator::OpNot => {
                    if let Some(ne_type_assert) = type_assert.get_negation() {
                        broadcast_up(
                            db,
                            flow_chain,
                            path,
                            unary_expr.get_parent::<LuaAst>()?,
                            LuaAst::LuaUnaryExpr(unary_expr),
                            ne_type_assert,
                        );
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
    Some(())
}

fn broadcast_down(
    db: &mut DbIndex,
    flow_chain: &mut LuaFlowChain,
    path: &str,
    node: LuaAst,
    type_assert: TypeAssertion,
    continue_broadcast_outside: bool,
) -> Option<()> {
    let parent_block = node.get_parent::<LuaBlock>()?;
    let parent_range = parent_block.get_range();
    let range = node.get_range();
    if range.end() < parent_range.end() {
        let range = TextRange::new(range.end(), parent_range.end());
        flow_chain.add_type_assert(path, type_assert.clone(), range);
    }

    if continue_broadcast_outside {
        broadcast_outside(db, flow_chain, path, parent_block, type_assert);
    }

    Some(())
}

fn broadcast_outside(
    db: &mut DbIndex,
    flow_chain: &mut LuaFlowChain,
    path: &str,
    node: LuaBlock,
    type_assert: TypeAssertion,
) -> Option<()> {
    let parent = node.get_parent::<LuaAst>()?;
    match &parent {
        LuaAst::LuaIfStat(_)
        | LuaAst::LuaDoStat(_)
        | LuaAst::LuaWhileStat(_)
        | LuaAst::LuaForStat(_)
        | LuaAst::LuaForRangeStat(_)
        | LuaAst::LuaRepeatStat(_) => {
            broadcast_down(db, flow_chain, path, parent, type_assert, false);
        }
        LuaAst::LuaElseIfClauseStat(_) | LuaAst::LuaElseClauseStat(_) => {
            broadcast_down(
                db,
                flow_chain,
                path,
                parent.get_parent::<LuaAst>()?,
                type_assert,
                false,
            );
        }
        _ => {}
    }

    Some(())
}

fn infer_call_arg_list(
    db: &mut DbIndex,
    flow_chain: &mut LuaFlowChain,
    type_assert: TypeAssertion,
    path: &str,
    call_arg: LuaCallArgList,
) -> Option<()> {
    let parent = call_arg.get_parent::<LuaAst>()?;
    match parent {
        LuaAst::LuaCallExpr(call_expr) => {
            if let LuaExpr::NameExpr(prefix_expr) = call_expr.get_prefix_expr()? {
                let name_text = prefix_expr.get_name_text()?;
                match name_text.as_str() {
                    "type" => {
                        infer_lua_type_assert(db, flow_chain, path, call_expr);
                    }
                    "assert" => {
                        infer_lua_assert(db, flow_chain, type_assert, path, call_expr);
                    }
                    _ => {}
                };
            }
        }
        _ => {}
    }

    Some(())
}

fn infer_lua_type_assert(
    db: &mut DbIndex,
    flow_chain: &mut LuaFlowChain,
    path: &str,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let binary_expr = call_expr.get_parent::<LuaBinaryExpr>()?;
    let op = binary_expr.get_op_token()?;
    match op.get_op() {
        BinaryOperator::OpEq => {}
        _ => return None,
    };

    let operands = binary_expr.get_exprs()?;
    let literal_expr = if let LuaExpr::LiteralExpr(literal) = operands.0 {
        literal
    } else if let LuaExpr::LiteralExpr(literal) = operands.1 {
        literal
    } else {
        return None;
    };

    let type_literal = match literal_expr.get_literal()? {
        LuaLiteralToken::String(string) => string.get_value(),
        _ => return None,
    };

    let type_assert = match type_literal.as_str() {
        "number" => TypeAssertion::Narrow(LuaType::Number),
        "string" => TypeAssertion::Narrow(LuaType::String),
        "boolean" => TypeAssertion::Narrow(LuaType::Boolean),
        "table" => TypeAssertion::Narrow(LuaType::Table),
        "function" => TypeAssertion::Narrow(LuaType::Function),
        "thread" => TypeAssertion::Narrow(LuaType::Thread),
        "userdata" => TypeAssertion::Narrow(LuaType::Userdata),
        "nil" => TypeAssertion::Narrow(LuaType::Nil),
        // extend usage
        str => TypeAssertion::Narrow(LuaType::Ref(LuaTypeDeclId::new(str))),
    };

    broadcast_up(
        db,
        flow_chain,
        path,
        binary_expr.get_parent::<LuaAst>()?,
        LuaAst::LuaBinaryExpr(binary_expr),
        type_assert,
    );

    Some(())
}

fn is_block_has_return(block: LuaBlock) -> Option<bool> {
    for stat in block.get_stats() {
        if is_stat_change_flow(stat.clone()).unwrap_or(false) {
            return Some(true);
        }
    }

    Some(false)
}

fn is_stat_change_flow(stat: LuaStat) -> Option<bool> {
    match stat {
        LuaStat::CallExprStat(call_stat) => {
            let call_expr = call_stat.get_call_expr()?;
            let prefix_expr = call_expr.get_prefix_expr()?;
            if let LuaExpr::NameExpr(name_expr) = prefix_expr {
                let name = name_expr.get_name_text()?;
                if name == "error" {
                    return Some(true);
                }
            }
            Some(false)
        }
        LuaStat::ReturnStat(_) => Some(true),
        LuaStat::DoStat(do_stat) => {
            Some(is_block_has_return(do_stat.get_block()?).unwrap_or(false))
        }
        _ => Some(false),
    }
}

fn infer_lua_assert(
    db: &mut DbIndex,
    flow_chain: &mut LuaFlowChain,
    type_assert: TypeAssertion,
    path: &str,
    call_expr: LuaCallExpr,
) -> Option<()> {
    broadcast_down(
        db,
        flow_chain,
        path,
        LuaAst::LuaCallExpr(call_expr),
        type_assert,
        false,
    );
    Some(())
}
