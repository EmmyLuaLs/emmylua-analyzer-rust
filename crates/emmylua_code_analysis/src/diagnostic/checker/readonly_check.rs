//! Readonly check — salsa-native.

use emmylua_parser::{LuaAssignStat, LuaAstNode, LuaExpr, LuaSyntaxId, LuaSyntaxKind};
use rowan::TextRange;

use crate::compilation::SalsaDocTagPropertyEntrySummary;
use crate::semantic_model::{OwnerPosition, SemanticModel};
use crate::{DiagnosticCode, LuaDeclId, LuaMemberId, LuaSemanticDeclId, SemanticDeclLevel};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for assign in root.descendants::<LuaAssignStat>() {
        check_assign(context, model, &assign);
    }
}

fn check_assign(context: &mut DiagnosticContext, model: &SemanticModel, assign: &LuaAssignStat) {
    let (vars, _) = assign.get_var_and_expr_list();
    for var in vars {
        let mut cur = LuaExpr::cast(var.syntax().clone());
        while let Some(expr) = cur {
            let Some(decl_id) =
                model.find_decl_by_node(expr.syntax().clone(), SemanticDeclLevel::default())
            else {
                break;
            };
            check_readonly(context, model, expr.get_range(), &decl_id);
            cur = match &expr {
                LuaExpr::IndexExpr(ix) => ix.get_prefix_expr(),
                _ => break,
            };
        }
    }
}

fn check_readonly(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    range: TextRange,
    decl_id: &LuaSemanticDeclId,
) {
    let (file_id, offset) = match decl_id {
        LuaSemanticDeclId::LuaDecl(id) => {
            let self_id = LuaDeclId::new(context.get_file_id(), range.start());
            if *id == self_id {
                return;
            }
            (id.file_id, id.position)
        }
        LuaSemanticDeclId::Member(member_id) => {
            let syntax_id = LuaSyntaxId::new(LuaSyntaxKind::IndexExpr.into(), range);
            let self_id = LuaMemberId::new(syntax_id, context.get_file_id());
            if *member_id == self_id {
                return;
            }
            (
                member_id.file_id,
                member_id.get_syntax_id().get_range().start(),
            )
        }
        _ => return,
    };

    if model.decl_has_doc_property(file_id, OwnerPosition(offset), SalsaDocTagPropertyEntrySummary::Readonly) {
        context.add_diagnostic(
            DiagnosticCode::ReadOnly,
            range,
            t!("The variable is marked as readonly and cannot be assigned to.").to_string(),
            None,
        );
    }
}
