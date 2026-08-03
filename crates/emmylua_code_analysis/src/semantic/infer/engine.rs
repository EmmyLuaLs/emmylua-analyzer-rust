//! Inference engine: replaces native recursion with an explicit task stack
//!
//! Recursion risk mainly comes from "linear structural chains": paren nesting, member chains, call chains, binary chains,
//! ternary chains, etc. The depth is fully determined by user code and can reach tens of thousands of levels. This engine converts expression inference
//! into a dispatch loop plus an explicit continuation stack, keeping the native call stack depth constant.
//!
//! The parts that still use native recursion (member lookup, call resolution internals, generic instantiation, etc.) are bounded
//! by the `LuaInferCache::infer_depth` budget; when exceeded, they return
//! `InferFailReason::DepthLimit` to degrade gracefully instead of crashing with a stack overflow.

use emmylua_parser::{
    LuaAstNode, LuaBinaryExpr, LuaCallExpr, LuaExpr, LuaIndexExpr, LuaIndexKey, LuaIndexMemberExpr,
    LuaSyntaxId, LuaTernaryExpr, LuaUnaryExpr,
};

use crate::{
    CacheEntry, DbIndex, InferGuard, LuaInferCache, TypeOps,
    db_index::LuaType,
    semantic::infer::{
        infer_binary::infer_binary_expr_result,
        infer_call::{
            check_can_infer, infer_call_expr_func, infer_require_call, infer_setmetatable_call,
        },
        infer_index::{infer_index_expr_with_member, infer_member, infer_member_by_key_type},
        infer_unary::infer_unary_expr_result,
        narrow::get_type_at_call_expr_inline_cast,
    },
};

use super::{
    InferFailReason, InferResult, infer_closure_expr, infer_literal_expr, infer_name_expr,
    infer_table_expr, prepare_expr_cache,
};

/// Maximum depth of the explicit stack, preventing pathological inputs from consuming too much memory
pub(super) const MAX_ENGINE_STACK_DEPTH: usize = 65536;
/// Maximum native recursion nesting depth (all inference entry points share the same budget)
pub(super) const MAX_INFER_DEPTH: u32 = 1024;

/// Engine scheduler step
enum Step {
    /// Dispatch a subtask; push the continuation onto the stack first if present
    Task(Task, Option<Continuation>),
    /// Task completed, the result propagates upward (pop the stack to resume the parent task, or return it as the final result)
    Complete(InferResult),
}

/// Task: currently only "infer one expression"; the rest of the logic runs natively in resume
enum Task {
    Expr(LuaExpr),
}

/// Suspended parent task state
enum Continuation {
    /// Paren expression: write back to the parent cache after the inner expression completes
    ExprFinalize { syntax_id: LuaSyntaxId },
    /// Call expression: continue once the prefix type is known
    CallPrefix { call_expr: LuaCallExpr },
    /// Index expression: continue once the prefix type is known
    MemberPrefix {
        index_expr: LuaIndexExpr,
        pass_flow: bool,
    },
    /// Index expression: continue once the key expression type is known
    IndexKey {
        index_expr: LuaIndexExpr,
        prefix_type: LuaType,
        pass_flow: bool,
    },
    /// Binary operation: left operand completed
    BinaryLeft { binary_expr: LuaBinaryExpr },
    /// Binary operation: right operand completed
    BinaryRight {
        binary_expr: LuaBinaryExpr,
        left_type: LuaType,
    },
    /// Unary operation: operand completed
    UnaryInner { unary_expr: LuaUnaryExpr },
    /// Ternary operation: true branch completed
    TernaryTrue { ternary_expr: LuaTernaryExpr },
    /// Ternary operation: false branch completed
    TernaryFalse {
        ternary_expr: LuaTernaryExpr,
        true_type: LuaType,
    },
}

/// Inference engine
pub(super) struct InferEngine<'a> {
    db: &'a DbIndex,
    cache: &'a mut LuaInferCache,
    stack: Vec<Continuation>,
}

impl<'a> InferEngine<'a> {
    pub(super) fn new(db: &'a DbIndex, cache: &'a mut LuaInferCache) -> Self {
        Self {
            db,
            cache,
            stack: Vec::new(),
        }
    }

