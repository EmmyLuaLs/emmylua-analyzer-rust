use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaAstToken, LuaBreakStat, LuaCallExprStat, LuaDoStat, LuaExpr,
    LuaForRangeStat, LuaForStat, LuaFuncStat, LuaGlobalStat, LuaGotoStat, LuaIfStat, LuaLabelStat,
    LuaLocalFuncStat, LuaLocalStat, LuaRepeatStat, LuaReturnStat, LuaStat, LuaWhileStat,
};

use crate::ir::{self, DocIR, EqSplit};

use super::FormatContext;
use super::block::format_block;
use super::comment::collect_orphan_comments;
use super::expression::format_expr;

/// Format a statement (dispatch)
pub fn format_stat(ctx: &FormatContext, stat: &LuaStat) -> Vec<DocIR> {
    match stat {
        LuaStat::LocalStat(s) => format_local_stat(ctx, s),
        LuaStat::AssignStat(s) => format_assign_stat(ctx, s),
        LuaStat::CallExprStat(s) => format_call_expr_stat(ctx, s),
        LuaStat::FuncStat(s) => format_func_stat(ctx, s),
        LuaStat::LocalFuncStat(s) => format_local_func_stat(ctx, s),
        LuaStat::IfStat(s) => format_if_stat(ctx, s),
        LuaStat::WhileStat(s) => format_while_stat(ctx, s),
        LuaStat::DoStat(s) => format_do_stat(ctx, s),
        LuaStat::ForStat(s) => format_for_stat(ctx, s),
        LuaStat::ForRangeStat(s) => format_for_range_stat(ctx, s),
        LuaStat::RepeatStat(s) => format_repeat_stat(ctx, s),
        LuaStat::BreakStat(s) => format_break_stat(ctx, s),
        LuaStat::ReturnStat(s) => format_return_stat(ctx, s),
        LuaStat::GotoStat(s) => format_goto_stat(ctx, s),
        LuaStat::LabelStat(s) => format_label_stat(ctx, s),
        LuaStat::EmptyStat(_) => vec![ir::text(";")],
        LuaStat::GlobalStat(s) => format_global_stat(ctx, s),
    }
}

/// local name1, name2 = expr1, expr2
/// local x <const> = 1
fn format_local_stat(ctx: &FormatContext, stat: &LuaLocalStat) -> Vec<DocIR> {
    let mut docs = vec![ir::text("local"), ir::space()];

    // Variable name list (with attributes)
    let local_names: Vec<_> = stat.get_local_name_list().collect();

    for (i, local_name) in local_names.iter().enumerate() {
        if i > 0 {
            docs.push(ir::text(","));
            docs.push(ir::space());
        }
        if let Some(token) = local_name.get_name_token() {
            docs.push(ir::text(token.get_name_text().to_string()));
        }
        // <const> / <close> attribute
        if let Some(attrib) = local_name.get_attrib() {
            docs.push(ir::space());
            docs.push(ir::text("<"));
            if let Some(name_token) = attrib.get_name_token() {
                docs.push(ir::text(name_token.get_name_text().to_string()));
            }
            docs.push(ir::text(">"));
        }
    }

    // Value list
    let exprs: Vec<_> = stat.get_value_exprs().collect();
    if !exprs.is_empty() {
        docs.push(ir::space());
        docs.push(ir::text("="));

        let expr_docs: Vec<Vec<DocIR>> = exprs.iter().map(|e| format_expr(ctx, e)).collect();
        let separated = ir::intersperse(expr_docs, vec![ir::text(","), ir::space()]);

        // Single-value assignment to function/table: join with space, no line break
        if exprs.len() == 1 && is_block_like_expr(&exprs[0]) {
            docs.push(ir::space());
            docs.push(ir::list(separated));
        } else {
            // When value is too long, break after = and indent
            docs.push(ir::group(vec![ir::indent(vec![
                ir::soft_line(),
                ir::list(separated),
            ])]));
        }
    }

    docs
}

