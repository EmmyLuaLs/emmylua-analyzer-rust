use emmylua_parser::{LuaAst, LuaAstNode, LuaIndexExpr, LuaNameExpr};

use crate::{
    DiagnosticCode, LuaDeclId, LuaDeprecated, LuaMemberId, LuaSemanticDeclId, LuaType,
    SemanticDeclLevel, SemanticModel,
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

    check_deprecated(
        context,
        semantic_model,
        &semantic_decl,
        name_expr.get_range(),
    );
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
    let index_name_range = index_expr.get_index_name_token()?.text_range();
    check_deprecated(context, semantic_model, &semantic_decl, index_name_range);
    Some(())
}

fn check_deprecated(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    semantic_decl: &LuaSemanticDeclId,
    range: rowan::TextRange,
) {
    let property = semantic_model
        .get_db()
        .get_property_index()
        .get_property(semantic_decl);
    let Some(property) = property else {
        return;
    };
    if let Some(deprecated) = property.deprecated() {
        let deprecated_message = match deprecated {
            LuaDeprecated::Deprecated => "deprecated".to_string(),
            LuaDeprecated::DeprecatedWithMessage(message) => message.to_string(),
        };

        context.add_diagnostic(DiagnosticCode::Deprecated, range, deprecated_message, None);
    }
    // 检查特性
    if let Some(attribute_uses) = property.attribute_uses() {
        for attribute_use in attribute_uses.iter() {
            if attribute_use.id.get_name() == "deprecated" {
                let deprecated_message =
                    match attribute_use.args.first().and_then(|(_, typ)| typ.as_ref()) {
                        Some(LuaType::DocStringConst(message)) => message.as_ref().to_string(),
                        _ => "deprecated".to_string(),
                    };
                context.add_diagnostic(DiagnosticCode::Deprecated, range, deprecated_message, None);
            }
        }
    }
}
