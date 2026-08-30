//! # check_return_count — mismatch between function body return count and `---@return` annotations
//!
//! The return count is expanded from the annotation types:
//! - Fixed annotations -> both min and max are the annotation count; `?` / `any` reduce the minimum;
//! - `T...` -> `Variadic::Base`, with no upper bound on the max count;
//! - `return foo()` expands according to foo's multi-return types (`Variadic::Multi`).
//!
//! `MissingReturn` uses control-flow analysis consistent with the old implementation: truthy/falsy/never
//! condition judgment, break/infinite-loop paths in while/repeat loops, and if-branch merging.

use std::sync::Arc;

use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaAstToken, LuaBlock, LuaCallExprStat, LuaClosureExpr, LuaDoStat,
    LuaExpr, LuaForRangeStat, LuaForStat, LuaIfClauseStat, LuaIfStat, LuaRepeatStat, LuaReturnStat,
    LuaStat, LuaTokenKind, LuaWhileStat,
};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;
use crate::{LuaType, VariadicType};

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

            let expectation = return_expectation_of(semantic_model, signature, &closure);
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

impl ReturnExpectation {
    fn none() -> Self {
        Self { min: 0, max: None }
    }

    fn fixed(count: usize) -> Self {
        Self {
            min: count,
            max: Some(count),
        }
    }
}

/// Signature doc annotation + contextual member annotation -> expected return count.
fn return_expectation_of(
    semantic_model: &SemanticModel<'_>,
    signature: &crate::salsa_builder::def::Signature,
    closure: &LuaClosureExpr,
) -> ReturnExpectation {
    let mut syntaxes: Vec<(rowan::TextSize, emmylua_parser::LuaSyntaxId)> = Vec::new();
    if let Some(docs) = &signature.docs {
        syntaxes.extend(
            docs.returns
                .iter()
                .map(|syntax| (syntax.get_range().start(), *syntax)),
        );
        syntaxes.extend(
            docs.return_overloads
                .iter()
                .filter(|(name, _)| name.is_some())
                .map(|(_, syntax)| (syntax.get_range().start(), *syntax)),
        );
    }
    if !syntaxes.is_empty() {
        syntaxes.sort_by_key(|(start, _)| *start);
        let types: Vec<LuaType> = syntaxes
            .into_iter()
            .map(|(_, syntax)| project_return_annotation(semantic_model, syntax))
            .collect();
        return expectation_from_types(&types);
    }

    // No own `---@return`: contextual function types such as `---@field event fun(aaa)` may constrain the return count.
    contextual_return_expectation(semantic_model, closure).unwrap_or_else(ReturnExpectation::none)
}

/// Anonymous function for `---@field event fun(aaa)`: take the return annotation from the contextual member type.
fn contextual_return_expectation(
    semantic_model: &SemanticModel<'_>,
    closure: &LuaClosureExpr,
) -> Option<ReturnExpectation> {
    let facts = semantic_model.file_facts()?;
    let member = facts
        .members
        .iter()
        .find(|member| member.value_syntax == Some(closure.get_syntax_id()))?;
    let key = member.key.name()?;
    let owner_ty = semantic_model.type_of_decl(&member.owner)?;
    let (LuaType::Ref(id) | LuaType::Def(id)) = &owner_ty else {
        return None;
    };
    let def = semantic_model.type_def_of(id)?;
    let field_ref = semantic_model
        .members_of_owner(&def.id)
        .into_iter()
        .find(|field| field.name.as_str() == key)?;
    let field_facts = semantic_model.file_facts_of(field_ref.file_id)?;
    let field = field_facts.member_by_id(&field_ref.id)?;
    let syntax = field.value_syntax?;
    let tree = semantic_model.syntax_tree_of(field_ref.file_id)?;
    let node = syntax.to_node_from_root(&tree.get_red_root())?;
    let doc_ty = emmylua_parser::LuaDocType::cast(node)?;
    let emmylua_parser::LuaDocType::Func(func) = doc_ty else {
        return None;
    };
    let Some(return_list) = func.get_return_type_list() else {
        // `fun(aaa)` has no return type -> 0 return values.
        return Some(ReturnExpectation::fixed(0));
    };
    let types: Vec<LuaType> = return_list
        .get_return_type_list()
        .filter_map(|ret| ret.get_name_and_type().1)
        .map(|ty| {
            project_return_annotation_in(semantic_model, field_ref.file_id, ty.get_syntax_id())
        })
        .collect();
    Some(expectation_from_types(&types))
}

