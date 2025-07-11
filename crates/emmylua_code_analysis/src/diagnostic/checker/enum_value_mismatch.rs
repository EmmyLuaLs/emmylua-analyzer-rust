use emmylua_parser::{BinaryOperator, LuaAst, LuaAstNode, LuaBinaryExpr, LuaExpr};

use crate::{
    diagnostic::checker::humanize_lint_type, DiagnosticCode, LuaMemberKey, LuaType, LuaTypeDeclId,
    SemanticModel,
};

use super::{Checker, DiagnosticContext};

pub struct EnumValueMismatchChecker;

impl Checker for EnumValueMismatchChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::EnumValueMismatch];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for node in root.descendants::<LuaAst>() {
            let condition_expr = match node {
                LuaAst::LuaIfStat(if_stat) => if_stat.get_condition_expr(),
                LuaAst::LuaElseIfClauseStat(elseif_stat) => elseif_stat.get_condition_expr(),
                _ => None,
            };

            if let Some(expr) = condition_expr {
                check_condition_expr(context, semantic_model, expr);
            }
        }
    }
}

fn check_condition_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    condition_expr: LuaExpr,
) -> Option<()> {
    if let LuaExpr::BinaryExpr(binary_expr) = condition_expr {
        check_binary_expr(context, semantic_model, binary_expr);
    }
    Some(())
}

fn check_binary_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    binary_expr: LuaBinaryExpr,
) -> Option<()> {
    let op_token = binary_expr.get_op_token()?;
    let operator = op_token.get_op();

    if !matches!(operator, BinaryOperator::OpEq | BinaryOperator::OpNe) {
        return Some(());
    }

    let (left_expr, right_expr) = binary_expr.get_exprs()?;
    let left_type = semantic_model.infer_expr(left_expr.clone()).ok()?;
    let right_type = semantic_model.infer_expr(right_expr.clone()).ok()?;

    if check_enum_value_pair(context, &right_expr, &left_type, &right_type).is_some() {
        return Some(());
    }
    if check_enum_value_pair(context, &left_expr, &right_type, &left_type).is_some() {
        return Some(());
    }

    Some(())
}

fn check_enum_value_pair(
    context: &mut DiagnosticContext,
    value_expr: &LuaExpr,
    enum_type: &LuaType,
    value_type: &LuaType,
) -> Option<()> {
    let enum_decl_id = match &enum_type {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        _ => return None,
    };
    let type_decl = context.db.get_type_index().get_type_decl(enum_decl_id)?;
    if !type_decl.is_enum() {
        return None;
    }

    let constant_type = get_constant_type(value_type)?;
    let enum_value_types = get_enum_value_types(context, enum_decl_id)?;

    if !enum_value_types.contains(constant_type) {
        let constant_value_str = humanize_lint_type(context.db, constant_type);
        let enum_values_str: Vec<String> = enum_value_types
            .iter()
            .map(|typ| humanize_lint_type(context.db, typ))
            .collect();
        context.add_diagnostic(
            DiagnosticCode::EnumValueMismatch,
            value_expr.get_range(),
            t!(
                "Value '%{value}' does not match any enum value. Expected one of: %{enum_values}",
                value = constant_value_str,
                enum_values = enum_values_str.join(", ")
            )
            .to_string(),
            None,
        );
    }

    Some(())
}

fn get_enum_value_types(
    context: &DiagnosticContext,
    enum_decl_id: &LuaTypeDeclId,
) -> Option<Vec<LuaType>> {
    let type_decl = context.db.get_type_index().get_type_decl(enum_decl_id)?;
    let mut values = Vec::new();
    let is_enum_key = type_decl.is_enum_key();

    if let Some(members) = context
        .db
        .get_member_index()
        .get_members(&enum_decl_id.clone().into())
    {
        for member in members {
            if is_enum_key {
                let key = member.get_key();
                match key {
                    LuaMemberKey::Name(name) => {
                        values.push(LuaType::StringConst(name.clone().into()));
                    }
                    LuaMemberKey::Integer(i) => {
                        values.push(LuaType::IntegerConst(*i));
                    }
                    LuaMemberKey::ExprType(typ) => {
                        if let Some(value) = get_constant_type(typ) {
                            values.push(value.clone());
                        }
                    }
                    _ => {}
                }
            } else {
                if let Some(type_cache) = context
                    .db
                    .get_type_index()
                    .get_type_cache(&member.get_id().into())
                {
                    if let Some(value) = get_constant_type(type_cache.as_type()) {
                        values.push(value.clone());
                    }
                }
            }
        }
    }

    Some(values)
}

fn get_constant_type(typ: &LuaType) -> Option<&LuaType> {
    match typ {
        LuaType::StringConst(_)
        | LuaType::DocStringConst(_)
        | LuaType::IntegerConst(_)
        | LuaType::DocIntegerConst(_)
        | LuaType::FloatConst(_)
        | LuaType::BooleanConst(_)
        | LuaType::DocBooleanConst(_)
        | LuaType::TableConst(_) => Some(typ),
        _ => None,
    }
}
