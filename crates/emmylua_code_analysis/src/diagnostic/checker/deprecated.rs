//! Deprecated checker — pure salsa.

use emmylua_parser::{LuaAst, LuaAstNode, LuaIndexExpr, LuaNameExpr};

use crate::compilation::SalsaDocTagPropertyEntrySummary;
use crate::semantic_model::SemanticModel;
use crate::{
    DiagnosticCode, LuaSemanticDeclId, SalsaDocOwnerKindSummary, SalsaDocOwnerSummary,
    SemanticDeclLevel,
};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for node in root.descendants::<LuaAst>() {
        match node {
            LuaAst::LuaNameExpr(name) => check_name(context, model, name),
            LuaAst::LuaIndexExpr(ix) => check_index(context, model, ix),
            _ => {}
        }
    }
}

fn check_name(context: &mut DiagnosticContext, model: &SemanticModel, name: LuaNameExpr) {
    let Some(decl) = model.find_decl_by_node(name.syntax().clone(), SemanticDeclLevel::default())
    else {
        return;
    };
    check_deprecated(context, model, name.get_range(), &decl);
}

fn check_index(context: &mut DiagnosticContext, model: &SemanticModel, ix: LuaIndexExpr) {
    let Some(decl) = model.find_decl_by_node(ix.syntax().clone(), SemanticDeclLevel::default())
    else {
        return;
    };
    let Some(tk) = ix.get_index_name_token() else {
        return;
    };
    check_deprecated(context, model, tk.text_range(), &decl);
}

// fn check_doc_name_type(
//     context: &mut DiagnosticContext,
//     semantic_model: &SemanticModel,
//     name_type: LuaDocNameType,
// ) -> Option<()> {
//     let semantic_decl = semantic_model.find_decl(
//         rowan::NodeOrToken::Node(name_type.syntax().clone()),
//         SemanticDeclLevel::default(),
//     )?;

//     let LuaSemanticDeclId::TypeDecl(_) = &semantic_decl else {
//         return Some(());
//     };

//     if let Some(deprecated_message) = get_deprecated_message(semantic_model, &semantic_decl) {
//         context.add_diagnostic(
//             DiagnosticCode::Deprecated,
//             name_type.get_range(),
//             deprecated_message,
//             None,
//         );
//     }

//     Some(())
// }

fn check_deprecated(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    range: rowan::TextRange,
    decl: &LuaSemanticDeclId,
) {
    let (file_id, offset) = match decl {
        LuaSemanticDeclId::LuaDecl(id) => (id.file_id, id.position),
        LuaSemanticDeclId::Member(id) => (id.file_id, id.get_syntax_id().get_range().start()),
        _ => return,
    };
    let db = model.salsa_db();
    let owner = SalsaDocOwnerSummary {
        kind: SalsaDocOwnerKindSummary::None,
        syntax_offset: Some(offset),
    };
    if let Some(props) = db.doc().tag_property(file_id, owner) {
        for entry in &props.entries {
            if let SalsaDocTagPropertyEntrySummary::Deprecated(msg) = entry {
                let message = msg
                    .as_ref()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "deprecated".to_string());
                context.add_diagnostic(DiagnosticCode::Deprecated, range, message, None);
                return;
            }
        }
    }
}

// fn get_deprecated_message(
//     semantic_model: &SemanticModel,
//     semantic_decl: &LuaSemanticDeclId,
// ) -> Option<String> {
//     let property = semantic_model
//         .get_db()
//         .get_property_index()
//         .get_property(semantic_decl);
//     let property = property?;
//     if let Some(deprecated) = property.deprecated() {
//         let deprecated_message = match deprecated {
//             LuaDeprecated::Deprecated => "deprecated".to_string(),
//             LuaDeprecated::DeprecatedWithMessage(message) => message.to_string(),
//         };
//         return Some(deprecated_message);
//     }

//     None
// }