/// Annotation type list -> min/max count.
fn expectation_from_types(types: &[LuaType]) -> ReturnExpectation {
    if types.is_empty() {
        return ReturnExpectation::fixed(0);
    }
    if types.len() == 1 {
        let ty = &types[0];
        if let LuaType::Variadic(variadic) = ty {
            return expectation_from_variadic(variadic);
        }
        if matches!(ty, LuaType::Any | LuaType::Unknown) {
            return ReturnExpectation {
                min: 0,
                max: Some(1),
            };
        }
        if matches!(ty, LuaType::Nil) {
            return ReturnExpectation {
                min: 0,
                max: Some(0),
            };
        }
        let min = usize::from(!ty.is_optional());
        return ReturnExpectation { min, max: Some(1) };
    }

    let mut min = types.len();
    // Trailing consecutive optionals are not required to be returned.
    for ty in types.iter().rev() {
        if ty.is_optional() {
            min -= 1;
        } else {
            break;
        }
    }
    let mut max = Some(0usize);
    for ty in types {
        match ty {
            LuaType::Variadic(variadic) => {
                let Some(len) = variadic.get_max_len() else {
                    return ReturnExpectation { min, max: None };
                };
                *max.as_mut().expect("max is Some") += len;
            }
            _ => *max.as_mut().expect("max is Some") += 1,
        }
    }
    ReturnExpectation { min, max }
}

fn expectation_from_variadic(variadic: &VariadicType) -> ReturnExpectation {
    let Some(min_len) = variadic.get_min_len() else {
        return ReturnExpectation::none();
    };
    let mut min = min_len;
    if min_len > 0 {
        for idx in (0..min_len).rev() {
            if let Some(ty) = variadic.get_type(idx)
                && ty.is_optional()
            {
                min -= 1;
            } else {
                break;
            }
        }
    }
    ReturnExpectation {
        min,
        max: variadic.get_max_len(),
    }
}

/// Project the `---@return` type node (`T...` becomes Variadic::Base).
fn project_return_annotation(
    semantic_model: &SemanticModel<'_>,
    syntax: emmylua_parser::LuaSyntaxId,
) -> LuaType {
    project_return_annotation_in(semantic_model, semantic_model.file_id(), syntax)
}

fn project_return_annotation_in(
    semantic_model: &SemanticModel<'_>,
    file_id: crate::FileId,
    syntax: emmylua_parser::LuaSyntaxId,
) -> LuaType {
    if let Some(tree) = semantic_model.syntax_tree_of(file_id)
        && let Some(node) = syntax.to_node_from_root(&tree.get_red_root())
        && let Some(doc_ty) = emmylua_parser::LuaDocType::cast(node)
    {
        if let emmylua_parser::LuaDocType::Variadic(variadic) = &doc_ty {
            let inner = variadic
                .get_type()
                .map(|ty| project_return_annotation_in(semantic_model, file_id, ty.get_syntax_id()))
                .unwrap_or(LuaType::Any);
            return LuaType::Variadic(Arc::new(VariadicType::Base(inner)));
        }
    }
    semantic_model.doc_type_lua_rich_in(file_id, syntax)
}

/// Only check return statements belonging to this closure itself.
fn own_return_stats(closure: &LuaClosureExpr) -> Vec<LuaReturnStat> {
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
        let Some((total, redundant_ranges)) =
            analyze_return_values(semantic_model, &return_stat, expectation.max)
        else {
            continue;
        };
        if total < expectation.min {
            context.add_diagnostic(
                DiagnosticCode::MissingReturnValue,
                return_stat.get_range(),
                t!(
                    "Annotations specify that at least %{min} return value(s) are required, found %{rmin} returned here instead.",
                    min = expectation.min,
                    rmin = total
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
                        rmax = total
                    ),
                );
            }
        }
    }
}

