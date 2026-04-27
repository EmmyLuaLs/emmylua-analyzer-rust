use emmylua_parser::{
    LuaAstNode, LuaCallExpr, LuaChunk, LuaExpr, LuaFuncStat, LuaIndexExpr, LuaLiteralToken,
    LuaNameExpr, LuaVarExpr,
};
use emmylua_parser::LuaAssignStat;
use smol_str::SmolStr;

use super::super::{
    SalsaCallKindSummary, SalsaCallUseSummary, SalsaDeclKindSummary, SalsaDeclTreeSummary,
    SalsaMemberUseSummary, SalsaNameUseResolutionSummary, SalsaNameUseSummary,
    SalsaUseSiteIndexSummary, SalsaUseSiteRoleSummary,
};
use super::support::{extract_member_target_from_index_expr, find_visible_decl_before_offset};

pub fn analyze_use_site_summary(
    decl_tree: &SalsaDeclTreeSummary,
    chunk: LuaChunk,
) -> SalsaUseSiteIndexSummary {
    let names = chunk
        .descendants::<LuaNameExpr>()
        .filter_map(|name_expr| build_name_use(decl_tree, name_expr))
        .collect();
    let members = chunk
        .descendants::<LuaIndexExpr>()
        .filter_map(|index_expr| build_member_use(decl_tree, index_expr))
        .collect();
    let calls = chunk
        .descendants::<LuaCallExpr>()
        .map(|call_expr| build_call_use(decl_tree, call_expr))
        .collect();

    SalsaUseSiteIndexSummary {
        names,
        members,
        calls,
    }
}

fn build_name_use(
    decl_tree: &SalsaDeclTreeSummary,
    name_expr: LuaNameExpr,
) -> Option<SalsaNameUseSummary> {
    let name = SmolStr::new(name_expr.get_name_text()?);
    let role = name_role(&name_expr);
    let resolution =
        match find_visible_decl_before_offset(decl_tree, &name, name_expr.get_position()) {
            Some(decl) if !matches!(decl.kind, SalsaDeclKindSummary::Global) => {
                SalsaNameUseResolutionSummary::LocalDecl(decl.id)
            }
            _ => SalsaNameUseResolutionSummary::Global,
        };

    Some(SalsaNameUseSummary {
        syntax_offset: name_expr.get_position(),
        syntax_id: name_expr.get_syntax_id().into(),
        name,
        role,
        resolution,
    })
}

fn build_member_use(
    decl_tree: &SalsaDeclTreeSummary,
    index_expr: LuaIndexExpr,
) -> Option<SalsaMemberUseSummary> {
    Some(SalsaMemberUseSummary {
        syntax_offset: index_expr.get_position(),
        syntax_id: index_expr.get_syntax_id().into(),
        role: index_role(&index_expr),
        target: extract_member_target_from_index_expr(decl_tree, &index_expr)?.into(),
    })
}

fn build_call_use(decl_tree: &SalsaDeclTreeSummary, call_expr: LuaCallExpr) -> SalsaCallUseSummary {
    let kind = if call_expr.is_require() {
        SalsaCallKindSummary::Require
    } else if call_expr.is_error() {
        SalsaCallKindSummary::Error
    } else if call_expr.is_assert() {
        SalsaCallKindSummary::Assert
    } else if call_expr.is_type() {
        SalsaCallKindSummary::Type
    } else if call_expr.is_setmetatable() {
        SalsaCallKindSummary::SetMetatable
    } else {
        SalsaCallKindSummary::Normal
    };

    let arg_count = call_expr.get_args_count().unwrap_or_default();
    let require_path = if matches!(kind, SalsaCallKindSummary::Require) {
        call_expr
            .get_args_list()
            .and_then(|args| args.get_args().next())
            .and_then(|expr| match expr {
                LuaExpr::LiteralExpr(literal_expr) => literal_expr.get_literal(),
                _ => None,
            })
            .and_then(|literal| match literal {
                LuaLiteralToken::String(string_token) => Some(string_token.get_value()),
                _ => None,
            })
            .map(Into::into)
    } else {
        None
    };
    let (callee_name, callee_member) = match call_expr.get_prefix_expr() {
        Some(LuaExpr::NameExpr(name_expr)) => (name_expr.get_name_text().map(Into::into), None),
        Some(LuaExpr::IndexExpr(index_expr)) => (
            None,
            extract_member_target_from_index_expr(decl_tree, &index_expr),
        ),
        _ => (None, None),
    };

    SalsaCallUseSummary {
        syntax_offset: call_expr.get_position(),
        syntax_id: call_expr.get_syntax_id().into(),
        kind,
        is_colon_call: call_expr.is_colon_call(),
        arg_count,
        require_path,
        callee_name,
        callee_member: callee_member.map(Into::into),
    }
}

fn name_role(name_expr: &LuaNameExpr) -> SalsaUseSiteRoleSummary {
    if let Some(assign_stat) = name_expr.get_parent::<LuaAssignStat>() {
        let (vars, _) = assign_stat.get_var_and_expr_list();
        if vars.iter().any(|var| matches!(var, LuaVarExpr::NameExpr(expr) if expr.get_range() == name_expr.get_range())) {
            return SalsaUseSiteRoleSummary::Write;
        }
    }

    if let Some(func_stat) = name_expr.get_parent::<LuaFuncStat>()
        && matches!(func_stat.get_func_name(), Some(LuaVarExpr::NameExpr(expr)) if expr.get_range() == name_expr.get_range())
    {
        return SalsaUseSiteRoleSummary::Write;
    }

    if let Some(call_expr) = name_expr.get_parent::<LuaCallExpr>()
        && matches!(call_expr.get_prefix_expr(), Some(LuaExpr::NameExpr(expr)) if expr.get_range() == name_expr.get_range())
    {
        return SalsaUseSiteRoleSummary::CallCallee;
    }

    SalsaUseSiteRoleSummary::Read
}

fn index_role(index_expr: &LuaIndexExpr) -> SalsaUseSiteRoleSummary {
    if let Some(assign_stat) = index_expr.get_parent::<LuaAssignStat>() {
        let (vars, _) = assign_stat.get_var_and_expr_list();
        if vars.iter().any(|var| matches!(var, LuaVarExpr::IndexExpr(expr) if expr.get_range() == index_expr.get_range())) {
            return SalsaUseSiteRoleSummary::Write;
        }
    }

    if let Some(func_stat) = index_expr.get_parent::<LuaFuncStat>()
        && matches!(func_stat.get_func_name(), Some(LuaVarExpr::IndexExpr(expr)) if expr.get_range() == index_expr.get_range())
    {
        return SalsaUseSiteRoleSummary::Write;
    }

    if let Some(call_expr) = index_expr.get_parent::<LuaCallExpr>()
        && matches!(call_expr.get_prefix_expr(), Some(LuaExpr::IndexExpr(expr)) if expr.get_range() == index_expr.get_range())
    {
        return SalsaUseSiteRoleSummary::CallCallee;
    }

    SalsaUseSiteRoleSummary::Read
}
