//! Discard returns checker — pure salsa.

use emmylua_parser::{LuaAstNode, LuaCallExprStat};

use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaType};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for stat in root.descendants::<LuaCallExprStat>() {
        let Some(call) = stat.get_call_expr() else {
            continue;
        };
        let Some(prefix) = call.get_prefix_expr() else {
            continue;
        };
        let range = prefix.syntax().text_range();

        let Ok(prefix_type) = model.infer_expr(prefix) else {
            continue;
        };

        let sig_offset = match &prefix_type {
            LuaType::Signature(sig) => Some(sig.get_position()),
            _ => None,
        };
        if let Some(offset) = sig_offset {
            if let Some(sig) = model.get_signature(model.get_file_id(), offset) {
                if let Some(msg) = sig.nodiscard() {
                    context.add_diagnostic(DiagnosticCode::DiscardReturns, range, msg, None);
                }
            }
        }
    }
}