/// Expand the `return` expression list: multi-return calls expand by Variadic::Multi.
/// Returns `(total count, redundant expression ranges)`; returns `None` when the max argument count is unknown.
fn analyze_return_values(
    semantic_model: &SemanticModel<'_>,
    return_stat: &LuaReturnStat,
    expected_max: Option<usize>,
) -> Option<(usize, Vec<rowan::TextRange>)> {
    let expr_list = return_stat.get_expr_list().collect::<Vec<_>>();
    let mut total = 0usize;
    let mut redundant_ranges = Vec::new();
    let mut tail_return_nil = false;

    for (index, expr) in expr_list.iter().enumerate() {
        let expr_type = full_return_type(semantic_model, expr);
        match &expr_type {
            LuaType::Variadic(variadic) => {
                total += variadic.get_max_len()?;
            }
            LuaType::Nil => {
                if index == expr_list.len() - 1 {
                    tail_return_nil = true;
                }
                total += 1;
            }
            _ => total += 1,
        }
        if let Some(max) = expected_max
            && total > max
        {
            if tail_return_nil && total - 1 == max {
                continue;
            }
            redundant_ranges.push(expr.get_range());
        }
    }
    Some((total, redundant_ranges))
}

/// Full return-expression type: multi-return call expressions expand by the callee's signature annotations into `Variadic::Multi`.
fn full_return_type(semantic_model: &SemanticModel<'_>, expr: &LuaExpr) -> LuaType {
    if let LuaExpr::CallExpr(call_expr) = expr
        && let Some(types) = call_return_annotations(semantic_model, call_expr)
    {
        return match types.len() {
            0 => LuaType::Nil,
            1 => types.into_iter().next().expect("len checked"),
            _ => LuaType::Variadic(Arc::new(VariadicType::Multi(types))),
        };
    }
    semantic_model.type_of_expr_at(expr.get_syntax_id(), expr.get_range().start())
}

