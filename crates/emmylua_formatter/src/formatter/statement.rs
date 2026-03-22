use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaAstToken, LuaBlock, LuaBreakStat, LuaCallExprStat,
    LuaClosureExpr, LuaComment, LuaDoStat, LuaExpr, LuaForRangeStat, LuaForStat, LuaFuncStat,
    LuaGlobalStat, LuaGotoStat, LuaIfStat, LuaKind, LuaLabelStat, LuaLocalFuncStat, LuaLocalName,
    LuaLocalStat, LuaRepeatStat, LuaReturnStat, LuaStat, LuaSyntaxKind, LuaSyntaxNode,
    LuaTokenKind, LuaVarExpr, LuaWhileStat,
};

use crate::config::LuaFormatConfig;
use crate::ir::{self, DocIR, EqSplit};

use super::FormatContext;
use super::block::format_block;
use super::comment::{collect_orphan_comments, extract_trailing_comment, format_comment};
use super::expression::format_expr;
use super::sequence::{
    SequenceEntry, SequenceLayoutCandidates, SequenceLayoutPolicy, choose_sequence_layout,
    comma_entry, render_sequence, sequence_ends_with_comment, sequence_has_comment,
    sequence_starts_with_comment,
};
use super::spacing::space_around_assign;
use super::tokens::{comma_space_sep, tok};
use super::trivia::{node_has_direct_comment_child, node_has_direct_same_line_inline_comment};

/// Format a statement (dispatch)
pub fn format_stat(ctx: &FormatContext, stat: &LuaStat) -> Vec<DocIR> {
    if should_preserve_raw_statement_with_inline_comments(stat) {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    }

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
        LuaStat::EmptyStat(_) => vec![tok(LuaTokenKind::TkSemicolon)],
        LuaStat::GlobalStat(s) => format_global_stat(ctx, s),
    }
}

/// local name1, name2 = expr1, expr2
/// local x <const> = 1
fn format_local_stat(ctx: &FormatContext, stat: &LuaLocalStat) -> Vec<DocIR> {
    if node_has_direct_comment_child(stat.syntax()) {
        return format_local_stat_trivia_aware(ctx, stat);
    }

    let mut docs = vec![tok(LuaTokenKind::TkLocal), ir::space()];

    // Variable name list (with attributes)
    let local_names: Vec<_> = stat.get_local_name_list().collect();

    for (i, local_name) in local_names.iter().enumerate() {
        if i > 0 {
            docs.push(tok(LuaTokenKind::TkComma));
            docs.push(ir::space());
        }
        if let Some(token) = local_name.get_name_token() {
            docs.push(ir::source_token(token.syntax().clone()));
        }
        // <const> / <close> attribute
        if let Some(attrib) = local_name.get_attrib() {
            docs.push(ir::space());
            docs.push(ir::text("<"));
            if let Some(name_token) = attrib.get_name_token() {
                docs.push(ir::source_token(name_token.syntax().clone()));
            }
            docs.push(ir::text(">"));
        }
    }

    // Value list
    let exprs: Vec<_> = stat.get_value_exprs().collect();
    if !exprs.is_empty() {
        let assign_space = space_around_assign(ctx.config).to_ir();
        docs.push(assign_space);
        docs.push(tok(LuaTokenKind::TkAssign));

        let expr_docs: Vec<Vec<DocIR>> = exprs.iter().map(|e| format_expr(ctx, e)).collect();

        // Keep block-like / preserved multiline RHS heads attached to `=` while
        // ordinary expressions remain width-driven.
        if exprs.len() == 1 && should_attach_single_value_head(&exprs[0]) {
            let assign_space_after = space_around_assign(ctx.config).to_ir();
            docs.push(assign_space_after);
            docs.push(ir::list(expr_docs.into_iter().next().unwrap_or_default()));
        } else {
            let leading_docs = if ctx.config.spacing.space_around_assign_operator {
                vec![ir::space()]
            } else {
                vec![]
            };
            let prefix_width = exprs
                .first()
                .map(|expr| source_line_prefix_width(expr.syntax()))
                .unwrap_or(0);
            docs.extend(format_statement_expr_list(
                ctx,
                leading_docs,
                expr_docs,
                prefix_width,
            ));
        }
    }

    docs
}

/// var1, var2 = expr1, expr2 (or compound: var += expr)
fn format_assign_stat(ctx: &FormatContext, stat: &LuaAssignStat) -> Vec<DocIR> {
    if node_has_direct_comment_child(stat.syntax()) {
        return format_assign_stat_trivia_aware(ctx, stat);
    }

    let mut docs = Vec::new();
    let (vars, exprs) = stat.get_var_and_expr_list();

    // Variable list
    let var_docs: Vec<Vec<DocIR>> = vars
        .iter()
        .map(|v| format_expr(ctx, &v.clone().into()))
        .collect();

    docs.extend(ir::intersperse(
        var_docs,
        vec![tok(LuaTokenKind::TkComma), ir::space()],
    ));

    // Assignment operator
    if let Some(op) = stat.get_assign_op() {
        let assign_space = space_around_assign(ctx.config).to_ir();
        docs.push(assign_space);
        docs.push(ir::source_token(op.syntax().clone()));
    }

    // Value list
    let expr_docs: Vec<Vec<DocIR>> = exprs.iter().map(|e| format_expr(ctx, e)).collect();

    // Keep block-like / preserved multiline RHS heads attached to the operator
    // while ordinary expressions remain width-driven.
    if exprs.len() == 1 && should_attach_single_value_head(&exprs[0]) {
        let assign_space_after = space_around_assign(ctx.config).to_ir();
        docs.push(assign_space_after);
        docs.push(ir::list(expr_docs.into_iter().next().unwrap_or_default()));
    } else {
        let leading_docs = if ctx.config.spacing.space_around_assign_operator {
            vec![ir::space()]
        } else {
            vec![]
        };
        let prefix_width = exprs
            .first()
            .map(|expr| source_line_prefix_width(expr.syntax()))
            .unwrap_or(0);
        docs.extend(format_statement_expr_list(
            ctx,
            leading_docs,
            expr_docs,
            prefix_width,
        ));
    }

    docs
}

fn format_local_stat_trivia_aware(ctx: &FormatContext, stat: &LuaLocalStat) -> Vec<DocIR> {
    let StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    } = collect_local_stat_entries(ctx, stat);
    let mut docs = vec![tok(LuaTokenKind::TkLocal)];

    if !lhs_entries.is_empty() {
        docs.push(ir::space());
        render_sequence(&mut docs, &lhs_entries, false);
    }

    if let Some(assign_op) = assign_op {
        if sequence_has_comment(&lhs_entries) {
            if !sequence_ends_with_comment(&lhs_entries) {
                docs.push(ir::hard_line());
            }
            docs.push(assign_op.clone());
        } else {
            docs.push(space_around_assign(ctx.config).to_ir());
            docs.push(assign_op);
        }

        if !rhs_entries.is_empty() {
            if sequence_has_comment(&rhs_entries) {
                docs.push(ir::hard_line());
                render_sequence(&mut docs, &rhs_entries, true);
            } else {
                docs.push(space_around_assign(ctx.config).to_ir());
                render_sequence(&mut docs, &rhs_entries, false);
            }
        }
    }

    docs
}

