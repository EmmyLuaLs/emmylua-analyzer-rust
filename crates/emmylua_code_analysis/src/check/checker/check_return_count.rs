//! # check_return_count — mismatch between function body return count and `---@return` annotations
//!
//! Declared and inferred results use the same fixed slots and repeating tails.
//!
//! `MissingReturn` uses control-flow analysis consistent with the old implementation: truthy/falsy/never
//! condition judgment, break/infinite-loop paths in while/repeat loops, and if-branch merging.

use crate::semantic_db;

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaBlock, LuaCallExprStat, LuaClosureExpr, LuaDoStat, LuaDocType,
    LuaExpr, LuaForRangeStat, LuaForStat, LuaIfClauseStat, LuaIfStat, LuaRepeatStat, LuaReturnStat,
    LuaStat, LuaTokenKind, LuaWhileStat,
};

use crate::DiagnosticCode;
use crate::semantic_db::def::Signature;
use crate::semantic_model::{
    SemanticModel,
    result_pack::{ResultPack, can_expand_results},
};
use crate::{GenericTpl, LuaType};

use super::{CheckContext, Checker};

pub struct CheckReturnCountChecker;

impl Checker for CheckReturnCountChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::RedundantReturnValue,
        DiagnosticCode::MissingReturnValue,
        DiagnosticCode::MissingReturn,
    ];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(signatures) = semantic_model.signatures() else {
            return;
        };
        let signatures = signatures.to_vec();
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for signature in &signatures {
            let Some(node) = signature.closure_syntax.to_node_from_root(&root) else {
                continue;
            };
            let Some(closure) = LuaClosureExpr::cast(node) else {
                continue;
            };

            let Some(expectation) = return_expectation_of(semantic_model, signature, &closure)
            else {
                continue;
            };
            check_return_stats(context, semantic_model, &closure, &expectation);
            check_missing_return(context, semantic_model, &closure, &expectation);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ReturnExpectation {
    min: usize,
    max: Option<usize>,
}

/// Own annotations take precedence over the function type declared on a field.
pub(super) fn declared_return_type(
    model: &SemanticModel<'_>,
    signature: &Signature,
) -> Option<(LuaType, Vec<GenericTpl>)> {
    if let Some(docs) = &signature.docs {
        if !docs.returns.is_empty() {
            let scope = model.doc_type_scope(model.file_id(), &docs.generic_params);
            let ty = model.project_return_types_in_scope(&docs.returns, &scope);
            return Some((ty, scope.templates));
        }
        // The body can implement several call overloads with different result packs.
        if !docs.overloads.is_empty() {
            return None;
        }
    }

    let functions = model.expected_member_signatures_for_closure(signature.closure_syntax)?;
    let [function] = functions.as_slice() else {
        return None;
    };
    Some((function.get_ret().clone(), Vec::new()))
}

fn return_expectation_of(
    model: &SemanticModel<'_>,
    signature: &Signature,
    closure: &LuaClosureExpr,
) -> Option<ReturnExpectation> {
    let (ty, _) = declared_return_type(model, signature)?;
    let use_context = signature
        .docs
        .as_ref()
        .is_none_or(|docs| docs.returns.is_empty());
    if ty == LuaType::Nil
        || (ty == LuaType::Unknown
            && use_context
            && field_omits_return_list(model, closure).unwrap_or(false))
    {
        return Some(ReturnExpectation {
            min: 0,
            max: Some(0),
        });
    }

    let pack = ResultPack::from_type(&ty, Some(model));
    Some(ReturnExpectation {
        min: pack.required_len(),
        max: pack.bounds().1,
    })
}

/// The projected type uses Unknown both for `fun()` and `fun(): unknown`.
/// Preserve the zero-return convention for a field that omits the list.
fn field_omits_return_list(model: &SemanticModel<'_>, closure: &LuaClosureExpr) -> Option<bool> {
    let facts = model.file_facts()?;
    let member = facts
        .members
        .iter()
        .find(|member| member.value_syntax == Some(closure.get_syntax_id()))?;
    let key = member.key.name()?;
    let owner_ty = model.type_of_decl(&member.owner)?;
    let (LuaType::Ref(id) | LuaType::Def(id)) = &owner_ty else {
        return None;
    };
    let def = model.type_def_of(id)?;
    let members = model.members_of_owner(&def.id);
    let field_ref = members.iter().find(|field| field.name.as_str() == key)?;
    let field_facts = model.file_facts_of(field_ref.file_id)?;
    let field = field_facts.member_by_id(&field_ref.id)?;
    let syntax = field.value_syntax?;
    let tree = model.syntax_tree_of(field_ref.file_id)?;
    let node = syntax.to_node_from_root(&tree.get_red_root())?;
    let LuaDocType::Func(func) = LuaDocType::cast(node)? else {
        return None;
    };
    Some(func.get_return_type_list().is_none())
}

/// Only check return statements belonging to this closure itself.
pub(super) fn own_return_stats(closure: &LuaClosureExpr) -> Vec<LuaReturnStat> {
    closure
        .descendants::<LuaReturnStat>()
        .filter(|stat| {
            stat.ancestors::<LuaClosureExpr>()
                .next()
                .is_some_and(|expr| &expr == closure)
        })
        .collect()
}

fn check_return_stats(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    closure: &LuaClosureExpr,
    expectation: &ReturnExpectation,
) {
    if expectation.max.is_none() && expectation.min == 0 {
        return;
    }
    for return_stat in own_return_stats(closure) {
        let (min, max, redundant_ranges) =
            analyze_return_values(semantic_model, &return_stat, expectation.max);
        if let Some(max) = max
            && max < expectation.min
        {
            context.add_diagnostic(
                DiagnosticCode::MissingReturnValue,
                return_stat.get_range(),
                t!(
                    "Annotations specify that at least %{min} return value(s) are required, found %{rmin} returned here instead.",
                    min = expectation.min,
                    rmin = max
                ),
            );
        }
        if let Some(max) = expectation.max {
            for range in redundant_ranges {
                context.add_diagnostic(
                    DiagnosticCode::RedundantReturnValue,
                    range,
                    t!(
                        "Annotations specify that at most %{max} return value(s) are required, found %{rmax} returned here instead.",
                        max = max,
                        rmax = min
                    ),
                );
            }
        }
    }
}

/// A count error must apply to every result alternative. Only the final
/// expression can expand, so each preceding expression supplies one value.
fn analyze_return_values(
    model: &SemanticModel<'_>,
    return_stat: &LuaReturnStat,
    expected_max: Option<usize>,
) -> (usize, Option<usize>, Vec<rowan::TextRange>) {
    let exprs = return_stat.get_expr_list().collect::<Vec<_>>();
    let Some((last, prefix)) = exprs.split_last() else {
        return (0, Some(0), Vec::new());
    };
    let actual = ResultPack::from_expr(model, last);
    let (last_min, last_max) = if can_expand_results(last) {
        (actual.min_len(), actual.bounds().1)
    } else {
        (1, Some(1))
    };
    let min = prefix.len() + last_min;
    let max = last_max.map(|count| prefix.len() + count);

    let mut redundant_ranges = Vec::new();
    if let Some(allowed) = expected_max {
        redundant_ranges.extend(prefix.iter().skip(allowed).map(LuaAstNode::get_range));
        // Allow a single trailing nil beyond the declared result count.
        let trailing_nil = prefix.len() == allowed && actual.is_single(&LuaType::Nil);
        if last_min > 0 && min > allowed && !trailing_nil {
            redundant_ranges.push(last.get_range());
        }
    }
    (min, max, redundant_ranges)
}

/// `LuaClosureExpr::get_block()` is unreliable for `local function`/`function` statements
/// (the closure node only has ParamList); recover the function-body Block from the parent statement.
fn closure_body_block(closure: &LuaClosureExpr) -> Option<LuaBlock> {
    if let Some(block) = closure
        .get_block()
        .or_else(|| closure.descendants::<LuaBlock>().next())
    {
        return Some(block);
    }
    let parent = closure.syntax().parent()?;
    parent.descendants().filter_map(LuaBlock::cast).next()
}

fn check_missing_return(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    closure: &LuaClosureExpr,
    expectation: &ReturnExpectation,
) {
    if expectation.min == 0 {
        return;
    }
    let flow = closure_body_block(closure)
        .map(|block| analyze_block_returns(&block, semantic_model))
        .unwrap_or_else(ReturnFlow::fallthrough);
    if !flow.can_fall_through && !flow.can_break {
        return;
    }
    let range = closure_body_block(closure)
        .and_then(|block| {
            block
                .token_by_kind(LuaTokenKind::TkEnd)
                .map(|t| t.syntax().text_range())
        })
        .unwrap_or_else(|| closure.get_range());
    context.add_diagnostic(
        DiagnosticCode::MissingReturn,
        range,
        t!("Annotations specify that a return value is required here."),
    );
}

// ── Control flow: whether it can fall through / break (ported from the old analyze_func_body_missing_return_flags_with) ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConditionState {
    Dynamic,
    Truthy,
    Falsy,
    Never,
}

