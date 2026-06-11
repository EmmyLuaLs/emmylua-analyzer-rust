//! Access invisible checker — salsa-native.

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaIndexExpr, LuaNameExpr, LuaNameToken, LuaSyntaxToken,
};
use rowan::TextRange;

use crate::compilation::{SalsaDocOwnerKindSummary, SalsaDocOwnerSummary, SalsaDocVisibilityKindSummary};
use crate::semantic_model::SemanticModel;
use crate::{
    DiagnosticCode, LuaDeclId, LuaMemberId, LuaSemanticDeclId, SemanticDeclLevel,
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
    let Some(semantic_decl) = model.find_decl_by_node(
        name.syntax().clone(),
        SemanticDeclLevel::default(),
    ) else { return };

    let self_id = LuaDeclId::new(model.get_file_id(), name.get_position());
    if matches!(&semantic_decl, LuaSemanticDeclId::LuaDecl(id) if *id == self_id) {
        return;
    }

    let Some(name_tk) = name.get_name_token() else { return };
    let range = name_tk.get_range();
    let token = <LuaNameToken as LuaAstToken>::syntax(&name_tk).clone();
    check_visibility(context, model, token, range, &semantic_decl);
}

fn check_index(context: &mut DiagnosticContext, model: &SemanticModel, ix: LuaIndexExpr) {
    let Some(semantic_decl) = model.find_decl_by_node(
        ix.syntax().clone(),
        SemanticDeclLevel::default(),
    ) else { return };

    let self_id = LuaMemberId::new(ix.get_syntax_id(), model.get_file_id());
    if matches!(&semantic_decl, LuaSemanticDeclId::Member(id) if *id == self_id) {
        return;
    }

    let Some(idx_tk) = ix.get_index_name_token() else { return };
    let range = idx_tk.text_range();
    check_visibility(context, model, idx_tk, range, &semantic_decl);
}

fn check_visibility(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    token: emmylua_parser::LuaSyntaxToken,
    range: TextRange,
    decl: &LuaSemanticDeclId,
) {
    let visible = model.is_visible(token, decl, None);
    if visible == Some(false) {
        // 获取 visibility 种类来提供更好的错误信息
        let db = model.salsa_db().read().unwrap_or_else(|e| e.into_inner());
        let owner = decl_to_doc_owner(decl);
        let vis = owner
            .and_then(|o| db.doc().tag_property(model.get_file_id(), o))
            .and_then(|p| p.visibility().cloned());

        let message = match vis {
            Some(SalsaDocVisibilityKindSummary::Protected) => {
                t!("The property is protected and cannot be accessed outside its subclasses.")
            }
            Some(SalsaDocVisibilityKindSummary::Private) => {
                t!("The property is private and cannot be accessed outside the class.")
            }
            Some(SalsaDocVisibilityKindSummary::Package) => {
                t!("The property is package-private and cannot be accessed outside the package.")
            }
            Some(SalsaDocVisibilityKindSummary::Internal) => {
                t!("The property is internal and cannot be accessed outside the current project.")
            }
            _ => {
                t!("The property is not visible in this context.")
            }
        };

        context.add_diagnostic(
            DiagnosticCode::AccessInvisible,
            range,
            message.to_string(),
            None,
        );
    }
}

fn decl_to_doc_owner(decl: &LuaSemanticDeclId) -> Option<SalsaDocOwnerSummary> {
    match decl {
        LuaSemanticDeclId::LuaDecl(id) => Some(SalsaDocOwnerSummary {
            kind: SalsaDocOwnerKindSummary::None,
            syntax_offset: Some(id.position),
        }),
        LuaSemanticDeclId::Member(id) => Some(SalsaDocOwnerSummary {
            kind: SalsaDocOwnerKindSummary::None,
            syntax_offset: Some(id.get_syntax_id().get_range().start()),
        }),
        _ => None,
    }
}
