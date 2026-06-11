//! Discard returns checker — hybrid.

use emmylua_parser::{LuaAstNode, LuaCallExprStat};

use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaNoDiscard, LuaSemanticDeclId, LuaType, SemanticDeclLevel};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for stat in root.descendants::<LuaCallExprStat>() {
        check_call(context, model, stat);
    }
}

fn check_call(context: &mut DiagnosticContext, model: &SemanticModel, stat: LuaCallExprStat) {
    let Some(call) = stat.get_call_expr() else { return };
    let Some(prefix) = call.get_prefix_expr() else { return };
    let prefix_node = prefix.syntax().clone();

    let Some(decl) = model.find_decl_by_node(prefix_node.clone(), SemanticDeclLevel::default()) else {
        return;
    };

    let sig_id = match &decl {
        LuaSemanticDeclId::LuaDecl(id) => {
            let Some(tc) = context.db.get_type_index().get_type_cache(&(*id).into()) else { return };
            match tc.as_type() {
                LuaType::Signature(sig) => *sig,
                _ => return,
            }
        }
        LuaSemanticDeclId::Member(id) => {
            let Some(tc) = context.db.get_type_index().get_type_cache(&(*id).into()) else { return };
            match tc.as_type() {
                LuaType::Signature(sig) => *sig,
                _ => return,
            }
        }
        LuaSemanticDeclId::Signature(sig) => *sig,
        _ => return,
    };

    let Some(sig) = context.db.get_signature_index().get(&sig_id) else { return };
    if let Some(nd) = &sig.nodiscard {
        let msg = match nd {
            LuaNoDiscard::NoDiscard => "no discard".to_string(),
            LuaNoDiscard::NoDiscardWithMessage(m) => m.to_string(),
        };
        context.add_diagnostic(DiagnosticCode::DiscardReturns, prefix_node.text_range(), msg, None);
    }
}