/// var1, var2 = expr1, expr2 (or compound: var += expr)
fn format_assign_stat(ctx: &FormatContext, stat: &LuaAssignStat) -> Vec<DocIR> {
    let mut docs = Vec::new();
    let (vars, exprs) = stat.get_var_and_expr_list();

    // Variable list
    let var_docs: Vec<Vec<DocIR>> = vars
        .iter()
        .map(|v| format_expr(ctx, &v.clone().into()))
        .collect();

    docs.extend(ir::intersperse(var_docs, vec![ir::text(","), ir::space()]));

    // Assignment operator
    if let Some(op) = stat.get_assign_op() {
        docs.push(ir::space());
        docs.push(ir::text(op.syntax().text().to_string()));
    }

    // Value list
    let expr_docs: Vec<Vec<DocIR>> = exprs.iter().map(|e| format_expr(ctx, e)).collect();
    let separated = ir::intersperse(expr_docs, vec![ir::text(","), ir::space()]);

    // Single-value assignment to function/table: join with space, no line break
    if exprs.len() == 1 && is_block_like_expr(&exprs[0]) {
        docs.push(ir::space());
        docs.push(ir::list(separated));
    } else {
        // When value is too long, break after = and indent
        docs.push(ir::group(vec![ir::indent(vec![
            ir::soft_line(),
            ir::list(separated),
        ])]));
    }

    docs
}

/// Function call statement f(x)
fn format_call_expr_stat(ctx: &FormatContext, stat: &LuaCallExprStat) -> Vec<DocIR> {
    if let Some(call_expr) = stat.get_call_expr() {
        format_expr(ctx, &call_expr.into())
    } else {
        vec![]
    }
}

/// function name() ... end
fn format_func_stat(ctx: &FormatContext, stat: &LuaFuncStat) -> Vec<DocIR> {
    // Compact output when function body is empty
    if let Some(compact) = format_empty_func_stat(ctx, stat) {
        return compact;
    }

    let mut docs = vec![ir::text("function"), ir::space()];

    if let Some(name) = stat.get_func_name() {
        docs.extend(format_expr(ctx, &name.into()));
    }

    if let Some(closure) = stat.get_closure() {
        docs.extend(format_closure_body(ctx, &closure));
    }

    docs
}

/// local function name() ... end
fn format_local_func_stat(ctx: &FormatContext, stat: &LuaLocalFuncStat) -> Vec<DocIR> {
    // Compact output when function body is empty
    if let Some(compact) = format_empty_local_func_stat(ctx, stat) {
        return compact;
    }

    let mut docs = vec![
        ir::text("local"),
        ir::space(),
        ir::text("function"),
        ir::space(),
    ];

    if let Some(name) = stat.get_local_name()
        && let Some(token) = name.get_name_token()
    {
        docs.push(ir::text(token.get_name_text().to_string()));
    }

    if let Some(closure) = stat.get_closure() {
        docs.extend(format_closure_body(ctx, &closure));
    }

    docs
}

/// Single-line function definition: keep single-line output when body is empty
/// e.g. `function foo() end`
fn format_empty_func_stat(ctx: &FormatContext, stat: &LuaFuncStat) -> Option<Vec<DocIR>> {
    let closure = stat.get_closure()?;
    let block = closure.get_block()?;
    let block_docs = format_block(ctx, &block);
    if !block_docs.is_empty() {
        return None;
    }

    let mut docs = vec![ir::text("function"), ir::space()];
    if let Some(name) = stat.get_func_name() {
        docs.extend(format_expr(ctx, &name.into()));
    }

    if ctx.config.space_before_func_paren {
        docs.push(ir::space());
    }

    docs.push(ir::text("("));
    if let Some(params) = closure.get_params_list() {
        let mut param_docs: Vec<Vec<DocIR>> = Vec::new();
        for p in params.get_params() {
            if p.is_dots() {
                param_docs.push(vec![ir::text("...")]);
            } else if let Some(token) = p.get_name_token() {
                param_docs.push(vec![ir::text(token.get_name_text().to_string())]);
            }
        }
        if !param_docs.is_empty() {
            let inner = ir::intersperse(param_docs, vec![ir::text(","), ir::space()]);
            docs.extend(inner);
        }
    }
    docs.push(ir::text(")"));
    docs.push(ir::space());
    docs.push(ir::text("end"));
    Some(docs)
}