fn format_assign_stat_trivia_aware(ctx: &FormatContext, stat: &LuaAssignStat) -> Vec<DocIR> {
    let StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    } = collect_assign_stat_entries(ctx, stat);
    let mut docs = Vec::new();

    render_sequence(&mut docs, &lhs_entries, false);

    if let Some(assign_op) = assign_op {
        if sequence_has_comment(&lhs_entries) {
            if !sequence_ends_with_comment(&lhs_entries) {
                docs.push(ir::hard_line());
            }
            docs.push(assign_op.clone());
        } else {
            docs.push(space_around_assign(ctx.config).to_ir());
            docs.push(assign_op);
        }

        if !rhs_entries.is_empty() {
            if sequence_has_comment(&rhs_entries) {
                docs.push(ir::hard_line());
                render_sequence(&mut docs, &rhs_entries, true);
            } else {
                docs.push(space_around_assign(ctx.config).to_ir());
                render_sequence(&mut docs, &rhs_entries, false);
            }
        }
    }

    docs
}

struct StatementAssignSplit {
    lhs_entries: Vec<SequenceEntry>,
    assign_op: Option<DocIR>,
    rhs_entries: Vec<SequenceEntry>,
}

enum FunctionHeaderEntry {
    Name(Vec<DocIR>),
    Comment(Vec<DocIR>),
    Closure(Vec<DocIR>),
}

fn collect_local_stat_entries(ctx: &FormatContext, stat: &LuaLocalStat) -> StatementAssignSplit {
    let mut lhs_entries = Vec::new();
    let mut rhs_entries = Vec::new();
    let mut assign_op = None;
    let mut meet_assign = false;

    for child in stat.syntax().children_with_tokens() {
        match child.kind() {
            LuaKind::Token(token_kind) if token_kind.is_assign_op() => {
                meet_assign = true;
                assign_op = child
                    .as_token()
                    .map(|token| ir::source_token(token.clone()));
            }
            LuaKind::Token(LuaTokenKind::TkComma) => {
                if meet_assign {
                    rhs_entries.push(comma_entry());
                } else {
                    lhs_entries.push(comma_entry());
                }
            }
            LuaKind::Syntax(LuaSyntaxKind::LocalName) => {
                if let Some(node) = child.as_node()
                    && let Some(local_name) = LuaLocalName::cast(node.clone())
                {
                    let entry = SequenceEntry::Item(format_local_name_ir(&local_name));
                    if meet_assign {
                        rhs_entries.push(entry);
                    } else {
                        lhs_entries.push(entry);
                    }
                }
            }
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
                    let entry = SequenceEntry::Comment(format_comment(ctx.config, &comment));
                    if meet_assign {
                        rhs_entries.push(entry);
                    } else {
                        lhs_entries.push(entry);
                    }
                }
            }
            _ => {
                if let Some(node) = child.as_node()
                    && let Some(expr) = LuaExpr::cast(node.clone())
                {
                    let entry = SequenceEntry::Item(format_expr(ctx, &expr));
                    if meet_assign {
                        rhs_entries.push(entry);
                    } else {
                        lhs_entries.push(entry);
                    }
                }
            }
        }
    }

    StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    }
}

fn collect_assign_stat_entries(ctx: &FormatContext, stat: &LuaAssignStat) -> StatementAssignSplit {
    let mut lhs_entries = Vec::new();
    let mut rhs_entries = Vec::new();
    let mut assign_op = None;
    let mut meet_assign = false;

    for child in stat.syntax().children_with_tokens() {
        match child.kind() {
            LuaKind::Token(token_kind) if token_kind.is_assign_op() => {
                meet_assign = true;
                assign_op = child
                    .as_token()
                    .map(|token| ir::source_token(token.clone()));
            }
            LuaKind::Token(LuaTokenKind::TkComma) => {
                if meet_assign {
                    rhs_entries.push(comma_entry());
                } else {
                    lhs_entries.push(comma_entry());
                }
            }
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
                    let entry = SequenceEntry::Comment(format_comment(ctx.config, &comment));
                    if meet_assign {
                        rhs_entries.push(entry);
                    } else {
                        lhs_entries.push(entry);
                    }
                }
            }
            _ => {
                if let Some(node) = child.as_node() {
                    if !meet_assign {
                        if let Some(var) = LuaVarExpr::cast(node.clone()) {
                            lhs_entries.push(SequenceEntry::Item(format_expr(ctx, &var.into())));
                        }
                    } else if let Some(expr) = LuaExpr::cast(node.clone()) {
                        rhs_entries.push(SequenceEntry::Item(format_expr(ctx, &expr)));
                    }
                }
            }
        }
    }

    StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    }
}

