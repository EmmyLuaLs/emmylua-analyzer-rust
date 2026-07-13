//! Enum value mismatch — pure salsa.

use emmylua_parser::{BinaryOperator, LuaAst, LuaAstNode, LuaBinaryExpr, LuaExpr};

use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaType};

use super::{DiagnosticContext, humanize_lint_type_salsa};

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for node in root.descendants::<LuaAst>() {
        let cond = match node {
            LuaAst::LuaIfStat(s) => s.get_condition_expr(),
            LuaAst::LuaElseIfClauseStat(s) => s.get_condition_expr(),
            _ => None,
        };
        if let Some(LuaExpr::BinaryExpr(binary)) = cond {
            check_binary(context, model, binary);
        }
    }
}

fn check_binary(context: &mut DiagnosticContext, model: &SemanticModel, binary: LuaBinaryExpr) {
    let Some(op) = binary.get_op_token().map(|t| t.get_op()) else {
        return;
    };
    if !matches!(op, BinaryOperator::OpEq | BinaryOperator::OpNe) {
        return;
    }
    let Some((left, right)) = binary.get_exprs() else {
        return;
    };

    let Ok(left_ty) = model.infer_expr(left.clone()) else {
        return;
    };
    let Ok(right_ty) = model.infer_expr(right.clone()) else {
        return;
    };

    // Check if comparing enum ref to a non-member value
    check_enum_compare(context, model, &left_ty, &right_ty, right.get_range());
    check_enum_compare(context, model, &right_ty, &left_ty, left.get_range());
}

fn check_enum_compare(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    enum_ty: &LuaType,
    val_ty: &LuaType,
    range: rowan::TextRange,
) {
    let type_name = match enum_ty {
        LuaType::Ref(id) | LuaType::Def(id) => {
            if !model.is_enum_type(id) {
                return;
            }
            id.get_name().to_string()
        }
        _ => return,
    };

    let member_value = matches!(
        val_ty,
        LuaType::StringConst(_)
            | LuaType::DocStringConst(_)
            | LuaType::IntegerConst(_)
            | LuaType::DocIntegerConst(_)
    );
    if !member_value {
        return;
    }

    // Check if this value is a valid enum member
    let Some(members) = model.get_property_entries(&type_name) else {
        return;
    };
    let valid = match val_ty {
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => {
            members.iter().any(|m| matches!(&m.key, crate::compilation::SalsaPropertyKeySummary::Name(n) if n.as_str() == s.as_str()))
        }
        LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => {
            let _ik = crate::LuaMemberKey::Integer(*i);
            members.iter().any(|m| m.key == crate::compilation::SalsaPropertyKeySummary::Integer(*i))
        }
        _ => false,
    };

    if !valid {
        context.add_diagnostic(
            DiagnosticCode::EnumValueMismatch,
            range,
            t!(
                "%{val} is not a valid enum value",
                val =
                    humanize_lint_type_salsa(context.get_salsa_db(), context.get_file_id(), val_ty)
            )
            .to_string(),
            None,
        );
    }
}