/// All `---@return` types from the callee signature doc (sorted by source position).
fn call_return_annotations(
    semantic_model: &SemanticModel<'_>,
    call_expr: &emmylua_parser::LuaCallExpr,
) -> Option<Vec<LuaType>> {
    let callee = call_expr.get_prefix_expr()?;
    let (file_id, closure_syntax) = match &callee {
        LuaExpr::NameExpr(name_expr) => {
            let decl = semantic_model.resolve_name(name_expr.get_position())?;
            let crate::salsa_builder::def::SemanticId::Decl(decl_key) = decl else {
                return None;
            };
            let facts = semantic_model.file_facts_of(decl_key.file_id)?;
            let decl = facts.decl_by_id(&crate::salsa_builder::def::SemanticId::Decl(
                decl_key.clone(),
            ))?;
            (decl.file_id, decl.value_expr_syntax?)
        }
        LuaExpr::IndexExpr(index_expr) => {
            let resolved = semantic_model.resolve_member(index_expr)?;
            let member_file = resolved.file_id?;
            let facts = semantic_model.file_facts_of(member_file)?;
            let member = facts.member_by_id(&resolved.member_id?)?;
            (member_file, member.value_syntax?)
        }
        _ => return None,
    };
    let facts = semantic_model.file_facts_of(file_id)?;
    let signature = facts.signature_by_closure(closure_syntax)?;
    let docs = signature.docs.as_ref()?;
    let mut syntaxes: Vec<(rowan::TextSize, emmylua_parser::LuaSyntaxId)> = docs
        .returns
        .iter()
        .map(|syntax| (syntax.get_range().start(), *syntax))
        .chain(
            docs.return_overloads
                .iter()
                .filter(|(name, _)| name.is_some())
                .map(|(_, syntax)| (syntax.get_range().start(), *syntax)),
        )
        .collect();
    if syntaxes.is_empty() {
        return Some(Vec::new());
    }
    syntaxes.sort_by_key(|(start, _)| *start);
    Some(
        syntaxes
            .into_iter()
            .map(|(_, syntax)| project_return_annotation_in(semantic_model, file_id, syntax))
            .collect(),
    )
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
    if let Some(state) = static_condition_state(condition) {
        return state;
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

/// Syntax-level constant folding: `1 == 1` / `true` / `{}` / `function() end`.
/// The VM only gives broad `Boolean` / `Number` types, so comparison results must be computed from literals.
fn static_condition_state(expr: &LuaExpr) -> Option<ConditionState> {
    match expr {
        LuaExpr::LiteralExpr(literal) => match literal.get_literal() {
            Some(emmylua_parser::LuaLiteralToken::Nil(_)) => Some(ConditionState::Falsy),
            Some(emmylua_parser::LuaLiteralToken::Bool(token)) => Some(if token.is_true() {
                ConditionState::Truthy
            } else {
                ConditionState::Falsy
            }),
            Some(emmylua_parser::LuaLiteralToken::String(_))
            | Some(emmylua_parser::LuaLiteralToken::Number(_)) => Some(ConditionState::Truthy),
            _ => None,
        },
        LuaExpr::TableExpr(_) | LuaExpr::ClosureExpr(_) => Some(ConditionState::Truthy),
        LuaExpr::ParenExpr(paren) => paren
            .get_expr()
            .and_then(|inner| static_condition_state(&inner)),
        LuaExpr::UnaryExpr(unary) => {
            if unary
                .get_op_token()
                .is_some_and(|op| op.get_op() == emmylua_parser::UnaryOperator::OpNot)
            {
                unary
                    .get_expr()
                    .and_then(|inner| static_condition_state(&inner).map(not_condition_state))
            } else {
                None
            }
        }
        LuaExpr::BinaryExpr(binary) => {
            let (left, right) = binary.get_exprs()?;
            let op = binary.get_op_token()?.get_op();
            match op {
                BinaryOperator::OpAnd => match static_condition_state(&left) {
                    Some(ConditionState::Falsy) => Some(ConditionState::Falsy),
                    Some(ConditionState::Truthy) => static_condition_state(&right),
                    _ => None,
                },
                BinaryOperator::OpOr => match static_condition_state(&left) {
                    Some(ConditionState::Truthy) => Some(ConditionState::Truthy),
                    Some(ConditionState::Falsy) => static_condition_state(&right),
                    _ => None,
                },
                BinaryOperator::OpEq | BinaryOperator::OpNe => {
                    let (left_value, right_value) = (static_value(&left)?, static_value(&right)?);
                    let equal = left_value == right_value;
                    let truthy = if op == BinaryOperator::OpEq {
                        equal
                    } else {
                        !equal
                    };
                    Some(if truthy {
                        ConditionState::Truthy
                    } else {
                        ConditionState::Falsy
                    })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn not_condition_state(state: ConditionState) -> ConditionState {
    match state {
        ConditionState::Truthy => ConditionState::Falsy,
        ConditionState::Falsy => ConditionState::Truthy,
        other => other,
    }
}

#[derive(Debug, Clone, PartialEq)]
enum StaticValue {
    Nil,
    Bool(bool),
    Number(f64),
    String(String),
}

fn static_value(expr: &LuaExpr) -> Option<StaticValue> {
    match expr {
        LuaExpr::LiteralExpr(literal) => match literal.get_literal() {
            Some(emmylua_parser::LuaLiteralToken::Nil(_)) => Some(StaticValue::Nil),
            Some(emmylua_parser::LuaLiteralToken::Bool(token)) => {
                Some(StaticValue::Bool(token.is_true()))
            }
            Some(emmylua_parser::LuaLiteralToken::String(token)) => {
                Some(StaticValue::String(token.get_value()))
            }
            Some(emmylua_parser::LuaLiteralToken::Number(token)) => {
                match token.get_number_value() {
                    emmylua_parser::NumberResult::Int(i) => Some(StaticValue::Number(i as f64)),
                    emmylua_parser::NumberResult::Uint(u) => Some(StaticValue::Number(u as f64)),
                    emmylua_parser::NumberResult::Float(f) => Some(StaticValue::Number(f)),
                    emmylua_parser::NumberResult::Number => None,
                }
            }
            _ => None,
        },
        LuaExpr::ParenExpr(paren) => paren.get_expr().and_then(|inner| static_value(&inner)),
        _ => None,
    }
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