/// Single-line local function: keep single-line output when body is empty
/// e.g. `local function foo() end`
fn format_empty_local_func_stat(
    ctx: &FormatContext,
    stat: &LuaLocalFuncStat,
) -> Option<Vec<DocIR>> {
    let closure = stat.get_closure()?;
    let block = closure.get_block()?;
    let block_docs = format_block(ctx, &block);
    if !block_docs.is_empty() {
        return None;
    }

    let mut docs = vec![
        ir::text("local"),
        ir::space(),
        ir::text("function"),
        ir::space(),
    ];

    if let Some(name) = stat.get_local_name()
        && let Some(token) = name.get_name_token()
    {
        docs.push(ir::text(token.get_name_text().to_string()));
    }

    if ctx.config.space_before_func_paren {
        docs.push(ir::space());
    }

    docs.push(ir::text("("));
    if let Some(params) = closure.get_params_list() {
        let mut param_docs: Vec<Vec<DocIR>> = Vec::new();
        for p in params.get_params() {
            if p.is_dots() {
                param_docs.push(vec![ir::text("...")]);
            } else if let Some(token) = p.get_name_token() {
                param_docs.push(vec![ir::text(token.get_name_text().to_string())]);
            }
        }
        if !param_docs.is_empty() {
            let inner = ir::intersperse(param_docs, vec![ir::text(","), ir::space()]);
            docs.extend(inner);
        }
    }
    docs.push(ir::text(")"));
    docs.push(ir::space());
    docs.push(ir::text("end"));
    Some(docs)
}

/// if cond then ... elseif cond then ... else ... end
fn format_if_stat(ctx: &FormatContext, stat: &LuaIfStat) -> Vec<DocIR> {
    let mut docs = vec![ir::text("if"), ir::space()];

    // if condition
    if let Some(cond) = stat.get_condition_expr() {
        docs.extend(format_expr(ctx, &cond));
    }

    docs.push(ir::space());
    docs.push(ir::text("then"));

    // if body
    let _has_block =
        format_block_or_orphan_comments(ctx, stat.get_block().as_ref(), stat.syntax(), &mut docs);

    // elseif branches
    for clause in stat.get_else_if_clause_list() {
        docs.push(ir::hard_line());
        docs.push(ir::text("elseif"));
        docs.push(ir::space());
        if let Some(cond) = clause.get_condition_expr() {
            docs.extend(format_expr(ctx, &cond));
        }
        docs.push(ir::space());
        docs.push(ir::text("then"));
        format_block_or_orphan_comments(
            ctx,
            clause.get_block().as_ref(),
            clause.syntax(),
            &mut docs,
        );
    }

    // else branch
    if let Some(else_clause) = stat.get_else_clause() {
        docs.push(ir::hard_line());
        docs.push(ir::text("else"));
        format_block_or_orphan_comments(
            ctx,
            else_clause.get_block().as_ref(),
            else_clause.syntax(),
            &mut docs,
        );
    }

    docs.push(ir::hard_line());
    docs.push(ir::text("end"));

    docs
}

/// while cond do ... end
fn format_while_stat(ctx: &FormatContext, stat: &LuaWhileStat) -> Vec<DocIR> {
    let mut docs = vec![ir::text("while"), ir::space()];

    if let Some(cond) = stat.get_condition_expr() {
        docs.extend(format_expr(ctx, &cond));
    }

    docs.push(ir::space());
    docs.push(ir::text("do"));

    format_body_end_with_parent(
        ctx,
        stat.get_block().as_ref(),
        Some(stat.syntax()),
        &mut docs,
    );

    docs
}

/// do ... end
fn format_do_stat(ctx: &FormatContext, stat: &LuaDoStat) -> Vec<DocIR> {
    let mut docs = vec![ir::text("do")];

    format_body_end_with_parent(
        ctx,
        stat.get_block().as_ref(),
        Some(stat.syntax()),
        &mut docs,
    );

    docs
}