    /// Run inference until the root task completes
    pub(super) fn run(&mut self, expr: LuaExpr) -> InferResult {
        let mut step = Step::Task(Task::Expr(expr), None);
        loop {
            step = match step {
                Step::Task(task, continuation) => {
                    if let Some(continuation) = continuation {
                        if self.stack.len() >= MAX_ENGINE_STACK_DEPTH {
                            // Depth limit: degrade all pending tasks with DepthLimit
                            let err = InferFailReason::DepthLimit;
                            while let Some(pending) = self.stack.pop() {
                                let _ = self.resume(pending, Err(err.clone()));
                            }
                            return Err(err);
                        }
                        self.stack.push(continuation);
                    }
                    self.evaluate(task)
                }
                Step::Complete(result) => match self.stack.pop() {
                    Some(continuation) => self.resume(continuation, result),
                    None => return result,
                },
            };
        }
    }

    fn evaluate(&mut self, task: Task) -> Step {
        match task {
            Task::Expr(expr) => self.evaluate_expr(expr),
        }
    }

    fn evaluate_expr(&mut self, expr: LuaExpr) -> Step {
        let no_flow = self.cache.is_no_flow();
        let syntax_id = expr.get_syntax_id();
        match prepare_expr_cache(self.db, self.cache, syntax_id) {
            Ok(Some(ty)) => return Step::Complete(Ok(ty)),
            Ok(None) => {}
            Err(err) => return Step::Complete(Err(err)),
        }

        if no_flow
            && matches!(expr, LuaExpr::TableExpr(_))
            && !self.cache.no_flow_table_exprs.contains(&syntax_id)
        {
            self.cache
                .expr_no_flow_cache
                .insert(syntax_id, CacheEntry::Cache(None));
            return Step::Complete(Err(InferFailReason::None));
        }

        match expr {
            LuaExpr::CallExpr(call_expr) => self.evaluate_call_expr(syntax_id, call_expr),
            LuaExpr::TableExpr(table_expr) => {
                let result = infer_table_expr(self.db, self.cache, table_expr);
                self.complete_expr(syntax_id, result)
            }
            LuaExpr::LiteralExpr(literal_expr) => {
                let result = infer_literal_expr(self.db, self.cache, literal_expr);
                self.complete_expr(syntax_id, result)
            }
            LuaExpr::BinaryExpr(binary_expr) => {
                let Some(op_token) = binary_expr.get_op_token() else {
                    return self.complete_expr(syntax_id, Err(InferFailReason::None));
                };
                let _ = op_token.get_op();
                let Some((left, _)) = binary_expr.get_exprs() else {
                    return self.complete_expr(syntax_id, Err(InferFailReason::None));
                };
                Step::Task(
                    Task::Expr(left),
                    Some(Continuation::BinaryLeft { binary_expr }),
                )
            }
            LuaExpr::UnaryExpr(unary_expr) => {
                let Some(op_token) = unary_expr.get_op_token() else {
                    return self.complete_expr(syntax_id, Err(InferFailReason::None));
                };
                let _ = op_token.get_op();
                let Some(inner_expr) = unary_expr.get_expr() else {
                    return self.complete_expr(syntax_id, Err(InferFailReason::None));
                };
                Step::Task(
                    Task::Expr(inner_expr),
                    Some(Continuation::UnaryInner { unary_expr }),
                )
            }
            LuaExpr::ClosureExpr(closure_expr) => {
                let result = infer_closure_expr(self.db, self.cache, closure_expr);
                self.complete_expr(syntax_id, result)
            }
            LuaExpr::ParenExpr(paren_expr) => {
                let Some(inner_expr) = paren_expr.get_expr() else {
                    return self.complete_expr(syntax_id, Err(InferFailReason::None));
                };
                Step::Task(
                    Task::Expr(inner_expr),
                    Some(Continuation::ExprFinalize { syntax_id }),
                )
            }
            LuaExpr::NameExpr(name_expr) => {
                let result = infer_name_expr(self.db, self.cache, name_expr);
                self.complete_expr(syntax_id, result)
            }
            LuaExpr::IndexExpr(index_expr) => {
                let Some(prefix_expr) = index_expr.get_prefix_expr() else {
                    return self.complete_expr(syntax_id, Err(InferFailReason::None));
                };
                Step::Task(
                    Task::Expr(prefix_expr),
                    Some(Continuation::MemberPrefix {
                        index_expr,
                        pass_flow: !no_flow,
                    }),
                )
            }
            LuaExpr::TernaryExpr(ternary_expr) => {
                let Some((true_expr, _)) = ternary_expr.get_true_false_exprs() else {
                    return self.complete_expr(syntax_id, Err(InferFailReason::None));
                };
                Step::Task(
                    Task::Expr(true_expr),
                    Some(Continuation::TernaryTrue { ternary_expr }),
                )
            }
        }
    }

