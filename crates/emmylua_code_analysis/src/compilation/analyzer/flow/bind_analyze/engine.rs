//! Bind engine: converts the AST recursive traversal of the flow binding phase (`bind_analyze`) into an explicit task stack
//!
//! In the original implementation, functions like `bind_expr`/`bind_block`/`bind_condition_expr` recurse into each other,
//! and the recursion depth is determined by the nesting depth of user code (member chains, call chains, parens, nested blocks, etc.),
//! so deeply nested inputs directly overflow the native stack. This engine converts all recursive call sites into a heap-based
//! task stack plus continuations, keeping the native call stack depth constant.
//!
//! Key constraint: the execution order of all side effects (`create_node`/`add_antecedent`/`bind_syntax_node`, etc.)
//! must be exactly identical to the original recursive implementation, because flow ids depend on the node creation order.

use emmylua_parser::{
    BinaryOperator, LuaAssignStat, LuaAst, LuaAstNode, LuaAstToken, LuaBlock, LuaCallExprStat,
    LuaElseIfClauseStat, LuaExpr, LuaForRangeStat, LuaForStat, LuaFuncStat, LuaIfStat,
    LuaIndexExpr, LuaLocalStat, LuaRepeatStat, LuaVarExpr, LuaWhileStat, UnaryOperator,
};

use super::{
    exprs::is_binary_logical,
    finish_flow_label,
    stats::{
        bind_multi_return_refs, check_local_immutable, check_value_expr_is_check_expr,
        finish_entered_loop_post_flow, get_local_decl_ids, get_var_decl_ids,
        static_literal_truthiness, static_number_value,
    },
};
use crate::{
    FlowId, FlowNodeKind, LuaClosureId, LuaDeclId, compilation::analyzer::flow::binder::FlowBinder,
};

/// Task: carries the input current and produces one FlowId delivered to the continuation on the stack
enum Task {
    /// Bind a plain expression (the result always equals the input current)
    Expr(LuaExpr, FlowId),
    /// `bind_condition_expr`: bind the condition expression and create condition nodes
    Cond(LuaExpr, FlowId, FlowId, FlowId),
    /// `finish_flow_label`
    Finish(FlowId, FlowId),
    /// `bind_node`
    Node(LuaAst, FlowId),
    /// `bind_block`
    Block(LuaBlock, FlowId),
    /// Pass a value through
    Pass(FlowId),
}