/// for i = start, stop[, step] do ... end
fn format_for_stat(ctx: &FormatContext, stat: &LuaForStat) -> Vec<DocIR> {
    let mut docs = vec![ir::text("for"), ir::space()];

    if let Some(var_name) = stat.get_var_name() {
        docs.push(ir::text(var_name.get_name_text().to_string()));
    }

    docs.push(ir::space());
    docs.push(ir::text("="));
    docs.push(ir::space());

    let iter_exprs: Vec<_> = stat.get_iter_expr().collect();
    let iter_docs: Vec<Vec<DocIR>> = iter_exprs.iter().map(|e| format_expr(ctx, e)).collect();
    docs.extend(ir::intersperse(iter_docs, vec![ir::text(","), ir::space()]));

    docs.push(ir::space());
    docs.push(ir::text("do"));

    format_body_end_with_parent(
        ctx,
        stat.get_block().as_ref(),
        Some(stat.syntax()),
        &mut docs,
    );

    docs
}

/// for k, v in expr_list do ... end
fn format_for_range_stat(ctx: &FormatContext, stat: &LuaForRangeStat) -> Vec<DocIR> {
    let mut docs = vec![ir::text("for"), ir::space()];

    let var_names: Vec<_> = stat
        .get_var_name_list()
        .map(|n| n.get_name_text().to_string())
        .collect();
    for (i, name) in var_names.iter().enumerate() {
        if i > 0 {
            docs.push(ir::text(","));
            docs.push(ir::space());
        }
        docs.push(ir::text(name.as_str()));
    }

    docs.push(ir::space());
    docs.push(ir::text("in"));
    docs.push(ir::space());

    let expr_list: Vec<_> = stat.get_expr_list().collect();
    let expr_docs: Vec<Vec<DocIR>> = expr_list.iter().map(|e| format_expr(ctx, e)).collect();
    docs.extend(ir::intersperse(expr_docs, vec![ir::text(","), ir::space()]));

    docs.push(ir::space());
    docs.push(ir::text("do"));

    format_body_end_with_parent(
        ctx,
        stat.get_block().as_ref(),
        Some(stat.syntax()),
        &mut docs,
    );

    docs
}

/// repeat ... until cond
fn format_repeat_stat(ctx: &FormatContext, stat: &LuaRepeatStat) -> Vec<DocIR> {
    let mut docs = vec![ir::text("repeat")];

    let mut has_body = false;
    if let Some(block) = stat.get_block() {
        let block_docs = format_block(ctx, &block);
        if !block_docs.is_empty() {
            let mut indented = vec![ir::hard_line()];
            indented.extend(block_docs);
            docs.push(ir::indent(indented));
            has_body = true;
        }
    }
    if !has_body {
        let comment_docs = collect_orphan_comments(stat.syntax());
        if !comment_docs.is_empty() {
            let mut indented = vec![ir::hard_line()];
            indented.extend(comment_docs);
            docs.push(ir::indent(indented));
        }
    }

    docs.push(ir::hard_line());
    docs.push(ir::text("until"));
    docs.push(ir::space());

    if let Some(cond) = stat.get_condition_expr() {
        docs.extend(format_expr(ctx, &cond));
    }

    docs
}

/// break
fn format_break_stat(_ctx: &FormatContext, _stat: &LuaBreakStat) -> Vec<DocIR> {
    vec![ir::text("break")]
}

/// return expr1, expr2, ...
fn format_return_stat(ctx: &FormatContext, stat: &LuaReturnStat) -> Vec<DocIR> {
    let mut docs = vec![ir::text("return")];

    let exprs: Vec<_> = stat.get_expr_list().collect();
    if !exprs.is_empty() {
        let expr_docs: Vec<Vec<DocIR>> = exprs.iter().map(|e| format_expr(ctx, e)).collect();
        let separated = ir::intersperse(expr_docs, vec![ir::text(","), ir::space()]);

        docs.push(ir::group(vec![ir::indent(vec![
            ir::soft_line(),
            ir::list(separated),
        ])]));
    }

    docs
}

/// goto label
fn format_goto_stat(_ctx: &FormatContext, stat: &LuaGotoStat) -> Vec<DocIR> {
    let mut docs = vec![ir::text("goto"), ir::space()];
    if let Some(label) = stat.get_label_name_token() {
        docs.push(ir::text(label.get_name_text().to_string()));
    }
    docs
}

/// ::label::
fn format_label_stat(_ctx: &FormatContext, stat: &LuaLabelStat) -> Vec<DocIR> {
    let mut docs = vec![ir::text("::")];
    if let Some(label) = stat.get_label_name_token() {
        docs.push(ir::text(label.get_name_text().to_string()));
    }
    docs.push(ir::text("::"));
    docs
}