    fn evaluate_call_expr(&mut self, syntax_id: LuaSyntaxId, call_expr: LuaCallExpr) -> Step {
        if call_expr.is_require() {
            let result = infer_require_call(self.db, self.cache, call_expr);
            return self.complete_expr(syntax_id, result);
        }
        if call_expr.is_setmetatable() {
            let result = infer_setmetatable_call(self.db, self.cache, call_expr);
            return self.complete_expr(syntax_id, result);
        }
        if let Err(err) = check_can_infer(self.db, self.cache, &call_expr) {
            return self.complete_expr(syntax_id, Err(err));
        }
        let Some(prefix_expr) = call_expr.get_prefix_expr() else {
            return self.complete_expr(syntax_id, Err(InferFailReason::None));
        };
        Step::Task(
            Task::Expr(prefix_expr),
            Some(Continuation::CallPrefix { call_expr }),
        )
    }

    fn evaluate_call_expr_with_prefix(
        &mut self,
        call_expr: LuaCallExpr,
        prefix_type: LuaType,
    ) -> Step {
        let syntax_id = call_expr.get_syntax_id();
        let is_safe_call = call_expr.has_safe_navigation();
        let ret_type = match infer_call_expr_func(
            self.db,
            self.cache,
            call_expr.clone(),
            prefix_type.clone(),
            &InferGuard::new(),
            None,
        ) {
            Ok(func_ty) => func_ty.get_ret().clone(),
            Err(err) => return self.complete_expr(syntax_id, Err(err)),
        };
        let ret_type = if is_safe_call && prefix_type.is_nullable() {
            TypeOps::Union.apply(self.db, &ret_type, &LuaType::Nil)
        } else {
            ret_type
        };
        let ret_type = if !self.cache.is_no_flow()
            && let Some(tree) = self
                .db
                .get_flow_index()
                .get_flow_tree(&self.cache.get_file_id())
            && let Some(flow_id) = tree.get_flow_id(call_expr.get_syntax_id())
            && let Some(flow_ret_type) = get_type_at_call_expr_inline_cast(
                self.db,
                self.cache,
                tree,
                call_expr,
                flow_id,
                ret_type.clone(),
            ) {
            flow_ret_type
        } else {
            ret_type
        };
        self.complete_expr(syntax_id, Ok(ret_type))
    }

    fn evaluate_index_member(
        &mut self,
        index_expr: LuaIndexExpr,
        prefix_type: LuaType,
        pass_flow: bool,
        key_type: Option<LuaType>,
    ) -> Step {
        let syntax_id = index_expr.get_syntax_id();
        let index_member_expr = LuaIndexMemberExpr::IndexExpr(index_expr.clone());
        let member_type = match key_type {
            Some(key_type) => infer_member_by_key_type(
                self.db,
                self.cache,
                &prefix_type,
                index_member_expr,
                &key_type,
                &InferGuard::new(),
            ),
            None => infer_member(
                self.db,
                self.cache,
                &prefix_type,
                index_member_expr,
                &InferGuard::new(),
            ),
        };
        let member_type = match member_type {
            Ok(ty) => ty,
            Err(err) => return self.complete_expr(syntax_id, Err(err)),
        };
        let result = infer_index_expr_with_member(
            self.db,
            self.cache,
            index_expr,
            prefix_type,
            member_type,
            pass_flow,
        );
        self.complete_expr(syntax_id, result)
    }

    /// Write the result back to the cache after the expression completes and propagate it upward
    fn complete_expr(&mut self, syntax_id: LuaSyntaxId, result: InferResult) -> Step {
        Step::Complete(self.finalize_expr(syntax_id, result))
    }

