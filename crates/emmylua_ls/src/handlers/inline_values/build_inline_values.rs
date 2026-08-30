use emmylua_code_analysis::{DocumentView, SalsaSemanticModel};
use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaSyntaxKind};
use lsp_types::{InlineValue, InlineValueVariableLookup, Position};
use rowan::TokenAtOffset;

pub fn build_inline_values(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    position: Position,
) -> Option<Vec<InlineValue>> {
    let mut result = Vec::new();
    let root = model.chunk()?;
    let offset = document.get_offset(position.line as usize, position.character as usize)?;
    let token = match root.syntax().token_at_offset(offset) {
        TokenAtOffset::Between(left, _) => left,
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::None => return None,
    };

    let block = token.parent_ancestors().find_map(|node| {
        if node.kind() == LuaSyntaxKind::Block.into() {
            Some(node)
        } else {
            None
        }
    })?;

    let mut node = block;
    if let Some(closure) = node.parent()
        && closure.kind() == LuaSyntaxKind::ClosureExpr.into()
    {
        node = closure;
    }

    let ast_node = LuaAst::cast(node)?;
    for node in ast_node.descendants::<LuaAst>() {
        match node {
            LuaAst::LuaLocalName(local_name) => {
                let name_token = local_name.get_name_token()?;
                let value = name_token.get_name_text();
                let range = document.to_lsp_range(name_token.get_range())?;
                result.push(InlineValue::VariableLookup(InlineValueVariableLookup {
                    variable_name: Some(value.to_string()),
                    range,
                    case_sensitive_lookup: true,
                }));
            }
            LuaAst::LuaParamName(param_name) => {
                let name_token = param_name.get_name_token()?;
                let value = name_token.get_name_text();
                let range = document.to_lsp_range(name_token.get_range())?;
                result.push(InlineValue::VariableLookup(InlineValueVariableLookup {
                    variable_name: Some(value.to_string()),
                    range,
                    case_sensitive_lookup: true,
                }));
            }
            LuaAst::LuaNameExpr(name_expr) => {
                let name_token = name_expr.get_name_token()?;
                let value = name_token.get_name_text();
                let range = document.to_lsp_range(name_token.get_range())?;
                result.push(InlineValue::VariableLookup(InlineValueVariableLookup {
                    variable_name: Some(value.to_string()),
                    range,
                    case_sensitive_lookup: true,
                }));
            }
            _ => {}
        }
    }

    Some(result)
}