/// Format the parameter list and body of a closure (excluding function keyword and name)
fn format_closure_body(
    ctx: &FormatContext,
    closure: &emmylua_parser::LuaClosureExpr,
) -> Vec<DocIR> {
    let mut docs = Vec::new();

    if ctx.config.space_before_func_paren {
        docs.push(ir::space());
    }

    // Parameter list
    docs.push(ir::text("("));
    if let Some(params) = closure.get_params_list() {
        docs.extend(super::expression::format_params_ir(ctx, &params));
    }
    docs.push(ir::text(")"));

    // body
    format_body_end_with_parent(
        ctx,
        closure.get_block().as_ref(),
        Some(closure.syntax()),
        &mut docs,
    );

    docs
}

/// global name1, name2 / global <attr> name1 / global *
fn format_global_stat(_ctx: &FormatContext, stat: &LuaGlobalStat) -> Vec<DocIR> {
    let mut docs = vec![ir::text("global")];

    // global * : declare all variables as global
    if stat.is_any_global() {
        docs.push(ir::space());
        docs.push(ir::text("*"));
        return docs;
    }

    // global <attr> name1, name2 : declaration with attribute
    if let Some(attrib) = stat.get_attrib() {
        docs.push(ir::space());
        docs.push(ir::text("<"));
        if let Some(name_token) = attrib.get_name_token() {
            docs.push(ir::text(name_token.get_name_text().to_string()));
        }
        docs.push(ir::text(">"));
    }

    // Variable name list
    let names: Vec<_> = stat
        .get_local_name_list()
        .filter_map(|n| {
            let token = n.get_name_token()?;
            Some(token.get_name_text().to_string())
        })
        .collect();

    for (i, name) in names.iter().enumerate() {
        if i == 0 {
            docs.push(ir::space());
        } else {
            docs.push(ir::text(","));
            docs.push(ir::space());
        }
        docs.push(ir::text(name.as_str()));
    }

    docs
}

/// Format a block structure with body + end (with optional parent node for collecting orphan comments)
/// Empty blocks produce compact output `... end`; non-empty blocks are indented with line breaks
pub fn format_body_end_with_parent(
    ctx: &FormatContext,
    block: Option<&emmylua_parser::LuaBlock>,
    parent: Option<&emmylua_parser::LuaSyntaxNode>,
    docs: &mut Vec<DocIR>,
) {
    if let Some(block) = block {
        let block_docs = format_block(ctx, block);
        if !block_docs.is_empty() {
            let mut indented = vec![ir::hard_line()];
            indented.extend(block_docs);
            docs.push(ir::indent(indented));
            docs.push(ir::hard_line());
            docs.push(ir::text("end"));
            return;
        }
    }
    // Block is empty (or missing): check parent node for orphan comments
    if let Some(parent) = parent {
        let comment_docs = collect_orphan_comments(parent);
        if !comment_docs.is_empty() {
            let mut indented = vec![ir::hard_line()];
            indented.extend(comment_docs);
            docs.push(ir::indent(indented));
            docs.push(ir::hard_line());
            docs.push(ir::text("end"));
            return;
        }
    }
    // Empty block: compact output ` end`
    docs.push(ir::space());
    docs.push(ir::text("end"));
}

/// Format block or orphan comments (for if/elseif/else bodies that don't end with `end`)
fn format_block_or_orphan_comments(
    ctx: &FormatContext,
    block: Option<&emmylua_parser::LuaBlock>,
    parent: &emmylua_parser::LuaSyntaxNode,
    docs: &mut Vec<DocIR>,
) -> bool {
    if let Some(block) = block {
        let block_docs = format_block(ctx, block);
        if !block_docs.is_empty() {
            let mut indented = vec![ir::hard_line()];
            indented.extend(block_docs);
            docs.push(ir::indent(indented));
            return true;
        }
    }
    // Block is empty: check parent node for orphan comments
    let comment_docs = collect_orphan_comments(parent);
    if !comment_docs.is_empty() {
        let mut indented = vec![ir::hard_line()];
        indented.extend(comment_docs);
        docs.push(ir::indent(indented));
        return true;
    }
    false
}