    /// Write the expression inference result back to the cache, consistent with the error handling semantics of the original recursive implementation
    fn finalize_expr(&mut self, syntax_id: LuaSyntaxId, result_type: InferResult) -> InferResult {
        let no_flow = self.cache.is_no_flow();
        match &result_type {
            Ok(result_type) => {
                if no_flow {
                    self.cache
                        .expr_no_flow_cache
                        .insert(syntax_id, CacheEntry::Cache(Some(result_type.clone())));
                } else {
                    self.cache
                        .expr_cache
                        .insert(syntax_id, CacheEntry::Cache(result_type.clone()));
                }
            }
            Err(InferFailReason::None)
            | Err(InferFailReason::RecursiveInfer)
            | Err(InferFailReason::DepthLimit) => {
                if no_flow {
                    self.cache
                        .expr_no_flow_cache
                        .insert(syntax_id, CacheEntry::Cache(None));
                } else {
                    self.cache
                        .expr_cache
                        .insert(syntax_id, CacheEntry::Cache(LuaType::Unknown));
                    return Ok(LuaType::Unknown);
                }
            }
            Err(InferFailReason::FieldNotFound) => {
                if no_flow {
                    self.cache.expr_no_flow_cache.remove(&syntax_id);
                } else if self.cache.get_config().analysis_phase.is_force() {
                    self.cache
                        .expr_cache
                        .insert(syntax_id, CacheEntry::Cache(LuaType::Nil));
                    return Ok(LuaType::Nil);
                } else {
                    self.cache.expr_cache.remove(&syntax_id);
                }
            }
            _ => {
                if no_flow {
                    self.cache.expr_no_flow_cache.remove(&syntax_id);
                } else {
                    self.cache.expr_cache.remove(&syntax_id);
                }
            }
        }

        result_type
    }

    /// Resume a suspended parent task
    fn resume(&mut self, continuation: Continuation, result: InferResult) -> Step {
        match continuation {
            Continuation::ExprFinalize { syntax_id } => self.complete_expr(syntax_id, result),
            Continuation::CallPrefix { call_expr } => {
                let syntax_id = call_expr.get_syntax_id();
                let prefix_type = match result {
                    Ok(ty) => ty,
                    Err(err) => return self.complete_expr(syntax_id, Err(err)),
                };
                self.evaluate_call_expr_with_prefix(call_expr, prefix_type)
            }
            Continuation::MemberPrefix {
                index_expr,
                pass_flow,
            } => {
                let syntax_id = index_expr.get_syntax_id();
                let no_flow = self.cache.is_no_flow();
                if no_flow
                    && let Some(prefix_expr) = index_expr.get_prefix_expr()
                    && is_declined_index_prefix(&prefix_expr)
                {
                    // Consistent with `try_infer_expr_for_index`: closure prefixes are declined for inference in no_flow mode
                    return self.complete_expr(syntax_id, Err(InferFailReason::None));
                }
                let prefix_type = match result {
                    Ok(ty) => ty,
                    Err(err) => {
                        if no_flow {
                            // In no_flow mode, `try_infer_expr_for_index` maps all failures to None
                            return self.complete_expr(syntax_id, Err(InferFailReason::None));
                        }
                        return self.complete_expr(syntax_id, Err(err));
                    }
                };
                match index_expr.get_index_key() {
                    Some(LuaIndexKey::Expr(key_expr)) => {
                        if no_flow && is_declined_index_prefix(&key_expr) {
                            return self.complete_expr(syntax_id, Err(InferFailReason::None));
                        }
                        Step::Task(
                            Task::Expr(key_expr),
                            Some(Continuation::IndexKey {
                                index_expr,
                                prefix_type,
                                pass_flow,
                            }),
                        )
                    }
                    _ => self.evaluate_index_member(index_expr, prefix_type, pass_flow, None),
                }
            }
            Continuation::IndexKey {
                index_expr,
                prefix_type,
                pass_flow,
            } => {
                let syntax_id = index_expr.get_syntax_id();
                let key_type = match result {
                    Ok(ty) => ty,
                    Err(err) => {
                        if self.cache.is_no_flow() {
                            return self.complete_expr(syntax_id, Err(InferFailReason::None));
                        }
                        return self.complete_expr(syntax_id, Err(err));
                    }
                };
                self.evaluate_index_member(index_expr, prefix_type, pass_flow, Some(key_type))
            }
            Continuation::BinaryLeft { binary_expr } => {
                let syntax_id = binary_expr.get_syntax_id();
                let left_type = match result {
                    Ok(ty) => ty,
                    Err(err) => return self.complete_expr(syntax_id, Err(err)),
                };
                let Some((_, right)) = binary_expr.get_exprs() else {
                    return self.complete_expr(syntax_id, Err(InferFailReason::None));
                };
                Step::Task(
                    Task::Expr(right),
                    Some(Continuation::BinaryRight {
                        binary_expr,
                        left_type,
                    }),
                )
            }
            Continuation::BinaryRight {
                binary_expr,
                left_type,
            } => {
                let syntax_id = binary_expr.get_syntax_id();
                let right_type = match result {
                    Ok(ty) => ty,
                    Err(err) => return self.complete_expr(syntax_id, Err(err)),
                };
                let result = infer_binary_expr_result(self.db, binary_expr, left_type, right_type);
                self.complete_expr(syntax_id, result)
            }
            Continuation::UnaryInner { unary_expr } => {
                let syntax_id = unary_expr.get_syntax_id();
                let inner_type = match result {
                    Ok(ty) => ty,
                    Err(err) => return self.complete_expr(syntax_id, Err(err)),
                };
                let result = match unary_expr.get_op_token() {
                    Some(op_token) => {
                        infer_unary_expr_result(self.db, op_token.get_op(), inner_type)
                    }
                    None => Err(InferFailReason::None),
                };
                self.complete_expr(syntax_id, result)
            }
            Continuation::TernaryTrue { ternary_expr } => {
                let syntax_id = ternary_expr.get_syntax_id();
                let true_type = match result {
                    Ok(ty) => ty,
                    Err(err) => return self.complete_expr(syntax_id, Err(err)),
                };
                let Some((_, false_expr)) = ternary_expr.get_true_false_exprs() else {
                    return self.complete_expr(syntax_id, Err(InferFailReason::None));
                };
                Step::Task(
                    Task::Expr(false_expr),
                    Some(Continuation::TernaryFalse {
                        ternary_expr,
                        true_type,
                    }),
                )
            }
            Continuation::TernaryFalse {
                ternary_expr,
                true_type,
            } => {
                let syntax_id = ternary_expr.get_syntax_id();
                let false_type = match result {
                    Ok(ty) => ty,
                    Err(err) => return self.complete_expr(syntax_id, Err(err)),
                };
                let result = TypeOps::Union.apply(self.db, &true_type, &false_type);
                self.complete_expr(syntax_id, Ok(result))
            }
        }
    }
}