#[derive(Debug, Default)]
struct ReturnFlow {
    can_fall_through: bool,
    can_break: bool,
    is_infinite: bool,
    may_diverge: bool,
}

impl ReturnFlow {
    fn fallthrough() -> Self {
        Self {
            can_fall_through: true,
            ..Default::default()
        }
    }

    fn merge_choice(&mut self, other: Self) {
        self.can_fall_through |= other.can_fall_through;
        self.can_break |= other.can_break;
        self.is_infinite |= other.is_infinite;
        self.may_diverge |= other.may_diverge;
    }
}

fn analyze_block_returns(block: &LuaBlock, model: &SemanticModel<'_>) -> ReturnFlow {
    let mut flow = ReturnFlow::default();
    let mut can_fall_through = true;
    for stat in block.get_stats() {
        if !can_fall_through {
            break;
        }
        let stat_flow = analyze_stat_returns(&stat, model);
        flow.can_break |= stat_flow.can_break;
        flow.is_infinite |= stat_flow.is_infinite;
        flow.may_diverge |= stat_flow.may_diverge;
        can_fall_through = stat_flow.can_fall_through;
    }
    flow.can_fall_through = can_fall_through;
    flow
}

fn analyze_optional_block_returns(
    block: Option<LuaBlock>,
    model: &SemanticModel<'_>,
) -> ReturnFlow {
    match block {
        Some(block) => analyze_block_returns(&block, model),
        None => ReturnFlow::fallthrough(),
    }
}