fn format_local_name_ir(local_name: &LuaLocalName) -> Vec<DocIR> {
    let mut docs = Vec::new();

    if let Some(token) = local_name.get_name_token() {
        docs.push(ir::source_token(token.syntax().clone()));
    }
    if let Some(attrib) = local_name.get_attrib() {
        docs.push(ir::space());
        docs.push(ir::text("<"));
        if let Some(name_token) = attrib.get_name_token() {
            docs.push(ir::source_token(name_token.syntax().clone()));
        }
        docs.push(ir::text(">"));
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
    if node_has_direct_comment_child(stat.syntax()) {
        return format_func_stat_trivia_aware(ctx, stat);
    }

    // Compact output when function body is empty
    if let Some(compact) = format_empty_func_stat(ctx, stat) {
        return compact;
    }

    let mut head_docs = vec![ir::space()];

    if let Some(name) = stat.get_func_name() {
        head_docs.extend(format_expr(ctx, &name.into()));
    }

    if let Some(closure) = stat.get_closure() {
        head_docs.extend(format_closure_body(ctx, &closure));
    }

    format_keyword_header(vec![tok(LuaTokenKind::TkFunction)], head_docs)
}

/// local function name() ... end
fn format_local_func_stat(ctx: &FormatContext, stat: &LuaLocalFuncStat) -> Vec<DocIR> {
    if node_has_direct_comment_child(stat.syntax()) {
        return format_local_func_stat_trivia_aware(ctx, stat);
    }

    // Compact output when function body is empty
    if let Some(compact) = format_empty_local_func_stat(ctx, stat) {
        return compact;
    }

    let leading_docs = vec![
        tok(LuaTokenKind::TkLocal),
        ir::space(),
        tok(LuaTokenKind::TkFunction),
    ];
    let mut head_docs = vec![ir::space()];

    if let Some(name) = stat.get_local_name()
        && let Some(token) = name.get_name_token()
    {
        head_docs.push(ir::source_token(token.syntax().clone()));
    }

    if let Some(closure) = stat.get_closure() {
        head_docs.extend(format_closure_body(ctx, &closure));
    }

    format_keyword_header(leading_docs, head_docs)
}

fn format_func_stat_trivia_aware(ctx: &FormatContext, stat: &LuaFuncStat) -> Vec<DocIR> {
    let entries = collect_func_stat_header_entries(ctx, stat);
    format_function_header_entries(vec![tok(LuaTokenKind::TkFunction)], &entries)
}

fn format_local_func_stat_trivia_aware(ctx: &FormatContext, stat: &LuaLocalFuncStat) -> Vec<DocIR> {
    let entries = collect_local_func_stat_header_entries(ctx, stat);
    format_function_header_entries(
        vec![
            tok(LuaTokenKind::TkLocal),
            ir::space(),
            tok(LuaTokenKind::TkFunction),
        ],
        &entries,
    )
}

fn collect_func_stat_header_entries(
    ctx: &FormatContext,
    stat: &LuaFuncStat,
) -> Vec<FunctionHeaderEntry> {
    let mut entries = Vec::new();

    for child in stat.syntax().children() {
        if let Some(name) = LuaVarExpr::cast(child.clone()) {
            entries.push(FunctionHeaderEntry::Name(format_expr(ctx, &name.into())));
        } else if let Some(comment) = LuaComment::cast(child.clone()) {
            entries.push(FunctionHeaderEntry::Comment(format_comment(
                ctx.config, &comment,
            )));
        } else if let Some(closure) = LuaClosureExpr::cast(child) {
            entries.push(FunctionHeaderEntry::Closure(
                format_closure_body_with_prefix_space(ctx, &closure, false),
            ));
        }
    }

    entries
}

fn collect_local_func_stat_header_entries(
    ctx: &FormatContext,
    stat: &LuaLocalFuncStat,
) -> Vec<FunctionHeaderEntry> {
    let mut entries = Vec::new();

    for child in stat.syntax().children() {
        if let Some(name) = LuaLocalName::cast(child.clone()) {
            entries.push(FunctionHeaderEntry::Name(format_local_name_ir(&name)));
        } else if let Some(comment) = LuaComment::cast(child.clone()) {
            entries.push(FunctionHeaderEntry::Comment(format_comment(
                ctx.config, &comment,
            )));
        } else if let Some(closure) = LuaClosureExpr::cast(child) {
            entries.push(FunctionHeaderEntry::Closure(
                format_closure_body_with_prefix_space(ctx, &closure, false),
            ));
        }
    }

    entries
}

fn format_function_header_entries(
    leading_docs: Vec<DocIR>,
    entries: &[FunctionHeaderEntry],
) -> Vec<DocIR> {
    if !function_header_has_comment(entries) {
        let mut head_docs = vec![ir::space()];
        for entry in entries {
            match entry {
                FunctionHeaderEntry::Name(name_docs) => head_docs.extend(name_docs.clone()),
                FunctionHeaderEntry::Closure(closure_docs) => {
                    head_docs.extend(closure_docs.clone())
                }
                FunctionHeaderEntry::Comment(_) => {}
            }
        }
        return format_keyword_header(leading_docs, head_docs);
    }

    let mut docs = leading_docs;
    let mut prev_was_comment = false;
    let mut has_seen_header_content = false;

    for entry in entries {
        match entry {
            FunctionHeaderEntry::Name(name_docs) => {
                if prev_was_comment {
                    docs.push(ir::hard_line());
                } else {
                    docs.push(ir::space());
                }
                docs.extend(name_docs.clone());
                prev_was_comment = false;
                has_seen_header_content = true;
            }
            FunctionHeaderEntry::Comment(comment_docs) => {
                if has_seen_header_content {
                    docs.push(ir::hard_line());
                } else {
                    docs.push(ir::space());
                }
                docs.extend(comment_docs.clone());
                prev_was_comment = true;
                has_seen_header_content = true;
            }
            FunctionHeaderEntry::Closure(closure_docs) => {
                if prev_was_comment {
                    docs.push(ir::hard_line());
                }
                docs.extend(closure_docs.clone());
                prev_was_comment = false;
                has_seen_header_content = true;
            }
        }
    }

    docs
}

fn function_header_has_comment(entries: &[FunctionHeaderEntry]) -> bool {
    entries
        .iter()
        .any(|entry| matches!(entry, FunctionHeaderEntry::Comment(_)))
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

    let mut docs = vec![tok(LuaTokenKind::TkFunction), ir::space()];
    if let Some(name) = stat.get_func_name() {
        docs.extend(format_expr(ctx, &name.into()));
    }

    if ctx.config.spacing.space_before_func_paren {
        docs.push(ir::space());
    }

    docs.push(tok(LuaTokenKind::TkLeftParen));
    if let Some(params) = closure.get_params_list() {
        let mut param_docs: Vec<Vec<DocIR>> = Vec::new();
        for p in params.get_params() {
            if p.is_dots() {
                param_docs.push(vec![ir::text("...")]);
            } else if let Some(token) = p.get_name_token() {
                param_docs.push(vec![ir::source_token(token.syntax().clone())]);
            }
        }
        if !param_docs.is_empty() {
            let inner = ir::intersperse(param_docs, comma_space_sep());
            docs.extend(inner);
        }
    }
    docs.push(tok(LuaTokenKind::TkRightParen));
    docs.push(ir::space());
    docs.push(tok(LuaTokenKind::TkEnd));
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
        tok(LuaTokenKind::TkLocal),
        ir::space(),
        tok(LuaTokenKind::TkFunction),
        ir::space(),
    ];

    if let Some(name) = stat.get_local_name()
        && let Some(token) = name.get_name_token()
    {
        docs.push(ir::source_token(token.syntax().clone()));
    }

    if ctx.config.spacing.space_before_func_paren {
        docs.push(ir::space());
    }

    docs.push(tok(LuaTokenKind::TkLeftParen));
    if let Some(params) = closure.get_params_list() {
        let mut param_docs: Vec<Vec<DocIR>> = Vec::new();
        for p in params.get_params() {
            if p.is_dots() {
                param_docs.push(vec![ir::text("...")]);
            } else if let Some(token) = p.get_name_token() {
                param_docs.push(vec![ir::source_token(token.syntax().clone())]);
            }
        }
        if !param_docs.is_empty() {
            let inner = ir::intersperse(param_docs, comma_space_sep());
            docs.extend(inner);
        }
    }
    docs.push(tok(LuaTokenKind::TkRightParen));
    docs.push(ir::space());
    docs.push(tok(LuaTokenKind::TkEnd));
    Some(docs)
}