/// Consistent with the no_flow semantics of `try_infer_expr_for_index`:
/// after paren unwrapping, a closure is declined as an index prefix/key
fn is_declined_index_prefix(expr: &LuaExpr) -> bool {
    let mut current = expr.clone();
    while let LuaExpr::ParenExpr(paren) = &current {
        match paren.get_expr() {
            Some(inner) => current = inner,
            None => break,
        }
    }
    matches!(current, LuaExpr::ClosureExpr(_))
}

#[cfg(test)]
mod tests {
    use crate::VirtualWorkspace;

    // Deep index chain: the old implementation overflows the native stack at this depth; the explicit task stack no longer recurses
    #[test]
    fn test_deep_index_chain() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@type { x: integer }\nlocal t");

        let mut expr = "t".to_string();
        for _ in 0..1_500 {
            expr.push_str(".x");
        }
        let _ = ws.expr_ty(&expr);
    }

    // Deep call chain: also handled by the explicit task stack
    #[test]
    fn test_deep_call_chain() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@type fun(): any\nlocal f");

        let mut expr = "f".to_string();
        for _ in 0..2_000 {
            expr.push_str("()");
        }
        let _ = ws.expr_ty(&expr);
    }

    // Deep paren chain
    #[test]
    fn test_deep_paren_chain() {
        let mut ws = VirtualWorkspace::new();
        let mut expr = "1".to_string();
        for _ in 0..150 {
            expr.insert(0, '(');
            expr.push(')');
        }
        let ty = ws.expr_ty(&expr);
        assert!(ty.is_integer());
    }

    // Deep binary chain (left-associative tree)
    #[test]
    fn test_deep_binary_chain() {
        let mut ws = VirtualWorkspace::new();
        let mut expr = "1".to_string();
        for _ in 0..1_500 {
            expr.push_str(" + 1");
        }
        let ty = ws.expr_ty(&expr);
        assert!(ty.is_integer());
    }
}
