//! Duplicate require checker — salsa-native.

use emmylua_parser::{LuaAstNode, LuaBlock, LuaCallExpr, LuaIndexExpr};
use rowan::TextRange;

use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaType};

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    let mut require_calls: Vec<(TextRange, String)> = Vec::new();
    for call_expr in root.descendants::<LuaCallExpr>() {
        if call_expr.is_require() {
            check_require(context, model, call_expr, &mut require_calls);
        }
    }
}

fn check_require(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    call_expr: LuaCallExpr,
    require_calls: &mut Vec<(TextRange, String)>,
) {
    if call_expr.get_parent::<LuaIndexExpr>().is_some() {
        return;
    }
    let Some(args) = call_expr.get_args_list() else { return };
    let Some(arg) = args.get_args().next() else { return };

    let ty = model.infer_expr(arg).unwrap_or(LuaType::Any);
    if let LuaType::StringConst(s) = ty {
        let parent_block = call_expr
            .ancestors::<LuaBlock>()
            .next()
            .unwrap_or_else(|| model.get_root().get_block().expect("chunk always has block"));
        let parent_pos = parent_block.get_position();
        for (range, name) in require_calls.iter() {
            if range.contains(parent_pos) && name == s.as_str() {
                context.add_diagnostic(
                    DiagnosticCode::DuplicateRequire,
                    call_expr.get_range(),
                    t!("The same file is required multiple times.").to_string(),
                    None,
                );
                return;
            }
        }
        require_calls.push((parent_block.get_range(), s.as_str().to_string()));
    }
}