/// if cond then ... elseif cond then ... else ... end
fn format_if_stat(ctx: &FormatContext, stat: &LuaIfStat) -> Vec<DocIR> {
    if let Some(preserved) = try_preserve_single_line_if_body(ctx, stat) {
        return preserved;
    }

    if should_preserve_raw_if_header_inline_comment(stat) {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    }

    if should_preserve_raw_if_stat_with_comments(stat) {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    }

    if should_preserve_raw_if_stat_trivia_aware(ctx, stat) {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    }

    if node_has_direct_comment_child(stat.syntax()) {
        return format_if_stat_trivia_aware(ctx, stat);
    }

    let mut head_docs = vec![ir::space()];

    if let Some(cond) = stat.get_condition_expr() {
        head_docs.extend(format_expr(ctx, &cond));
    }

    let mut docs = format_control_header(LuaTokenKind::TkIf, head_docs, LuaTokenKind::TkThen);

    // if body
    format_block_or_orphan_comments(ctx, stat.get_block().as_ref(), stat.syntax(), &mut docs);

    // elseif branches
    for clause in stat.get_else_if_clause_list() {
        docs.push(ir::hard_line());
        let mut clause_head_docs = vec![ir::space()];
        if let Some(cond) = clause.get_condition_expr() {
            clause_head_docs.extend(format_expr(ctx, &cond));
        }
        docs.extend(format_control_header(
            LuaTokenKind::TkElseIf,
            clause_head_docs,
            LuaTokenKind::TkThen,
        ));
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
        docs.push(tok(LuaTokenKind::TkElse));
        format_block_or_orphan_comments(
            ctx,
            else_clause.get_block().as_ref(),
            else_clause.syntax(),
            &mut docs,
        );
    }

    docs.push(ir::hard_line());
    docs.push(tok(LuaTokenKind::TkEnd));

    docs
}

fn should_preserve_raw_if_stat_trivia_aware(ctx: &FormatContext, stat: &LuaIfStat) -> bool {
    if node_has_direct_comment_child(stat.syntax())
        && should_preserve_raw_empty_loop_with_comments(ctx, stat.get_block().as_ref())
    {
        return true;
    }

    stat.get_else_if_clause_list().any(|clause| {
        node_has_direct_comment_child(clause.syntax())
            && should_preserve_raw_empty_loop_with_comments(ctx, clause.get_block().as_ref())
    })
}

fn should_preserve_raw_if_stat_with_comments(stat: &LuaIfStat) -> bool {
    let text = stat.syntax().text().to_string();
    text.contains("elseif") && text.contains("--")
}

fn should_preserve_raw_if_header_inline_comment(stat: &LuaIfStat) -> bool {
    stat.syntax().text().to_string().lines().any(|line| {
        line.find("then")
            .map(|index| line[index + 4..].contains("--"))
            .unwrap_or(false)
    })
}

fn format_if_stat_trivia_aware(ctx: &FormatContext, stat: &LuaIfStat) -> Vec<DocIR> {
    let mut docs = if let Some(raw_header) =
        try_format_raw_clause_header_until_block(stat.syntax(), stat.get_block().as_ref())
    {
        raw_header
    } else {
        format_sequence_control_header(
            vec![tok(LuaTokenKind::TkIf)],
            &collect_if_clause_entries(ctx, stat.syntax()),
            LuaTokenKind::TkThen,
        )
    };

    format_block_or_orphan_comments(ctx, stat.get_block().as_ref(), stat.syntax(), &mut docs);

    for clause in stat.get_else_if_clause_list() {
        docs.push(ir::hard_line());
        if let Some(raw_header) =
            try_format_raw_clause_header_until_block(clause.syntax(), clause.get_block().as_ref())
        {
            docs.extend(raw_header);
        } else {
            let clause_entries = collect_if_clause_entries(ctx, clause.syntax());
            docs.extend(format_sequence_control_header(
                vec![tok(LuaTokenKind::TkElseIf)],
                &clause_entries,
                LuaTokenKind::TkThen,
            ));
        }
        format_block_or_orphan_comments(
            ctx,
            clause.get_block().as_ref(),
            clause.syntax(),
            &mut docs,
        );
    }

    if let Some(else_clause) = stat.get_else_clause() {
        docs.push(ir::hard_line());
        docs.push(tok(LuaTokenKind::TkElse));
        format_block_or_orphan_comments(
            ctx,
            else_clause.get_block().as_ref(),
            else_clause.syntax(),
            &mut docs,
        );
    }

    docs.push(ir::hard_line());
    docs.push(tok(LuaTokenKind::TkEnd));
    docs
}

fn collect_if_clause_entries(ctx: &FormatContext, syntax: &LuaSyntaxNode) -> Vec<SequenceEntry> {
    let mut entries = Vec::new();

    for child in syntax.children_with_tokens() {
        match child.kind() {
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
                    entries.push(SequenceEntry::Comment(format_comment(ctx.config, &comment)));
                }
            }
            _ => {
                if let Some(node) = child.as_node()
                    && let Some(expr) = LuaExpr::cast(node.clone())
                {
                    entries.push(SequenceEntry::Item(format_expr(ctx, &expr)));
                }
            }
        }
    }

    entries
}

fn try_format_raw_clause_header_until_block(
    syntax: &LuaSyntaxNode,
    block: Option<&LuaBlock>,
) -> Option<Vec<DocIR>> {
    let block = block?;
    let text = syntax.text().to_string();
    if !text.contains("--") {
        return None;
    }

    let start = syntax.text_range().start();
    let block_start = block.syntax().text_range().start();
    if block_start <= start {
        return None;
    }

    let header_len = usize::from(block_start - start);
    let header = text
        .get(..header_len)?
        .trim_end_matches(['\r', '\n', ' ', '\t']);
    Some(vec![ir::text(header.to_string())])
}

fn try_preserve_single_line_if_body(ctx: &FormatContext, stat: &LuaIfStat) -> Option<Vec<DocIR>> {
    if stat.syntax().text().contains_char('\n') {
        return None;
    }

    let text_len: u32 = stat.syntax().text().len().into();
    let reserve_width = if ctx.config.layout.max_line_width > 40 {
        8
    } else {
        4
    };
    if text_len as usize + reserve_width > ctx.config.layout.max_line_width {
        return None;
    }

    if stat.get_else_clause().is_some() || stat.get_else_if_clause_list().next().is_some() {
        return None;
    }

    let block = stat.get_block()?;
    let mut stats = block.get_stats();
    let only_stat = stats.next()?;
    if stats.next().is_some() {
        return None;
    }

    if !is_simple_single_line_if_body(&only_stat) {
        return None;
    }

    Some(vec![ir::source_node(stat.syntax().clone())])
}

fn is_simple_single_line_if_body(stat: &LuaStat) -> bool {
    match stat {
        LuaStat::ReturnStat(_)
        | LuaStat::BreakStat(_)
        | LuaStat::GotoStat(_)
        | LuaStat::CallExprStat(_) => true,
        LuaStat::LocalStat(local) => {
            let exprs: Vec<_> = local.get_value_exprs().collect();
            exprs.len() <= 1 && exprs.iter().all(|expr| !is_block_like_expr(expr))
        }
        LuaStat::AssignStat(assign) => {
            let (_, exprs) = assign.get_var_and_expr_list();
            exprs.len() <= 1 && exprs.iter().all(|expr| !is_block_like_expr(expr))
        }
        _ => false,
    }
}