fn analyze_stat_returns(stat: &LuaStat, model: &SemanticModel<'_>) -> ReturnFlow {
    match stat {
        LuaStat::DoStat(do_stat) => analyze_do_stat_returns(do_stat, model),
        LuaStat::WhileStat(while_stat) => analyze_while_stat_returns(while_stat, model),
        LuaStat::RepeatStat(repeat_stat) => analyze_repeat_stat_returns(repeat_stat, model),
        LuaStat::IfStat(if_stat) => analyze_if_stat_returns(if_stat, model),
        LuaStat::ForStat(for_stat) => analyze_for_stat_returns(for_stat, model),
        LuaStat::ForRangeStat(for_range_stat) => {
            analyze_for_range_stat_returns(for_range_stat, model)
        }
        LuaStat::CallExprStat(call_expr) => analyze_call_expr_stat_returns(call_expr),
        LuaStat::BreakStat(_) => ReturnFlow {
            can_break: true,
            ..Default::default()
        },
        LuaStat::ReturnStat(_) => ReturnFlow::default(),
        _ => ReturnFlow::fallthrough(),
    }
}

fn analyze_do_stat_returns(do_stat: &LuaDoStat, model: &SemanticModel<'_>) -> ReturnFlow {
    analyze_optional_block_returns(do_stat.get_block(), model)
}

fn analyze_while_stat_returns(while_stat: &LuaWhileStat, model: &SemanticModel<'_>) -> ReturnFlow {
    let condition_state = condition_state(while_stat.get_condition_expr().as_ref(), model);
    match condition_state {
        ConditionState::Falsy => ReturnFlow::fallthrough(),
        ConditionState::Never => ReturnFlow::default(),
        ConditionState::Truthy => {
            let body = analyze_optional_block_returns(while_stat.get_block(), model);
            ReturnFlow {
                can_fall_through: body.can_break,
                can_break: false,
                is_infinite: (body.can_fall_through && !body.can_break) || body.is_infinite,
                may_diverge: (body.can_fall_through && body.can_break) || body.may_diverge,
            }
        }
        ConditionState::Dynamic => {
            let body = analyze_optional_block_returns(while_stat.get_block(), model);
            ReturnFlow {
                can_fall_through: true,
                can_break: false,
                is_infinite: body.is_infinite,
                may_diverge: body.can_fall_through || body.may_diverge,
            }
        }
    }
}

fn analyze_repeat_stat_returns(
    repeat_stat: &LuaRepeatStat,
    model: &SemanticModel<'_>,
) -> ReturnFlow {
    let body = analyze_optional_block_returns(repeat_stat.get_block(), model);
    let mut flow = ReturnFlow {
        can_fall_through: body.can_break,
        can_break: false,
        is_infinite: body.is_infinite,
        may_diverge: body.may_diverge,
    };
    if !body.can_fall_through {
        return flow;
    }
    match condition_state(repeat_stat.get_condition_expr().as_ref(), model) {
        ConditionState::Truthy => {
            flow.can_fall_through = true;
        }
        ConditionState::Falsy => {
            if body.can_break {
                flow.may_diverge = true;
            } else {
                flow.is_infinite = true;
            }
        }
        ConditionState::Dynamic => {
            flow.can_fall_through = true;
            flow.may_diverge = true;
        }
        ConditionState::Never => {}
    }
    flow
}

