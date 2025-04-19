use std::sync::Arc;

use emmylua_parser::{
    BinaryOperator, LuaAst, LuaAstNode, LuaBinaryExpr, LuaExpr, LuaLiteralToken, UnaryOperator,
};
use smol_str::SmolStr;

use crate::{DbIndex, LuaType, TypeAssertion};

use super::{
    broadcast_inside::broadcast_inside_condition_block, infer_call_arg_list,
    unresolve_trace::UnResolveTraceId, var_trace_info::VarTraceInfo, VarTrace,
};

pub fn broadcast_up(
    db: &mut DbIndex,
    var_trace: &mut VarTrace,
    trace_info: Arc<VarTraceInfo>,
    current: LuaAst,
) -> Option<()> {
    match current {
        LuaAst::LuaIfStat(if_stat) => {
            if let Some(block) = if_stat.get_block() {
                broadcast_inside_condition_block(db, var_trace, trace_info.clone(), block, true);
            }

            // todo
            if !trace_info.check_cover_all_branch() {
                return Some(());
            }

            if let Some(ne_type_assert) = trace_info.type_assertion.get_negation() {
                if let Some(else_stat) = if_stat.get_else_clause() {
                    broadcast_inside_condition_block(
                        db,
                        var_trace,
                        trace_info.with_type_assertion(ne_type_assert.clone()),
                        else_stat.get_block()?,
                        true,
                    );
                }

                for else_if_clause in if_stat.get_else_if_clause_list() {
                    let range = else_if_clause.get_range();
                    var_trace.add_assert(ne_type_assert.clone(), range);
                }
            }
        }
        LuaAst::LuaWhileStat(while_stat) => {
            // this mean the name_expr is a condition and the name_expr is not nil and is not false
            let block = while_stat.get_block()?;
            broadcast_inside_condition_block(db, var_trace, trace_info, block, false);
        }
        LuaAst::LuaElseIfClauseStat(else_if_clause_stat) => {
            // this mean the name_expr is a condition and the name_expr is not nil and is not false
            if let Some(block) = else_if_clause_stat.get_block() {
                broadcast_inside_condition_block(db, var_trace, trace_info, block, false);
            }
        }
        LuaAst::LuaParenExpr(paren_expr) => {
            broadcast_up(
                db,
                var_trace,
                trace_info,
                paren_expr.get_parent::<LuaAst>()?,
            );
        }
        LuaAst::LuaBinaryExpr(binary_expr) => {
            let op = binary_expr.get_op_token()?;
            match op.get_op() {
                BinaryOperator::OpAnd => {
                    broadcast_up_and(db, var_trace, trace_info, binary_expr.clone());
                }
                BinaryOperator::OpOr => {
                    broadcast_up_or(db, var_trace, trace_info, binary_expr.clone());
                }
                BinaryOperator::OpEq => {
                    let (left, right) = binary_expr.get_exprs()?;
                    let expr = if left.get_position() == trace_info.node.get_position() {
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
                                    TypeAssertion::Force(LuaType::IntegerConst(i.get_int_value()))
                                } else {
                                    TypeAssertion::Force(LuaType::Number)
                                }
                            }
                            LuaLiteralToken::String(s) => TypeAssertion::Force(
                                LuaType::StringConst(SmolStr::new(s.get_value()).into()),
                            ),
                            _ => return None,
                        };

                        broadcast_up(
                            db,
                            var_trace,
                            VarTraceInfo::new(
                                type_assert,
                                LuaAst::cast(binary_expr.syntax().clone())?,
                            )
                            .into(),
                            binary_expr.get_parent::<LuaAst>()?,
                        );
                    }
                }
                BinaryOperator::OpNe => {
                    let (left, right) = binary_expr.get_exprs()?;
                    let expr = if left.get_position() == trace_info.node.get_position() {
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
                            var_trace,
                            VarTraceInfo::new(
                                type_assert,
                                LuaAst::cast(binary_expr.syntax().clone())?,
                            )
                            .into(),
                            binary_expr.get_parent::<LuaAst>()?,
                        );
                    }
                }

                _ => {}
            }
        }
        LuaAst::LuaCallArgList(call_args_list) => {
            infer_call_arg_list(db, var_trace, trace_info, call_args_list)?;
        }
        LuaAst::LuaUnaryExpr(unary_expr) => {
            let op = unary_expr.get_op_token()?;
            match op.get_op() {
                UnaryOperator::OpNot => {
                    if let Some(ne_type_assert) = trace_info.type_assertion.get_negation() {
                        broadcast_up(
                            db,
                            var_trace,
                            VarTraceInfo::new(
                                ne_type_assert,
                                LuaAst::cast(unary_expr.syntax().clone())?,
                            )
                            .into(),
                            unary_expr.get_parent::<LuaAst>()?,
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

pub fn broadcast_up_and(
    db: &mut DbIndex,
    var_trace: &mut VarTrace,
    trace_info: Arc<VarTraceInfo>,
    binary_expr: LuaBinaryExpr,
) -> Option<()> {
    let (left, right) = binary_expr.get_exprs()?;
    if left.get_range().contains(trace_info.node.get_position()) {
        var_trace.add_assert(trace_info.type_assertion.clone(), right.get_range());

        if var_trace.check_var_use_in_range(right.get_range()) {
            let trace_id = UnResolveTraceId::Expr(LuaExpr::cast(trace_info.node.syntax().clone())?);
            var_trace.add_unresolve_trace(trace_id, trace_info);
            return Some(());
        }
    } else {
        let left_id = UnResolveTraceId::Expr(left);
        if let Some(left_unresolve_trace_info) = var_trace.pop_unresolve_trace(&left_id) {
            let left_trace_info = left_unresolve_trace_info.1.get_trace_info()?;
            let new_assert = left_trace_info
                .type_assertion
                .and_assert(trace_info.type_assertion.clone());

            broadcast_up(
                db,
                var_trace,
                VarTraceInfo::new(new_assert, LuaAst::cast(binary_expr.syntax().clone())?).into(),
                binary_expr.get_parent::<LuaAst>()?,
            );

            return Some(());
        }
    }

    broadcast_up(
        db,
        var_trace,
        VarTraceInfo::new(
            trace_info.type_assertion.clone(),
            LuaAst::cast(binary_expr.syntax().clone())?,
        )
        .into(),
        binary_expr.get_parent::<LuaAst>()?,
    );

    Some(())
}

pub fn broadcast_up_or(
    db: &mut DbIndex,
    var_trace: &mut VarTrace,
    trace_info: Arc<VarTraceInfo>,
    binary_expr: LuaBinaryExpr,
) -> Option<()> {
    let (left, right) = binary_expr.get_exprs()?;
    if left.get_range().contains(trace_info.node.get_position()) {
        if let Some(ne) = trace_info.type_assertion.get_negation() {
            var_trace.add_assert(ne, right.get_range());
        }

        if var_trace.check_var_use_in_range(right.get_range()) {
            let trace_id = UnResolveTraceId::Expr(LuaExpr::cast(trace_info.node.syntax().clone())?);
            var_trace.add_unresolve_trace(trace_id, trace_info);
            return Some(());
        }
    } else {
        let left_id = UnResolveTraceId::Expr(left);
        if let Some(left_unresolve_trace_info) = var_trace.pop_unresolve_trace(&left_id) {
            let left_trace_info = left_unresolve_trace_info.1.get_trace_info()?;
            let new_assert = left_trace_info
                .type_assertion
                .or_assert(trace_info.type_assertion.clone());
            broadcast_up(
                db,
                var_trace,
                VarTraceInfo::new(new_assert, LuaAst::cast(binary_expr.syntax().clone())?).into(),
                binary_expr.get_parent::<LuaAst>()?,
            );

            return Some(());
        }
    }

    broadcast_up(
        db,
        var_trace,
        VarTraceInfo::new(
            trace_info.type_assertion.clone(),
            LuaAst::cast(binary_expr.syntax().clone())?,
        )
        .into(),
        binary_expr.get_parent::<LuaAst>()?,
    );

    Some(())
}
