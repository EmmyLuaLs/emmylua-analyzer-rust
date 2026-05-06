use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaChunk, LuaDocOpType, LuaExpr, LuaIndexKey, LuaIndexMemberExpr,
    LuaSyntaxId, LuaTableExpr, LuaVarExpr,
};
use hashbrown::HashSet;
use std::{rc::Rc, sync::Arc};

use crate::{
    CacheEntry, DbIndex, FlowId, FlowNode, FlowNodeKind, FlowTree, InferFailReason, LuaDeclId,
    LuaInferCache, LuaMemberId, LuaSignatureId, LuaType, TypeOps, check_type_compact,
    semantic::{
        cache::{FlowAssignmentInfo, FlowMode, FlowVarCache},
        infer::{
            InferResult, VarRefId,
            narrow::{
                condition_flow::{
                    ConditionFlowAction, CorrelatedSubquery, ExprTypeContinuation,
                    FieldConditionKind, FieldLiteralSiblingSubquery, InferConditionFlow,
                    PendingConditionNarrow, always_literal_equal,
                    correlated_flow::{
                        PendingCorrelatedCondition, advance_pending_correlated_condition,
                    },
                    get_type_at_condition_flow, resolve_correlated_subquery,
                    resolve_expr_type_continuation,
                },
                get_multi_antecedents, get_single_antecedent,
                get_type_at_cast_flow::cast_type,
                get_var_ref_type, narrow_down_type,
                var_ref_id::get_var_expr_var_ref_id,
            },
            try_infer_expr_no_flow,
        },
        member::find_members,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
// One cached flow query: one ref at one flow node, optionally without replaying
// pending condition narrows.
// Example: "what is `x` at flow 42, with current guards applied?"
struct FlowQuery {
    var_ref_id: VarRefId,
    var_cache_idx: u32,
    flow_id: FlowId,
    mode: FlowMode,
}

impl FlowQuery {
    fn new(cache: &mut LuaInferCache, var_ref_id: &VarRefId, flow_id: FlowId) -> Self {
        Self {
            var_ref_id: var_ref_id.clone(),
            var_cache_idx: get_flow_cache_var_ref_id(cache, var_ref_id),
            flow_id,
            mode: FlowMode::WithConditions,
        }
    }

    fn at_flow(&self, flow_id: FlowId, mode: FlowMode) -> Self {
        Self {
            flow_id,
            mode,
            ..self.clone()
        }
    }
}

#[derive(Debug)]
// Suspended state of one query's straight-line backward walk. We keep
// collecting pending condition narrows until some node produces a final type or
// needs another query first.
// Example: while walking backward through `if x then ... end`, remember that
// `x` must be truthy when the final type is produced.
struct QueryWalk {
    query: FlowQuery,
    antecedent_flow_id: FlowId,
    pending_condition_narrows: Vec<PendingConditionNarrow>,
}

// Explicit engine stack of suspended queries. We push one of these when the current query cannot
// finish until another `FlowQuery` runs first. Each entry stores the suspended `QueryWalk` plus the
// extra data needed to resume after that dependency query finishes. A dependency query is just
// another `FlowQuery` started while resolving the current one.
enum Continuation {
    // Saved branch-merge state while one incoming branch query is in flight.
    // Example: `if cond then x = "a" else x = 1 end` queries each incoming
    // branch, then unions the results here.
    Merge {
        walk: QueryWalk,
        branch_flow_ids: Arc<[FlowId]>,
        next_pending_idx: usize,
        merged_type: LuaType,
    },
    // Resume an assignment once we know the pre-assignment type of the same ref.
    // Example: for `x = expr`, first query `x` just before the assignment,
    // then combine that antecedent type with the expression type here.
    AssignmentAntecedent {
        walk: QueryWalk,
        antecedent_flow_id: FlowId,
        expr_type: LuaType,
        reuse_antecedent_narrowing: bool,
    },
    // Resume structural expression replay after resolving the flow-aware refs
    // it depends on. The replay itself stays no-flow; only this continuation
    // may schedule the dependency queries.
    ExprReplay {
        walk: QueryWalk,
        replay: FlowExprReplay,
        replay_query: FlowReplayQuery,
    },
    // Resume a tag cast after reading the antecedent value that the cast rewrites.
    // Example: `---@cast x Foo` first queries `x` before the cast node, then
    // applies the cast operators here.
    TagCastAntecedent {
        walk: QueryWalk,
        cast_op_types: Vec<LuaDocOpType>,
    },
    // Resume a condition after querying another ref that the condition depends on.
    // Example: `if #xs > 0 then` or `if shape.kind == "circle" then` needs the
    // antecedent type of another ref before this query can narrow.
    ConditionDependency {
        walk: QueryWalk,
        flow_id: FlowId,
        condition_flow: InferConditionFlow,
        subquery: CorrelatedSubquery,
    },
    FieldLiteralSiblingDependency {
        walk: QueryWalk,
        flow_id: FlowId,
        condition_flow: InferConditionFlow,
        subquery: FieldLiteralSiblingSubquery,
    },
    // Resume correlated return-overload narrowing after querying one pending root.
    // Example: `local ok, value = f(); if ok then ... value ... end` may need to
    // query one multi-return search root at a time before it can narrow `value`.
    CorrelatedSearchRoot {
        walk: QueryWalk,
        flow_id: FlowId,
        condition_flow: InferConditionFlow,
        pending_correlated_condition: PendingCorrelatedCondition,
    },
}

enum FlowExprReplay {
    Assignment {
        antecedent_flow_id: FlowId,
        explicit_var_type: Option<LuaType>,
        result_slot: usize,
    },
    DeclInitializer {
        fail_reason: InferFailReason,
    },
    Condition {
        condition_flow_id: FlowId,
        condition_flow: InferConditionFlow,
        resume: ExprTypeContinuation,
    },
    FieldConditionKey {
        condition_flow_id: FlowId,
        condition_flow: InferConditionFlow,
        idx: LuaIndexMemberExpr,
        field_condition_flow: InferConditionFlow,
        kind: FieldConditionKind,
    },
}

// Dependency queries are flow-aware, but the final expression replay is not.
// This owns both phases so replay cannot accidentally re-enter flow.
struct FlowReplayQuery {
    flow_id: FlowId,
    expr: LuaExpr,
    allow_table_exprs: bool,
    dependency_queries: Vec<FlowExprTypeQuery>,
    next_dependency_idx: usize,
    dependency_types: Vec<(LuaSyntaxId, LuaType)>,
}

impl FlowReplayQuery {
    fn new(
        db: &DbIndex,
        tree: Option<&FlowTree>,
        cache: &mut LuaInferCache,
        flow_id: FlowId,
        expr: LuaExpr,
        allow_table_exprs: bool,
    ) -> Self {
        let mut dependency_queries = Vec::new();
        collect_expr_dependency_queries(db, tree, cache, flow_id, &expr, &mut dependency_queries);
        Self {
            flow_id,
            expr,
            allow_table_exprs,
            dependency_queries,
            next_dependency_idx: 0,
            dependency_types: Vec::new(),
        }
    }

    fn next_query(&self) -> Option<&FlowExprTypeQuery> {
        self.dependency_queries.get(self.next_dependency_idx)
    }

    fn accept_result(&mut self, dependency_result: InferResult) -> Result<(), InferFailReason> {
        let dependency_query = self
            .dependency_queries
            .get(self.next_dependency_idx)
            .ok_or(InferFailReason::None)?;

        match dependency_result {
            Ok(mut expr_type) => {
                if let Some(literal_shape_type) = &dependency_query.literal_shape_type {
                    expr_type = literal_equivalent_type(literal_shape_type, &expr_type)
                        .unwrap_or(expr_type);
                }

                self.dependency_types
                    .push((dependency_query.syntax_id, expr_type));
            }
            Err(
                InferFailReason::None
                | InferFailReason::RecursiveInfer
                | InferFailReason::FieldNotFound,
            ) => {}
            Err(err) => return Err(err),
        }

        self.next_dependency_idx += 1;
        Ok(())
    }

    fn replay_type(
        self,
        db: &DbIndex,
        cache: &mut LuaInferCache,
    ) -> Result<Option<LuaType>, InferFailReason> {
        let Self {
            expr,
            allow_table_exprs,
            dependency_types,
            ..
        } = self;
        replay_expr_no_flow(db, cache, expr, &dependency_types, allow_table_exprs)
    }
}

// The replay overlay should preserve declared doc literals when a flow query
// proves the same runtime literal value for an index expression.
fn literal_equivalent_type(source_type: &LuaType, target_type: &LuaType) -> Option<LuaType> {
    match source_type {
        LuaType::Union(union) => {
            let matches = union
                .into_vec()
                .into_iter()
                .filter(|candidate| always_literal_equal(candidate, target_type))
                .collect::<Vec<_>>();
            (!matches.is_empty()).then(|| LuaType::from_vec(matches))
        }
        _ if always_literal_equal(source_type, target_type) => Some(source_type.clone()),
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct FlowExprTypeQuery {
    var_ref_id: VarRefId,
    flow_id: FlowId,
    syntax_id: LuaSyntaxId,
    literal_shape_type: Option<LuaType>,
}

fn collect_expr_dependency_queries(
    db: &DbIndex,
    tree: Option<&FlowTree>,
    cache: &mut LuaInferCache,
    fallback_flow_id: FlowId,
    expr: &LuaExpr,
    dependency_queries: &mut Vec<FlowExprTypeQuery>,
) {
    if matches!(expr, LuaExpr::ClosureExpr(_)) {
        return;
    }

    if matches!(expr, LuaExpr::NameExpr(_)) {
        let flow_id = tree
            .and_then(|tree| tree.get_flow_id(expr.get_syntax_id()))
            .unwrap_or(fallback_flow_id);
        if let Some(var_ref_id) = get_var_expr_var_ref_id(db, cache, expr.clone()) {
            dependency_queries.push(FlowExprTypeQuery {
                var_ref_id,
                flow_id,
                syntax_id: expr.get_syntax_id(),
                literal_shape_type: None,
            });
        }
        return;
    }

    if let LuaExpr::IndexExpr(index_expr) = expr {
        if let Some(prefix_expr) = index_expr.get_prefix_expr() {
            collect_expr_dependency_queries(
                db,
                tree,
                cache,
                fallback_flow_id,
                &prefix_expr,
                dependency_queries,
            );
        }
        if let Some(LuaIndexKey::Expr(expr)) = index_expr.get_index_key() {
            collect_expr_dependency_queries(
                db,
                tree,
                cache,
                fallback_flow_id,
                &expr,
                dependency_queries,
            );
        }
        // Keep prefix/key deps first. The direct IndexRef repairs guards on
        // the member itself, and should use the replay point's flow.
        if let Some(var_ref_id) = get_var_expr_var_ref_id(db, cache, expr.clone()) {
            let literal_shape_type = try_infer_expr_no_flow(db, cache, expr.clone())
                .ok()
                .flatten();
            dependency_queries.push(FlowExprTypeQuery {
                var_ref_id,
                flow_id: fallback_flow_id,
                syntax_id: expr.get_syntax_id(),
                literal_shape_type,
            });
        }
        return;
    }

    for child_expr in expr.children::<LuaExpr>() {
        collect_expr_dependency_queries(
            db,
            tree,
            cache,
            fallback_flow_id,
            &child_expr,
            dependency_queries,
        );
    }
}

// The top-loop scheduler decision.
// `StartQuery` begins one query, optionally saving the current query first.
// `ContinueWalk` keeps scanning backward through the current query.
// `ResumeNext(result)` pops one suspended query from `stack` and resumes it
// with the result of the dependency query that just finished.
enum SchedulerStep {
    // Start or reuse one `(var_ref, flow_id, mode)` query.
    // If `continuation` is present, save that suspended query first so this
    // dependency result can resume it later.
    // Example: before resuming `x = rhs`, save the assignment continuation and
    // then query `x` at the antecedent flow id.
    StartQuery {
        query: FlowQuery,
        continuation: Option<Continuation>,
    },
    // Continue the straight-line backward walk for the current query.
    // Example: after replaying a pending guard, keep scanning toward the next
    // antecedent node.
    ContinueWalk(QueryWalk),
    // Pop one suspended query from `stack` and resume it with this dependency
    // query result.
    // Example: after querying `shape.kind`, continue narrowing
    // `if shape.kind == "circle" then`.
    ResumeNext(InferResult),
}

// Single owner of flow evaluation. Only this engine is allowed to schedule
// follow-up queries, which keeps the flow path iterative.
struct FlowTypeEngine<'a> {
    db: &'a DbIndex,
    tree: &'a FlowTree,
    cache: &'a mut LuaInferCache,
    root: &'a LuaChunk,
}

impl<'a> FlowTypeEngine<'a> {
    fn run(&mut self, var_ref_id: &VarRefId, flow_id: FlowId) -> InferResult {
        let mut stack = Vec::new();
        let mut step = SchedulerStep::StartQuery {
            query: FlowQuery::new(self.cache, var_ref_id, flow_id),
            continuation: None,
        };

        loop {
            step = match step {
                SchedulerStep::StartQuery {
                    query,
                    continuation,
                } => {
                    if let Some(continuation) = continuation {
                        stack.push(continuation);
                    }
                    self.start_query(query)
                }
                SchedulerStep::ContinueWalk(walk) => self.evaluate_walk(walk),
                SchedulerStep::ResumeNext(query_result) => match stack.pop() {
                    Some(Continuation::Merge {
                        walk,
                        branch_flow_ids,
                        next_pending_idx,
                        merged_type,
                    }) => self.resume_merge(
                        walk,
                        branch_flow_ids,
                        next_pending_idx,
                        merged_type,
                        query_result,
                    ),
                    Some(Continuation::AssignmentAntecedent {
                        walk,
                        antecedent_flow_id,
                        expr_type,
                        reuse_antecedent_narrowing,
                    }) => self.resume_assignment_antecedent(
                        walk,
                        antecedent_flow_id,
                        expr_type,
                        reuse_antecedent_narrowing,
                        query_result,
                    ),
                    Some(Continuation::ExprReplay {
                        walk,
                        replay,
                        replay_query,
                    }) => self.resume_expr_replay(walk, replay, replay_query, query_result),
                    Some(Continuation::TagCastAntecedent {
                        walk,
                        cast_op_types,
                    }) => self.resume_tag_cast_antecedent(walk, cast_op_types, query_result),
                    Some(Continuation::ConditionDependency {
                        walk,
                        flow_id,
                        condition_flow,
                        subquery,
                    }) => self.resume_condition_subquery(
                        walk,
                        flow_id,
                        condition_flow,
                        subquery,
                        query_result,
                    ),
                    Some(Continuation::FieldLiteralSiblingDependency {
                        walk,
                        flow_id,
                        condition_flow,
                        subquery,
                    }) => self.resume_field_literal_sibling_subquery(
                        walk,
                        flow_id,
                        condition_flow,
                        subquery,
                        query_result,
                    ),
                    Some(Continuation::CorrelatedSearchRoot {
                        walk,
                        flow_id,
                        condition_flow,
                        pending_correlated_condition,
                    }) => self.apply_condition_action(
                        walk,
                        flow_id,
                        condition_flow,
                        advance_pending_correlated_condition(
                            self.db,
                            pending_correlated_condition,
                            query_result,
                        ),
                    ),
                    // No suspended query is waiting on this result, so it is the
                    // final answer for the original `run(...)` request.
                    None => break query_result,
                },
            }
            .unwrap_or_else(|err| SchedulerStep::ResumeNext(Err(err)));
        }
    }

    // Begin one flow query. If this `(var_ref, flow_id, mode)` pair is already
    // resolved or already in progress, reuse that state; otherwise start the
    // backward walk that computes it.
    fn start_query(&mut self, query: FlowQuery) -> Result<SchedulerStep, InferFailReason> {
        let type_cache_key = (query.flow_id, query.mode);
        if let Some(cache_entry) = self
            .cache
            .flow_var_caches
            .get(query.var_cache_idx as usize)
            .and_then(|var_cache| var_cache.type_cache.get(&type_cache_key))
        {
            Ok(SchedulerStep::ResumeNext(match cache_entry {
                CacheEntry::Cache(narrow_type) => Ok(narrow_type.clone()),
                CacheEntry::Ready => Err(InferFailReason::RecursiveInfer),
            }))
        } else {
            get_flow_var_cache(self.cache, query.var_cache_idx)
                .type_cache
                .insert(type_cache_key, CacheEntry::Ready);
            self.evaluate_walk(QueryWalk {
                antecedent_flow_id: query.flow_id,
                query: query.clone(),
                pending_condition_narrows: Vec::new(),
            })
            .or_else(|err| self.fail_query(&query, err))
        }
    }

    // Consume one finished branch result, then either schedule the next branch
    // query or finish the merged result.
    fn resume_merge(
        &mut self,
        walk: QueryWalk,
        branch_flow_ids: Arc<[FlowId]>,
        next_pending_idx: usize,
        merged_type: LuaType,
        branch_result: InferResult,
    ) -> Result<SchedulerStep, InferFailReason> {
        let branch_type = match branch_result {
            Ok(branch_type) => branch_type,
            Err(err) => return self.fail_query(&walk.query, err),
        };

        let merged_type = TypeOps::Union.apply(self.db, &merged_type, &branch_type);
        if next_pending_idx == 0 {
            return Ok(self.finish_walk(walk, merged_type));
        }

        // Branches are resumed from the end because the initial merge setup
        // schedules the last incoming branch first.
        let branch_idx = next_pending_idx - 1;
        Ok(SchedulerStep::StartQuery {
            query: walk
                .query
                .at_flow(branch_flow_ids[branch_idx], walk.query.mode),
            continuation: Some(Continuation::Merge {
                walk,
                branch_flow_ids,
                next_pending_idx: branch_idx,
                merged_type,
            }),
        })
    }

    // Finish one assignment dependency query by reading the pre-assignment type
    // of the same ref, optionally retrying without condition narrows, then
    // combining that antecedent type with the expression type to finish the suspended
    // query.
    fn resume_assignment_antecedent(
        &mut self,
        walk: QueryWalk,
        antecedent_flow_id: FlowId,
        expr_type: LuaType,
        reuse_antecedent_narrowing: bool,
        antecedent_result: InferResult,
    ) -> Result<SchedulerStep, InferFailReason> {
        let antecedent_type = match antecedent_result {
            Ok(antecedent_type) => antecedent_type,
            Err(err) => return self.fail_query(&walk.query, err),
        };

        if reuse_antecedent_narrowing
            && !can_reuse_narrowed_assignment_source(self.db, &antecedent_type, &expr_type)
        {
            let next_query = walk
                .query
                .at_flow(antecedent_flow_id, FlowMode::WithoutConditions);
            return Ok(SchedulerStep::StartQuery {
                query: next_query,
                continuation: Some(Continuation::AssignmentAntecedent {
                    walk,
                    antecedent_flow_id,
                    expr_type,
                    reuse_antecedent_narrowing: false,
                }),
            });
        }

        let result_type = finish_assignment_result(
            self.db,
            self.cache,
            &antecedent_type,
            &expr_type,
            &walk.query.var_ref_id,
            reuse_antecedent_narrowing,
            None,
        );
        Ok(self.finish_walk(walk, result_type))
    }

    fn start_expr_replay(
        &mut self,
        walk: QueryWalk,
        replay: FlowExprReplay,
        replay_query: FlowReplayQuery,
    ) -> Result<SchedulerStep, InferFailReason> {
        let next_query = replay_query
            .next_query()
            .map(|query| (query.var_ref_id.clone(), query.flow_id));
        let Some((var_ref_id, flow_id)) = next_query else {
            return self.finish_expr_replay(walk, replay, replay_query);
        };

        Ok(SchedulerStep::StartQuery {
            query: FlowQuery::new(self.cache, &var_ref_id, flow_id),
            continuation: Some(Continuation::ExprReplay {
                walk,
                replay,
                replay_query,
            }),
        })
    }

    fn resume_expr_replay(
        &mut self,
        walk: QueryWalk,
        replay: FlowExprReplay,
        mut replay_query: FlowReplayQuery,
        query_result: InferResult,
    ) -> Result<SchedulerStep, InferFailReason> {
        match replay_query.accept_result(query_result) {
            Ok(()) => {}
            Err(err) => return self.finish_expr_replay_error(walk, replay, err),
        }

        self.start_expr_replay(walk, replay, replay_query)
    }

    fn finish_expr_replay(
        &mut self,
        walk: QueryWalk,
        replay: FlowExprReplay,
        replay_query: FlowReplayQuery,
    ) -> Result<SchedulerStep, InferFailReason> {
        match replay {
            FlowExprReplay::Assignment {
                antecedent_flow_id,
                explicit_var_type,
                result_slot,
            } => self.finish_assignment_expr(
                walk,
                antecedent_flow_id,
                explicit_var_type,
                result_slot,
                replay_query,
            ),
            FlowExprReplay::DeclInitializer { fail_reason } => {
                let query = walk.query.clone();
                let expr_type = match replay_query.replay_type(self.db, self.cache) {
                    Ok(Some(expr_type)) => expr_type,
                    Ok(None) => return self.fail_query(&query, fail_reason),
                    Err(err) => return self.fail_query(&query, err),
                };

                let Some(init_type) = expr_type.get_result_slot_type(0) else {
                    return self.fail_query(&query, fail_reason);
                };

                Ok(self.finish_walk(walk, init_type))
            }
            FlowExprReplay::Condition {
                condition_flow_id,
                condition_flow,
                resume,
            } => {
                let expr_flow_id = replay_query.flow_id;
                let query = walk.query.clone();
                let var_ref_id = query.var_ref_id.clone();
                let action_result = match replay_query.replay_type(self.db, self.cache) {
                    Ok(Some(expr_type)) => resolve_expr_type_continuation(
                        self.db,
                        self.cache,
                        &var_ref_id,
                        expr_flow_id,
                        resume,
                        expr_type,
                    ),
                    Ok(None) => Ok(ConditionFlowAction::Continue),
                    Err(err) => Err(err),
                };
                let action = match action_result {
                    Ok(action) => action,
                    Err(err) => {
                        return self.fail_condition_query(
                            &query,
                            condition_flow_id,
                            condition_flow,
                            err,
                        );
                    }
                };
                self.apply_condition_action(walk, condition_flow_id, condition_flow, action)
                    .or_else(|err| {
                        self.fail_condition_query(&query, condition_flow_id, condition_flow, err)
                    })
            }
            FlowExprReplay::FieldConditionKey {
                condition_flow_id,
                condition_flow,
                idx,
                field_condition_flow,
                kind,
            } => {
                let query = walk.query.clone();
                let key_type = match replay_query.replay_type(self.db, self.cache) {
                    Ok(key_type) => key_type,
                    Err(err) => {
                        return self.fail_condition_query(
                            &query,
                            condition_flow_id,
                            condition_flow,
                            err,
                        );
                    }
                };

                Ok(self.push_pending_condition(
                    walk,
                    condition_flow_id,
                    condition_flow,
                    PendingConditionNarrow::Field {
                        idx,
                        key_type,
                        condition_flow: field_condition_flow,
                        kind,
                    },
                ))
            }
        }
    }

    fn finish_expr_replay_error(
        &mut self,
        walk: QueryWalk,
        replay: FlowExprReplay,
        err: InferFailReason,
    ) -> Result<SchedulerStep, InferFailReason> {
        match replay {
            FlowExprReplay::Assignment {
                antecedent_flow_id,
                explicit_var_type,
                ..
            } => {
                self.finish_assignment_expr_error(walk, antecedent_flow_id, explicit_var_type, err)
            }
            FlowExprReplay::DeclInitializer { .. } => self.fail_query(&walk.query, err),
            FlowExprReplay::Condition {
                condition_flow_id,
                condition_flow,
                ..
            }
            | FlowExprReplay::FieldConditionKey {
                condition_flow_id,
                condition_flow,
                ..
            } => self.fail_condition_query(&walk.query, condition_flow_id, condition_flow, err),
        }
    }

    // Finish one tag-cast dependency query by reading the antecedent type and
    // replaying the cast operators in source order, then finish the suspended
    // query with the cast result.
    fn resume_tag_cast_antecedent(
        &mut self,
        walk: QueryWalk,
        cast_op_types: Vec<LuaDocOpType>,
        antecedent_result: InferResult,
    ) -> Result<SchedulerStep, InferFailReason> {
        let mut cast_input_type = match antecedent_result {
            Ok(resolved_type) => resolved_type,
            // `---@cast` is an explicit assertion, so unresolved source types
            // should still be narrowed by applying the cast from `unknown`.
            Err(_) => LuaType::Unknown,
        };
        for cast_op_type in cast_op_types {
            cast_input_type = match cast_type(
                self.db,
                self.cache.get_file_id(),
                cast_op_type,
                cast_input_type,
                InferConditionFlow::TrueCondition,
            ) {
                Ok(typ) => typ,
                Err(err) => return self.fail_query(&walk.query, err),
            };
        }

        Ok(self.finish_walk(walk, cast_input_type))
    }

    // Finish one condition dependency query, turn its result into a
    // `ConditionFlowAction`, and then feed that action back through the normal
    // condition path. If the dependency query fails, clear the condition cache
    // entry so a later lookup can retry instead of observing a stuck `Ready`.
    fn resume_condition_subquery(
        &mut self,
        walk: QueryWalk,
        flow_id: FlowId,
        condition_flow: InferConditionFlow,
        subquery: CorrelatedSubquery,
        antecedent_result: InferResult,
    ) -> Result<SchedulerStep, InferFailReason> {
        let query = walk.query.clone();
        let result = (|| {
            let flow_node = self
                .tree
                .get_flow_node(flow_id)
                .ok_or(InferFailReason::None)?;
            let action = resolve_correlated_subquery(
                self.db,
                self.tree,
                self.cache,
                self.root,
                &query.var_ref_id,
                flow_node,
                subquery,
                antecedent_result,
            )?;
            self.apply_condition_action(walk, flow_id, condition_flow, action)
        })();

        result.or_else(|err| self.fail_condition_query(&query, flow_id, condition_flow, err))
    }

    fn start_condition_subquery(
        &mut self,
        walk: QueryWalk,
        flow_id: FlowId,
        condition_flow: InferConditionFlow,
        subquery: CorrelatedSubquery,
    ) -> SchedulerStep {
        let (subquery_var_ref_id, subquery_flow_id) = subquery.next_flow_query();
        let query = FlowQuery::new(self.cache, subquery_var_ref_id, subquery_flow_id);
        SchedulerStep::StartQuery {
            query,
            continuation: Some(Continuation::ConditionDependency {
                walk,
                flow_id,
                condition_flow,
                subquery,
            }),
        }
    }

    fn resume_field_literal_sibling_subquery(
        &mut self,
        walk: QueryWalk,
        flow_id: FlowId,
        condition_flow: InferConditionFlow,
        subquery: FieldLiteralSiblingSubquery,
        antecedent_result: InferResult,
    ) -> Result<SchedulerStep, InferFailReason> {
        let query = walk.query.clone();
        let result = (|| {
            let antecedent_type = antecedent_result?;
            let action = subquery.resolve(self.db, self.cache, antecedent_type)?;
            self.apply_condition_action(walk, flow_id, condition_flow, action)
        })();

        result.or_else(|err| self.fail_condition_query(&query, flow_id, condition_flow, err))
    }

    fn start_field_literal_sibling_subquery(
        &mut self,
        walk: QueryWalk,
        flow_id: FlowId,
        condition_flow: InferConditionFlow,
        subquery: FieldLiteralSiblingSubquery,
    ) -> SchedulerStep {
        let (subquery_var_ref_id, subquery_flow_id) = subquery.next_flow_query();
        let query = FlowQuery::new(self.cache, subquery_var_ref_id, subquery_flow_id);
        SchedulerStep::StartQuery {
            query,
            continuation: Some(Continuation::FieldLiteralSiblingDependency {
                walk,
                flow_id,
                condition_flow,
                subquery,
            }),
        }
    }

    fn push_pending_condition(
        &mut self,
        mut walk: QueryWalk,
        flow_id: FlowId,
        condition_flow: InferConditionFlow,
        pending_condition_narrow: PendingConditionNarrow,
    ) -> SchedulerStep {
        get_flow_var_cache(self.cache, walk.query.var_cache_idx)
            .condition_cache
            .insert(
                (flow_id, condition_flow),
                CacheEntry::Cache(ConditionFlowAction::Pending(
                    pending_condition_narrow.clone(),
                )),
            );
        walk.pending_condition_narrows
            .push(pending_condition_narrow);
        SchedulerStep::ContinueWalk(walk)
    }

    fn start_pending_condition(
        &mut self,
        walk: QueryWalk,
        flow_id: FlowId,
        condition_flow: InferConditionFlow,
        pending_condition_narrow: PendingConditionNarrow,
    ) -> Result<SchedulerStep, InferFailReason> {
        let (idx, field_condition_flow, kind) = match pending_condition_narrow {
            PendingConditionNarrow::Field {
                idx,
                key_type: None,
                condition_flow: field_condition_flow,
                kind,
            } => (idx, field_condition_flow, kind),
            pending_condition_narrow => {
                return Ok(self.push_pending_condition(
                    walk,
                    flow_id,
                    condition_flow,
                    pending_condition_narrow,
                ));
            }
        };
        let antecedent_flow_id = walk.antecedent_flow_id;
        let Some(LuaIndexKey::Expr(expr)) = idx.get_index_key() else {
            return Ok(self.push_pending_condition(
                walk,
                flow_id,
                condition_flow,
                PendingConditionNarrow::Field {
                    idx,
                    key_type: None,
                    condition_flow: field_condition_flow,
                    kind,
                },
            ));
        };
        let replay_query =
            FlowReplayQuery::new(self.db, None, self.cache, antecedent_flow_id, expr, false);

        self.start_expr_replay(
            walk,
            FlowExprReplay::FieldConditionKey {
                condition_flow_id: flow_id,
                condition_flow,
                idx,
                field_condition_flow,
                kind,
            },
            replay_query,
        )
    }

    fn step_assignment(
        &mut self,
        mut walk: QueryWalk,
        flow_node: &FlowNode,
        assign_ptr: &emmylua_parser::LuaAstPtr<LuaAssignStat>,
    ) -> Result<SchedulerStep, InferFailReason> {
        let var_ref_id = walk.query.var_ref_id.clone();
        let assignment_info =
            get_flow_assignment_info(self.db, self.cache, self.root, flow_node.id, assign_ptr)?;
        let antecedent_flow_id = get_single_antecedent(flow_node)?;

        let Some(i) = assignment_info
            .var_ref_ids
            .iter()
            .position(|maybe_ref_id| maybe_ref_id.as_ref() == Some(&var_ref_id))
        else {
            walk.antecedent_flow_id = antecedent_flow_id;
            return Ok(SchedulerStep::ContinueWalk(walk));
        };

        let var_id = match &assignment_info.vars[i] {
            LuaVarExpr::NameExpr(name_expr) => {
                Some(LuaDeclId::new(self.cache.get_file_id(), name_expr.get_position()).into())
            }
            LuaVarExpr::IndexExpr(index_expr) => {
                Some(LuaMemberId::new(index_expr.get_syntax_id(), self.cache.get_file_id()).into())
            }
        };
        let explicit_var_type = var_id
            .and_then(|id| self.db.get_type_index().get_type_cache(&id))
            .filter(|tc| tc.is_doc())
            .map(|tc| tc.as_type().clone());

        if let Some(last_expr_idx) = assignment_info.exprs.len().checked_sub(1) {
            let expr_idx = i.min(last_expr_idx);
            let result_slot = i.saturating_sub(last_expr_idx);
            let expr = assignment_info.exprs[expr_idx].clone();
            let replay_query = FlowReplayQuery::new(
                self.db,
                Some(self.tree),
                self.cache,
                antecedent_flow_id,
                expr,
                true,
            );
            return self.start_expr_replay(
                walk,
                FlowExprReplay::Assignment {
                    antecedent_flow_id,
                    explicit_var_type,
                    result_slot,
                },
                replay_query,
            );
        }

        self.finish_assignment_expr_type(walk, antecedent_flow_id, explicit_var_type, LuaType::Nil)
    }

    fn finish_assignment_expr(
        &mut self,
        walk: QueryWalk,
        antecedent_flow_id: FlowId,
        explicit_var_type: Option<LuaType>,
        result_slot: usize,
        replay_query: FlowReplayQuery,
    ) -> Result<SchedulerStep, InferFailReason> {
        let expr_type = match replay_query.replay_type(self.db, self.cache) {
            Ok(Some(expr_type)) => expr_type
                .get_result_slot_type(result_slot)
                .unwrap_or(LuaType::Nil),
            Ok(None) => LuaType::Unknown,
            Err(err) => {
                return self.finish_assignment_expr_error(
                    walk,
                    antecedent_flow_id,
                    explicit_var_type,
                    err,
                );
            }
        };

        self.finish_assignment_expr_type(walk, antecedent_flow_id, explicit_var_type, expr_type)
    }

    fn finish_assignment_expr_type(
        &mut self,
        walk: QueryWalk,
        antecedent_flow_id: FlowId,
        explicit_var_type: Option<LuaType>,
        expr_type: LuaType,
    ) -> Result<SchedulerStep, InferFailReason> {
        let var_ref_id = walk.query.var_ref_id.clone();

        if let Some(explicit_var_type) = explicit_var_type {
            let result_type = finish_assignment_result(
                self.db,
                self.cache,
                &explicit_var_type,
                &expr_type,
                &var_ref_id,
                true,
                Some(explicit_var_type.clone()),
            );
            return Ok(self.finish_walk(walk, result_type));
        }

        let reuse_antecedent_narrowing = preserves_assignment_expr_type(&expr_type);
        let mode = if reuse_antecedent_narrowing {
            FlowMode::WithConditions
        } else {
            FlowMode::WithoutConditions
        };
        let subquery = walk.query.at_flow(antecedent_flow_id, mode);
        Ok(SchedulerStep::StartQuery {
            query: subquery,
            continuation: Some(Continuation::AssignmentAntecedent {
                walk,
                antecedent_flow_id,
                expr_type,
                reuse_antecedent_narrowing,
            }),
        })
    }

    fn finish_assignment_expr_error(
        &mut self,
        mut walk: QueryWalk,
        antecedent_flow_id: FlowId,
        explicit_var_type: Option<LuaType>,
        err: InferFailReason,
    ) -> Result<SchedulerStep, InferFailReason> {
        if let Some(explicit_var_type) = explicit_var_type {
            return Ok(self.finish_walk(walk, explicit_var_type));
        }

        let var_ref_id = walk.query.var_ref_id.clone();
        if matches!(var_ref_id, VarRefId::IndexRef(_, _))
            && let Ok(origin_type) = get_var_ref_type(self.db, self.cache, &var_ref_id)
        {
            let non_nil_origin = TypeOps::Remove.apply(self.db, &origin_type, &LuaType::Nil);
            return Ok(self.finish_walk(
                walk,
                if non_nil_origin.is_never() {
                    origin_type
                } else {
                    non_nil_origin
                },
            ));
        }

        if matches!(err, InferFailReason::FieldNotFound | InferFailReason::None) {
            return Ok(self.finish_walk(walk, LuaType::Nil));
        }

        walk.antecedent_flow_id = antecedent_flow_id;
        Ok(SchedulerStep::ContinueWalk(walk))
    }

    fn step_condition(
        &mut self,
        mut walk: QueryWalk,
        flow_node: &FlowNode,
        condition_ptr: &emmylua_parser::LuaAstPtr<LuaExpr>,
        condition_flow: InferConditionFlow,
    ) -> Result<SchedulerStep, InferFailReason> {
        let antecedent_flow_id = get_single_antecedent(flow_node)?;
        if !walk.query.mode.uses_conditions() {
            walk.antecedent_flow_id = antecedent_flow_id;
            return Ok(SchedulerStep::ContinueWalk(walk));
        }

        walk.antecedent_flow_id = antecedent_flow_id;
        let q = &walk.query;
        let var_ref_id = &q.var_ref_id;

        let cache_id = q.var_cache_idx;
        let flow_id = flow_node.id;
        let cache_key = (flow_id, condition_flow);
        let mut cached_action = false;
        let action = match self
            .cache
            .flow_var_caches
            .get(cache_id as usize)
            .and_then(|var_cache| var_cache.condition_cache.get(&cache_key))
        {
            Some(CacheEntry::Cache(action)) => {
                cached_action = true;
                action.clone()
            }
            Some(CacheEntry::Ready) => {
                return self.fail_query(q, InferFailReason::RecursiveInfer);
            }
            None => {
                let condition = condition_ptr
                    .to_node(self.root)
                    .ok_or(InferFailReason::None)?;
                get_flow_var_cache(self.cache, cache_id)
                    .condition_cache
                    .insert(cache_key, CacheEntry::Ready);
                match get_type_at_condition_flow(
                    self.db,
                    self.tree,
                    self.cache,
                    self.root,
                    var_ref_id,
                    flow_node,
                    condition,
                    condition_flow,
                ) {
                    Ok(action) => action,
                    Err(err) => {
                        return self.fail_condition_query(q, flow_id, condition_flow, err);
                    }
                }
            }
        };

        if cached_action {
            return match action {
                ConditionFlowAction::Continue => Ok(SchedulerStep::ContinueWalk(walk)),
                ConditionFlowAction::Result(result_type) => Ok(self.finish_walk(walk, result_type)),
                ConditionFlowAction::Pending(pending_condition_narrow) => {
                    let mut walk = walk;
                    walk.pending_condition_narrows
                        .push(pending_condition_narrow);
                    Ok(SchedulerStep::ContinueWalk(walk))
                }
                action => self.apply_condition_action(walk, flow_id, condition_flow, action),
            };
        }

        self.apply_condition_action(walk, flow_id, condition_flow, action)
    }

    fn step_tag_cast(
        &mut self,
        mut walk: QueryWalk,
        flow_node: &FlowNode,
        cast_ast_ptr: &emmylua_parser::LuaAstPtr<emmylua_parser::LuaDocTagCast>,
    ) -> Result<SchedulerStep, InferFailReason> {
        let tag_cast = cast_ast_ptr
            .to_node(self.root)
            .ok_or(InferFailReason::None)?;
        let var_ref_id = &walk.query.var_ref_id;
        if let Some(key_expr) = tag_cast.get_key_expr() {
            let Some(key_ref_id) = get_var_expr_var_ref_id(self.db, self.cache, key_expr) else {
                walk.antecedent_flow_id = get_single_antecedent(flow_node)?;
                return Ok(SchedulerStep::ContinueWalk(walk));
            };
            if key_ref_id != *var_ref_id {
                walk.antecedent_flow_id = get_single_antecedent(flow_node)?;
                return Ok(SchedulerStep::ContinueWalk(walk));
            }
        }

        let cast_op_types = tag_cast.get_op_types().collect::<Vec<_>>();
        if cast_op_types.is_empty() {
            walk.antecedent_flow_id = get_single_antecedent(flow_node)?;
            return Ok(SchedulerStep::ContinueWalk(walk));
        }

        let antecedent_flow_id = get_single_antecedent(flow_node)?;
        let subquery = walk
            .query
            .at_flow(antecedent_flow_id, FlowMode::WithConditions);
        Ok(SchedulerStep::StartQuery {
            query: subquery,
            continuation: Some(Continuation::TagCastAntecedent {
                walk,
                cast_op_types,
            }),
        })
    }

    // Walk one query backward through straight-line antecedents until it either
    // produces a final type, needs another query first, or reaches a saved
    // continuation point like a branch merge.
    fn evaluate_walk(&mut self, mut walk: QueryWalk) -> Result<SchedulerStep, InferFailReason> {
        loop {
            let flow_node = self
                .tree
                .get_flow_node(walk.antecedent_flow_id)
                .ok_or(InferFailReason::None)?;

            match &flow_node.kind {
                FlowNodeKind::Start | FlowNodeKind::Unreachable => {
                    let result_type =
                        get_var_ref_type(self.db, self.cache, &walk.query.var_ref_id)?;
                    return Ok(self.finish_walk(walk, result_type));
                }
                FlowNodeKind::LoopLabel
                | FlowNodeKind::Break
                | FlowNodeKind::Return
                | FlowNodeKind::ForIStat(_) => {
                    walk.antecedent_flow_id = get_single_antecedent(flow_node)?;
                }
                FlowNodeKind::BranchLabel | FlowNodeKind::NamedLabel(_) => {
                    let branch_flow_ids = if matches!(&flow_node.kind, FlowNodeKind::BranchLabel) {
                        get_branch_label_flow_ids(self.tree, self.cache, flow_node)?
                    } else {
                        Arc::<[FlowId]>::from(get_multi_antecedents(self.tree, flow_node)?)
                    };
                    let Some(next_pending_idx) = branch_flow_ids.len().checked_sub(1) else {
                        return Ok(self.finish_walk(walk, LuaType::Never));
                    };
                    let q = &walk.query;
                    let next_query = q.at_flow(branch_flow_ids[next_pending_idx], q.mode);
                    return Ok(SchedulerStep::StartQuery {
                        query: next_query,
                        continuation: Some(Continuation::Merge {
                            walk,
                            branch_flow_ids,
                            next_pending_idx,
                            merged_type: LuaType::Never,
                        }),
                    });
                }
                FlowNodeKind::DeclPosition(position) => {
                    let var_ref_id = &walk.query.var_ref_id;
                    if *position <= var_ref_id.get_position() {
                        match get_var_ref_type(self.db, self.cache, var_ref_id) {
                            Ok(var_type) => {
                                return Ok(self.finish_walk(walk, var_type));
                            }
                            Err(err) => {
                                let Some(decl_id) = var_ref_id.get_decl_id_ref() else {
                                    return self.fail_query(&walk.query, err);
                                };
                                let decl = self
                                    .db
                                    .get_decl_index()
                                    .get_decl(&decl_id)
                                    .ok_or(InferFailReason::None)?;
                                if let Some(value_syntax_id) = decl.get_value_syntax_id()
                                    && let Some(node) =
                                        value_syntax_id.to_node_from_root(self.root.syntax())
                                    && let Some(expr) = LuaExpr::cast(node)
                                {
                                    let expr_flow_id = self
                                        .tree
                                        .get_flow_id(expr.get_syntax_id())
                                        .unwrap_or(walk.antecedent_flow_id);
                                    let replay_query = FlowReplayQuery::new(
                                        self.db,
                                        Some(self.tree),
                                        self.cache,
                                        expr_flow_id,
                                        expr,
                                        false,
                                    );
                                    return self.start_expr_replay(
                                        walk,
                                        FlowExprReplay::DeclInitializer { fail_reason: err },
                                        replay_query,
                                    );
                                }

                                return self.fail_query(&walk.query, err);
                            }
                        }
                    } else {
                        walk.antecedent_flow_id = get_single_antecedent(flow_node)?;
                    }
                }
                FlowNodeKind::Assignment(assign_ptr) => {
                    match self.step_assignment(walk, flow_node, assign_ptr)? {
                        SchedulerStep::ContinueWalk(next_walk) => walk = next_walk,
                        step => return Ok(step),
                    }
                }
                FlowNodeKind::ImplFunc(func_ptr) => {
                    let func_stat = func_ptr.to_node(self.root).ok_or(InferFailReason::None)?;
                    let Some(func_name) = func_stat.get_func_name() else {
                        walk.antecedent_flow_id = get_single_antecedent(flow_node)?;
                        continue;
                    };

                    let Some(ref_id) =
                        get_var_expr_var_ref_id(self.db, self.cache, func_name.to_expr())
                    else {
                        walk.antecedent_flow_id = get_single_antecedent(flow_node)?;
                        continue;
                    };

                    if ref_id == walk.query.var_ref_id {
                        let Some(closure) = func_stat.get_closure() else {
                            return self.fail_query(&walk.query, InferFailReason::None);
                        };

                        return Ok(self.finish_walk(
                            walk,
                            LuaType::Signature(LuaSignatureId::from_closure(
                                self.cache.get_file_id(),
                                &closure,
                            )),
                        ));
                    } else {
                        walk.antecedent_flow_id = get_single_antecedent(flow_node)?;
                    }
                }
                FlowNodeKind::TrueCondition(condition_ptr)
                | FlowNodeKind::FalseCondition(condition_ptr) => {
                    let condition_flow =
                        if matches!(&flow_node.kind, FlowNodeKind::TrueCondition(_)) {
                            InferConditionFlow::TrueCondition
                        } else {
                            InferConditionFlow::FalseCondition
                        };
                    match self.step_condition(walk, flow_node, condition_ptr, condition_flow)? {
                        SchedulerStep::ContinueWalk(next_walk) => walk = next_walk,
                        step => return Ok(step),
                    }
                }
                FlowNodeKind::TagCast(cast_ast_ptr) => {
                    match self.step_tag_cast(walk, flow_node, cast_ast_ptr)? {
                        SchedulerStep::ContinueWalk(next_walk) => walk = next_walk,
                        step => return Ok(step),
                    }
                }
            }
        }
    }

    fn apply_condition_action(
        &mut self,
        walk: QueryWalk,
        flow_id: FlowId,
        condition_flow: InferConditionFlow,
        action: ConditionFlowAction,
    ) -> Result<SchedulerStep, InferFailReason> {
        match action {
            ConditionFlowAction::Continue => {
                get_flow_var_cache(self.cache, walk.query.var_cache_idx)
                    .condition_cache
                    .insert(
                        (flow_id, condition_flow),
                        CacheEntry::Cache(ConditionFlowAction::Continue),
                    );
                Ok(SchedulerStep::ContinueWalk(walk))
            }
            ConditionFlowAction::Result(result_type) => {
                get_flow_var_cache(self.cache, walk.query.var_cache_idx)
                    .condition_cache
                    .insert(
                        (flow_id, condition_flow),
                        CacheEntry::Cache(ConditionFlowAction::Result(result_type.clone())),
                    );
                Ok(self.finish_walk(walk, result_type))
            }
            ConditionFlowAction::Pending(pending_condition_narrow) => self.start_pending_condition(
                walk,
                flow_id,
                condition_flow,
                pending_condition_narrow,
            ),
            ConditionFlowAction::NeedExprType {
                flow_id: expr_flow_id,
                expr,
                resume,
            } => {
                let replay_query = FlowReplayQuery::new(
                    self.db,
                    Some(self.tree),
                    self.cache,
                    expr_flow_id,
                    expr,
                    false,
                );
                self.start_expr_replay(
                    walk,
                    FlowExprReplay::Condition {
                        condition_flow_id: flow_id,
                        condition_flow,
                        resume,
                    },
                    replay_query,
                )
            }
            ConditionFlowAction::NeedSubquery(subquery) => {
                Ok(self.start_condition_subquery(walk, flow_id, condition_flow, subquery))
            }
            ConditionFlowAction::NeedFieldLiteralSibling(subquery) => Ok(
                self.start_field_literal_sibling_subquery(walk, flow_id, condition_flow, subquery)
            ),
            ConditionFlowAction::NeedCorrelated(pending_correlated_condition) => {
                let subquery = walk.query.at_flow(
                    pending_correlated_condition.current_search_root_flow_id,
                    FlowMode::WithConditions,
                );
                Ok(SchedulerStep::StartQuery {
                    query: subquery,
                    continuation: Some(Continuation::CorrelatedSearchRoot {
                        walk,
                        flow_id,
                        condition_flow,
                        pending_correlated_condition,
                    }),
                })
            }
        }
    }

    fn finish_walk(&mut self, walk: QueryWalk, narrow_type: LuaType) -> SchedulerStep {
        let QueryWalk {
            query,
            pending_condition_narrows,
            ..
        } = walk;
        let mut final_type = narrow_type;
        if query.mode.uses_conditions() {
            for pending_condition_narrow in pending_condition_narrows.into_iter().rev() {
                final_type = pending_condition_narrow.apply(self.db, self.cache, final_type);
            }
        }
        get_flow_var_cache(self.cache, query.var_cache_idx)
            .type_cache
            .insert(
                (query.flow_id, query.mode),
                CacheEntry::Cache(final_type.clone()),
            );
        SchedulerStep::ResumeNext(Ok(final_type))
    }

    fn fail_query<T>(
        &mut self,
        query: &FlowQuery,
        err: InferFailReason,
    ) -> Result<T, InferFailReason> {
        get_flow_var_cache(self.cache, query.var_cache_idx)
            .type_cache
            .remove(&(query.flow_id, query.mode));
        Err(err)
    }

    fn fail_condition_query<T>(
        &mut self,
        query: &FlowQuery,
        flow_id: FlowId,
        condition_flow: InferConditionFlow,
        err: InferFailReason,
    ) -> Result<T, InferFailReason> {
        get_flow_var_cache(self.cache, query.var_cache_idx)
            .condition_cache
            .remove(&(flow_id, condition_flow));
        self.fail_query(query, err)
    }
}

pub(super) fn get_type_at_flow(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_id: FlowId,
) -> InferResult {
    FlowTypeEngine {
        db,
        tree,
        cache,
        root,
    }
    .run(var_ref_id, flow_id)
}

fn get_flow_cache_var_ref_id(cache: &mut LuaInferCache, var_ref_id: &VarRefId) -> u32 {
    if let Some(var_ref_cache_id) = cache.flow_cache_var_ref_ids.get(var_ref_id) {
        return *var_ref_cache_id;
    }

    let var_ref_cache_id = cache.next_flow_cache_var_ref_id;
    cache.next_flow_cache_var_ref_id += 1;
    cache
        .flow_cache_var_ref_ids
        .insert(var_ref_id.clone(), var_ref_cache_id);
    var_ref_cache_id
}

fn get_flow_var_cache(cache: &mut LuaInferCache, var_ref_cache_id: u32) -> &mut FlowVarCache {
    let outer_index = var_ref_cache_id as usize;
    if cache.flow_var_caches.len() <= outer_index {
        cache
            .flow_var_caches
            .resize_with(outer_index + 1, FlowVarCache::default);
    }
    &mut cache.flow_var_caches[outer_index]
}

fn replay_expr_no_flow(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    expr: LuaExpr,
    dependency_types: &[(LuaSyntaxId, LuaType)],
    allow_table_exprs: bool,
) -> Result<Option<LuaType>, InferFailReason> {
    let mut table_exprs = Vec::new();
    if allow_table_exprs {
        if let LuaExpr::TableExpr(table_expr) = &expr {
            table_exprs.push(table_expr.get_syntax_id());
        }
        table_exprs.extend(
            expr.descendants::<LuaTableExpr>()
                .map(|table_expr| table_expr.get_syntax_id()),
        );
    }

    cache.with_replay_overlay(dependency_types, &table_exprs, |cache| {
        try_infer_expr_no_flow(db, cache, expr)
    })
}

fn can_reuse_narrowed_assignment_source(
    db: &DbIndex,
    narrowed_source_type: &LuaType,
    expr_type: &LuaType,
) -> bool {
    if matches!(expr_type, LuaType::TableConst(_) | LuaType::Object(_)) {
        return is_partial_assignment_expr_compatible(db, narrowed_source_type, expr_type);
    }

    if !is_exact_assignment_expr_type(expr_type) {
        return false;
    }

    match narrow_down_type(db, narrowed_source_type.clone(), expr_type.clone(), None) {
        Some(narrowed_expr_type) => narrowed_expr_type == *expr_type,
        None => true,
    }
}

fn preserves_assignment_expr_type(typ: &LuaType) -> bool {
    matches!(typ, LuaType::TableConst(_) | LuaType::Object(_)) || is_exact_assignment_expr_type(typ)
}

fn is_partial_assignment_expr_compatible(
    db: &DbIndex,
    source_type: &LuaType,
    expr_type: &LuaType,
) -> bool {
    if check_type_compact(db, source_type, expr_type).is_ok() {
        return true;
    }

    if !matches!(expr_type, LuaType::TableConst(_) | LuaType::Object(_)) {
        return false;
    }

    let expr_members = find_members(db, expr_type).unwrap_or_default();

    if expr_members.is_empty() {
        return true;
    }

    let Some(source_members) = find_members(db, source_type) else {
        return false;
    };

    expr_members.into_iter().all(|expr_member| {
        match source_members
            .iter()
            .find(|source_member| source_member.key == expr_member.key)
        {
            Some(source_member) => {
                is_partial_assignment_expr_compatible(db, &source_member.typ, &expr_member.typ)
            }
            None => true,
        }
    })
}

fn is_exact_assignment_expr_type(typ: &LuaType) -> bool {
    match typ {
        LuaType::Nil | LuaType::DocBooleanConst(_) => true,
        typ if typ.is_const() => !matches!(typ, LuaType::TableConst(_)),
        LuaType::Union(union) => union.into_vec().iter().all(is_exact_assignment_expr_type),
        LuaType::MultiLineUnion(multi_union) => {
            is_exact_assignment_expr_type(&multi_union.to_union())
        }
        LuaType::TypeGuard(inner) => is_exact_assignment_expr_type(inner),
        _ => false,
    }
}

fn get_branch_label_flow_ids(
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    flow_node: &FlowNode,
) -> Result<Arc<[FlowId]>, InferFailReason> {
    let flow_index = flow_node.id.0 as usize;
    if let Some(Some(flow_ids)) = cache.flow_branch_inputs_cache.get(flow_index) {
        return Ok(flow_ids.clone());
    }

    let mut pending = get_multi_antecedents(tree, flow_node)?;
    let mut visited_labels = HashSet::with_capacity(pending.len());
    let mut branch_flow_ids = Vec::with_capacity(pending.len());

    while let Some(flow_id) = pending.pop() {
        let branch_flow_node = tree.get_flow_node(flow_id).ok_or(InferFailReason::None)?;
        match &branch_flow_node.kind {
            FlowNodeKind::BranchLabel => {
                if !visited_labels.insert(flow_id) {
                    continue;
                }

                if let Some(Some(cached_flow_ids)) =
                    cache.flow_branch_inputs_cache.get(flow_id.0 as usize)
                {
                    branch_flow_ids.extend(cached_flow_ids.iter().copied());
                } else {
                    pending.extend(get_multi_antecedents(tree, branch_flow_node)?);
                }
            }
            _ => branch_flow_ids.push(flow_id),
        }
    }

    if cache.flow_branch_inputs_cache.len() <= flow_index {
        cache
            .flow_branch_inputs_cache
            .resize_with(flow_index + 1, || None);
    }
    let branch_flow_ids = Arc::<[FlowId]>::from(branch_flow_ids);
    cache.flow_branch_inputs_cache[flow_index] = Some(branch_flow_ids.clone());
    Ok(branch_flow_ids)
}

fn get_flow_assignment_info(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    flow_id: FlowId,
    assign_ptr: &emmylua_parser::LuaAstPtr<LuaAssignStat>,
) -> Result<Rc<FlowAssignmentInfo>, InferFailReason> {
    let flow_index = flow_id.0 as usize;
    if let Some(Some(info)) = cache.flow_assignment_info_cache.get(flow_index) {
        return Ok(info.clone());
    }

    let assign_stat = assign_ptr.to_node(root).ok_or(InferFailReason::None)?;
    let (vars, exprs) = assign_stat.get_var_and_expr_list();
    let var_ref_ids = vars
        .iter()
        .map(|var| get_var_expr_var_ref_id(db, cache, var.to_expr()))
        .collect::<Vec<_>>();
    let info = Rc::new(FlowAssignmentInfo {
        vars,
        exprs,
        var_ref_ids,
    });
    if cache.flow_assignment_info_cache.len() <= flow_index {
        cache
            .flow_assignment_info_cache
            .resize_with(flow_index + 1, || None);
    }
    cache.flow_assignment_info_cache[flow_index] = Some(info.clone());
    Ok(info)
}

fn finish_assignment_result(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    source_type: &LuaType,
    expr_type: &LuaType,
    var_ref_id: &VarRefId,
    reuse_source_narrowing: bool,
    fallback_type: Option<LuaType>,
) -> LuaType {
    // Unknown RHS usually means the lookup failed, so keep the last known runtime type.
    if expr_type.is_unknown() {
        return source_type.clone();
    }

    let narrowed = if *source_type == LuaType::Nil {
        None
    } else {
        let declared = get_var_ref_type(db, cache, var_ref_id)
            .ok()
            .and_then(|decl| match decl {
                LuaType::Def(_) | LuaType::Ref(_) => Some(decl),
                _ => None,
            });

        narrow_down_type(db, source_type.clone(), expr_type.clone(), declared)
    };

    if reuse_source_narrowing || preserves_assignment_expr_type(expr_type) {
        narrowed.unwrap_or_else(|| fallback_type.unwrap_or_else(|| expr_type.clone()))
    } else {
        expr_type.clone()
    }
}