/// while cond do ... end
fn format_while_stat(ctx: &FormatContext, stat: &LuaWhileStat) -> Vec<DocIR> {
    if node_has_direct_comment_child(stat.syntax())
        && should_preserve_raw_empty_loop_with_comments(ctx, stat.get_block().as_ref())
    {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    }

    if node_has_direct_comment_child(stat.syntax()) {
        return format_while_stat_trivia_aware(ctx, stat);
    }

    let mut head_docs = vec![ir::space()];
    if let Some(cond) = stat.get_condition_expr() {
        head_docs.extend(format_expr(ctx, &cond));
    }

    let mut docs = format_control_header(LuaTokenKind::TkWhile, head_docs, LuaTokenKind::TkDo);

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
    let mut docs = vec![tok(LuaTokenKind::TkDo)];

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
    if node_has_direct_comment_child(stat.syntax())
        && should_preserve_raw_empty_loop_with_comments(ctx, stat.get_block().as_ref())
    {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    }

    if node_has_direct_comment_child(stat.syntax()) {
        return format_for_stat_trivia_aware(ctx, stat);
    }

    let mut head_docs = vec![ir::space()];

    if let Some(var_name) = stat.get_var_name() {
        head_docs.push(ir::source_token(var_name.syntax().clone()));
    }

    head_docs.push(ir::space());
    head_docs.push(tok(LuaTokenKind::TkAssign));

    let iter_exprs: Vec<_> = stat.get_iter_expr().collect();
    let iter_docs: Vec<Vec<DocIR>> = iter_exprs.iter().map(|e| format_expr(ctx, e)).collect();
    let prefix_width = iter_exprs
        .first()
        .map(|expr| source_line_prefix_width(expr.syntax()))
        .unwrap_or(0);
    head_docs.extend(format_statement_expr_list(
        ctx,
        vec![ir::space()],
        iter_docs,
        prefix_width,
    ));

    let mut docs = format_control_header(LuaTokenKind::TkFor, head_docs, LuaTokenKind::TkDo);

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
    if node_has_direct_comment_child(stat.syntax())
        && should_preserve_raw_empty_loop_with_comments(ctx, stat.get_block().as_ref())
    {
        return vec![ir::source_node_trimmed(stat.syntax().clone())];
    }

    if node_has_direct_comment_child(stat.syntax()) {
        return format_for_range_stat_trivia_aware(ctx, stat);
    }

    let mut head_docs = vec![ir::space()];

    let var_names: Vec<_> = stat.get_var_name_list().collect();
    for (i, name) in var_names.iter().enumerate() {
        if i > 0 {
            head_docs.push(tok(LuaTokenKind::TkComma));
            head_docs.push(ir::space());
        }
        head_docs.push(ir::source_token(name.syntax().clone()));
    }

    head_docs.push(ir::space());
    head_docs.push(tok(LuaTokenKind::TkIn));

    let expr_list: Vec<_> = stat.get_expr_list().collect();
    let expr_docs: Vec<Vec<DocIR>> = expr_list.iter().map(|e| format_expr(ctx, e)).collect();
    let prefix_width = expr_list
        .first()
        .map(|expr| source_line_prefix_width(expr.syntax()))
        .unwrap_or(0);
    head_docs.extend(format_statement_expr_list(
        ctx,
        vec![ir::space()],
        expr_docs,
        prefix_width,
    ));

    let mut docs = format_control_header(LuaTokenKind::TkFor, head_docs, LuaTokenKind::TkDo);

    format_body_end_with_parent(
        ctx,
        stat.get_block().as_ref(),
        Some(stat.syntax()),
        &mut docs,
    );

    docs
}

fn format_while_stat_trivia_aware(ctx: &FormatContext, stat: &LuaWhileStat) -> Vec<DocIR> {
    let entries = collect_while_stat_entries(ctx, stat);
    let mut docs = format_sequence_control_header(
        vec![tok(LuaTokenKind::TkWhile)],
        &entries,
        LuaTokenKind::TkDo,
    );

    format_body_end_with_parent(
        ctx,
        stat.get_block().as_ref(),
        Some(stat.syntax()),
        &mut docs,
    );
    docs
}

fn collect_while_stat_entries(ctx: &FormatContext, stat: &LuaWhileStat) -> Vec<SequenceEntry> {
    let mut entries = Vec::new();

    for child in stat.syntax().children_with_tokens() {
        match child.kind() {
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
                    entries.push(SequenceEntry::Comment(format_comment(ctx.config, &comment)));
                }
            }
            _ => {
                if let Some(node) = child.as_node()
                    && let Some(expr) = LuaExpr::cast(node.clone())
                {
                    entries.push(SequenceEntry::Item(format_expr(ctx, &expr)));
                }
            }
        }
    }

    entries
}

fn format_for_stat_trivia_aware(ctx: &FormatContext, stat: &LuaForStat) -> Vec<DocIR> {
    let StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    } = collect_for_stat_entries(ctx, stat);
    let mut docs = format_split_control_header(
        vec![tok(LuaTokenKind::TkFor)],
        &lhs_entries,
        assign_op.as_ref(),
        &rhs_entries,
        LuaTokenKind::TkDo,
    );

    format_body_end_with_parent(
        ctx,
        stat.get_block().as_ref(),
        Some(stat.syntax()),
        &mut docs,
    );
    docs
}

fn collect_for_stat_entries(ctx: &FormatContext, stat: &LuaForStat) -> StatementAssignSplit {
    let mut lhs_entries = Vec::new();
    let mut rhs_entries = Vec::new();
    let mut assign_op = None;
    let mut meet_assign = false;

    for child in stat.syntax().children_with_tokens() {
        match child.kind() {
            LuaKind::Token(LuaTokenKind::TkAssign) => {
                meet_assign = true;
                assign_op = Some(tok(LuaTokenKind::TkAssign));
            }
            LuaKind::Token(LuaTokenKind::TkComma) => {
                if meet_assign {
                    rhs_entries.push(comma_entry());
                } else {
                    lhs_entries.push(comma_entry());
                }
            }
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
                    let entry = SequenceEntry::Comment(format_comment(ctx.config, &comment));
                    if meet_assign {
                        rhs_entries.push(entry);
                    } else {
                        lhs_entries.push(entry);
                    }
                }
            }
            _ => {
                if let Some(token) = child.as_token()
                    && token.kind() == LuaTokenKind::TkName.into()
                    && !meet_assign
                {
                    lhs_entries.push(SequenceEntry::Item(vec![ir::source_token(token.clone())]));
                    continue;
                }

                if let Some(node) = child.as_node()
                    && let Some(expr) = LuaExpr::cast(node.clone())
                {
                    rhs_entries.push(SequenceEntry::Item(format_expr(ctx, &expr)));
                }
            }
        }
    }

    StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    }
}

fn format_for_range_stat_trivia_aware(ctx: &FormatContext, stat: &LuaForRangeStat) -> Vec<DocIR> {
    let StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    } = collect_for_range_stat_entries(ctx, stat);
    let mut docs = format_split_control_header(
        vec![tok(LuaTokenKind::TkFor)],
        &lhs_entries,
        assign_op.as_ref(),
        &rhs_entries,
        LuaTokenKind::TkDo,
    );

    format_body_end_with_parent(
        ctx,
        stat.get_block().as_ref(),
        Some(stat.syntax()),
        &mut docs,
    );
    docs
}