/// Suspended parent task state
enum Continuation {
    /// Execute the remaining tasks in order, passing the result through
    Seq { pending: Vec<Task> },
    /// Condition node creation phase (restore the targets, then create True/False condition nodes)
    CondNodes {
        expr: LuaExpr,
        current: FlowId,
        true_target: FlowId,
        false_target: FlowId,
        old_true: FlowId,
        old_false: FlowId,
    },
    /// Create a Finish task after receiving a value
    ThenFinish {
        label: FlowId,
        default: Option<FlowId>,
    },
    /// Create a Cond task after receiving a value (the value is the current of the condition expression)
    ThenCond {
        expr: LuaExpr,
        true_target: FlowId,
        false_target: FlowId,
    },
    /// Create a Block task after receiving a value
    ThenBlock { block: LuaBlock },
    /// Safe index: bind child nodes after the prefix condition completes
    SafeIndexDone { index_expr: LuaIndexExpr },
    /// Unary not: restore the condition targets
    UnaryNotDone { old_true: FlowId, old_false: FlowId },
    /// assert args: condition binding completed
    AssertCond {
        args: Vec<LuaExpr>,
        idx: usize,
        labels: Vec<FlowId>,
        false_target: FlowId,
    },
    /// assert args: label merge completed
    AssertFinish {
        args: Vec<LuaExpr>,
        idx: usize,
        labels: Vec<FlowId>,
        false_target: FlowId,
    },
    /// local statement: create the decl node after the value expressions are bound
    LocalDone {
        local_stat: LuaLocalStat,
        current: FlowId,
    },
    /// assignment statement: create the node after the values/variables are bound
    AssignDone {
        assign_stat: LuaAssignStat,
        current: FlowId,
    },
    /// return statement completed
    ReturnDone { current: FlowId },
    /// call statement completed
    CallStatDone {
        call_expr_stat: LuaCallExprStat,
        current: FlowId,
        kind: CallStatKind,
    },
    /// function definition statement completed
    FuncDone {
        func_stat: LuaFuncStat,
        current: FlowId,
    },
    /// local function statement completed
    LocalFuncDone { current: FlowId },
    /// while loop: post-process after the loop body completes
    WhileDone {
        after_label: FlowId,
        loop_enters: bool,
        has_block: bool,
        current: FlowId,
        old_loop_label: FlowId,
        old_break_target_label: FlowId,
    },
    /// repeat loop: post-process after the loop completes
    RepeatDone {
        post_label: FlowId,
        old_loop_label: FlowId,
        old_break_target_label: FlowId,
    },
    /// for loop: post-process after the loop body completes
    ForDone {
        post_label: FlowId,
        loop_enters: bool,
        has_block: bool,
        current: FlowId,
        old_loop_label: FlowId,
        old_break_target_label: FlowId,
    },
    /// for range: post-process after completion
    ForRangeDone {
        current: FlowId,
        old_loop_label: FlowId,
        old_break_target_label: FlowId,
    },
    /// for loop: create the ForIStat node after the iteration expressions complete
    ThenForNode {
        for_stat: LuaForStat,
        pre_label: FlowId,
        block: Option<LuaBlock>,
    },
    /// for range: create the decl node after the iteration expressions complete
    ThenForRangeDecl {
        for_range_stat: LuaForRangeStat,
        pre_label: FlowId,
    },
    /// if statement: process the remaining branches after one branch block completes
    IfBranchDone {
        clauses: Vec<LuaElseIfClauseStat>,
        idx: usize,
        else_label: FlowId,
        post_if: FlowId,
        current: FlowId,
        else_block: Option<LuaBlock>,
    },
    /// if statement: finalize after the else block completes
    IfFinal { post_if: FlowId, else_label: FlowId },
    /// block: bind child nodes in order
    BlockIter {
        children: Vec<LuaAst>,
        idx: usize,
        current: FlowId,
        can_change_flow: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallStatKind {
    Normal,
    Error,
}

enum Step {
    Task(Task),
    Done(FlowId),
    Resume(Continuation, FlowId),
}

/// Bind engine
pub(super) struct BindEngine<'a, 'b> {
    binder: &'a mut FlowBinder<'b>,
    stack: Vec<Continuation>,
}

impl<'a, 'b> BindEngine<'a, 'b> {
    fn new(binder: &'a mut FlowBinder<'b>) -> Self {
        Self {
            binder,
            stack: Vec::new(),
        }
    }

    fn run(mut self, root: Task) -> FlowId {
        let mut step = Step::Task(root);
        loop {
            step = match step {
                Step::Task(task) => self.evaluate(task),
                Step::Done(value) => match self.stack.pop() {
                    Some(continuation) => Step::Resume(continuation, value),
                    None => return value,
                },
                Step::Resume(continuation, value) => self.resume(continuation, value),
            };
        }
    }

    fn evaluate(&mut self, task: Task) -> Step {
        match task {
            Task::Pass(value) => Step::Done(value),
            Task::Finish(label, default) => {
                Step::Done(finish_flow_label(self.binder, label, default))
            }
            Task::Expr(expr, current) => self.evaluate_expr(expr, current),
            Task::Cond(expr, current, true_target, false_target) => {
                let old_true = self.binder.true_target;
                let old_false = self.binder.false_target;
                self.binder.true_target = true_target;
                self.binder.false_target = false_target;
                self.stack.push(Continuation::CondNodes {
                    expr: expr.clone(),
                    current,
                    true_target,
                    false_target,
                    old_true,
                    old_false,
                });
                Step::Task(Task::Expr(expr, current))
            }
            Task::Node(node, current) => self.evaluate_node(node, current),
            Task::Block(block, current) => {
                let children = block.children::<LuaAst>().collect::<Vec<_>>();
                if children.is_empty() {
                    Step::Done(current)
                } else {
                    let first = children[0].clone();
                    self.stack.push(Continuation::BlockIter {
                        children,
                        idx: 0,
                        current,
                        can_change_flow: true,
                    });
                    Step::Task(Task::Node(first, current))
                }
            }
        }
    }

    fn evaluate_expr(&mut self, expr: LuaExpr, current: FlowId) -> Step {
        match expr {
            LuaExpr::NameExpr(name_expr) => {
                self.binder
                    .bind_syntax_node(name_expr.get_syntax_id(), current);
                Step::Done(current)
            }
            LuaExpr::LiteralExpr(_) => Step::Done(current),
            LuaExpr::ParenExpr(paren_expr) => match paren_expr.get_expr() {
                Some(inner) => Step::Task(Task::Expr(inner, current)),
                None => Step::Done(current),
            },
            LuaExpr::ClosureExpr(closure_expr) => {
                self.spawn_children(LuaAst::LuaClosureExpr(closure_expr), current)
            }
            LuaExpr::CallExpr(call_expr) => {
                self.spawn_children(LuaAst::LuaCallExpr(call_expr), current)
            }
            LuaExpr::TableExpr(table_expr) => {
                self.spawn_children(LuaAst::LuaTableExpr(table_expr), current)
            }
            LuaExpr::IndexExpr(index_expr) => {
                self.binder
                    .bind_syntax_node(index_expr.get_syntax_id(), current);
                if index_expr.is_safe_index() {
                    let pre_access = self.binder.create_branch_label();
                    let Some(prefix_expr) = index_expr.get_prefix_expr() else {
                        return Step::Done(current);
                    };
                    self.stack.push(Continuation::SafeIndexDone { index_expr });
                    self.stack.push(Continuation::ThenFinish {
                        label: pre_access,
                        default: None,
                    });
                    Step::Task(Task::Cond(
                        prefix_expr,
                        current,
                        pre_access,
                        self.binder.false_target,
                    ))
                } else {
                    self.spawn_children(LuaAst::LuaIndexExpr(index_expr), current)
                }
            }
            LuaExpr::BinaryExpr(binary_expr) => {
                let Some(op_token) = binary_expr.get_op_token() else {
                    return Step::Done(current);
                };
                let Some((left, right)) = binary_expr.get_exprs() else {
                    return Step::Done(current);
                };
                match op_token.get_op() {
                    BinaryOperator::OpAnd => {
                        let pre_right = self.binder.create_branch_label();
                        self.stack.push(Continuation::ThenCond {
                            expr: right,
                            true_target: self.binder.true_target,
                            false_target: self.binder.false_target,
                        });
                        self.stack.push(Continuation::ThenFinish {
                            label: pre_right,
                            default: None,
                        });
                        Step::Task(Task::Cond(
                            left,
                            current,
                            pre_right,
                            self.binder.false_target,
                        ))
                    }
                    BinaryOperator::OpOr | BinaryOperator::OpNilCoalescing => {
                        let pre_right = self.binder.create_branch_label();
                        self.stack.push(Continuation::ThenCond {
                            expr: right,
                            true_target: self.binder.true_target,
                            false_target: self.binder.false_target,
                        });
                        self.stack.push(Continuation::ThenFinish {
                            label: pre_right,
                            default: None,
                        });
                        Step::Task(Task::Cond(
                            left,
                            current,
                            self.binder.true_target,
                            pre_right,
                        ))
                    }
                    _ => self.spawn_children(LuaAst::LuaBinaryExpr(binary_expr), current),
                }
            }
            LuaExpr::UnaryExpr(unary_expr) => {
                let is_not = unary_expr
                    .get_op_token()
                    .is_some_and(|op| op.get_op() == UnaryOperator::OpNot);
                if !is_not {
                    return self.spawn_children(LuaAst::LuaUnaryExpr(unary_expr), current);
                }
                let Some(inner_expr) = unary_expr.get_expr() else {
                    return Step::Done(current);
                };
                // not swaps the condition targets; restore them after the inner binding completes
                let old_true = self.binder.true_target;
                let old_false = self.binder.false_target;
                self.binder.true_target = old_false;
                self.binder.false_target = old_true;
                self.stack.push(Continuation::UnaryNotDone {
                    old_true,
                    old_false,
                });
                Step::Task(Task::Expr(inner_expr, current))
            }
            LuaExpr::TernaryExpr(ternary_expr) => {
                let Some(condition) = ternary_expr.get_condition_expr() else {
                    return Step::Done(current);
                };
                let Some((true_expr, false_expr)) = ternary_expr.get_true_false_exprs() else {
                    return Step::Done(current);
                };
                let true_branch_label = self.binder.create_branch_label();
                let false_branch_label = self.binder.create_branch_label();
                let unreachable = self.binder.unreachable;
                let true_target = self.binder.true_target;
                let false_target = self.binder.false_target;
                self.stack.push(Continuation::ThenCond {
                    expr: false_expr,
                    true_target,
                    false_target,
                });
                self.stack.push(Continuation::ThenFinish {
                    label: false_branch_label,
                    default: Some(unreachable),
                });
                self.stack.push(Continuation::ThenCond {
                    expr: true_expr,
                    true_target,
                    false_target,
                });
                self.stack.push(Continuation::ThenFinish {
                    label: true_branch_label,
                    default: Some(unreachable),
                });
                Step::Task(Task::Cond(
                    condition,
                    current,
                    true_branch_label,
                    false_branch_label,
                ))
            }
        }
    }

    /// Bind all children of an AST node in order (results are ignored; current is passed through)
    fn spawn_children(&mut self, node: LuaAst, current: FlowId) -> Step {
        let children = node.children::<LuaAst>().collect::<Vec<_>>();
        if children.is_empty() {
            return Step::Done(current);
        }
        let mut pending = Vec::with_capacity(children.len() - 1);
        for child in children.iter().skip(1).rev() {
            pending.push(Task::Node(child.clone(), current));
        }
        self.stack.push(Continuation::Seq { pending });
        Step::Task(Task::Node(children[0].clone(), current))
    }

    fn evaluate_node(&mut self, node: LuaAst, current: FlowId) -> Step {
        match node {
            LuaAst::LuaBlock(block) => Step::Task(Task::Block(block, current)),
            LuaAst::LuaAssignStat(assign_stat) => {
                let (vars, values) = assign_stat.get_var_and_expr_list();
                let mut pending = Vec::new();
                // Bind the values first, then the variables (pop order = original recursive binding order)
                for var in vars.iter().rev() {
                    if let Some(ast) = LuaAst::cast(var.syntax().clone()) {
                        pending.push(Task::Node(ast, current));
                    }
                }
                for expr in values.iter().rev() {
                    if let Some(ast) = LuaAst::cast(expr.syntax().clone()) {
                        pending.push(Task::Node(ast, current));
                    }
                }
                self.stack.push(Continuation::AssignDone {
                    assign_stat,
                    current,
                });
                if pending.is_empty() {
                    Step::Task(Task::Pass(current))
                } else {
                    self.stack.push(Continuation::Seq { pending });
                    Step::Task(Task::Pass(current))
                }
            }
            LuaAst::LuaLocalStat(local_stat) => {
                let local_names = local_stat.get_local_name_list().collect::<Vec<_>>();
                let values = local_stat.get_value_exprs().collect::<Vec<_>>();
                let min_len = local_names.len().min(values.len());
                for i in 0..min_len {
                    let name = &local_names[i];
                    let value = &values[i];
                    let decl_id = LuaDeclId::new(self.binder.file_id, name.get_position());
                    if check_local_immutable(self.binder, decl_id)
                        && check_value_expr_is_check_expr(value.clone())
                    {
                        self.binder
                            .decl_bind_expr_ref
                            .insert(decl_id, value.to_ptr());
                    }
                }
                self.stack.push(Continuation::LocalDone {
                    local_stat,
                    current,
                });
                self.spawn_expr_sequence(&values, current)
            }
            LuaAst::LuaReturnStat(return_stat) => {
                let exprs = return_stat.get_expr_list().collect::<Vec<_>>();
                self.stack.push(Continuation::ReturnDone { current });
                self.spawn_expr_sequence(&exprs, current)
            }
            LuaAst::LuaCallExprStat(call_expr_stat) => {
                self.evaluate_call_expr_stat(call_expr_stat, current)
            }
            LuaAst::LuaLabelStat(label_stat) => {
                let Some(label_name_token) = label_stat.get_label_name_token() else {
                    return Step::Done(current);
                };
                let label_name = label_name_token.get_name_text();
                let closure_id = LuaClosureId::from_node(label_stat.syntax());
                self.binder
                    .db
                    .get_reference_index_mut()
                    .add_label_declaration(
                        self.binder.file_id,
                        closure_id,
                        label_name,
                        label_name_token.get_range(),
                    );
                let name_label = self.binder.create_name_label(label_name, closure_id);
                self.binder.add_antecedent(name_label, current);
                Step::Done(name_label)
            }
            LuaAst::LuaBreakStat(break_stat) => {
                let break_flow_id = self.binder.create_break();
                if let Some(loop_flow) = self.binder.get_flow(self.binder.loop_label)
                    && loop_flow.kind.is_unreachable()
                {
                    self.binder.report_error(crate::AnalyzeError::new(
                        crate::DiagnosticCode::SyntaxError,
                        &t!("Break outside loop"),
                        break_stat.get_range(),
                    ));
                    return Step::Done(current);
                }
                self.binder.add_antecedent(break_flow_id, current);
                self.binder
                    .add_antecedent(self.binder.break_target_label, break_flow_id);
                Step::Done(break_flow_id)
            }
            LuaAst::LuaContinueStat(continue_stat) => {
                let continue_flow_id = self.binder.create_continue();
                if let Some(loop_flow) = self.binder.get_flow(self.binder.loop_label)
                    && loop_flow.kind.is_unreachable()
                {
                    self.binder.report_error(crate::AnalyzeError::new(
                        crate::DiagnosticCode::SyntaxError,
                        &t!("Continue outside loop"),
                        continue_stat.get_range(),
                    ));
                    return Step::Done(current);
                }
                self.binder.add_antecedent(continue_flow_id, current);
                self.binder
                    .add_antecedent(self.binder.loop_label, continue_flow_id);
                Step::Done(continue_flow_id)
            }
            LuaAst::LuaGotoStat(goto_stat) => {
                let closure_id = LuaClosureId::from_node(goto_stat.syntax());
                let Some(label_token) = goto_stat.get_label_name_token() else {
                    return Step::Done(current);
                };
                let label_name = label_token.get_name_text();
                self.binder
                    .db
                    .get_reference_index_mut()
                    .add_label_reference(
                        self.binder.file_id,
                        closure_id,
                        label_name,
                        label_token.get_range(),
                    );
                let return_flow_id = self.binder.create_return();
                self.binder.cache_goto_flow(
                    closure_id,
                    label_token.clone(),
                    label_name,
                    return_flow_id,
                );
                self.binder.add_antecedent(return_flow_id, current);
                Step::Done(return_flow_id)
            }
            LuaAst::LuaDoStat(do_stat) => match do_stat.get_block() {
                Some(block) => Step::Task(Task::Block(block, current)),
                None => Step::Done(current),
            },
            LuaAst::LuaWhileStat(while_stat) => self.evaluate_while_stat(while_stat, current),
            LuaAst::LuaRepeatStat(repeat_stat) => self.evaluate_repeat_stat(repeat_stat, current),
            LuaAst::LuaIfStat(if_stat) => self.evaluate_if_stat(if_stat, current),
            LuaAst::LuaForStat(for_stat) => self.evaluate_for_stat(for_stat, current),
            LuaAst::LuaForRangeStat(for_range_stat) => {
                self.evaluate_for_range_stat(for_range_stat, current)
            }
            LuaAst::LuaFuncStat(func_stat) => {
                if func_stat.get_func_name().is_none() {
                    return Step::Done(current);
                }
                self.stack.push(Continuation::FuncDone {
                    func_stat: func_stat.clone(),
                    current,
                });
                self.spawn_children(LuaAst::LuaFuncStat(func_stat), current)
            }
            LuaAst::LuaLocalFuncStat(local_func_stat) => {
                self.stack.push(Continuation::LocalFuncDone { current });
                self.spawn_children(LuaAst::LuaLocalFuncStat(local_func_stat), current)
            }
            LuaAst::LuaComment(comment) => {
                Step::Done(super::comment::bind_comment(self.binder, comment, current))
            }
            // exprs
            LuaAst::LuaNameExpr(_)
            | LuaAst::LuaIndexExpr(_)
            | LuaAst::LuaTableExpr(_)
            | LuaAst::LuaBinaryExpr(_)
            | LuaAst::LuaUnaryExpr(_)
            | LuaAst::LuaParenExpr(_)
            | LuaAst::LuaCallExpr(_)
            | LuaAst::LuaLiteralExpr(_)
            | LuaAst::LuaClosureExpr(_) => match LuaExpr::cast(node.syntax().clone()) {
                Some(expr) => Step::Task(Task::Expr(expr, current)),
                None => Step::Done(current),
            },
            LuaAst::LuaTableField(_)
            | LuaAst::LuaParamList(_)
            | LuaAst::LuaParamName(_)
            | LuaAst::LuaCallArgList(_)
            | LuaAst::LuaLocalName(_) => self.spawn_children(node, current),
            _ => Step::Done(current),
        }
    }

    /// Bind a sequence of value expressions in order (current is passed through)
    fn spawn_expr_sequence(&mut self, exprs: &[LuaExpr], current: FlowId) -> Step {
        if exprs.is_empty() {
            return Step::Task(Task::Pass(current));
        }
        let mut pending = Vec::with_capacity(exprs.len() - 1);
        for expr in exprs.iter().skip(1).rev() {
            pending.push(Task::Expr(expr.clone(), current));
        }
        self.stack.push(Continuation::Seq { pending });
        Step::Task(Task::Expr(exprs[0].clone(), current))
    }

    fn evaluate_call_expr_stat(
        &mut self,
        call_expr_stat: LuaCallExprStat,
        current: FlowId,
    ) -> Step {
        let Some(call_expr) = call_expr_stat.get_call_expr() else {
            return Step::Done(current);
        };

        if call_expr.is_assert() {
            let Some(arg_list) = call_expr.get_args_list() else {
                return Step::Done(current);
            };
            let args = arg_list.get_args().collect::<Vec<_>>();
            if args.is_empty() {
                return Step::Done(current);
            }
            let false_target = self.binder.unreachable;
            let labels = args
                .iter()
                .map(|_| self.binder.create_branch_label())
                .collect::<Vec<_>>();
            let first_arg = args[0].clone();
            let first_label = labels[0];
            self.stack.push(Continuation::AssertFinish {
                args,
                idx: 0,
                labels,
                false_target,
            });
            Step::Task(Task::Cond(first_arg, current, first_label, false_target))
        } else {
            let kind = if call_expr.is_error() {
                CallStatKind::Error
            } else {
                CallStatKind::Normal
            };
            self.stack.push(Continuation::CallStatDone {
                call_expr_stat,
                current,
                kind,
            });
            match LuaAst::cast(call_expr.syntax().clone()) {
                Some(ast) => self.spawn_children(ast, current),
                None => Step::Task(Task::Pass(current)),
            }
        }
    }

    fn evaluate_while_stat(&mut self, while_stat: LuaWhileStat, current: FlowId) -> Step {
        let pre_while_label = self.binder.create_loop_label();
        let after_while_label = self.binder.create_branch_label();
        let pre_block_label = self.binder.create_branch_label();
        self.binder.add_antecedent(pre_while_label, current);
        let Some(condition_expr) = while_stat.get_condition_expr() else {
            return Step::Done(current);
        };

        let old_loop_label = self.binder.loop_label;
        let old_break_target_label = self.binder.break_target_label;
        self.binder.loop_label = pre_while_label;
        self.binder.break_target_label = after_while_label;

        let has_block = while_stat.get_block().is_some();
        match static_literal_truthiness(&condition_expr) {
            Some(false) => {
                self.binder.loop_label = old_loop_label;
                self.binder.break_target_label = old_break_target_label;
                Step::Done(current)
            }
            Some(true) => {
                self.stack.push(Continuation::WhileDone {
                    after_label: after_while_label,
                    loop_enters: true,
                    has_block,
                    current,
                    old_loop_label,
                    old_break_target_label,
                });
                match while_stat.get_block() {
                    Some(block) => Step::Task(Task::Block(block, current)),
                    None => Step::Task(Task::Pass(current)),
                }
            }
            None => {
                self.stack.push(Continuation::WhileDone {
                    after_label: after_while_label,
                    loop_enters: false,
                    has_block,
                    current,
                    old_loop_label,
                    old_break_target_label,
                });
                if let Some(block) = while_stat.get_block() {
                    self.stack.push(Continuation::ThenBlock { block });
                }
                self.stack.push(Continuation::ThenFinish {
                    label: pre_block_label,
                    default: None,
                });
                Step::Task(Task::Cond(
                    condition_expr,
                    current,
                    pre_block_label,
                    after_while_label,
                ))
            }
        }
    }

    fn evaluate_repeat_stat(&mut self, repeat_stat: LuaRepeatStat, current: FlowId) -> Step {
        let pre_repeat_label = self.binder.create_loop_label();
        let post_repeat_label = self.binder.create_branch_label();
        self.binder.add_antecedent(pre_repeat_label, current);

        let old_loop_label = self.binder.loop_label;
        let old_break_target_label = self.binder.break_target_label;
        self.binder.loop_label = pre_repeat_label;
        self.binder.break_target_label = post_repeat_label;

        self.stack.push(Continuation::RepeatDone {
            post_label: post_repeat_label,
            old_loop_label,
            old_break_target_label,
        });
        if let Some(condition_expr) = repeat_stat.get_condition_expr() {
            self.stack.push(Continuation::ThenCond {
                expr: condition_expr,
                true_target: post_repeat_label,
                false_target: pre_repeat_label,
            });
        }
        if let Some(block) = repeat_stat.get_block() {
            self.stack.push(Continuation::ThenBlock { block });
        }
        Step::Task(Task::Finish(pre_repeat_label, current))
    }

    fn evaluate_if_stat(&mut self, if_stat: LuaIfStat, current: FlowId) -> Step {
        let post_if_label = self.binder.create_branch_label();
        let else_label = self.binder.create_branch_label();
        let then_label = self.binder.create_branch_label();
        let clauses = if_stat.get_else_if_clause_list().collect::<Vec<_>>();
        let else_clause = if_stat.get_else_clause();
        let else_block = else_clause.and_then(|clause| clause.get_block());

        self.stack.push(Continuation::IfBranchDone {
            clauses,
            idx: 0,
            else_label,
            post_if: post_if_label,
            current,
            else_block,
        });
        if let Some(then_block) = if_stat.get_block() {
            self.stack
                .push(Continuation::ThenBlock { block: then_block });
        }
        self.stack.push(Continuation::ThenFinish {
            label: then_label,
            default: Some(current),
        });
        match if_stat.get_condition_expr() {
            Some(condition_expr) => {
                Step::Task(Task::Cond(condition_expr, current, then_label, else_label))
            }
            None => Step::Task(Task::Pass(current)),
        }
    }

    fn evaluate_for_stat(&mut self, for_stat: LuaForStat, current: FlowId) -> Step {
        let pre_for_label = self.binder.create_loop_label();
        let post_for_label = self.binder.create_branch_label();
        self.binder.add_antecedent(pre_for_label, current);

        let iter_exprs = for_stat.get_iter_expr().collect::<Vec<_>>();
        let loop_enters = match iter_exprs.as_slice() {
            [start_expr, stop_expr] => match (
                static_number_value(start_expr),
                static_number_value(stop_expr),
            ) {
                (Some(start), Some(stop)) => start <= stop,
                _ => false,
            },
            [start_expr, stop_expr, step_expr, ..] => match (
                static_number_value(start_expr),
                static_number_value(stop_expr),
                static_number_value(step_expr),
            ) {
                (Some(start), Some(stop), Some(step)) => {
                    (step > 0.0 && start <= stop) || (step < 0.0 && start >= stop)
                }
                _ => false,
            },
            _ => false,
        };

        let old_loop_label = self.binder.loop_label;
        let old_break_target_label = self.binder.break_target_label;
        self.binder.loop_label = pre_for_label;
        self.binder.break_target_label = post_for_label;

        let block = for_stat.get_block();
        self.stack.push(Continuation::ForDone {
            post_label: post_for_label,
            loop_enters,
            has_block: block.is_some(),
            current,
            old_loop_label,
            old_break_target_label,
        });
        self.stack.push(Continuation::ThenForNode {
            for_stat,
            pre_label: pre_for_label,
            block,
        });
        self.spawn_expr_sequence(&iter_exprs, current)
    }

    fn evaluate_for_range_stat(
        &mut self,
        for_range_stat: LuaForRangeStat,
        current: FlowId,
    ) -> Step {
        let pre_for_range_label = self.binder.create_loop_label();
        let post_for_range_label = self.binder.create_branch_label();
        self.binder.add_antecedent(pre_for_range_label, current);

        let old_loop_label = self.binder.loop_label;
        let old_break_target_label = self.binder.break_target_label;
        self.binder.loop_label = pre_for_range_label;
        self.binder.break_target_label = post_for_range_label;

        let exprs = for_range_stat.get_expr_list().collect::<Vec<_>>();
        self.stack.push(Continuation::ForRangeDone {
            current,
            old_loop_label,
            old_break_target_label,
        });
        if let Some(block) = for_range_stat.get_block() {
            self.stack.push(Continuation::ThenBlock { block });
        }
        self.stack.push(Continuation::ThenForRangeDecl {
            for_range_stat,
            pre_label: pre_for_range_label,
        });
        self.spawn_expr_sequence(&exprs, current)
    }

    fn resume(&mut self, continuation: Continuation, value: FlowId) -> Step {
        match continuation {
            Continuation::Seq { mut pending } => match pending.pop() {
                Some(task) => {
                    self.stack.push(Continuation::Seq { pending });
                    Step::Task(task)
                }
                None => Step::Done(value),
            },
            Continuation::CondNodes {
                expr,
                current,
                true_target,
                false_target,
                old_true,
                old_false,
            } => {
                self.binder.true_target = old_true;
                self.binder.false_target = old_false;
                if !is_binary_logical(&expr) {
                    let true_condition = self
                        .binder
                        .create_node(FlowNodeKind::TrueCondition(expr.to_ptr()));
                    self.binder.add_antecedent(true_condition, current);
                    self.binder.add_antecedent(true_target, true_condition);

                    let false_condition = self
                        .binder
                        .create_node(FlowNodeKind::FalseCondition(expr.to_ptr()));
                    self.binder.add_antecedent(false_condition, current);
                    self.binder.add_antecedent(false_target, false_condition);
                }
                Step::Done(current)
            }
            Continuation::ThenFinish { label, default } => {
                Step::Task(Task::Finish(label, default.unwrap_or(value)))
            }
            Continuation::ThenCond {
                expr,
                true_target,
                false_target,
            } => Step::Task(Task::Cond(expr, value, true_target, false_target)),
            Continuation::ThenBlock { block } => Step::Task(Task::Block(block, value)),
            Continuation::SafeIndexDone { index_expr } => {
                self.spawn_children(LuaAst::LuaIndexExpr(index_expr), value)
            }
            Continuation::UnaryNotDone {
                old_true,
                old_false,
            } => {
                self.binder.true_target = old_true;
                self.binder.false_target = old_false;
                Step::Done(value)
            }
            Continuation::AssertCond {
                args,
                idx,
                labels,
                false_target,
            } => {
                if idx >= args.len() {
                    Step::Done(value)
                } else {
                    let arg = args[idx].clone();
                    let label = labels[idx];
                    self.stack.push(Continuation::AssertFinish {
                        args,
                        idx,
                        labels,
                        false_target,
                    });
                    Step::Task(Task::Cond(arg, value, label, false_target))
                }
            }
            Continuation::AssertFinish {
                args,
                idx,
                labels,
                false_target,
            } => {
                let label = labels[idx];
                self.stack.push(Continuation::AssertCond {
                    args,
                    idx: idx + 1,
                    labels,
                    false_target,
                });
                Step::Task(Task::Finish(label, value))
            }
            Continuation::LocalDone {
                local_stat,
                current,
            } => {
                let local_flow_id = self.binder.create_decl(local_stat.get_position());
                self.binder.add_antecedent(local_flow_id, current);
                let local_names = local_stat.get_local_name_list().collect::<Vec<_>>();
                let values = local_stat.get_value_exprs().collect::<Vec<_>>();
                bind_multi_return_refs(
                    self.binder,
                    &get_local_decl_ids(self.binder, &local_names),
                    &values,
                    local_stat.get_position(),
                    local_flow_id,
                );
                Step::Done(local_flow_id)
            }
            Continuation::AssignDone {
                assign_stat,
                current,
            } => {
                let assignment_kind = FlowNodeKind::Assignment(assign_stat.to_ptr());
                let flow_id = self.binder.create_node(assignment_kind);
                self.binder.add_antecedent(flow_id, current);
                let (vars, values) = assign_stat.get_var_and_expr_list();
                bind_multi_return_refs(
                    self.binder,
                    &get_var_decl_ids(self.binder, &vars),
                    &values,
                    assign_stat.get_position(),
                    flow_id,
                );
                Step::Done(flow_id)
            }
            Continuation::ReturnDone { current } => {
                let return_flow_id = self.binder.create_return();
                self.binder.add_antecedent(return_flow_id, current);
                Step::Done(return_flow_id)
            }
            Continuation::CallStatDone {
                call_expr_stat,
                current,
                kind,
            } => match kind {
                CallStatKind::Normal => {
                    let flow_id = self
                        .binder
                        .create_node(FlowNodeKind::CallExprStat(call_expr_stat.to_ptr()));
                    self.binder.add_antecedent(flow_id, current);
                    Step::Done(flow_id)
                }
                CallStatKind::Error => {
                    let return_flow_id = self.binder.create_return();
                    self.binder.add_antecedent(return_flow_id, current);
                    Step::Done(return_flow_id)
                }
            },
            Continuation::FuncDone { func_stat, current } => match func_stat.get_func_name() {
                Some(LuaVarExpr::NameExpr(_)) => {
                    let func_kind = FlowNodeKind::ImplFunc(func_stat.to_ptr());
                    let flow_id = self.binder.create_node(func_kind);
                    self.binder.add_antecedent(flow_id, current);
                    Step::Done(flow_id)
                }
                _ => Step::Done(current),
            },
            Continuation::LocalFuncDone { current } => Step::Done(current),
            Continuation::WhileDone {
                after_label,
                loop_enters,
                has_block,
                current,
                old_loop_label,
                old_break_target_label,
            } => {
                self.binder.loop_label = old_loop_label;
                self.binder.break_target_label = old_break_target_label;
                if loop_enters && has_block {
                    Step::Done(finish_entered_loop_post_flow(
                        self.binder,
                        after_label,
                        value,
                    ))
                } else {
                    Step::Done(current)
                }
            }
            Continuation::RepeatDone {
                post_label,
                old_loop_label,
                old_break_target_label,
            } => {
                self.binder.loop_label = old_loop_label;
                self.binder.break_target_label = old_break_target_label;
                Step::Done(finish_flow_label(self.binder, post_label, value))
            }
            Continuation::ForDone {
                post_label,
                loop_enters,
                has_block,
                current,
                old_loop_label,
                old_break_target_label,
            } => {
                self.binder.loop_label = old_loop_label;
                self.binder.break_target_label = old_break_target_label;
                if loop_enters && has_block {
                    Step::Done(finish_entered_loop_post_flow(
                        self.binder,
                        post_label,
                        value,
                    ))
                } else {
                    Step::Done(current)
                }
            }
            Continuation::ForRangeDone {
                current,
                old_loop_label,
                old_break_target_label,
            } => {
                self.binder.loop_label = old_loop_label;
                self.binder.break_target_label = old_break_target_label;
                Step::Done(current)
            }
            Continuation::ThenForNode {
                for_stat,
                pre_label,
                block,
            } => {
                let for_node = self
                    .binder
                    .create_node(FlowNodeKind::ForIStat(for_stat.to_ptr()));
                self.binder.add_antecedent(for_node, pre_label);
                match block {
                    Some(block) => Step::Task(Task::Block(block, for_node)),
                    None => Step::Task(Task::Pass(for_node)),
                }
            }
            Continuation::ThenForRangeDecl {
                for_range_stat,
                pre_label,
            } => {
                let decl_flow = self.binder.create_decl(for_range_stat.get_position());
                self.binder.add_antecedent(decl_flow, pre_label);
                Step::Task(Task::Finish(pre_label, value))
            }
            Continuation::IfBranchDone {
                clauses,
                idx,
                else_label,
                post_if,
                current,
                else_block,
            } => {
                self.binder.add_antecedent(post_if, value);
                if idx >= clauses.len() {
                    // else branch
                    match else_block {
                        Some(block) => {
                            self.stack.push(Continuation::IfFinal {
                                post_if,
                                else_label,
                            });
                            Step::Task(Task::Block(block, else_label))
                        }
                        None => {
                            self.binder.add_antecedent(post_if, else_label);
                            Step::Done(finalize_if(self.binder, post_if, else_label))
                        }
                    }
                } else {
                    // process one elseif clause
                    let clause = clauses[idx].clone();
                    let elseif_then_label = self.binder.create_branch_label();
                    let post_elseif_label = self.binder.create_branch_label();
                    self.stack.push(Continuation::IfBranchDone {
                        clauses,
                        idx: idx + 1,
                        else_label: post_elseif_label,
                        post_if,
                        current,
                        else_block,
                    });
                    if let Some(block) = clause.get_block() {
                        self.stack.push(Continuation::ThenBlock { block });
                    }
                    self.stack.push(Continuation::ThenFinish {
                        label: elseif_then_label,
                        default: Some(current),
                    });
                    if let Some(condition_expr) = clause.get_condition_expr() {
                        self.stack.push(Continuation::ThenCond {
                            expr: condition_expr,
                            true_target: elseif_then_label,
                            false_target: post_elseif_label,
                        });
                    }
                    Step::Task(Task::Finish(else_label, current))
                }
            }
            Continuation::IfFinal {
                post_if,
                else_label,
            } => {
                self.binder.add_antecedent(post_if, value);
                Step::Done(finalize_if(self.binder, post_if, else_label))
            }
            Continuation::BlockIter {
                children,
                mut idx,
                mut current,
                mut can_change_flow,
            } => {
                if can_change_flow {
                    current = value;
                }
                if let Some(flow_node) = self.binder.get_flow(current) {
                    match &flow_node.kind {
                        FlowNodeKind::Return | FlowNodeKind::Break | FlowNodeKind::Continue => {
                            current = self.binder.unreachable;
                            can_change_flow = false;
                        }
                        _ => {}
                    }
                }
                idx += 1;
                if idx < children.len() {
                    let next = children[idx].clone();
                    self.stack.push(Continuation::BlockIter {
                        children,
                        idx,
                        current,
                        can_change_flow,
                    });
                    Step::Task(Task::Node(next, current))
                } else {
                    Step::Done(current)
                }
            }
        }
    }
}

fn finalize_if(binder: &mut FlowBinder<'_>, post_if: FlowId, else_label: FlowId) -> FlowId {
    if let Some(flow_node) = binder.get_flow(post_if)
        && flow_node.antecedent.is_none()
    {
        return binder.unreachable;
    }

    finish_flow_label(binder, post_if, else_label)
}

/// Entry: bind a block and return the final flow id
pub(super) fn run_bind_block(binder: &mut FlowBinder, block: LuaBlock, current: FlowId) -> FlowId {
    BindEngine::new(binder).run(Task::Block(block, current))
}

/// Entry: bind an expression
pub(super) fn run_bind_expr(binder: &mut FlowBinder, expr: LuaExpr, current: FlowId) -> FlowId {
    BindEngine::new(binder).run(Task::Expr(expr, current))
}

#[cfg(test)]
mod tests {
    use crate::VirtualWorkspace;

    #[test]
    fn test_flow_bind_deep_index_chain() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@type { x: integer }\nlocal t");

        let mut expr = "t".to_string();
        for _ in 0..1_500 {
            expr.push_str(".x");
        }
        let _ = ws.expr_ty(&expr);
    }

    #[test]
    fn test_flow_bind_deep_call_chain() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@type fun(): any\nlocal f");

        let mut expr = "f".to_string();
        for _ in 0..2_000 {
            expr.push_str("()");
        }
        let _ = ws.expr_ty(&expr);
    }

    #[test]
    fn test_flow_bind_deep_paren_chain() {
        let mut ws = VirtualWorkspace::new();
        let mut expr = "1".to_string();
        for _ in 0..150 {
            expr.insert(0, '(');
            expr.push(')');
        }
        let _ = ws.expr_ty(&expr);
    }

    #[test]
    fn test_flow_bind_deep_logical_chain() {
        let mut ws = VirtualWorkspace::new();
        let mut expr = "1".to_string();
        for _ in 0..1_500 {
            expr.push_str(" and 1");
        }
        let ty = ws.expr_ty(&expr);
        assert!(ty.is_integer());
    }

    // Deeply nested if blocks
    #[test]
    fn test_flow_bind_deep_nested_blocks() {
        let mut ws = VirtualWorkspace::new();
        let mut code = String::new();
        for _ in 0..100 {
            code.push_str("if true then ");
        }
        code.push_str("local x = 1");
        for _ in 0..100 {
            code.push_str(" end");
        }
        ws.def(&code);
    }
}