fn analyze_for_stat_returns(for_stat: &LuaForStat, model: &SemanticModel<'_>) -> ReturnFlow {
    let mut flow = analyze_optional_block_returns(for_stat.get_block(), model);
    flow.can_fall_through = true;
    flow.can_break = false;
    flow
}

fn analyze_for_range_stat_returns(
    for_range_stat: &LuaForRangeStat,
    model: &SemanticModel<'_>,
) -> ReturnFlow {
    let mut flow = analyze_optional_block_returns(for_range_stat.get_block(), model);
    flow.can_fall_through = true;
    flow.can_break = false;
    flow
}

fn analyze_if_stat_returns(if_stat: &LuaIfStat, model: &SemanticModel<'_>) -> ReturnFlow {
    let mut flow = ReturnFlow::default();
    let mut can_reach_next_clause = true;
    match condition_state(if_stat.get_condition_expr().as_ref(), model) {
        ConditionState::Truthy => {
            return analyze_optional_block_returns(if_stat.get_block(), model);
        }
        ConditionState::Falsy => {}
        ConditionState::Dynamic => {
            flow.merge_choice(analyze_optional_block_returns(if_stat.get_block(), model));
        }
        ConditionState::Never => return flow,
    }

    for clause in if_stat.get_all_clause() {
        if !can_reach_next_clause {
            break;
        }
        match clause {
            LuaIfClauseStat::ElseIf(clause) => {
                match condition_state(clause.get_condition_expr().as_ref(), model) {
                    ConditionState::Truthy => {
                        flow.merge_choice(analyze_optional_block_returns(
                            clause.get_block(),
                            model,
                        ));
                        can_reach_next_clause = false;
                    }
                    ConditionState::Falsy => {}
                    ConditionState::Dynamic => {
                        flow.merge_choice(analyze_optional_block_returns(
                            clause.get_block(),
                            model,
                        ));
                    }
                    ConditionState::Never => can_reach_next_clause = false,
                }
            }
            LuaIfClauseStat::Else(clause) => {
                flow.merge_choice(analyze_optional_block_returns(clause.get_block(), model));
                can_reach_next_clause = false;
            }
        }
    }

    if can_reach_next_clause {
        flow.can_fall_through = true;
    }
    flow
}

fn analyze_call_expr_stat_returns(call_expr_stat: &LuaCallExprStat) -> ReturnFlow {
    if call_expr_stat
        .get_call_expr()
        .is_some_and(|call| call.is_error())
    {
        return ReturnFlow::default();
    }
    ReturnFlow::fallthrough()
}

fn condition_state(condition: Option<&LuaExpr>, model: &SemanticModel<'_>) -> ConditionState {
    let Some(condition) = condition else {
        return ConditionState::Dynamic;
    };
    if let Some(truthy) = semantic_db::facts::static_truthiness(condition) {
        return if truthy {
            ConditionState::Truthy
        } else {
            ConditionState::Falsy
        };
    }
    if !can_analyze_condition(condition) {
        return ConditionState::Dynamic;
    }
    let ty = model.type_of_expr(condition.get_syntax_id());
    if ty.is_never() {
        return ConditionState::Never;
    }
    if ty.is_always_truthy() {
        return ConditionState::Truthy;
    }
    if ty.is_always_falsy() {
        return ConditionState::Falsy;
    }
    ConditionState::Dynamic
}

fn can_analyze_condition(expr: &LuaExpr) -> bool {
    match expr {
        LuaExpr::LiteralExpr(_) | LuaExpr::TableExpr(_) | LuaExpr::ClosureExpr(_) => true,
        LuaExpr::CallExpr(_) => false,
        LuaExpr::ParenExpr(paren_expr) => paren_expr
            .get_expr()
            .is_some_and(|expr| can_analyze_condition(&expr)),
        LuaExpr::UnaryExpr(unary_expr) => unary_expr
            .get_expr()
            .is_some_and(|expr| can_analyze_condition(&expr)),
        LuaExpr::BinaryExpr(binary_expr) => binary_expr.get_exprs().is_some_and(|(left, right)| {
            can_analyze_condition(&left) && can_analyze_condition(&right)
        }),
        LuaExpr::NameExpr(_) | LuaExpr::IndexExpr(_) => false,
        _ => true,
    }
}