fn collect_for_range_stat_entries(
    ctx: &FormatContext,
    stat: &LuaForRangeStat,
) -> StatementAssignSplit {
    let mut lhs_entries = Vec::new();
    let mut rhs_entries = Vec::new();
    let mut assign_op = None;
    let mut meet_in = false;

    for child in stat.syntax().children_with_tokens() {
        match child.kind() {
            LuaKind::Token(LuaTokenKind::TkIn) => {
                meet_in = true;
                assign_op = Some(tok(LuaTokenKind::TkIn));
            }
            LuaKind::Token(LuaTokenKind::TkComma) => {
                if meet_in {
                    rhs_entries.push(comma_entry());
                } else {
                    lhs_entries.push(comma_entry());
                }
            }
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
                    let entry = SequenceEntry::Comment(format_comment(ctx.config, &comment));
                    if meet_in {
                        rhs_entries.push(entry);
                    } else {
                        lhs_entries.push(entry);
                    }
                }
            }
            _ => {
                if let Some(token) = child.as_token()
                    && token.kind() == LuaTokenKind::TkName.into()
                    && !meet_in
                {
                    lhs_entries.push(SequenceEntry::Item(vec![ir::source_token(token.clone())]));
                    continue;
                }

                if let Some(node) = child.as_node()
                    && let Some(expr) = LuaExpr::cast(node.clone())
                {
                    let entry = SequenceEntry::Item(format_expr(ctx, &expr));
                    if meet_in {
                        rhs_entries.push(entry);
                    } else {
                        lhs_entries.push(entry);
                    }
                }
            }
        }
    }

    StatementAssignSplit {
        lhs_entries,
        assign_op,
        rhs_entries,
    }
}

/// repeat ... until cond
fn format_repeat_stat(ctx: &FormatContext, stat: &LuaRepeatStat) -> Vec<DocIR> {
    let mut docs = vec![tok(LuaTokenKind::TkRepeat)];

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
        let comment_docs = collect_orphan_comments(ctx.config, stat.syntax());
        if !comment_docs.is_empty() {
            let mut indented = vec![ir::hard_line()];
            indented.extend(comment_docs);
            docs.push(ir::indent(indented));
        }
    }

    docs.push(ir::hard_line());

    let mut head_docs = vec![ir::space()];

    if let Some(cond) = stat.get_condition_expr() {
        head_docs.extend(format_expr(ctx, &cond));
    }

    docs.extend(format_keyword_header(
        vec![tok(LuaTokenKind::TkUntil)],
        head_docs,
    ));

    docs
}

/// break
fn format_break_stat(_ctx: &FormatContext, _stat: &LuaBreakStat) -> Vec<DocIR> {
    vec![tok(LuaTokenKind::TkBreak)]
}

/// return expr1, expr2, ...
fn format_return_stat(ctx: &FormatContext, stat: &LuaReturnStat) -> Vec<DocIR> {
    if node_has_direct_comment_child(stat.syntax()) {
        return format_return_stat_trivia_aware(ctx, stat);
    }

    let mut docs = vec![tok(LuaTokenKind::TkReturn)];

    let exprs: Vec<_> = stat.get_expr_list().collect();
    if !exprs.is_empty() {
        let expr_docs: Vec<Vec<DocIR>> = exprs.iter().map(|e| format_expr(ctx, e)).collect();

        if exprs.len() == 1 && should_attach_single_value_head(&exprs[0]) {
            docs.push(ir::space());
            docs.push(ir::list(expr_docs.into_iter().next().unwrap_or_default()));
        } else {
            let prefix_width = exprs
                .first()
                .map(|expr| source_line_prefix_width(expr.syntax()))
                .unwrap_or(0);
            docs.extend(format_statement_expr_list(
                ctx,
                vec![ir::space()],
                expr_docs,
                prefix_width,
            ));
        }
    }

    docs
}

fn format_return_stat_trivia_aware(ctx: &FormatContext, stat: &LuaReturnStat) -> Vec<DocIR> {
    let entries = collect_return_stat_entries(ctx, stat);
    let mut docs = vec![tok(LuaTokenKind::TkReturn)];

    if entries.is_empty() {
        return docs;
    }

    if sequence_has_comment(&entries) {
        docs.push(ir::hard_line());
        render_sequence(&mut docs, &entries, true);
    } else {
        docs.push(ir::space());
        render_sequence(&mut docs, &entries, false);
    }

    docs
}

fn collect_return_stat_entries(ctx: &FormatContext, stat: &LuaReturnStat) -> Vec<SequenceEntry> {
    let mut entries = Vec::new();

    for child in stat.syntax().children_with_tokens() {
        match child.kind() {
            LuaKind::Token(LuaTokenKind::TkComma) => entries.push(comma_entry()),
            LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                if let Some(node) = child.as_node()
                    && let Some(comment) = LuaComment::cast(node.clone())
                {
                    entries.push(SequenceEntry::Comment(format_comment(ctx.config, &comment)));
                }
            }
            _ => {
                if let Some(node) = child.as_node()
                    && let Some(expr) = LuaExpr::cast(node.clone())
                {
                    entries.push(SequenceEntry::Item(format_expr(ctx, &expr)));
                }
            }
        }
    }

    entries
}

fn format_statement_expr_list(
    ctx: &FormatContext,
    leading_docs: Vec<DocIR>,
    expr_docs: Vec<Vec<DocIR>>,
    first_line_prefix_width: usize,
) -> Vec<DocIR> {
    if expr_docs.is_empty() {
        return Vec::new();
    }

    if expr_docs.len() == 1 {
        let mut docs = leading_docs;
        docs.extend(expr_docs.into_iter().next().unwrap_or_default());
        return docs;
    }

    let fill_parts = build_statement_expr_fill_parts(leading_docs.clone(), expr_docs.clone());
    let packed = build_statement_expr_packed(leading_docs.clone(), expr_docs.clone());
    let one_per_line = build_statement_expr_one_per_line(leading_docs, expr_docs);

    choose_sequence_layout(
        ctx,
        SequenceLayoutCandidates {
            fill: Some(vec![ir::group(vec![ir::indent(vec![ir::fill(
                fill_parts,
            )])])]),
            packed: Some(packed),
            one_per_line: Some(one_per_line),
            ..Default::default()
        },
        SequenceLayoutPolicy {
            allow_alignment: false,
            allow_fill: true,
            allow_preserve: false,
            prefer_preserve_multiline: false,
            force_break_on_standalone_comments: false,
            prefer_balanced_break_lines: true,
            first_line_prefix_width,
        },
    )
}

fn source_line_prefix_width(node: &LuaSyntaxNode) -> usize {
    let mut root = node.clone();
    while let Some(parent) = root.parent() {
        root = parent;
    }

    let text = root.text().to_string();
    let start = usize::from(node.text_range().start());
    let line_start = text[..start]
        .rfind(['\n', '\r'])
        .map(|index| index + 1)
        .unwrap_or(0);

    start.saturating_sub(line_start)
}

fn build_statement_expr_fill_parts(
    leading_docs: Vec<DocIR>,
    expr_docs: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    let mut parts = Vec::with_capacity(expr_docs.len().saturating_mul(2));
    let mut expr_docs = expr_docs.into_iter();
    let mut first_chunk = leading_docs;
    first_chunk.extend(expr_docs.next().unwrap_or_default());
    parts.push(ir::list(first_chunk));

    for expr_doc in expr_docs {
        parts.push(ir::list(vec![tok(LuaTokenKind::TkComma), ir::soft_line()]));
        parts.push(ir::list(expr_doc));
    }

    parts
}

