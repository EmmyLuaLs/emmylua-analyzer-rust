use emmylua_parser::{LuaAssignStat, LuaAst, LuaAstNode, LuaExpr, LuaSyntaxId, LuaSyntaxKind};
use rowan::{NodeOrToken, TextRange};

use crate::{
    DiagnosticCode, LuaDeclId, LuaMemberId, LuaSemanticDeclId, PropertyDeclFeature,
    SemanticDeclLevel, SemanticModel,
};

use super::{Checker, DiagnosticContext};

pub struct ReadOnlyChecker;

impl Checker for ReadOnlyChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::ReadOnly];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for ast_node in root.descendants::<LuaAst>() {
            match ast_node {
                LuaAst::LuaAssignStat(assign_stat) => {
                    check_assign_stat(context, semantic_model, &assign_stat);
                }
                // need check?
                LuaAst::LuaFuncStat(_) => {}
                // we need known function is readonly
                LuaAst::LuaCallExpr(_) => {}
                _ => {}
            }
        }
    }
}

fn check_and_report_semantic_id(
    context: &mut DiagnosticContext,
    range: TextRange,
    semantic_decl_id: LuaSemanticDeclId,
) -> Option<()> {
    match semantic_decl_id {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            let self_decl_id = LuaDeclId::new(context.file_id, range.start());
            if decl_id == self_decl_id {
                return None;
            }
        }
        LuaSemanticDeclId::Member(member_id) => {
            let syntax_id = LuaSyntaxId::new(LuaSyntaxKind::IndexExpr.into(), range);
            let self_member_id = LuaMemberId::new(syntax_id, context.file_id);
            if member_id == self_member_id {
                return None;
            }
        }
        _ => {}
    }

    // TODO filter self
    let property_index = context.db().get_property_index();
    if let Some(property) = property_index.get_property(&semantic_decl_id) {
        if property
            .decl_features
            .has_feature(PropertyDeclFeature::ReadOnly)
        {
            context.add_diagnostic(
                DiagnosticCode::ReadOnly,
                range,
                t!("The variable is marked as readonly and cannot be assigned to.").to_string(),
                None,
            );
        }
    }

    Some(())
}

fn check_assign_stat(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    assign_stat: &LuaAssignStat,
) -> Option<()> {
    let (vars, _) = assign_stat.get_var_and_expr_list();
    for var in vars {
        let mut var = LuaExpr::cast(var.syntax().clone())?;
        loop {
            let node_or_token = NodeOrToken::Node(var.syntax().clone());
            let semantic_decl_id =
                semantic_model.find_decl(node_or_token, SemanticDeclLevel::default());
            if let Some(semantic_decl_id) = semantic_decl_id {
                check_and_report_semantic_id(context, var.get_range(), semantic_decl_id);
            }
            match var {
                LuaExpr::IndexExpr(index_expr) => {
                    var = index_expr.get_prefix_expr()?;
                }
                _ => {
                    break;
                }
            }
        }
    }

    Some(())
}