/// Expressions with their own block structure (function/table), should not break at assignment
fn is_block_like_expr(expr: &LuaExpr) -> bool {
    matches!(expr, LuaExpr::ClosureExpr(_) | LuaExpr::TableExpr(_))
}

/// Check if a statement can participate in `=` alignment.
/// Only simple local/assign statements with values qualify.
pub fn is_eq_alignable(stat: &LuaStat) -> bool {
    match stat {
        LuaStat::LocalStat(s) => {
            // Must have values (local x = ...) and no block-like RHS
            let exprs: Vec<_> = s.get_value_exprs().collect();
            if exprs.is_empty() {
                return false;
            }
            // Skip if RHS is function/table (shouldn't be aligned)
            if exprs.len() == 1 && is_block_like_expr(&exprs[0]) {
                return false;
            }
            true
        }
        LuaStat::AssignStat(s) => {
            let (_, exprs) = s.get_var_and_expr_list();
            if exprs.is_empty() {
                return false;
            }
            if exprs.len() == 1 && is_block_like_expr(&exprs[0]) {
                return false;
            }
            true
        }
        _ => false,
    }
}

/// Format a statement split at the `=` for alignment.
/// Returns `(before_eq, after_eq)` where before_eq is the LHS and after_eq starts with `=`.
pub fn format_stat_eq_split(ctx: &super::FormatContext, stat: &LuaStat) -> Option<EqSplit> {
    match stat {
        LuaStat::LocalStat(s) => format_local_stat_eq_split(ctx, s),
        LuaStat::AssignStat(s) => format_assign_stat_eq_split(ctx, s),
        _ => None,
    }
}

/// Split local stat at `=`: before = ["local", " ", names...], after = ["=", " ", values...]
fn format_local_stat_eq_split(ctx: &super::FormatContext, stat: &LuaLocalStat) -> Option<EqSplit> {
    let exprs: Vec<_> = stat.get_value_exprs().collect();
    if exprs.is_empty() {
        return None;
    }

    // Build LHS: "local name1, name2 <attr>"
    let mut before = vec![ir::text("local"), ir::space()];
    let local_names: Vec<_> = stat.get_local_name_list().collect();
    for (i, local_name) in local_names.iter().enumerate() {
        if i > 0 {
            before.push(ir::text(","));
            before.push(ir::space());
        }
        if let Some(token) = local_name.get_name_token() {
            before.push(ir::text(token.get_name_text().to_string()));
        }
        if let Some(attrib) = local_name.get_attrib() {
            before.push(ir::space());
            before.push(ir::text("<"));
            if let Some(name_token) = attrib.get_name_token() {
                before.push(ir::text(name_token.get_name_text().to_string()));
            }
            before.push(ir::text(">"));
        }
    }

    // Build RHS: "= value1, value2"
    let mut after = vec![ir::text("="), ir::space()];
    let expr_docs: Vec<Vec<DocIR>> = exprs.iter().map(|e| format_expr(ctx, e)).collect();
    after.extend(ir::intersperse(expr_docs, vec![ir::text(","), ir::space()]));

    Some((before, after))
}

/// Split assign stat at `=`: before = [vars...], after = ["=", " ", values...]
fn format_assign_stat_eq_split(
    ctx: &super::FormatContext,
    stat: &LuaAssignStat,
) -> Option<EqSplit> {
    let (vars, exprs) = stat.get_var_and_expr_list();
    if exprs.is_empty() {
        return None;
    }

    // Build LHS
    let var_docs: Vec<Vec<DocIR>> = vars
        .iter()
        .map(|v| format_expr(ctx, &v.clone().into()))
        .collect();
    let before = ir::intersperse(var_docs, vec![ir::text(","), ir::space()]);

    // Build RHS
    let mut after = Vec::new();
    if let Some(op) = stat.get_assign_op() {
        after.push(ir::text(op.syntax().text().to_string()));
    }
    after.push(ir::space());
    let expr_docs: Vec<Vec<DocIR>> = exprs.iter().map(|e| format_expr(ctx, e)).collect();
    after.extend(ir::intersperse(expr_docs, vec![ir::text(","), ir::space()]));

    Some((before, after))
}
