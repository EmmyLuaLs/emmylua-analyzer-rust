use emmylua_code_analysis::{MemberInfo, SemanticModel};
use emmylua_parser::{LuaAssignStat, LuaAstNode, LuaIndexExpr, LuaSyntaxToken};

pub(crate) fn directional_index_at(
    model: &SemanticModel,
    token: &LuaSyntaxToken,
) -> Option<(MemberInfo, MemberInfo, bool)> {
    let index = token.parent_ancestors().find_map(LuaIndexExpr::cast)?;
    if !index
        .get_index_key()?
        .get_range()?
        .contains_range(token.text_range())
    {
        return None;
    }
    let (read, write) = model.directional_index(&index)?;
    let assignment = index.get_parent::<LuaAssignStat>().is_some_and(|assign| {
        assign
            .get_var_and_expr_list()
            .0
            .iter()
            .any(|var| var.get_range() == index.get_range())
    });
    Some((read, write, assignment))
}