fn build_statement_expr_one_per_line(
    leading_docs: Vec<DocIR>,
    expr_docs: Vec<Vec<DocIR>>,
) -> Vec<DocIR> {
    let mut docs = Vec::new();
    let mut expr_docs = expr_docs.into_iter();
    let mut first_chunk = leading_docs;
    first_chunk.extend(expr_docs.next().unwrap_or_default());
    docs.push(ir::list(first_chunk));

    for expr_doc in expr_docs {
        docs.push(ir::list(vec![tok(LuaTokenKind::TkComma)]));
        docs.push(ir::hard_line());
        docs.push(ir::list(expr_doc));
    }

    vec![ir::group_break(vec![ir::indent(docs)])]
}

fn build_statement_expr_packed(leading_docs: Vec<DocIR>, expr_docs: Vec<Vec<DocIR>>) -> Vec<DocIR> {
    let mut docs = Vec::new();
    let mut expr_docs = expr_docs.into_iter().peekable();
    let mut first_chunk = leading_docs;
    first_chunk.extend(expr_docs.next().unwrap_or_default());
    if expr_docs.peek().is_some() {
        first_chunk.push(tok(LuaTokenKind::TkComma));
    }
    docs.push(ir::list(first_chunk));

    let mut remaining = Vec::new();
    while let Some(expr_doc) = expr_docs.next() {
        let has_more = expr_docs.peek().is_some();
        remaining.push((expr_doc, has_more));
    }

    for chunk in remaining.chunks(2) {
        let mut line = Vec::new();
        for (index, (expr_doc, has_more)) in chunk.iter().enumerate() {
            if index > 0 {
                line.push(ir::space());
            }
            line.extend(expr_doc.clone());
            if *has_more {
                line.push(tok(LuaTokenKind::TkComma));
            }
        }

        docs.push(ir::hard_line());
        docs.push(ir::list(line));
    }

    vec![ir::group_break(vec![ir::indent(docs)])]
}

fn format_control_header(
    leading_keyword: LuaTokenKind,
    head_docs: Vec<DocIR>,
    trailing_keyword: LuaTokenKind,
) -> Vec<DocIR> {
    format_header_with_trailing(vec![tok(leading_keyword)], head_docs, trailing_keyword)
}

fn format_keyword_header(leading_docs: Vec<DocIR>, head_docs: Vec<DocIR>) -> Vec<DocIR> {
    vec![ir::group(vec![ir::list(leading_docs), ir::list(head_docs)])]
}

fn format_header_with_trailing(
    leading_docs: Vec<DocIR>,
    head_docs: Vec<DocIR>,
    trailing_keyword: LuaTokenKind,
) -> Vec<DocIR> {
    vec![ir::group(vec![
        ir::list(leading_docs),
        ir::list(head_docs),
        ir::space(),
        tok(trailing_keyword),
    ])]
}

fn format_sequence_control_header(
    leading_docs: Vec<DocIR>,
    entries: &[SequenceEntry],
    trailing_keyword: LuaTokenKind,
) -> Vec<DocIR> {
    if sequence_has_comment(entries) {
        let mut docs = leading_docs;
        if !entries.is_empty() {
            docs.push(ir::space());
            render_sequence(&mut docs, entries, false);
        }
        if !sequence_ends_with_comment(entries) {
            docs.push(ir::hard_line());
        }
        docs.push(tok(trailing_keyword));
        docs
    } else {
        let mut head_docs = vec![ir::space()];
        render_sequence(&mut head_docs, entries, false);
        format_header_with_trailing(leading_docs, head_docs, trailing_keyword)
    }
}

fn format_split_control_header(
    leading_docs: Vec<DocIR>,
    lhs_entries: &[SequenceEntry],
    split_op: Option<&DocIR>,
    rhs_entries: &[SequenceEntry],
    trailing_keyword: LuaTokenKind,
) -> Vec<DocIR> {
    if sequence_has_comment(lhs_entries) || sequence_has_comment(rhs_entries) {
        let mut docs = leading_docs;

        if !lhs_entries.is_empty() {
            docs.push(ir::space());
            render_sequence(&mut docs, lhs_entries, false);
        }

        if let Some(split_op) = split_op {
            if sequence_has_comment(lhs_entries) {
                if !sequence_ends_with_comment(lhs_entries) {
                    docs.push(ir::hard_line());
                }
                docs.push(split_op.clone());
            } else {
                docs.push(ir::space());
                docs.push(split_op.clone());
            }

            if !rhs_entries.is_empty() {
                if sequence_starts_with_comment(rhs_entries) {
                    docs.push(ir::hard_line());
                    render_sequence(&mut docs, rhs_entries, true);
                } else {
                    docs.push(ir::space());
                    render_sequence(&mut docs, rhs_entries, false);
                }
            }
        }

        if sequence_has_comment(rhs_entries) {
            if !sequence_ends_with_comment(rhs_entries) {
                docs.push(ir::hard_line());
            }
            docs.push(tok(trailing_keyword));
        } else {
            docs.push(ir::space());
            docs.push(tok(trailing_keyword));
        }

        docs
    } else {
        let mut head_docs = vec![ir::space()];
        render_sequence(&mut head_docs, lhs_entries, false);
        if let Some(split_op) = split_op {
            head_docs.push(ir::space());
            head_docs.push(split_op.clone());
            if !rhs_entries.is_empty() {
                head_docs.push(ir::space());
                render_sequence(&mut head_docs, rhs_entries, false);
            }
        }
        format_header_with_trailing(leading_docs, head_docs, trailing_keyword)
    }
}

/// goto label
fn format_goto_stat(_ctx: &FormatContext, stat: &LuaGotoStat) -> Vec<DocIR> {
    let mut docs = vec![tok(LuaTokenKind::TkGoto), ir::space()];
    if let Some(label) = stat.get_label_name_token() {
        docs.push(ir::source_token(label.syntax().clone()));
    }
    docs
}

/// ::label::
fn format_label_stat(_ctx: &FormatContext, stat: &LuaLabelStat) -> Vec<DocIR> {
    let mut docs = vec![ir::text("::")];
    if let Some(label) = stat.get_label_name_token() {
        docs.push(ir::source_token(label.syntax().clone()));
    }
    docs.push(ir::text("::"));
    docs
}

/// Format the parameter list and body of a closure (excluding function keyword and name)
fn format_closure_body(ctx: &FormatContext, closure: &LuaClosureExpr) -> Vec<DocIR> {
    format_closure_body_with_prefix_space(ctx, closure, true)
}

fn format_closure_body_with_prefix_space(
    ctx: &FormatContext,
    closure: &LuaClosureExpr,
    prefix_space_before_paren: bool,
) -> Vec<DocIR> {
    let mut docs = Vec::new();

    if prefix_space_before_paren && ctx.config.spacing.space_before_func_paren {
        docs.push(ir::space());
    }

    // Parameter list
    if let Some(params) = closure.get_params_list() {
        docs.extend(super::expression::format_param_list_ir(ctx, &params));
    } else {
        docs.push(tok(LuaTokenKind::TkLeftParen));
        docs.push(tok(LuaTokenKind::TkRightParen));
    }

    // body
    format_body_end_with_parent(
        ctx,
        closure.get_block().as_ref(),
        Some(closure.syntax()),
        &mut docs,
    );

    docs
}

