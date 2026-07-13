//! Deprecated checker — pure salsa.

use emmylua_parser::{LuaAst, LuaAstNode, LuaDocNameType, LuaIndexExpr, LuaNameExpr};

use crate::compilation::SalsaDocTagPropertyEntrySummary;
use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaSemanticDeclId, SalsaDocOwnerSummary, SemanticDeclLevel};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for node in root.descendants::<LuaAst>() {
        match node {
            LuaAst::LuaNameExpr(name) => check_name(context, model, name),
            LuaAst::LuaIndexExpr(ix) => check_index(context, model, ix),
            LuaAst::LuaDocNameType(doc_name) => check_doc_name(context, model, doc_name),
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

fn check_doc_name(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    doc_name: LuaDocNameType,
) {
    let name_text = doc_name.get_name_text();
    let Some(name) = name_text else {
        return;
    };
    let db = model.salsa_db();

    // Look up the type in the type index to find its deprecated status
    if let Some(type_def) = db.doc().type_def_by_name(model.get_file_id(), &name) {
        if let Some(owner_offset) = type_def.owner.syntax_offset {
            let owner = SalsaDocOwnerSummary {
                kind: type_def.owner.kind,
                syntax_offset: Some(owner_offset),
            };
            if let Some(props) = db.doc().tag_property(model.get_file_id(), owner) {
                if check_property_entries(context, doc_name.get_range(), &props.entries) {
                    return;
                }
            }
        }
    }
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

fn check_deprecated(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    range: rowan::TextRange,
    decl: &LuaSemanticDeclId,
) {
    let db = model.salsa_db();

    match decl {
        LuaSemanticDeclId::LuaDecl(id) => {
            use crate::compilation::SalsaDeclId;
            use crate::semantic_model::DeclPosition;
            let salsa_id = SalsaDeclId(DeclPosition(id.position));
            if let Some(resolves) = db.doc().owner_resolves_for_decl(id.file_id, salsa_id) {
                for resolve in &resolves {
                    let owner = SalsaDocOwnerSummary {
                        kind: resolve.owner_kind,
                        syntax_offset: Some(resolve.owner_offset),
                    };
                    if check_owner_properties(context, db, id.file_id, range, &owner) {
                        return;
                    }
                }
            }
        }
        LuaSemanticDeclId::Member(id) => {
            let offset = id.get_syntax_id().get_range().start();
            if let Some(properties) = db.doc().tag_properties(id.file_id) {
                for props in &properties {
                    if let Some(owner_offset) = props.owner.syntax_offset {
                        let dist = if owner_offset > offset {
                            u32::from(owner_offset) - u32::from(offset)
                        } else {
                            u32::from(offset) - u32::from(owner_offset)
                        };
                        if dist < 100 {
                            if check_property_entries(context, range, &props.entries) {
                                return;
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn check_owner_properties(
    context: &mut DiagnosticContext,
    db: &crate::SalsaSummaryDatabase,
    file_id: crate::FileId,
    range: rowan::TextRange,
    owner: &SalsaDocOwnerSummary,
) -> bool {
    if let Some(props) = db.doc().tag_property(file_id, *owner) {
        return check_property_entries(context, range, &props.entries);
    }
    false
}

fn check_property_entries(
    context: &mut DiagnosticContext,
    range: rowan::TextRange,
    entries: &[SalsaDocTagPropertyEntrySummary],
) -> bool {
    for entry in entries {
        if let SalsaDocTagPropertyEntrySummary::Deprecated(msg) = entry {
            let message = msg
                .as_ref()
                .map(|m| m.to_string())
                .unwrap_or_else(|| "deprecated".to_string());
            context.add_diagnostic(DiagnosticCode::Deprecated, range, message, None);
            return true;
        }
    }
    false
}
