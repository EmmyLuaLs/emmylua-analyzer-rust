mod builder;
mod comment;
mod expr;
mod stats;

use builder::{DocumentSymbolBuilder, LuaSymbol};
use emmylua_code_analysis::SemanticModel;
use emmylua_parser::{
    LuaAstNode, LuaBlock, LuaChunk, LuaComment, LuaExpr, LuaSingleArgExpr, LuaStat, LuaSyntaxId,
    LuaSyntaxNode,
};
use expr::{build_closure_expr_symbol, build_table_symbol};
use lsp_types::{
    ClientCapabilities, DocumentSymbol, DocumentSymbolOptions, DocumentSymbolParams,
    DocumentSymbolResponse, OneOf, ServerCapabilities, SymbolKind,
};
use stats::{
    IfSymbolContext, build_assign_stat_symbol, build_do_stat_symbol, build_for_range_stat_symbol,
    build_for_stat_symbol, build_func_stat_symbol, build_if_stat_symbol,
    build_local_func_stat_symbol, build_local_stat_symbol,
};
use tokio_util::sync::CancellationToken;

use crate::context::ServerContextSnapshot;

use super::RegisterCapabilities;
use comment::build_doc_region_symbol;

pub async fn on_document_symbol(
    context: ServerContextSnapshot,
    params: DocumentSymbolParams,
    _: CancellationToken,
) -> Option<DocumentSymbolResponse> {
    let uri = params.text_document.uri;
    let analysis = context.analysis().read().await;
    let file_id = analysis.get_file_id(&uri)?;
    let semantic_model = analysis.compilation.get_semantic_model(file_id)?;
    let document_symbol_root = build_document_symbol(&semantic_model)?;
    // remove root file symbol
    let children = document_symbol_root.children?;
    let response = DocumentSymbolResponse::Nested(children);
    Some(response)
}

fn build_document_symbol(semantic_model: &SemanticModel) -> Option<DocumentSymbol> {
    let document = semantic_model.get_document();
    let root = semantic_model.get_root();
    let file_id = semantic_model.get_file_id();
    let decl_tree = semantic_model
        .get_db()
        .get_decl_index()
        .get_decl_tree(&file_id)?;
    let db = semantic_model.get_db();

    let mut builder = DocumentSymbolBuilder::new(db, decl_tree, &document);
    let symbol = LuaSymbol::new("".into(), None, SymbolKind::FILE, root.get_range());
    let root_id = builder.add_node_symbol(root.syntax().clone(), symbol, None);
    build_child_document_symbols(&mut builder, root, root_id);

    Some(builder.build(root))
}

fn build_child_document_symbols(
    builder: &mut DocumentSymbolBuilder,
    root: &LuaChunk,
    root_id: LuaSyntaxId,
) -> Option<()> {
    process_chunk(builder, root, root_id)
}

fn process_chunk(
    builder: &mut DocumentSymbolBuilder,
    chunk: &LuaChunk,
    parent_id: LuaSyntaxId,
) -> Option<()> {
    for node in chunk.syntax().children() {
        match node {
            comment if LuaComment::can_cast(comment.kind().into()) => {
                let comment = LuaComment::cast(comment.clone())?;
                process_comment(builder, &comment, parent_id);
            }
            block if LuaBlock::can_cast(block.kind().into()) => {
                let block = LuaBlock::cast(block.clone())?;
                process_block(builder, block, parent_id);
            }
            _ => {}
        }
    }

    Some(())
}

fn process_comment(
    builder: &mut DocumentSymbolBuilder,
    comment: &LuaComment,
    parent_id: LuaSyntaxId,
) {
    build_doc_region_symbol(builder, comment.clone(), parent_id);
}

fn process_block(
    builder: &mut DocumentSymbolBuilder,
    block: LuaBlock,
    parent_id: LuaSyntaxId,
) -> Option<()> {
    for child in block.syntax().children() {
        match child {
            comment if LuaComment::can_cast(comment.kind().into()) => {
                let comment = LuaComment::cast(comment.clone())?;
                process_comment(builder, &comment, parent_id);
            }
            stat if LuaStat::can_cast(stat.kind().into()) => {
                let stat = LuaStat::cast(stat.clone())?;
                process_stat(builder, stat, parent_id)?;
            }
            _ => {}
        }
    }

    Some(())
}