/// global name1, name2 / global <attr> name1 / global <attr> *
fn format_global_stat(_ctx: &FormatContext, stat: &LuaGlobalStat) -> Vec<DocIR> {
    let mut docs = vec![tok(LuaTokenKind::TkGlobal)];

    if let Some(attrib) = stat.get_attrib() {
        docs.push(ir::space());
        docs.push(ir::text("<"));
        if let Some(name_token) = attrib.get_name_token() {
            docs.push(ir::source_token(name_token.syntax().clone()));
        }
        docs.push(ir::text(">"));
    }

    // global * : declare all variables as global
    if stat.is_any_global() {
        docs.push(ir::space());
        docs.push(ir::text("*"));
        return docs;
    }

    // Variable name list
    let names: Vec<_> = stat.get_local_name_list().collect();

    for (i, name) in names.iter().enumerate() {
        if i == 0 {
            docs.push(ir::space());
        } else {
            docs.push(tok(LuaTokenKind::TkComma));
            docs.push(ir::space());
        }
        docs.extend(format_local_name_ir(name));
    }

    docs
}

/// Format a block structure with body + end (with optional parent node for collecting orphan comments)
/// Empty blocks produce compact output `... end`; non-empty blocks are indented with line breaks
pub fn format_body_end_with_parent(
    ctx: &FormatContext,
    block: Option<&LuaBlock>,
    parent: Option<&LuaSyntaxNode>,
    docs: &mut Vec<DocIR>,
) {
    if let Some(block) = block {
        let block_docs = format_block(ctx, block);
        if !block_docs.is_empty() {
            let mut indented = vec![ir::hard_line()];
            indented.extend(block_docs);
            docs.push(ir::indent(indented));
            docs.push(ir::hard_line());
            docs.push(tok(LuaTokenKind::TkEnd));
            return;
        }
    }
    // Block is empty (or missing): check parent node for orphan comments
    if let Some(parent) = parent {
        let comment_docs = collect_orphan_comments(ctx.config, parent);
        if !comment_docs.is_empty() {
            let mut indented = vec![ir::hard_line()];
            indented.extend(comment_docs);
            docs.push(ir::indent(indented));
            docs.push(ir::hard_line());
            docs.push(tok(LuaTokenKind::TkEnd));
            return;
        }
    }
    // Empty block: compact output ` end`
    docs.push(ir::space());
    docs.push(tok(LuaTokenKind::TkEnd));
}

/// Format block or orphan comments (for if/elseif/else bodies that don't end with `end`)
fn format_block_or_orphan_comments(
    ctx: &FormatContext,
    block: Option<&LuaBlock>,
    parent: &LuaSyntaxNode,
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
    let comment_docs = collect_orphan_comments(ctx.config, parent);
    if !comment_docs.is_empty() {
        let mut indented = vec![ir::hard_line()];
        indented.extend(comment_docs);
        docs.push(ir::indent(indented));
        return true;
    }
    false
}

/// Expressions with their own block structure (function/table), should not break at alignment-only paths.
fn is_block_like_expr(expr: &LuaExpr) -> bool {
    matches!(expr, LuaExpr::ClosureExpr(_) | LuaExpr::TableExpr(_))
}

fn should_attach_single_value_head(expr: &LuaExpr) -> bool {
    is_block_like_expr(expr) || node_has_direct_comment_child(expr.syntax())
}

fn should_preserve_raw_empty_loop_with_comments(
    ctx: &FormatContext,
    block: Option<&LuaBlock>,
) -> bool {
    block
        .map(|block| format_block(ctx, block).is_empty())
        .unwrap_or(true)
}

fn should_preserve_raw_statement_with_inline_comments(stat: &LuaStat) -> bool {
    if node_has_direct_same_line_inline_comment(stat.syntax()) {
        return true;
    }

    match stat {
        LuaStat::LocalStat(_) | LuaStat::AssignStat(_) => false,
        LuaStat::FuncStat(func) => func
            .get_closure()
            .map(|closure| {
                node_has_direct_same_line_inline_comment(closure.syntax())
                    || closure
                        .get_params_list()
                        .map(|params| node_has_direct_same_line_inline_comment(params.syntax()))
                        .unwrap_or(false)
            })
            .unwrap_or(false),
        LuaStat::LocalFuncStat(func) => func
            .get_closure()
            .map(|closure| {
                node_has_direct_same_line_inline_comment(closure.syntax())
                    || closure
                        .get_params_list()
                        .map(|params| node_has_direct_same_line_inline_comment(params.syntax()))
                        .unwrap_or(false)
            })
            .unwrap_or(false),
        _ => false,
    }
}

/// Check if a statement can participate in `=` alignment.
/// Only simple local/assign statements with values qualify.
pub fn is_eq_alignable(config: &LuaFormatConfig, stat: &LuaStat) -> bool {
    match stat {
        LuaStat::LocalStat(s) => {
            if node_has_direct_comment_child(s.syntax())
                && extract_trailing_comment(config, s.syntax()).is_none()
            {
                return false;
            }
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
            if node_has_direct_comment_child(s.syntax())
                && extract_trailing_comment(config, s.syntax()).is_none()
            {
                return false;
            }
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
    let mut before = vec![tok(LuaTokenKind::TkLocal), ir::space()];
    let local_names: Vec<_> = stat.get_local_name_list().collect();
    for (i, local_name) in local_names.iter().enumerate() {
        if i > 0 {
            before.push(tok(LuaTokenKind::TkComma));
            before.push(ir::space());
        }
        if let Some(token) = local_name.get_name_token() {
            before.push(ir::source_token(token.syntax().clone()));
        }
        if let Some(attrib) = local_name.get_attrib() {
            before.push(ir::space());
            before.push(ir::text("<"));
            if let Some(name_token) = attrib.get_name_token() {
                before.push(ir::source_token(name_token.syntax().clone()));
            }
            before.push(ir::text(">"));
        }
    }

    // Build RHS: "= value1, value2"
    let assign_space = space_around_assign(ctx.config).to_ir();
    let mut after = vec![tok(LuaTokenKind::TkAssign), assign_space];
    let expr_docs: Vec<Vec<DocIR>> = exprs.iter().map(|e| format_expr(ctx, e)).collect();
    after.extend(ir::intersperse(expr_docs, comma_space_sep()));

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
    let before = ir::intersperse(var_docs, comma_space_sep());

    // Build RHS
    let mut after = Vec::new();
    if let Some(op) = stat.get_assign_op() {
        after.push(ir::source_token(op.syntax().clone()));
    }
    let assign_space = space_around_assign(ctx.config).to_ir();
    after.push(assign_space);
    let expr_docs: Vec<Vec<DocIR>> = exprs.iter().map(|e| format_expr(ctx, e)).collect();
    after.extend(ir::intersperse(expr_docs, comma_space_sep()));

    Some((before, after))
}
