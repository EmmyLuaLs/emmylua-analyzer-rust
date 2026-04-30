use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaChunk, LuaDocOpType, LuaExpr, LuaVarExpr, UnaryOperator,
};
use hashbrown::HashSet;
use std::{rc::Rc, sync::Arc};

use crate::{
    CacheEntry, DbIndex, FlowId, FlowNode, FlowNodeKind, FlowTree, InferFailReason, LuaDeclId,
    LuaInferCache, LuaMemberId, LuaSignatureId, LuaType, TypeOps, check_type_compact,
    semantic::{
        cache::{FlowAssignmentInfo, FlowConditionInfo, FlowMode, FlowVarCache},
        infer::{
            InferResult, VarRefId, infer_expr_list_value_type_at,
            narrow::{
                condition_flow::{
                    ConditionFlowAction, ConditionSubquery, InferConditionFlow,
                    PendingConditionNarrow,
                    correlated_flow::{
                        PendingCorrelatedCondition, advance_pending_correlated_condition,
                    },
                    get_type_at_condition_flow, resolve_condition_subquery,
                },
                get_multi_antecedents, get_single_antecedent,
                get_type_at_cast_flow::cast_type,
                get_var_ref_type, infer_expr_at_expr_flow, narrow_down_type,
                var_ref_id::get_var_expr_var_ref_id,
            },
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
    // Example: for `x = rhs`, first query `x` just before the assignment, then
    // combine that antecedent type with the RHS type here.
    AssignmentAntecedent {
        walk: QueryWalk,
        antecedent_flow_id: FlowId,
        expr_type: LuaType,
        reuse_source_narrowing: bool,
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
        subquery: ConditionSubquery,
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
                        reuse_source_narrowing,
                    }) => self.resume_assignment_antecedent(
                        walk,
                        antecedent_flow_id,
                        expr_type,
                        reuse_source_narrowing,
                        query_result,
                    ),
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
    // combining that antecedent type with the RHS type to finish the suspended
    // query.
    fn resume_assignment_antecedent(
        &mut self,
        walk: QueryWalk,
        antecedent_flow_id: FlowId,
        expr_type: LuaType,
        reuse_source_narrowing: bool,
        source_result: InferResult,
    ) -> Result<SchedulerStep, InferFailReason> {
        let source_type = match source_result {
            Ok(source_type) => source_type,
            Err(err) => return self.fail_query(&walk.query, err),
        };

        if reuse_source_narrowing
            && !can_reuse_narrowed_assignment_source(self.db, &source_type, &expr_type)
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
                    reuse_source_narrowing: false,
                }),
            });
        }

        let result_type = finish_assignment_result(
            self.db,
            self.cache,
            &source_type,
            &expr_type,
            &walk.query.var_ref_id,
            reuse_source_narrowing,
            None,
        );
        Ok(self.finish_walk(walk, result_type))
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
        subquery: ConditionSubquery,
        antecedent_result: InferResult,
    ) -> Result<SchedulerStep, InferFailReason> {
        let query = walk.query.clone();
        let result = (|| {
            let antecedent_type = antecedent_result?;
            let flow_node = self
                .tree
                .get_flow_node(flow_id)
                .ok_or(InferFailReason::None)?;
            let action = resolve_condition_subquery(
                self.db,
                self.tree,
                self.cache,
                self.root,
                &query.var_ref_id,
                flow_node,
                subquery,
                antecedent_type,
            )?;
            self.apply_condition_action(walk, flow_id, condition_flow, action)
        })();

        result.or_else(|err| {
            get_flow_var_cache(self.cache, query.var_cache_idx)
                .condition_cache
                .remove(&(flow_id, condition_flow));
            self.fail_query(&query, err)
        })
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

        let expr_type =
            match infer_expr_list_value_type_at(self.db, self.cache, &assignment_info.exprs, i) {
                Ok(expr_type) => expr_type,
                Err(err) => {
                    if let Some(explicit_var_type) = explicit_var_type.as_ref() {
                        return Ok(self.finish_walk(walk, explicit_var_type.clone()));
                    }

                    if matches!(var_ref_id, VarRefId::IndexRef(_, _))
                        && let Ok(origin_type) = get_var_ref_type(self.db, self.cache, &var_ref_id)
                    {
                        let non_nil_origin =
                            TypeOps::Remove.apply(self.db, &origin_type, &LuaType::Nil);
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
                    return Ok(SchedulerStep::ContinueWalk(walk));
                }
            };
        let Some(expr_type) = expr_type else {
            walk.antecedent_flow_id = antecedent_flow_id;
            return Ok(SchedulerStep::ContinueWalk(walk));
        };

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

        let reuse_source_narrowing = preserves_assignment_expr_type(&expr_type);
        let mode = if reuse_source_narrowing {
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
                reuse_source_narrowing,
            }),
        })
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

        let condition_info =
            get_flow_condition_info(self.db, self.cache, self.root, flow_node.id, condition_ptr)?;
        walk.antecedent_flow_id = antecedent_flow_id;
        let q = &walk.query;
        let var_ref_id = &q.var_ref_id;
        if condition_info.index_var_ref_id.is_some()
            && condition_info.index_var_ref_id.as_ref() != Some(var_ref_id)
            && condition_info.index_prefix_var_ref_id.as_ref() != Some(var_ref_id)
        {
            return Ok(SchedulerStep::ContinueWalk(walk));
        }

        let cache_id = q.var_cache_idx;
        let flow_id = flow_node.id;
        let cache_key = (flow_id, condition_flow);
        let action = match self
            .cache
            .flow_var_caches
            .get(cache_id as usize)
            .and_then(|var_cache| var_cache.condition_cache.get(&cache_key))
        {
            Some(CacheEntry::Cache(action)) => action.clone(),
            Some(CacheEntry::Ready) => {
                return self.fail_query(q, InferFailReason::RecursiveInfer);
            }
            None => {
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
                    condition_info.expr.clone(),
                    condition_flow,
                ) {
                    Ok(action) => action,
                    Err(err) => {
                        get_flow_var_cache(self.cache, cache_id)
                            .condition_cache
                            .remove(&cache_key);
                        return self.fail_query(q, err);
                    }
                }
            }
        };

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
                                    && let Some(expr_type) = infer_expr_at_expr_flow(
                                        self.db, self.tree, self.cache, self.root, expr,
                                    )?
                                    && let Some(init_type) = expr_type.get_result_slot_type(0)
                                {
                                    return Ok(self.finish_walk(walk, init_type));
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
        mut walk: QueryWalk,
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
            ConditionFlowAction::Pending(pending_condition_narrow) => {
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
                Ok(SchedulerStep::ContinueWalk(walk))
            }
            ConditionFlowAction::NeedSubquery(subquery) => {
                let (subquery_var_ref_id, subquery_antecedent_flow_id) = match &subquery {
                    ConditionSubquery::ArrayLen {
                        var_ref_id,
                        antecedent_flow_id,
                        ..
                    }
                    | ConditionSubquery::FieldLiteralEq {
                        var_ref_id,
                        antecedent_flow_id,
                        ..
                    }
                    | ConditionSubquery::Correlated {
                        var_ref_id,
                        antecedent_flow_id,
                        ..
                    } => (var_ref_id, *antecedent_flow_id),
                };
                let subquery_query =
                    FlowQuery::new(self.cache, subquery_var_ref_id, subquery_antecedent_flow_id);
                Ok(SchedulerStep::StartQuery {
                    query: subquery_query,
                    continuation: Some(Continuation::ConditionDependency {
                        walk,
                        flow_id,
                        condition_flow,
                        subquery,
                    }),
                })
            }
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

fn get_flow_condition_info(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    flow_id: FlowId,
    condition_ptr: &emmylua_parser::LuaAstPtr<LuaExpr>,
) -> Result<Rc<FlowConditionInfo>, InferFailReason> {
    let flow_index = flow_id.0 as usize;
    if let Some(Some(info)) = cache.flow_condition_info_cache.get(flow_index) {
        return Ok(info.clone());
    }

    let expr = condition_ptr.to_node(root).ok_or(InferFailReason::None)?;
    let (index_var_ref_id, index_prefix_var_ref_id) =
        get_condition_index_var_refs(db, cache, expr.clone());
    let info = Rc::new(FlowConditionInfo {
        expr,
        index_var_ref_id,
        index_prefix_var_ref_id,
    });
    if cache.flow_condition_info_cache.len() <= flow_index {
        cache
            .flow_condition_info_cache
            .resize_with(flow_index + 1, || None);
    }
    cache.flow_condition_info_cache[flow_index] = Some(info.clone());
    Ok(info)
}

fn get_condition_index_var_refs(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    condition: LuaExpr,
) -> (Option<VarRefId>, Option<VarRefId>) {
    match condition {
        LuaExpr::IndexExpr(index_expr) => {
            let index_var_ref_id =
                get_var_expr_var_ref_id(db, cache, LuaExpr::IndexExpr(index_expr.clone()));
            let index_prefix_var_ref_id = if index_var_ref_id.is_some() {
                index_expr
                    .get_prefix_expr()
                    .and_then(|prefix_expr| get_var_expr_var_ref_id(db, cache, prefix_expr))
            } else {
                None
            };
            (index_var_ref_id, index_prefix_var_ref_id)
        }
        LuaExpr::ParenExpr(paren_expr) => paren_expr
            .get_expr()
            .map(|expr| get_condition_index_var_refs(db, cache, expr))
            .unwrap_or((None, None)),
        LuaExpr::UnaryExpr(unary_expr) => {
            let Some(op_token) = unary_expr.get_op_token() else {
                return (None, None);
            };

            if op_token.get_op() != UnaryOperator::OpNot {
                return (None, None);
            }

            unary_expr
                .get_expr()
                .map(|expr| get_condition_index_var_refs(db, cache, expr))
                .unwrap_or((None, None))
        }
        _ => (None, None),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CacheOptions, FileId};

    #[test]
    fn test_flow_caches_stay_sparse_for_large_flow_ids() {
        let mut cache = LuaInferCache::new(FileId::new(0), CacheOptions::default());
        let var_ref_id = VarRefId::VarRef(LuaDeclId::new(FileId::new(0), 0.into()));
        let query = FlowQuery::new(&mut cache, &var_ref_id, FlowId(10_000));

        get_flow_var_cache(&mut cache, 0)
            .type_cache
            .insert((query.flow_id, query.mode), CacheEntry::Ready);
        get_flow_var_cache(&mut cache, 0).condition_cache.insert(
            (FlowId(20_000), InferConditionFlow::FalseCondition),
            CacheEntry::Ready,
        );

        assert_eq!(cache.flow_var_caches.len(), 1);
        assert_eq!(cache.flow_var_caches[0].type_cache.len(), 1);
        assert_eq!(cache.flow_var_caches[0].condition_cache.len(), 1);
        assert!(matches!(
            cache.flow_var_caches[0]
                .type_cache
                .get(&(query.flow_id, query.mode)),
            Some(CacheEntry::Ready)
        ));
        assert!(matches!(
            cache.flow_var_caches[0]
                .condition_cache
                .get(&(FlowId(20_000), InferConditionFlow::FalseCondition)),
            Some(CacheEntry::Ready)
        ));
    }
}
