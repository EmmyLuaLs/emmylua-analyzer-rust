use emmylua_parser::{LuaAstNode, LuaBlock, LuaCallExpr, LuaIndexExpr};
use rowan::TextRange;

use crate::{DiagnosticCode, LuaType, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct DuplicateRequireChecker;

impl Checker for DuplicateRequireChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::DuplicateRequire];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        let mut require_calls = Vec::new();
        for call_expr in root.descendants::<LuaCallExpr>() {
            if call_expr.is_require() {
                check_require_call_expr(context, semantic_model, call_expr, &mut require_calls);
            }
        }
    }
}

fn check_require_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
    require_calls: &mut Vec<(TextRange, String)>,
) -> Option<()> {
    if let Some(_) = call_expr.get_parent::<LuaIndexExpr>() {
        return Some(());
    }
    let args_list = call_expr.get_args_list()?;
    let arg_expr = args_list.get_args().next()?;

    let ty = semantic_model.infer_expr(arg_expr).unwrap_or(LuaType::Any);
    if let LuaType::StringConst(s) = ty {
        let parent_block = call_expr
            .ancestors::<LuaBlock>()
            .next()
            .unwrap_or(semantic_model.get_root().get_block()?);

        let parent_position = parent_block.get_position();
        for (range, file_name) in require_calls.iter() {
            if range.contains(parent_position) && file_name == s.as_str() {
                context.add_diagnostic(
                    DiagnosticCode::DuplicateRequire,
                    call_expr.get_range(),
                    t!("The same file is required multiple times.").to_string(),
                    None,
                );
                return Some(());
            }
        }

        require_calls.push((parent_block.get_range(), s.as_str().to_string()));
    }

    Some(())
}