fn process_stat(
    builder: &mut DocumentSymbolBuilder,
    stat: LuaStat,
    parent_id: LuaSyntaxId,
) -> Option<()> {
    match stat {
        LuaStat::LocalStat(local_stat) => {
            let bindings = build_local_stat_symbol(builder, local_stat, parent_id)?;
            for binding in bindings {
                if let Some(expr) = binding.value_expr {
                    process_expr(builder, expr, binding.symbol_id, true)?;
                }
            }
        }
        LuaStat::AssignStat(assign_stat) => {
            let bindings = build_assign_stat_symbol(builder, assign_stat.clone(), parent_id)?;
            for binding in bindings {
                if let Some(expr) = binding.value_expr {
                    process_expr(builder, expr, binding.symbol_id, true)?;
                }
            }
        }
        LuaStat::FuncStat(func_stat) => {
            let func_id = build_func_stat_symbol(builder, func_stat.clone(), parent_id)?;
            if let Some(closure) = func_stat.get_closure() {
                let scope_parent = build_closure_expr_symbol(builder, closure.clone(), func_id)?;
                if let Some(block) = closure.get_block() {
                    process_block(builder, block, scope_parent)?;
                }
            }
        }
        LuaStat::LocalFuncStat(local_func) => {
            let func_id = build_local_func_stat_symbol(builder, local_func.clone(), parent_id)?;
            if let Some(closure) = local_func.get_closure() {
                let scope_parent = build_closure_expr_symbol(builder, closure.clone(), func_id)?;
                if let Some(block) = closure.get_block() {
                    process_block(builder, block, scope_parent)?;
                }
            }
        }
        LuaStat::ForStat(for_stat) => {
            let for_id = build_for_stat_symbol(builder, for_stat.clone(), parent_id)?;
            process_exprs(builder, for_stat.syntax(), for_id)?;
            if let Some(block) = for_stat.get_block() {
                process_block(builder, block, for_id)?;
            }
        }
        LuaStat::ForRangeStat(for_range_stat) => {
            let for_range_id =
                build_for_range_stat_symbol(builder, for_range_stat.clone(), parent_id)?;
            process_exprs(builder, for_range_stat.syntax(), for_range_id)?;
            if let Some(block) = for_range_stat.get_block() {
                process_block(builder, block, for_range_id)?;
            }
        }
        LuaStat::IfStat(if_stat) => {
            let ctx = build_if_stat_symbol(builder, if_stat.clone(), parent_id)?;
            if let Some(condition) = if_stat.get_condition_expr() {
                process_expr(builder, condition, ctx.if_id, false)?;
            }
            if let Some(block) = if_stat.get_block() {
                process_block(builder, block, ctx.if_id)?;
            }
            process_if_clauses(builder, ctx)?;
        }
        LuaStat::WhileStat(while_stat) => {
            if let Some(condition) = while_stat.get_condition_expr() {
                process_expr(builder, condition, parent_id, false)?;
            }
            if let Some(block) = while_stat.get_block() {
                process_block(builder, block, parent_id)?;
            }
        }
        LuaStat::RepeatStat(repeat_stat) => {
            if let Some(block) = repeat_stat.get_block() {
                process_block(builder, block, parent_id)?;
            }
            if let Some(condition) = repeat_stat.get_condition_expr() {
                process_expr(builder, condition, parent_id, false)?;
            }
        }
        LuaStat::DoStat(do_stat) => {
            let do_id = build_do_stat_symbol(builder, do_stat.clone(), parent_id)?;
            if let Some(block) = do_stat.get_block() {
                process_block(builder, block, do_id)?;
            }
        }
        LuaStat::CallExprStat(call_stat) => {
            process_exprs(builder, call_stat.syntax(), parent_id)?;
        }
        LuaStat::ReturnStat(return_stat) => {
            process_exprs(builder, return_stat.syntax(), parent_id)?;
        }
        LuaStat::GlobalStat(global_stat) => {
            process_exprs(builder, global_stat.syntax(), parent_id)?;
        }
        LuaStat::GotoStat(_)
        | LuaStat::BreakStat(_)
        | LuaStat::ContinueStat(_)
        | LuaStat::LabelStat(_)
        | LuaStat::EmptyStat(_) => {}
    }

    Some(())
}

fn process_if_clauses(builder: &mut DocumentSymbolBuilder, ctx: IfSymbolContext) -> Option<()> {
    for (clause, clause_id) in ctx.clause_symbols {
        if let Some(condition) = clause.get_condition_expr() {
            process_expr(builder, condition, clause_id, false)?;
        }
        if let Some(block) = clause.get_block() {
            process_block(builder, block, clause_id)?;
        }
    }

    Some(())
}

