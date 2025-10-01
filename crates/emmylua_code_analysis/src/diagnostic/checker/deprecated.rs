use emmylua_parser::{LuaAst, LuaAstNode, LuaIndexExpr, LuaNameExpr};

use crate::{
    DiagnosticCode, LuaDeclId, LuaDeprecated, LuaMemberId, LuaSemanticDeclId, SemanticDeclLevel,
    SemanticModel,
};

use super::{Checker, DiagnosticContext};

pub struct DeprecatedChecker;

impl Checker for DeprecatedChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::Unused, DiagnosticCode::Deprecated];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for node in root.descendants::<LuaAst>() {
            match node {
                LuaAst::LuaNameExpr(name_expr) => {
                    check_name_expr(context, semantic_model, name_expr);
                }
                LuaAst::LuaIndexExpr(index_expr) => {
                    check_index_expr(context, semantic_model, index_expr);
                }
                _ => {}
            }
        }
    }
}

fn check_name_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    name_expr: LuaNameExpr,
) -> Option<()> {
    let semantic_decl = semantic_model.find_decl(
        rowan::NodeOrToken::Node(name_expr.syntax().clone()),
        SemanticDeclLevel::default(),
    )?;

    let decl_id = LuaDeclId::new(semantic_model.get_file_id(), name_expr.get_position());
    if let LuaSemanticDeclId::LuaDecl(id) = &semantic_decl
        && *id == decl_id
    {
        return Some(());
    }

    let property = semantic_model
        .get_db()
        .get_property_index()
        .get_property(&semantic_decl)?;
    if let Some(deprecated) = property.deprecated() {
        let deprecated_message = match deprecated {
            LuaDeprecated::Deprecated => "deprecated".to_string(),
            LuaDeprecated::DeprecatedWithMessage(message) => message.to_string(),
        };

        context.add_diagnostic(
            DiagnosticCode::Deprecated,
            name_expr.get_range(),
            deprecated_message,
            None,
        );
    }
    Some(())
}

fn check_index_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    index_expr: LuaIndexExpr,
) -> Option<()> {
    let semantic_decl = semantic_model.find_decl(
        rowan::NodeOrToken::Node(index_expr.syntax().clone()),
        SemanticDeclLevel::default(),
    )?;
    let member_id = LuaMemberId::new(index_expr.get_syntax_id(), semantic_model.get_file_id());
    if let LuaSemanticDeclId::Member(id) = &semantic_decl
        && *id == member_id
    {
        return Some(());
    }
    let property = semantic_model
        .get_db()
        .get_property_index()
        .get_property(&semantic_decl)?;
    if let Some(deprecated) = property.deprecated() {
        let deprecated_message = match deprecated {
            LuaDeprecated::Deprecated => "deprecated".to_string(),
            LuaDeprecated::DeprecatedWithMessage(message) => message.to_string(),
        };

        let index_name_range = index_expr.get_index_name_token()?.text_range();

        context.add_diagnostic(
            DiagnosticCode::Deprecated,
            index_name_range,
            deprecated_message,
            None,
        );
    }
    Some(())
}
