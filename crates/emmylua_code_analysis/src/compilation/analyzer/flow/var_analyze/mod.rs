mod broadcast_down;
mod broadcast_inside;
mod broadcast_outside;
mod broadcast_up;
mod unresolve_trace;
mod var_trace;
mod var_trace_info;

use std::sync::Arc;

use broadcast_down::broadcast_down_after_node;
pub use broadcast_up::broadcast_up;
use emmylua_parser::{
    BinaryOperator, LuaAssignStat, LuaAst, LuaAstNode, LuaBinaryExpr, LuaCallArgList, LuaCallExpr,
    LuaCallExprStat, LuaCommentOwner, LuaDocTag, LuaExpr, LuaLiteralToken, LuaVarExpr,
};

use crate::{
    db_index::{LuaType, TypeAssertion},
    DbIndex, FileId, LuaDeclId, LuaMemberId, LuaTypeDeclId, LuaTypeOwner,
};
#[allow(unused)]
pub use unresolve_trace::{UnResolveTraceId, UnResolveTraceInfo};
pub use var_trace::VarTrace;
pub use var_trace_info::VarTraceInfo;

pub fn analyze_ref_expr(
    db: &mut DbIndex,
    var_trace: &mut VarTrace,
    var_expr: &LuaVarExpr,
) -> Option<()> {
    let parent = var_expr.get_parent::<LuaAst>()?;
    let trace_info = Arc::new(VarTraceInfo::new(
        TypeAssertion::Exist,
        LuaAst::cast(var_expr.syntax().clone())?,
    ));
    broadcast_up(db, var_trace, trace_info, parent);

    Some(())
}

pub fn analyze_ref_assign(
    db: &mut DbIndex,
    var_trace: &mut VarTrace,
    var_expr: &LuaVarExpr,
    file_id: FileId,
) -> Option<()> {
    let assign_stat = var_expr.get_parent::<LuaAssignStat>()?;
    if is_decl_assign_stat(assign_stat.clone()).unwrap_or(false) {
        let type_owner = match var_expr {
            LuaVarExpr::IndexExpr(index_expr) => {
                let member_id = LuaMemberId::new(index_expr.get_syntax_id(), file_id);
                LuaTypeOwner::Member(member_id)
            }
            LuaVarExpr::NameExpr(name_expr) => {
                let decl_id = LuaDeclId::new(file_id, name_expr.get_position());
                LuaTypeOwner::Decl(decl_id)
            }
        };
        if let Some(type_cache) = db.get_type_index().get_type_cache(&type_owner) {
            let type_assert = TypeAssertion::Narrow(type_cache.as_type().clone());
            broadcast_down_after_node(
                db,
                var_trace,
                Arc::new(VarTraceInfo::new(
                    type_assert,
                    LuaAst::cast(var_expr.syntax().clone())?,
                )),
                LuaAst::LuaAssignStat(assign_stat),
                true,
            );
        }

        return None;
    }

    let (var_exprs, value_exprs) = assign_stat.get_var_and_expr_list();
    let var_index = var_exprs
        .iter()
        .position(|it| it.get_position() == var_expr.get_position())?;

    if value_exprs.len() == 0 {
        return None;
    }

    let (value_expr, idx) = if let Some(expr) = value_exprs.get(var_index) {
        (expr.clone(), 0)
    } else {
        (
            value_exprs.last()?.clone(),
            (var_index - (value_exprs.len() - 1)) as i32,
        )
    };

    let type_assert = TypeAssertion::Reassign((value_expr.get_syntax_id(), idx));
    broadcast_down_after_node(
        db,
        var_trace,
        Arc::new(VarTraceInfo::new(
            type_assert,
            LuaAst::cast(value_expr.syntax().clone())?,
        )),
        LuaAst::LuaAssignStat(assign_stat),
        true,
    );

    Some(())
}

fn is_decl_assign_stat(assign_stat: LuaAssignStat) -> Option<bool> {
    for comment in assign_stat.get_comments() {
        for tag in comment.get_doc_tags() {
            match tag {
                LuaDocTag::Type(_)
                | LuaDocTag::Class(_)
                | LuaDocTag::Module(_)
                | LuaDocTag::Enum(_) => {
                    return Some(true);
                }
                _ => {}
            }
        }
    }
    Some(false)
}

fn infer_call_arg_list(
    db: &mut DbIndex,
    var_trace: &mut VarTrace,
    trace_info: Arc<VarTraceInfo>,
    call_arg: LuaCallArgList,
) -> Option<()> {
    let parent = call_arg.get_parent::<LuaAst>()?;
    match parent {
        LuaAst::LuaCallExpr(call_expr) => {
            if call_expr.is_type() && trace_info.type_assertion.is_exist() {
                infer_lua_type_assert(db, var_trace, call_expr);
            } else if call_expr.is_assert() {
                broadcast_down_after_node(
                    db,
                    var_trace,
                    trace_info,
                    LuaAst::LuaCallExprStat(call_expr.get_parent::<LuaCallExprStat>()?),
                    true,
                );
            }
        }
        _ => {}
    }

    Some(())
}

fn infer_lua_type_assert(
    db: &mut DbIndex,
    var_trace: &mut VarTrace,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let binary_expr = call_expr.get_parent::<LuaBinaryExpr>()?;
    let op = binary_expr.get_op_token()?;
    let mut is_eq = true;
    match op.get_op() {
        BinaryOperator::OpEq => {}
        BinaryOperator::OpNe => {
            is_eq = false;
        }
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

    let mut type_assert = match type_literal.as_str() {
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

    if !is_eq {
        type_assert = type_assert.get_negation()?;
    }

    broadcast_up(
        db,
        var_trace,
        VarTraceInfo::new(type_assert, LuaAst::cast(binary_expr.syntax().clone())?).into(),
        binary_expr.get_parent::<LuaAst>()?,
    );

    Some(())
}