fn process_exprs(
    builder: &mut DocumentSymbolBuilder,
    syntax: &LuaSyntaxNode,
    parent_id: LuaSyntaxId,
) -> Option<()> {
    for child in syntax.children() {
        match child {
            expr if LuaExpr::can_cast(expr.kind().into()) => {
                let expr = LuaExpr::cast(expr.clone())?;
                process_expr(builder, expr, parent_id, false)?;
            }
            _ => {}
        }
    }
    Some(())
}

fn process_expr(
    builder: &mut DocumentSymbolBuilder,
    expr: LuaExpr,
    parent_id: LuaSyntaxId,
    inline_table_to_parent: bool,
) -> Option<()> {
    match expr {
        LuaExpr::TableExpr(table) => {
            if !inline_table_to_parent {
                if table.is_object() {
                    for field in table.get_fields() {
                        if let Some(value_expr) = field.get_value_expr() {
                            process_expr(builder, value_expr, parent_id, false)?;
                        }
                    }
                }
                return Some(());
            }
            let table_id =
                build_table_symbol(builder, table.clone(), parent_id, inline_table_to_parent)?;
            for field in table.get_fields() {
                if let Some(value_expr) = field.get_value_expr() {
                    let field_id =
                        LuaSyntaxId::new(field.syntax().kind(), field.syntax().text_range());
                    let next_parent = if builder.contains_symbol(&field_id) {
                        field_id
                    } else {
                        table_id
                    };
                    process_expr(builder, value_expr, next_parent, true)?;
                }
            }
        }
        LuaExpr::ClosureExpr(closure) => {
            if !inline_table_to_parent {
                return Some(());
            }
            let scope_parent = build_closure_expr_symbol(builder, closure.clone(), parent_id)?;
            if let Some(block) = closure.get_block() {
                process_block(builder, block, scope_parent)?;
            }
        }
        LuaExpr::BinaryExpr(binary) => {
            if let Some((left, right)) = binary.get_exprs() {
                process_expr(builder, left, parent_id, inline_table_to_parent)?;
                process_expr(builder, right, parent_id, inline_table_to_parent)?;
            }
        }
        LuaExpr::UnaryExpr(unary) => {
            if let Some(inner) = unary.get_expr() {
                process_expr(builder, inner, parent_id, inline_table_to_parent)?;
            }
        }
        LuaExpr::ParenExpr(paren) => {
            if let Some(inner) = paren.get_expr() {
                process_expr(builder, inner, parent_id, inline_table_to_parent)?;
            }
        }
        LuaExpr::CallExpr(call) => {
            if let Some(prefix) = call.get_prefix_expr() {
                process_expr(builder, prefix, parent_id, inline_table_to_parent)?;
            }
            if let Some(args) = call.get_args_list() {
                let collected: Vec<_> = args.get_args().collect();
                if collected.is_empty() && args.is_single_arg_no_parens() {
                    if let Some(single) = args.get_single_arg_expr() {
                        if let LuaSingleArgExpr::TableExpr(table) = single {
                            process_expr(
                                builder,
                                LuaExpr::TableExpr(table),
                                parent_id,
                                inline_table_to_parent,
                            )?;
                        }
                    }
                } else {
                    for arg in collected {
                        process_expr(builder, arg, parent_id, inline_table_to_parent)?;
                    }
                }
            }
        }
        LuaExpr::IndexExpr(index_expr) => {
            if let Some(prefix) = index_expr.get_prefix_expr() {
                process_expr(builder, prefix, parent_id, inline_table_to_parent)?;
            }
        }
        LuaExpr::NameExpr(_) | LuaExpr::LiteralExpr(_) => {}
        LuaExpr::TernaryExpr(ternary_expr) => {
            if let Some((condition, true_expr, false_expr)) = ternary_expr.get_exprs() {
                process_expr(builder, condition, parent_id, inline_table_to_parent)?;
                process_expr(builder, true_expr, parent_id, inline_table_to_parent)?;
                process_expr(builder, false_expr, parent_id, inline_table_to_parent)?;
            }
        }
    }

    Some(())
}

pub struct DocumentSymbolCapabilities;

impl RegisterCapabilities for DocumentSymbolCapabilities {
    fn register_capabilities(server_capabilities: &mut ServerCapabilities, _: &ClientCapabilities) {
        server_capabilities.document_symbol_provider = Some(OneOf::Right(DocumentSymbolOptions {
            label: Some("EmmyLua".into()),
            work_done_progress_options: Default::default(),
        }));
    }
}
