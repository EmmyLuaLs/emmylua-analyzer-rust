//! duplicate_require: duplicate requires of the same module in the same scope.

use emmylua_parser::{LuaAstNode, LuaBlock, LuaCallExpr, LuaExpr, LuaIndexExpr, LuaLiteralToken};
use rowan::TextRange;

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct DuplicateRequireChecker;

impl Checker for DuplicateRequireChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::DuplicateRequire];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(chunk) = semantic_model.chunk() else {
            return;
        };
        let mut requires: Vec<(TextRange, String)> = Vec::new();
        for call_expr in chunk.descendants::<LuaCallExpr>() {
            if !call_expr.is_require() || call_expr.get_parent::<LuaIndexExpr>().is_some() {
                continue;
            }
            let Some(module) = require_module_name(&call_expr) else {
                continue;
            };
            let parent_block = call_expr
                .ancestors::<LuaBlock>()
                .next()
                .unwrap_or_else(|| chunk.get_block().expect("chunk block"));
            let current_pos = parent_block.get_position();
            for (range, file_name) in &requires {
                if range.contains(current_pos) && file_name == &module {
                    context.add_diagnostic(
                        DiagnosticCode::DuplicateRequire,
                        call_expr.get_range(),
                        t!("The same file is required multiple times."),
                    );
                    break;
                }
            }
            requires.push((parent_block.get_range(), module));
        }
    }
}

/// The first argument of require (a string literal).
fn require_module_name(call_expr: &LuaCallExpr) -> Option<String> {
    let arg_list = call_expr.get_args_list()?;
    let first = arg_list.get_args().next()?;
    match first {
        LuaExpr::LiteralExpr(literal) => match literal.get_literal()? {
            LuaLiteralToken::String(token) => Some(token.get_value()),
            _ => None,
        },
        _ => None,
    }
}
