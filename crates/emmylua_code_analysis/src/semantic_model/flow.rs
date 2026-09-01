//! # flow — flow-sensitive type queries (consumes FlowTree)
//!
//! M0: flow-aware declaration/member type queries:
//! From the CFG node of the statement at `offset`, backtrack along antecedents;
//! take the RHS type of the target's most recent assignment, or the declared initial type.
//! At branch merges, take the union. Guards and `---@cast +T` on the backtracked path are
//! collected into a path state (PathState). After hitting a base type, apply them in
//! "far → near" order: casts widen first, guards then narrow (the most recent replacement guard wins).
//!
//! Supports:
//! - Narrowing via `type(x) == 'string'` / `x == literal` / `x ~= nil` / bare `x` / `not x`;
//! - `---@cast x +string` adds string to x's path type;
//! - Flow type for member assignment `t.x = v` / `self.x = v` (FlowEffect::AssignMember).

use std::collections::HashSet;
use std::sync::Arc;

use emmylua_parser::{
    BinaryOperator, LuaAssignStat, LuaAstNode, LuaCallExprStat, LuaDocTagAs, LuaExpr,
    LuaLiteralToken, UnaryOperator,
};
use rowan::TextSize;

use crate::salsa_builder::def::SemanticId;
use crate::salsa_builder::flow::{FlowAntecedent, FlowEffect, FlowId, FlowNodeKind, FlowTree};
use crate::{FileId, LuaMemberKey, LuaType};

use super::SemanticModel;

/// Backtracking options: independently collect condition guards, cast widening, and assignment hits.
#[derive(Clone, Copy)]
struct TraceOptions {
    guards: bool,
    casts: bool,
    assignments: bool,
}

impl TraceOptions {
    const FLOW_READ: Self = Self {
        guards: true,
        casts: true,
        assignments: true,
    };
    /// Assignment target: only the declaration type plus path casts, ignoring this and earlier assignments and branch narrowing.
    const ASSIGN_TARGET: Self = Self {
        guards: false,
        casts: true,
        assignments: false,
    };
}

/// Backtracking mode: point queries (within a path) versus branch-merge contributions (unreachable branches do not merge).
#[derive(Clone, Copy, PartialEq, Eq)]
enum TraceMode {
    Point,
    MergeBranch,
}

/// Flow-sensitive type of `decl` at `offset` (assignment-aware + condition narrowing; `Unknown` falls back to the declared type).
///
/// When `offset` lies on the assignment flow node for `x = value`, **this assignment does not participate in the target type** —
/// the query answers "what x was before the assignment" (`---@cast x +T` / narrowing still applies).
pub fn type_of_decl_at(model: &SemanticModel, decl: &SemanticId, offset: TextSize) -> LuaType {
    let fallback = || model.type_of_decl(decl).unwrap_or(LuaType::Unknown);
    let Some(tree) = model.flow_tree() else {
        return fallback();
    };
    let Some(flow_id) = tree.get_flow_id_at(offset) else {
        return fallback();
    };
    let mut visited = HashSet::new();
    let mut path = PathState::default();
    let start = skip_own_decl_assign(decl, &tree, flow_id, offset);
    trace_decl(
        model,
        decl,
        &tree,
        start,
        TraceOptions::FLOW_READ,
        TraceMode::Point,
        &mut visited,
        &mut path,
    )
    .unwrap_or_else(fallback)
}

/// Flow-sensitive type of `decl` at a concrete CFG start node.
#[allow(dead_code)] // Public flow API; retained for external/advanced callers.
pub fn type_of_decl_at_flow_id(model: &SemanticModel, decl: &SemanticId, start: FlowId) -> LuaType {
    let fallback = || model.type_of_decl(decl).unwrap_or(LuaType::Unknown);
    let Some(tree) = model.flow_tree() else {
        return fallback();
    };
    let mut visited = HashSet::new();
    let mut path = PathState::default();
    trace_decl(
        model,
        decl,
        &tree,
        start,
        TraceOptions::FLOW_READ,
        TraceMode::Point,
        &mut visited,
        &mut path,
    )
    .unwrap_or_else(fallback)
}

/// Flow-sensitive type of `member` at a concrete CFG start node.
#[allow(dead_code)] // Public flow API; retained for external/advanced callers.
pub fn type_of_member_at_flow_id(
    model: &SemanticModel,
    member: &SemanticId,
    start: FlowId,
) -> LuaType {
    let fallback = || flow_member_value_type(model, member).unwrap_or(LuaType::Unknown);
    let Some(tree) = model.flow_tree() else {
        return fallback();
    };
    let mut visited = HashSet::new();
    let mut path = PathState::default();
    trace_member(model, member, &tree, start, &mut visited, &mut path).unwrap_or_else(fallback)
}

/// Target type for assignment checks: apply `---@cast x +T` and exclude this assignment, but **do not apply branch narrowing** —
/// the declared type is the assignment target's contract, so inside an `x == 1` branch `x = "a"` is still accepted as declared `string|number`.
pub fn type_of_decl_assign_target_at(
    model: &SemanticModel,
    decl: &SemanticId,
    offset: TextSize,
) -> LuaType {
    let fallback = || model.type_of_decl(decl).unwrap_or(LuaType::Unknown);
    let Some(tree) = model.flow_tree() else {
        return fallback();
    };
    let Some(flow_id) = tree.get_flow_id_at(offset) else {
        return fallback();
    };
    let mut visited = HashSet::new();
    let mut path = PathState::default();
    let start = skip_own_decl_assign(decl, &tree, flow_id, offset);
    trace_decl(
        model,
        decl,
        &tree,
        start,
        TraceOptions::ASSIGN_TARGET,
        TraceMode::Point,
        &mut visited,
        &mut path,
    )
    .unwrap_or_else(fallback)
}

/// Skip this assignment only when `offset` is inside the `x = value` assignment statement and that node assigns `decl`.
/// Reads in later statements use the previous assignment node as the flow start, so they must not skip it.
pub(crate) fn skip_own_decl_assign(
    decl: &SemanticId,
    tree: &FlowTree,
    flow_id: FlowId,
    offset: TextSize,
) -> FlowId {
    let Some(node) = tree.get_flow_node(flow_id) else {
        return flow_id;
    };
    let FlowNodeKind::Assignment(assign_ptr) = &node.kind else {
        return flow_id;
    };
    if !assign_ptr.get_syntax_id().get_range().contains(offset) {
        return flow_id;
    }
    let has_own = tree.get_flow_effects(flow_id).iter().any(|effect| {
        matches!(effect, FlowEffect::AssignDecl { decl: assigned, .. } if assigned == decl)
    });
    if has_own && let Some(next) = first_antecedent(tree, node.antecedent.as_ref()) {
        return next;
    }
    flow_id
}

/// Flow declaration type for a table-literal field: use `type_of_expr_at` at the field definition position,
/// so `pair.left` retains the branch narrowing that `left` had when the table was constructed.
fn flow_member_value_type(model: &SemanticModel, member: &SemanticId) -> Option<LuaType> {
    let member_file = match member {
        SemanticId::Member(key) => key.file_id,
        _ => model.file_id(),
    };
    let facts = model.file_facts_of(member_file)?;
    let def = facts.member_by_id(member)?;
    if !matches!(def.owner, SemanticId::Member(_)) {
        return model
            .type_of_member(member)
            .map(|ty| apply_field_nullable(model, member, ty));
    }
    let value_syntax = def.value_syntax?;
    let Some(tree) = model.syntax_tree_of(member_file) else {
        return model
            .type_of_member(member)
            .map(|ty| apply_field_nullable(model, member, ty));
    };
    let Some(node) = value_syntax.to_node_from_root(&tree.get_red_root()) else {
        return model
            .type_of_member(member)
            .map(|ty| apply_field_nullable(model, member, ty));
    };
    if emmylua_parser::LuaClosureExpr::cast(node.clone()).is_some() {
        return model
            .type_of_member(member)
            .map(|ty| apply_field_nullable(model, member, ty));
    }
    // Enable flow only when the field value is a name reference: numeric/string literals keep their base-type projection,
    // avoiding `M.y = 1` being changed from Number back to IntegerConst(1).
    let is_name_ref = LuaExpr::cast(node).is_some_and(|expr| matches!(expr, LuaExpr::NameExpr(_)));
    if !is_name_ref {
        return model
            .type_of_member(member)
            .map(|ty| apply_field_nullable(model, member, ty));
    }
    let ty = model.type_of_expr_at(value_syntax, value_syntax.get_range().start());
    if matches!(ty, LuaType::Unknown) {
        model
            .type_of_member(member)
            .map(|ty| apply_field_nullable(model, member, ty))
    } else {
        Some(apply_field_nullable(model, member, ty))
    }
}

/// Nullable members declared with `---@field x? T` should also carry nil in flow queries.
fn apply_field_nullable(model: &SemanticModel, member: &SemanticId, ty: LuaType) -> LuaType {
    let member_file = match member {
        SemanticId::Member(key) => key.file_id,
        _ => model.file_id(),
    };
    if let Some(facts) = model.file_facts_of(member_file)
        && let Some(def) = facts.member_by_id(member)
        && def.is_nullable
        && !ty.is_nullable()
    {
        return merge_types(ty, LuaType::Nil);
    }
    ty
}

/// Flow-sensitive type of the member at `offset` (member assignment flow + `---@cast t.x +T` widening).
/// Same as `type_of_decl_at`: exclude this assignment when `offset` lies on `t.x = value`.
pub fn type_of_member_at(model: &SemanticModel, member: &SemanticId, offset: TextSize) -> LuaType {
    let fallback = || flow_member_value_type(model, member).unwrap_or(LuaType::Unknown);
    let Some(tree) = model.flow_tree() else {
        return fallback();
    };
    let Some(flow_id) = tree.get_flow_id_at(offset) else {
        return fallback();
    };
    let mut visited = HashSet::new();
    let mut path = PathState::default();
    let start = skip_own_member_assign(member, &tree, flow_id, offset);
    trace_member(model, member, &tree, start, &mut visited, &mut path).unwrap_or_else(fallback)
}

/// Skip this assignment only when `offset` is inside the `t.x = value` assignment and that node assigns this member.
pub(crate) fn skip_own_member_assign(
    member: &SemanticId,
    tree: &FlowTree,
    flow_id: FlowId,
    offset: TextSize,
) -> FlowId {
    let Some(node) = tree.get_flow_node(flow_id) else {
        return flow_id;
    };
    let FlowNodeKind::Assignment(assign_ptr) = &node.kind else {
        return flow_id;
    };
    if !assign_ptr.get_syntax_id().get_range().contains(offset) {
        return flow_id;
    }
    let has_own = tree.get_flow_effects(flow_id).iter().any(|effect| {
        matches!(effect, FlowEffect::AssignMember { member: assigned, .. } if assigned == member)
    });
    if has_own && let Some(next) = first_antecedent(tree, node.antecedent.as_ref()) {
        return next;
    }
    flow_id
}

/// Take one branch from the antecedent as the start for skipping this assignment (walk expands merge points).
fn first_antecedent(tree: &FlowTree, antecedent: Option<&FlowAntecedent>) -> Option<FlowId> {
    match antecedent {
        Some(FlowAntecedent::Single(next)) => Some(*next),
        Some(FlowAntecedent::Multiple(multi_id)) => {
            tree.get_multi_antecedents(*multi_id)?.first().copied()
        }
        None => None,
    }
}

/// Flow type of `decl` **before the flow node at `offset`**: used when checking `x = value` to get the pre-assignment target type
/// (apply `---@cast x +T` / branch narrowing first, then validate the RHS of this assignment).
pub fn type_of_decl_before_at(
    model: &SemanticModel,
    decl: &SemanticId,
    offset: TextSize,
) -> LuaType {
    let fallback = || model.type_of_decl(decl).unwrap_or(LuaType::Unknown);
    let Some(tree) = model.flow_tree() else {
        return fallback();
    };
    let Some(flow_id) = tree.get_flow_id_at(offset) else {
        return fallback();
    };
    let Some(node) = tree.get_flow_node(flow_id) else {
        return fallback();
    };
    let mut visited = HashSet::new();
    let mut path = PathState::default();
    walk_decl(
        model,
        decl,
        &tree,
        node.antecedent.as_ref(),
        TraceOptions::FLOW_READ,
        TraceMode::Point,
        &mut visited,
        &mut path,
    )
    .unwrap_or_else(fallback)
}

/// Flow type of the member **before the flow node at `offset`** (for assignment checks: apply `---@cast t.x +T` first).
pub fn type_of_member_before_at(
    model: &SemanticModel,
    member: &SemanticId,
    offset: TextSize,
) -> LuaType {
    let fallback = || flow_member_value_type(model, member).unwrap_or(LuaType::Unknown);
    let Some(tree) = model.flow_tree() else {
        return fallback();
    };
    let Some(flow_id) = tree.get_flow_id_at(offset) else {
        return fallback();
    };
    let Some(node) = tree.get_flow_node(flow_id) else {
        return fallback();
    };
    let mut visited = HashSet::new();
    let mut path = PathState::default();
    walk_member(
        model,
        member,
        &tree,
        node.antecedent.as_ref(),
        &mut visited,
        &mut path,
    )
    .unwrap_or_else(fallback)
}

/// Flow-sensitive type of an expression at `offset`: NameExpr → decl flow type; IndexExpr → member flow type;
/// other expressions fall back to ordinary inference.
pub fn type_of_expr_at(
    model: &SemanticModel,
    expr_syntax: emmylua_parser::LuaSyntaxId,
    offset: TextSize,
) -> LuaType {
    let Some(tree) = model.syntax_tree() else {
        return model.type_of_expr(expr_syntax);
    };
    let Some(node) = expr_syntax.to_node_from_root(&tree.get_red_root()) else {
        return model.type_of_expr(expr_syntax);
    };
    let Some(expr) = LuaExpr::cast(node) else {
        return model.type_of_expr(expr_syntax);
    };
    match &expr {
        LuaExpr::NameExpr(name_expr) => {
            if let Some(decl) = model.resolve_name(name_expr.get_position()) {
                return model.type_of_decl_at(&decl, offset);
            }
            model.type_of_expr(expr_syntax)
        }
        LuaExpr::IndexExpr(index_expr) => {
            if let Some(resolved) = model.resolve_member(index_expr)
                && let Some(member_id) = resolved.member_id
            {
                let mut member_ty = type_of_member_at(model, &member_id, offset);
                // Old semantics use `LuaType::Signature` to represent the closure member's definition site;
                // flow-sensitive member values also keep that identity, matching the VM's member access path.
                if matches!(member_ty, LuaType::Function)
                    && let Some(member_file) = match &member_id {
                        SemanticId::Member(key) => Some(key.file_id),
                        _ => None,
                    }
                    && let Some(facts) = model.file_facts_of(member_file)
                    && let Some(member_def) = facts.member_by_id(&member_id)
                    && let Some(value_syntax) = member_def.value_syntax
                    && let Some(tree) = model.syntax_tree_of(member_file)
                    && let Some(node) = value_syntax.to_node_from_root(&tree.get_red_root())
                    && let Some(closure) = emmylua_parser::LuaClosureExpr::cast(node)
                {
                    member_ty = LuaType::Signature(crate::signature::LuaSignatureId::from_closure(
                        member_file,
                        &closure,
                    ));
                }
                let resolved_is_runtime = model
                    .file_facts_of(match &member_id {
                        SemanticId::Member(key) => key.file_id,
                        _ => model.file_id(),
                    })
                    .is_some_and(|facts| {
                        facts
                            .members_of_owner(&resolved.owner)
                            .any(|m| m.id == member_id)
                    });
                // Runtime member assignment flow on local variables (including method `self`) takes precedence over class declaration fields;
                // if `resolve_member` already selected a specific runtime member (chosen by annotation/position among same-name assignments),
                // don't re-select the first same-name member, avoiding an annotated member falling back to an earlier assignment.
                // For declared named/generic types (`local bar: foo<{a:string}> = { a = "test" }`),
                // even when `resolve_member` lands on a table field, use the declared type's member as authoritative.
                if resolved_is_runtime
                    && let Some(key) = member_key_from_index_expr(model, index_expr)
                    && let Some(prefix) = index_expr.get_prefix_expr()
                    && let LuaExpr::NameExpr(_) = &prefix
                {
                    let prefix_ty = type_of_expr_at(model, prefix.get_syntax_id(), offset);
                    if matches!(
                        &prefix_ty,
                        LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_)
                    ) && let Some(prefix_member_ty) = model.member_type(&prefix_ty, &key)
                        && !matches!(prefix_member_ty, LuaType::Unknown)
                    {
                        return apply_expr_casts(model, expr_syntax, prefix_member_ty);
                    }
                }
                // Computed-key fields in table literals (`[key] = 1`) keep literal values in type queries:
                // when a dynamic index resolves to a table initializer field, return that member type directly (including literals).
                if resolved_is_runtime
                    && let Some(prefix) = index_expr.get_prefix_expr()
                    && let Some(emmylua_parser::LuaIndexKey::Expr(_)) = index_expr.get_index_key()
                    && let Some(member_file) = match &member_id {
                        SemanticId::Member(key) => Some(key.file_id),
                        _ => None,
                    }
                    && let Some(member_facts) = model.file_facts_of(member_file)
                    && let Some(member_def) = member_facts.member_by_id(&member_id)
                    && model.is_initializer_table_field(&member_id, member_def)
                {
                    let prefix_ty = type_of_expr_at(model, prefix.get_syntax_id(), offset);
                    if let Some(value_syntax) = member_def.value_syntax {
                        let expr_ty = model.type_of_expr(value_syntax);
                        match expr_ty {
                            // Dynamic access to a computed key `[key] = 1` can return `integer`;
                            // named fields still keep `number` through the existing projection.
                            LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => {
                                return apply_expr_casts(model, expr_syntax, LuaType::Integer);
                            }
                            LuaType::StringConst(_) | LuaType::DocStringConst(_) => {
                                return apply_expr_casts(model, expr_syntax, expr_ty);
                            }
                            _ => {}
                        }
                    }
                    if let Some(ty) = model.member_type(&prefix_ty, &member_def.key) {
                        return apply_expr_casts(model, expr_syntax, ty);
                    }
                }
                if let SemanticId::Decl(owner_decl) = &resolved.owner
                    && let Some(facts) = model.file_facts_of(owner_decl.file_id)
                {
                    if !resolved_is_runtime
                        && let Some(runtime_member) = facts
                            .members_of_owner(&resolved.owner)
                            .find(|m| m.key.name() == Some(resolved.name.as_str()))
                        // Table fields from a declared initializer are not "runtime overrides": `local b: B = { field = 1 }`
                        // field access is still governed by the class `@field` type, avoiding widening `integer` to `number`.
                        && !model.is_initializer_table_field(&runtime_member.id, runtime_member)
                    {
                        let runtime_ty = type_of_member_at(model, &runtime_member.id, offset);
                        if !matches!(runtime_ty, LuaType::Unknown) {
                            return apply_expr_casts(model, expr_syntax, runtime_ty);
                        }
                    }
                }
                // If the prefix was narrowed to a more specific instance/object type on the same flow path,
                // prefer the member type projected from that type (e.g. after `a = make()`, `a.a` uses the object field).
                if let Some(key) = member_key_from_index_expr(model, index_expr)
                    && let Some(prefix) = index_expr.get_prefix_expr()
                {
                    let prefix_ty = type_of_expr_at(model, prefix.get_syntax_id(), offset);
                    if let Some(prefix_member_ty) = model.member_type(&prefix_ty, &key)
                        && !matches!(prefix_member_ty, LuaType::Unknown | LuaType::Any)
                        && (member_type_more_specific(model, &prefix_member_ty, &member_ty)
                            || matches!(&prefix_ty, LuaType::Generic(_)))
                    {
                        return apply_expr_casts(model, expr_syntax, prefix_member_ty);
                    }
                }
                return member_ty;
            }
            // Inside a `#d == n` branch, `d[1]` no longer carries nil when the array length is known.
            if let Some(known_ty) = array_index_with_known_len(model, &index_expr, offset) {
                return apply_expr_casts(model, expr_syntax, known_ty);
            }
            apply_expr_casts(model, expr_syntax, model.type_of_expr(expr_syntax))
        }
        LuaExpr::CallExpr(_) | LuaExpr::TableExpr(_) | LuaExpr::LiteralExpr(_) => {
            apply_expr_casts(model, expr_syntax, model.type_of_expr(expr_syntax))
        }
        LuaExpr::ClosureExpr(closure_expr) => {
            // Anonymous closure keeps its actual signature on the flow path, avoiding the assigned function value degrading to bare `Function`.
            let base = model
                .type_of_signature(closure_expr.get_syntax_id())
                .map(|fun| LuaType::DocFunction(Arc::new(fun)))
                .unwrap_or_else(|| model.type_of_expr(expr_syntax));
            apply_expr_casts(model, expr_syntax, base)
        }
        LuaExpr::BinaryExpr(binary_expr) => {
            let Some(op_token) = binary_expr.get_op_token() else {
                return apply_expr_casts(model, expr_syntax, model.type_of_expr(expr_syntax));
            };
            let Some((left_expr, right_expr)) = binary_expr.get_exprs() else {
                return apply_expr_casts(model, expr_syntax, model.type_of_expr(expr_syntax));
            };
            // Preserve the legacy semantics that "array table literals in short-circuit expressions are tuple":
            // only when the right side is a sequential array table and the left side is a logical short-circuit, replace the right table type with tuple.
            if matches!(
                op_token.get_op(),
                BinaryOperator::OpAnd | BinaryOperator::OpOr
            ) && let Some(tuple_ty) = array_table_literal_tuple_type(model, &right_expr)
            {
                let left_ty = type_of_expr_at(model, left_expr.get_syntax_id(), offset);
                let result = logical_binary_type(model, op_token.get_op(), left_ty, tuple_ty);
                return apply_expr_casts(model, expr_syntax, result);
            }
            // General binary expressions: subexpressions first use flow-sensitive types (member/variable branch narrowing),
            // then merge with the same VM operator rules, ensuring `right.value + 1` uses the narrow type of `right.value` in the branch.
            let left_ty = type_of_expr_at(model, left_expr.get_syntax_id(), offset);
            let right_ty = type_of_expr_at(model, right_expr.get_syntax_id(), offset);
            // If either operand in an arithmetic expression fails to infer (unresolved global/field), treat it as an "inference error":
            // don't return the VM's fallback number; let the assignment layer keep the variable's previously known type.
            if matches!(
                op_token.get_op(),
                BinaryOperator::OpAdd
                    | BinaryOperator::OpSub
                    | BinaryOperator::OpMul
                    | BinaryOperator::OpDiv
                    | BinaryOperator::OpIDiv
                    | BinaryOperator::OpMod
                    | BinaryOperator::OpPow
            ) && (matches!(&left_ty, LuaType::Unknown | LuaType::Any)
                || matches!(&right_ty, LuaType::Unknown | LuaType::Any))
            {
                return apply_expr_casts(model, expr_syntax, LuaType::Unknown);
            }
            let result = crate::semantic_model::infer::vm::binary_type(
                model,
                op_token.get_op(),
                &left_ty,
                &right_ty,
            );
            apply_expr_casts(model, expr_syntax, result)
        }
        _ => model.type_of_expr(expr_syntax),
    }
}

/// Type query that additionally applies inline casts on the expression beyond the flow-sensitive branch.
/// Used for contexts like closure return inference that need "bare VM type + `---@as`", avoiding changing
/// the existing behavior of `type_of_expr_at` for ordinary expressions.
pub(crate) fn type_of_expr_with_cast(
    model: &SemanticModel,
    expr_syntax: emmylua_parser::LuaSyntaxId,
) -> LuaType {
    apply_expr_casts(model, expr_syntax, model.type_of_expr(expr_syntax))
}

/// Simple array table literal → `LuaType::Tuple` (legacy `{ 'a' }` stays tuple in short-circuit expressions).
fn array_table_literal_tuple_type(model: &SemanticModel, expr: &LuaExpr) -> Option<LuaType> {
    let LuaExpr::TableExpr(table_expr) = expr else {
        return None;
    };
    let fields: Vec<_> = table_expr.get_fields().collect();
    if fields.is_empty() {
        return None;
    }
    let mut types = Vec::with_capacity(fields.len());
    for (index, field) in fields.iter().enumerate() {
        if !field.is_value_field() {
            return None;
        }
        match field.get_field_key()? {
            emmylua_parser::LuaIndexKey::Idx(idx) if idx == index + 1 => {}
            _ => return None,
        }
        types.push(model.type_of_expr(field.get_value_expr()?.get_syntax_id()));
    }
    Some(LuaType::Tuple(Arc::new(crate::LuaTupleType::new(
        types,
        crate::LuaTupleStatus::InferResolve,
    ))))
}

/// Short-circuit binary expression type (same semantics as VM `binary_type`, used for tuple-table replacement scenarios).
fn logical_binary_type(
    _model: &SemanticModel,
    op: BinaryOperator,
    left: LuaType,
    right: LuaType,
) -> LuaType {
    match op {
        BinaryOperator::OpAnd => {
            if matches!(left, LuaType::Unknown) {
                LuaType::Nil
            } else if left.is_always_falsy() {
                left
            } else if left.is_always_truthy() {
                right
            } else {
                let mut types = logical_falsy_components(&left);
                types.push(right);
                LuaType::from_vec(types)
            }
        }
        BinaryOperator::OpOr => {
            if left.is_always_truthy() {
                left
            } else if left.is_always_falsy() {
                right
            } else {
                let mut types = logical_truthy_components(&left);
                types.push(right);
                LuaType::from_vec(types)
            }
        }
        _ => right,
    }
}

fn logical_falsy_components(ty: &LuaType) -> Vec<LuaType> {
    match ty {
        LuaType::Union(union) => union
            .into_vec()
            .into_iter()
            .flat_map(|ty| logical_falsy_components(&ty))
            .collect(),
        LuaType::Boolean => vec![LuaType::BooleanConst(false)],
        LuaType::BooleanConst(is_true) => {
            if *is_true {
                Vec::new()
            } else {
                vec![ty.clone()]
            }
        }
        LuaType::DocBooleanConst(is_true) => {
            if *is_true {
                Vec::new()
            } else {
                vec![ty.clone()]
            }
        }
        LuaType::Nil => vec![ty.clone()],
        LuaType::Unknown | LuaType::Any => vec![ty.clone()],
        _ => Vec::new(),
    }
}

fn logical_truthy_components(ty: &LuaType) -> Vec<LuaType> {
    match ty {
        LuaType::Union(union) => union
            .into_vec()
            .into_iter()
            .flat_map(|ty| logical_truthy_components(&ty))
            .collect(),
        LuaType::Boolean => vec![LuaType::BooleanConst(true)],
        LuaType::BooleanConst(is_true) => {
            if *is_true {
                vec![ty.clone()]
            } else {
                Vec::new()
            }
        }
        LuaType::DocBooleanConst(is_true) => {
            if *is_true {
                vec![ty.clone()]
            } else {
                Vec::new()
            }
        }
        LuaType::Nil => Vec::new(),
        LuaType::Unknown | LuaType::Any => vec![ty.clone()],
        other => vec![other.clone()],
    }
}

fn array_index_in_known_range(index: i64, len: &crate::LuaArrayLen) -> bool {
    match len {
        crate::LuaArrayLen::Max(max) => index > 0 && index <= *max,
        crate::LuaArrayLen::Min(min) => index > 0 && index <= *min,
        crate::LuaArrayLen::None => false,
    }
}

/// If the index is an integer constant and the prefix array already has a known length bound on the flow path, return the in-bounds element type.
/// Also recognizes loop variable indexes like `for i = 1, #arr do arr[i]`.
fn array_index_with_known_len(
    model: &SemanticModel,
    index_expr: &emmylua_parser::LuaIndexExpr,
    offset: TextSize,
) -> Option<LuaType> {
    let prefix = index_expr.get_prefix_expr()?;
    let LuaExpr::NameExpr(name_expr) = prefix else {
        return None;
    };
    let decl = model.resolve_name(name_expr.get_position())?;
    let decl_ty = model.type_of_decl_at(&decl, offset);
    let components: Vec<LuaType> = match decl_ty {
        LuaType::Union(union) => union.into_vec(),
        other => vec![other],
    };

    let index_key = index_expr.get_index_key()?;
    match index_key {
        emmylua_parser::LuaIndexKey::Expr(key_expr) => {
            // Inside a numeric for loop body, `i` is guaranteed to be in `1..#arr`.
            if numeric_for_loop_index(model, &key_expr, &decl) {
                for component in &components {
                    if let LuaType::Array(array) = component {
                        return Some(array.get_base().clone());
                    }
                }
            }
            None
        }
        emmylua_parser::LuaIndexKey::Integer(num) => {
            let index = match num.get_number_value() {
                emmylua_parser::NumberResult::Int(i) => i,
                emmylua_parser::NumberResult::Uint(i) => i as i64,
                _ => return None,
            };
            for component in &components {
                if let LuaType::Array(array) = component
                    && array_index_in_known_range(index, array.get_len())
                {
                    return Some(array.get_base().clone());
                }
            }
            None
        }
        emmylua_parser::LuaIndexKey::Idx(i) => {
            let index = i as i64;
            for component in &components {
                if let LuaType::Array(array) = component
                    && array_index_in_known_range(index, array.get_len())
                {
                    return Some(array.get_base().clone());
                }
            }
            None
        }
        _ => None,
    }
}

/// Returns whether a dynamic index expression is a numeric for-loop variable bounded by `#prefix`.
fn numeric_for_loop_index(
    model: &SemanticModel,
    key_expr: &LuaExpr,
    prefix_decl: &SemanticId,
) -> bool {
    let LuaExpr::NameExpr(name_expr) = key_expr else {
        return false;
    };
    let Some(tree) = model.syntax_tree() else {
        return false;
    };
    let root = tree.get_red_root();
    let Some(node) = name_expr.get_syntax_id().to_node_from_root(&root) else {
        return false;
    };
    let Some(for_stat) = node.ancestors().find_map(emmylua_parser::LuaForStat::cast) else {
        return false;
    };
    let Some(var_token) = for_stat.get_var_name() else {
        return false;
    };
    if var_token.get_name_text() != name_expr.get_name_text().as_deref().unwrap_or_default() {
        return false;
    }
    let iter_exprs: Vec<LuaExpr> = for_stat.get_iter_expr().collect();
    if iter_exprs.len() < 2 {
        return false;
    }
    iter_exprs.iter().any(|expr| {
        expr.descendants::<emmylua_parser::LuaUnaryExpr>()
            .any(|unary| {
                unary
                    .get_op_token()
                    .is_some_and(|op| op.get_op() == UnaryOperator::OpLen)
                    && unary
                        .get_expr()
                        .and_then(|inner| match inner {
                            LuaExpr::NameExpr(n) => model.resolve_name(n.get_position()),
                            _ => None,
                        })
                        .as_ref()
                        == Some(prefix_decl)
            })
    })
}

/// Convert a condition pointer back to an expression (for branch reachability checks).
fn flow_condition_expr(
    model: &SemanticModel,
    cond_ptr: &emmylua_parser::LuaAstPtr<LuaExpr>,
) -> Option<LuaExpr> {
    let chunk = model.chunk()?;
    LuaExpr::cast(cond_ptr.to_node(&chunk)?.syntax().clone())
}

/// Whether a `CallExprStat` is a call statement returning `never`.
/// Used to remove branches that cannot continue when backtracking at branch merges.
fn call_stat_returns_never(
    model: &SemanticModel,
    call_stat_ptr: &emmylua_parser::LuaAstPtr<LuaCallExprStat>,
) -> bool {
    let Some(chunk) = model.chunk() else {
        return false;
    };
    let Some(call_stat) = call_stat_ptr.to_node(&chunk) else {
        return false;
    };
    let Some(call_expr) = call_stat.get_call_expr() else {
        return false;
    };
    // Prefer signature projection; do not fall back to the VM when there is no signature. For many ordinary calls in large linear fragments,
    // VM evaluation repeatedly recurses into member reads in table arguments, causing nested flow backtracks.
    if let Some(ty) = call_signature_return(model, &call_expr) {
        return ty.is_never();
    }
    false
}

/// Whether a condition branch is unreachable: the call returns `never`, or a `false`/`true` constant makes one side unreachable.
fn condition_branch_unreachable(
    model: &SemanticModel,
    cond: &LuaExpr,
    branch_is_true: bool,
) -> bool {
    match cond {
        LuaExpr::ParenExpr(paren) => paren
            .get_expr()
            .is_some_and(|inner| condition_branch_unreachable(model, &inner, branch_is_true)),
        LuaExpr::UnaryExpr(unary) => {
            let Some(op) = unary.get_op_token() else {
                return false;
            };
            if op.get_op() != UnaryOperator::OpNot {
                return false;
            }
            unary
                .get_expr()
                .is_some_and(|inner| condition_branch_unreachable(model, &inner, !branch_is_true))
        }
        LuaExpr::CallExpr(call) => {
            // Prefer signature projection. Do not fall back to the VM without a signature: VM evaluation of ordinary calls (e.g. `enabled(...)`)
            // repeatedly triggers function-body/argument inference, causing nested backtracking on large linear CFGs.
            let Some(call_ty) = call_signature_return(model, call) else {
                return false;
            };
            if call_ty.is_never() {
                return true;
            }
            if branch_is_true {
                call_ty.is_always_falsy()
            } else {
                call_ty.is_always_truthy()
            }
        }
        _ => false,
    }
}

/// Whether the path before an assignment node is reachable (used to drop assignments in unreachable branches).
/// Note: this only checks control-flow reachability; it does not treat "the variable type itself is never" as unreachable.
fn assignment_antecedent_reachable(
    model: &SemanticModel,
    tree: &FlowTree,
    antecedent: Option<&FlowAntecedent>,
) -> bool {
    let mut reach_visited = HashSet::new();
    branch_path_reachable(model, tree, antecedent, &mut reach_visited)
}

/// Pure control-flow reachability traversal: only decides whether a CFG path can reach the assignment node.
/// Uses an explicit work stack to avoid deep CFG chains consuming the native call stack.
fn branch_path_reachable(
    model: &SemanticModel,
    tree: &FlowTree,
    start: Option<&FlowAntecedent>,
    visited: &mut HashSet<FlowId>,
) -> bool {
    let mut stack = vec![start.cloned()];
    while let Some(antecedent) = stack.pop() {
        match antecedent {
            None => return true,
            Some(FlowAntecedent::Single(flow_id)) => {
                if !visited.insert(flow_id) {
                    // Loop path: conservatively treat as reachable.
                    return true;
                }
                let Some(node) = tree.get_flow_node(flow_id) else {
                    return false;
                };
                match &node.kind {
                    FlowNodeKind::Unreachable
                    | FlowNodeKind::Return
                    | FlowNodeKind::Break
                    | FlowNodeKind::Continue => continue,
                    FlowNodeKind::TrueCondition(cond_ptr) => {
                        if let Some(cond) = flow_condition_expr(model, cond_ptr)
                            && condition_branch_unreachable(model, &cond, true)
                        {
                            continue;
                        }
                    }
                    FlowNodeKind::FalseCondition(cond_ptr) => {
                        if let Some(cond) = flow_condition_expr(model, cond_ptr)
                            && condition_branch_unreachable(model, &cond, false)
                        {
                            continue;
                        }
                    }
                    FlowNodeKind::CallExprStat(call_stat_ptr) => {
                        if call_stat_returns_never(model, call_stat_ptr) {
                            continue;
                        }
                    }
                    _ => {}
                }
                match &node.antecedent {
                    None => return true,
                    Some(next) => stack.push(Some(next.clone())),
                }
            }
            Some(FlowAntecedent::Multiple(multi_id)) => {
                let Some(branches) = tree.get_multi_antecedents(multi_id) else {
                    return false;
                };
                for branch in branches {
                    stack.push(Some(FlowAntecedent::Single(*branch)));
                }
            }
        }
    }
    false
}

/// Apply inline casts bound to this expression (e.g. `A:get(1) --[[@cast -?]]`).
/// `+T` widens, `-T` (including `-?`) removes, and no operator replaces.
fn apply_expr_casts(
    model: &SemanticModel,
    expr_syntax: emmylua_parser::LuaSyntaxId,
    base: LuaType,
) -> LuaType {
    let Some(tree) = model.flow_tree() else {
        return base;
    };
    // Inline casts may be bound to the expression itself or an ancestor node (e.g. an assignment statement),
    // so check both the expression and all ancestors' flow bindings.
    let mut flow_ids = Vec::new();
    if let Some(flow_id) = tree.get_flow_id(expr_syntax) {
        flow_ids.push(flow_id);
    }
    if let Some(syntax_tree) = model.syntax_tree()
        && let Some(node) = expr_syntax.to_node_from_root(&syntax_tree.get_red_root())
    {
        for ancestor in node.ancestors() {
            let ancestor_syntax = emmylua_parser::LuaSyntaxId::from_node(&ancestor);
            if let Some(flow_id) = tree.get_flow_id(ancestor_syntax)
                && !flow_ids.contains(&flow_id)
            {
                flow_ids.push(flow_id);
            }
        }
    }
    let mut ty = base;
    for flow_id in flow_ids {
        for effect in tree.get_flow_effects(flow_id) {
            match effect {
                FlowEffect::TagCast(cast_ptr) => {
                    let Some(chunk) = model.chunk() else {
                        continue;
                    };
                    let Some(cast) = cast_ptr.to_node(&chunk) else {
                        continue;
                    };
                    for op_type in cast.get_op_types() {
                        let Some(doc_type) = op_type.get_type() else {
                            // `-?`: ? is not a DocType node, only the subtraction operator → remove nil.
                            if op_type
                                .get_op()
                                .is_some_and(|op| op.get_op() == BinaryOperator::OpSub)
                            {
                                ty = remove_type(ty, &LuaType::Nil);
                            }
                            continue;
                        };
                        let target = if doc_type.get_text().trim() == "?" {
                            LuaType::Nil
                        } else {
                            model.doc_type_lua(doc_type.get_syntax_id())
                        };
                        match op_type.get_op().map(|op| op.get_op()) {
                            None => ty = target,
                            Some(BinaryOperator::OpAdd) => ty = merge_types(ty, target),
                            Some(BinaryOperator::OpSub) => ty = remove_type(ty, &target),
                            _ => {}
                        }
                    }
                }
                FlowEffect::AsCast(as_ptr) => {
                    let Some(chunk) = model.chunk() else {
                        continue;
                    };
                    let Some(as_tag) = as_ptr.to_node(&chunk) else {
                        continue;
                    };
                    if let Some(doc_type) = as_tag.get_type() {
                        ty = model.doc_type_lua(doc_type.get_syntax_id());
                    }
                }
                _ => {}
            }
        }
    }
    ty
}

/// `---@cast` path operation: widen / remove / replace.
#[derive(Clone)]
enum CastOp {
    Add(LuaType),
    Remove(LuaType),
    Replace(LuaType),
}

/// Guards and cast operations collected along the backtracking path.
#[derive(Default)]
struct PathState {
    /// Most recent guard first.
    narrowings: Vec<Narrowing>,
    /// Most recent cast operation first.
    casts: Vec<CastOp>,
}

/// Checks from an antecedent toward the declaration whether a narrowing node already exists.
/// Used for `self` return_cast: only allow it if the receiver was first narrowed by another condition/`@cast`.
/// Uses an explicit work stack; Multiple requires all branches to satisfy the condition.
fn antecedent_has_narrowing(tree: &FlowTree, start: Option<FlowAntecedent>) -> bool {
    let mut stack: Vec<(Option<FlowAntecedent>, HashSet<FlowId>)> = vec![(start, HashSet::new())];
    while let Some((antecedent, mut visited)) = stack.pop() {
        match antecedent {
            None => return false,
            Some(FlowAntecedent::Single(flow_id)) => {
                if !visited.insert(flow_id) {
                    return false;
                }
                let Some(node) = tree.get_flow_node(flow_id) else {
                    return false;
                };
                match &node.kind {
                    FlowNodeKind::TrueCondition(_)
                    | FlowNodeKind::FalseCondition(_)
                    | FlowNodeKind::TagCast(_)
                    | FlowNodeKind::AsCast(_) => {
                        // This path already hit a narrowing/cast, so the condition is satisfied.
                    }
                    FlowNodeKind::DeclPosition(_)
                    | FlowNodeKind::Start
                    | FlowNodeKind::Unreachable => return false,
                    _ => {
                        if let Some(next) = node.antecedent.clone() {
                            stack.push((Some(next), visited));
                        } else {
                            return false;
                        }
                    }
                }
            }
            Some(FlowAntecedent::Multiple(multi_id)) => {
                let Some(branches) = tree.get_multi_antecedents(multi_id) else {
                    return false;
                };
                for branch in branches {
                    stack.push((Some(FlowAntecedent::Single(*branch)), HashSet::new()));
                }
            }
        }
    }
    true
}

/// Declaration assignment frame on the explicit stack: continue backtracking the antecedent first to get the "pre-assignment type", then compute the assignment result.
struct DeclAssignmentFrame {
    /// Single antecedent of the current assignment node.
    antecedent: FlowAntecedent,
    /// Current assignment flow node id.
    flow_id: FlowId,
    assign_pos: TextSize,
    value_syntax: emmylua_parser::LuaSyntaxId,
    /// Path state after the assignment (closer to the query on the backtracking path).
    assign_path: PathState,
    base_before: LuaType,
}

/// After backtracking reaches the start, expand the assignment frames on the explicit stack in earliest-to-latest order.
fn finish_decl_frames(
    model: &SemanticModel,
    decl: &SemanticId,
    tree: &FlowTree,
    mut result: LuaType,
    frames: Vec<DeclAssignmentFrame>,
    mode: TraceMode,
) -> Option<LuaType> {
    let mut frames = frames;
    while let Some(frame) = frames.pop() {
        let narrowed_before = result;
        let value_ty = assigned_value_type(
            model,
            tree,
            decl,
            frame.flow_id,
            frame.assign_pos,
            frame.value_syntax,
            &narrowed_before,
        );
        let value_ty = coerce_table_assign_to_narrowed(
            model,
            decl,
            value_ty,
            &narrowed_before,
            &frame.base_before,
        );
        let value_ty = if value_ty.is_unknown()
            && !matches!(narrowed_before, LuaType::Unknown | LuaType::Any)
        {
            widen_const_type(narrowed_before.clone())
        } else {
            value_ty
        };
        let assign_result = finalize(model, value_ty, &frame.assign_path);
        // When branch-merge backtracking reaches an assignment node, the assignment may itself be in an unreachable branch
        // (e.g. `if always_false() then x = 1 end`). In that case the assignment does not participate in merging;
        // but the "assignable" fact about the variable should still be kept, so widen the previous literal to its base type,
        // avoiding retaining a precise literal that only belongs to the discarded branch after the merge.
        if mode == TraceMode::MergeBranch
            && !assignment_antecedent_reachable(model, tree, Some(&frame.antecedent))
        {
            if narrowed_before.is_never() {
                return Some(LuaType::Never);
            }
            result = widen_const_type(narrowed_before);
        } else {
            result = assign_result;
        }
    }
    Some(result)
}

/// Backtrack along the CFG for decl's most recent assignment type. With `options` disabling guards/assignments, the path keeps only declaration + casts.
fn trace_decl(
    model: &SemanticModel,
    decl: &SemanticId,
    tree: &FlowTree,
    flow_id: FlowId,
    options: TraceOptions,
    mut mode: TraceMode,
    visited: &mut HashSet<FlowId>,
    path: &mut PathState,
) -> Option<LuaType> {
    let root_mode = mode;
    let mut current = flow_id;
    let mut frames: Vec<DeclAssignmentFrame> = Vec::new();
    loop {
        if !visited.insert(current) {
            return None;
        }
        let node = tree.get_flow_node(current)?;
        match &node.kind {
            FlowNodeKind::Assignment(assign_ptr) => {
                // Effect summary: assignment of decl ← value on this node (ASSIGN_TARGET mode ignores assignments).
                if options.assignments {
                    let assign_pos = assign_ptr.get_syntax_id().get_range().start();
                    for effect in tree.get_flow_effects(current) {
                        if let FlowEffect::AssignDecl {
                            decl: assigned_decl,
                            value_syntax,
                        } = effect
                            && assigned_decl == decl
                        {
                            // Assignment hit: the assignment becomes the new base value, but guards after it (closer to the query on the backtrack path) are kept.
                            // If this assignment is not from a multi-return call source, earlier return_overload correlated narrowing no longer applies.
                            let is_multi_return = tree
                                .get_decl_multi_return_ref_on_flow(decl, assign_pos, current)
                                .and_then(|at| at.reference.as_ref())
                                .is_some();
                            let narrowings = if is_multi_return {
                                path.narrowings.iter().map(|n| n.clone_ref()).collect()
                            } else {
                                path.narrowings
                                    .iter()
                                    .filter(|n| !matches!(n, Narrowing::Correlated { .. }))
                                    .map(|n| n.clone_ref())
                                    .collect()
                            };
                            let assign_path = PathState {
                                narrowings,
                                casts: path.casts.clone(),
                            };
                            let base_before = model.type_of_decl(decl).unwrap_or(LuaType::Unknown);
                            // Ordinary linear assignment with a single predecessor: push onto the explicit stack and keep backtracking forward,
                            // avoiding `narrowed_before` and RHS variable initialization recursively causing exponential backtracking.
                            if let Some(FlowAntecedent::Single(next)) = node.antecedent.as_ref() {
                                frames.push(DeclAssignmentFrame {
                                    antecedent: FlowAntecedent::Single(*next),
                                    flow_id: current,
                                    assign_pos,
                                    value_syntax: *value_syntax,
                                    assign_path,
                                    base_before,
                                });
                                *path = PathState::default();
                                // The assignment's "before state" is always backtracked in Point mode; MergeBranch semantics only affect
                                // the merge result when expanding assignment frames.
                                mode = TraceMode::Point;
                                current = *next;
                                continue;
                            }
                            // Multiple/None predecessor: keep the old recursive walk implementation (branch-merge paths are already expanded per branch).
                            let mut before_visited = visited.clone();
                            let mut before_path = PathState::default();
                            let narrowed_before = walk_decl(
                                model,
                                decl,
                                tree,
                                node.antecedent.as_ref(),
                                TraceOptions::FLOW_READ,
                                TraceMode::Point,
                                &mut before_visited,
                                &mut before_path,
                            )
                            .unwrap_or_else(|| finalize(model, base_before.clone(), path));
                            let value_ty = assigned_value_type(
                                model,
                                tree,
                                decl,
                                current,
                                assign_pos,
                                *value_syntax,
                                &narrowed_before,
                            );
                            let value_ty = coerce_table_assign_to_narrowed(
                                model,
                                decl,
                                value_ty,
                                &narrowed_before,
                                &base_before,
                            );
                            let value_ty = if value_ty.is_unknown()
                                && !matches!(narrowed_before, LuaType::Unknown | LuaType::Any)
                            {
                                widen_const_type(narrowed_before.clone())
                            } else {
                                value_ty
                            };
                            let result = finalize(model, value_ty, &assign_path);
                            if mode == TraceMode::MergeBranch
                                && !assignment_antecedent_reachable(
                                    model,
                                    tree,
                                    node.antecedent.as_ref(),
                                )
                            {
                                if narrowed_before.is_never() {
                                    return Some(LuaType::Never);
                                }
                                return Some(widen_const_type(narrowed_before));
                            }
                            return Some(result);
                        }
                    }
                }
            }
            FlowNodeKind::CallExprStat(call_stat_ptr) => {
                if mode == TraceMode::MergeBranch && call_stat_returns_never(model, call_stat_ptr) {
                    return Some(LuaType::Never);
                }
            }
            FlowNodeKind::DeclPosition(pos) => {
                // Target decl's declaration statement position → initial type; if assignment frames were pushed, expand them in order.
                if let Some(facts) = model.file_facts()
                    && let Some(d) = facts.decl_by_id(decl)
                    && d.owner_syntax.map(|s| s.get_range().start()) == Some(*pos)
                {
                    return finish_decl_frames(
                        model,
                        decl,
                        tree,
                        finalize(
                            model,
                            model.type_of_decl(decl).unwrap_or(LuaType::Unknown),
                            path,
                        ),
                        frames,
                        root_mode,
                    );
                }
            }
            FlowNodeKind::TrueCondition(cond_ptr) => {
                if mode == TraceMode::MergeBranch
                    && let Some(cond) = flow_condition_expr(model, cond_ptr)
                    && condition_branch_unreachable(model, &cond, true)
                {
                    return Some(LuaType::Never);
                }
                if options.guards
                    && let Some(narrowing) = narrow_condition(
                        model,
                        tree,
                        decl,
                        cond_ptr,
                        current,
                        antecedent_has_narrowing(tree, node.antecedent.clone()),
                    )
                {
                    path.narrowings.push(narrowing);
                }
            }
            FlowNodeKind::FalseCondition(cond_ptr) => {
                if mode == TraceMode::MergeBranch
                    && let Some(cond) = flow_condition_expr(model, cond_ptr)
                    && condition_branch_unreachable(model, &cond, false)
                {
                    return Some(LuaType::Never);
                }
                if options.guards
                    && let Some(narrowing) = narrow_condition_false(
                        model,
                        tree,
                        decl,
                        cond_ptr,
                        current,
                        antecedent_has_narrowing(tree, node.antecedent.clone()),
                    )
                {
                    path.narrowings.push(narrowing);
                }
            }
            FlowNodeKind::TagCast(cast_ptr) => {
                if options.casts {
                    collect_decl_cast(model, decl, cast_ptr, path);
                }
            }
            FlowNodeKind::AsCast(as_ptr) => {
                if options.casts {
                    collect_decl_as_cast(model, decl, as_ptr, path);
                }
            }
            // Backtracked to the start (e.g. function parameters with no DeclPosition node): use the declaration type as the base and apply path guards/casts.
            FlowNodeKind::Start | FlowNodeKind::Unreachable => {
                if matches!(node.kind, FlowNodeKind::Unreachable) && mode == TraceMode::MergeBranch
                {
                    return Some(LuaType::Never);
                } else {
                    let base = model
                        .type_of_decl(decl)
                        .map(|ty| finalize(model, ty, path))
                        .unwrap_or(LuaType::Unknown);
                    return finish_decl_frames(model, decl, tree, base, frames, root_mode);
                }
            }
            _ => {}
        }

        match &node.antecedent {
            None => {
                let base = model
                    .type_of_decl(decl)
                    .map(|ty| finalize(model, ty, path))
                    .unwrap_or(LuaType::Unknown);
                return finish_decl_frames(model, decl, tree, base, frames, root_mode);
            }
            Some(FlowAntecedent::Single(next)) => {
                current = *next;
                continue;
            }
            Some(FlowAntecedent::Multiple(multi_id)) => {
                // If all branches before the merge point are pure linear fragments, jump directly to the common predecessor and continue backtracking,
                // avoiding recursive per-branch expansion across many consecutive if/conditions.
                if let Some(branches) = tree.get_multi_antecedents(*multi_id)
                    && let Some(common) = local_merge_common(tree, branches)
                    && branches.iter().all(|branch| {
                        decl_branch_is_pure(model, tree, decl, options, *branch, common)
                    })
                {
                    current = common;
                    continue;
                }
                if frames.is_empty() {
                    return walk_decl(
                        model,
                        decl,
                        tree,
                        node.antecedent.as_ref(),
                        options,
                        mode,
                        visited,
                        path,
                    );
                }
                // When there are already assignment frames on the explicit stack, first use walk_decl to compute the merged "pre-assignment state",
                // then let finish_decl_frames expand those frames.
                let before = walk_decl(
                    model,
                    decl,
                    tree,
                    node.antecedent.as_ref(),
                    options,
                    mode,
                    visited,
                    path,
                )
                .unwrap_or_else(|| {
                    finalize(
                        model,
                        model.type_of_decl(decl).unwrap_or(LuaType::Unknown),
                        path,
                    )
                });
                return finish_decl_frames(model, decl, tree, before, frames, root_mode);
            }
        }
    }
}

/// The `---@type` annotation type on a member declaration/assignment statement (used for assignment fallback).
fn member_annotation_type(model: &SemanticModel, member: &SemanticId) -> Option<LuaType> {
    let member_file = match member {
        SemanticId::Member(key) => key.file_id,
        _ => model.file_id(),
    };
    let facts = model.file_facts_of(member_file)?;
    let member_def = facts.member_by_id(member)?;
    let syntax = member_def.doc_type_syntax?;
    Some(model.doc_type_lua_in(member_file, syntax, &[]))
}

/// Backtrack along the CFG for the member's most recent assignment type.
fn trace_member(
    model: &SemanticModel,
    member: &SemanticId,
    tree: &FlowTree,
    flow_id: FlowId,
    visited: &mut HashSet<FlowId>,
    path: &mut PathState,
) -> Option<LuaType> {
    let mut current = flow_id;
    loop {
        if !visited.insert(current) {
            return None;
        }
        let node = tree.get_flow_node(current)?;
        match &node.kind {
            FlowNodeKind::Assignment(_) => {
                for effect in tree.get_flow_effects(current) {
                    if let FlowEffect::AssignMember {
                        member: assigned_member,
                        value_syntax,
                        ..
                    } = effect
                        && assigned_member == member
                    {
                        // Member assignment overrides earlier guard narrowing; cast widening is retained.
                        let assign_path = PathState {
                            narrowings: Vec::new(),
                            casts: path.casts.clone(),
                        };
                        let mut value_ty = model.type_of_expr(*value_syntax);
                        if let Some(annotation) = member_annotation_type(model, member) {
                            if model.type_check(&annotation, &value_ty) {
                                value_ty = widen_const_type(value_ty);
                            } else {
                                value_ty = annotation;
                            }
                        }
                        return Some(finalize(model, value_ty, &assign_path));
                    }
                }
            }
            FlowNodeKind::TagCast(cast_ptr) => {
                collect_member_cast(model, member, cast_ptr, path);
            }
            FlowNodeKind::AsCast(as_ptr) => {
                collect_member_as_cast(model, member, as_ptr, path);
            }
            FlowNodeKind::TrueCondition(cond_ptr) => {
                if let Some(narrowing) = narrow_member_condition(model, member, cond_ptr, true) {
                    path.narrowings.push(narrowing);
                }
            }
            FlowNodeKind::FalseCondition(cond_ptr) => {
                if let Some(narrowing) = narrow_member_condition(model, member, cond_ptr, false) {
                    path.narrowings.push(narrowing);
                }
            }
            FlowNodeKind::Start | FlowNodeKind::Unreachable => {
                return flow_member_value_type(model, member).map(|ty| finalize(model, ty, path));
            }
            _ => {}
        }

        match &node.antecedent {
            None => {
                return flow_member_value_type(model, member).map(|ty| finalize(model, ty, path));
            }
            Some(FlowAntecedent::Single(next)) => {
                current = *next;
                continue;
            }
            Some(FlowAntecedent::Multiple(multi_id)) => {
                // Consistent with `walk_member`'s branch optimization: when all branches are pure linear fragments for the current member,
                // jump directly to the common predecessor and continue backtracking, avoiding equal-depth recursion from consecutive ifs.
                if let Some(branches) = tree.get_multi_antecedents(*multi_id)
                    && let Some(common) = local_merge_common(tree, branches)
                    && branches
                        .iter()
                        .all(|branch| member_branch_is_pure(tree, member, *branch, common))
                {
                    current = common;
                    continue;
                }
                return walk_member(model, member, tree, node.antecedent.as_ref(), visited, path);
            }
        }
    }
}

/// Narrowing kinds.
#[derive(Debug)]
enum Narrowing {
    /// Replacement: the value is exactly `ty` on this path (type guard / `== literal`).
    Replace(LuaType),
    /// Removal: remove the specified component from the base type (false branch of `== literal`).
    Remove(LuaType),
    /// Truthy check: remove false and nil.
    Truthy,
    /// Falsy check: keep only false / nil (`not x` is true).
    Falsy,
    /// Not nil: remove nil.
    NotNil,
    /// return_overload correlated narrowing: the `ok` branch narrows `result` from the same call source to the matching row.
    Correlated {
        matching_target_types: Vec<LuaType>,
        correlated_candidate_types: Vec<LuaType>,
    },
    /// Base-type guards like `type(x) == 'table'`: filter union components by the actual primitive type.
    FilterPrimitive {
        primitive: LuaType,
        keep_matching: bool,
    },
    /// Member discriminants like `x.kind == 'A'`: filter union components by member type.
    MemberDiscriminant {
        key: LuaMemberKey,
        literal: LuaType,
        keep_matching: bool,
    },
    /// Dynamic member truthiness checks like `obj[key]`: filter union components by whether the member is truthy.
    MemberTruthy {
        key: LuaMemberKey,
        keep_matching: bool,
    },
    /// Array-length guards like `#arr == n`: narrow the array length on the path to a known upper bound.
    ArrayLen(i64),
    /// Guards like `#arr >= n` / `#arr > n`: narrow the array length on the path to a known lower bound.
    ArrayMinLen(i64),
}

/// After hitting a base type, apply the path state: first apply guard narrowing in "far → near" order,
/// then apply `---@cast +T` widening last — casts are cumulative widening on the declaration and narrowing cannot erase them.
fn finalize(model: &SemanticModel, mut base: LuaType, path: &PathState) -> LuaType {
    for narrowing in path.narrowings.iter().rev() {
        base = apply_narrowing(model, base, narrowing);
    }
    for cast in path.casts.iter().rev() {
        match cast {
            CastOp::Add(ty) => base = merge_types(base, ty.clone()),
            CastOp::Remove(ty) => base = remove_type(base, ty),
            CastOp::Replace(ty) => base = ty.clone(),
        }
    }
    base
}

/// Apply narrowing: replacement returns directly; removal removes components.
fn apply_narrowing(model: &SemanticModel, mut ty: LuaType, narrowing: &Narrowing) -> LuaType {
    match narrowing {
        Narrowing::Replace(replacement) => replacement.clone(),
        Narrowing::Remove(removed) => {
            let out = remove_type(ty.clone(), removed);
            // `local x = 0; if x ~= 0 then`: when removing that constant from the only integer constant,
            // don't get never; instead widen x to the integer type.
            if out.is_unknown()
                && matches!(ty, LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_))
            {
                return LuaType::Integer;
            }
            out
        }
        Narrowing::FilterPrimitive {
            primitive,
            keep_matching,
        } => filter_type_by_primitive(model, ty, primitive, *keep_matching),
        Narrowing::MemberDiscriminant {
            key,
            literal,
            keep_matching,
        } => filter_type_by_member_discriminant(model, ty, key, literal, *keep_matching),
        Narrowing::MemberTruthy { key, keep_matching } => {
            filter_type_by_member_truthiness(model, ty, key, *keep_matching)
        }
        Narrowing::ArrayLen(max) => {
            if let LuaType::Array(array) = &mut ty {
                let base = array.get_base().clone();
                ty = LuaType::Array(Arc::new(crate::LuaArrayType::new(
                    base,
                    crate::LuaArrayLen::Max(*max),
                )));
            }
            ty
        }
        Narrowing::ArrayMinLen(min) => {
            if let LuaType::Array(array) = &mut ty {
                let base = array.get_base().clone();
                ty = LuaType::Array(Arc::new(crate::LuaArrayType::new(
                    base,
                    crate::LuaArrayLen::Min(*min),
                )));
            }
            ty
        }
        Narrowing::Truthy => remove_falsy(ty),
        Narrowing::Falsy => remove_truthy(ty),
        Narrowing::NotNil => remove_type(ty, &LuaType::Nil),
        Narrowing::Correlated {
            matching_target_types,
            correlated_candidate_types,
        } => {
            let matching = LuaType::from_vec(matching_target_types.clone());
            let narrowed = narrow_intersect_types(ty.clone(), matching);
            if narrowed.is_never() {
                return ty;
            }
            let remaining = remove_type(ty, &LuaType::from_vec(correlated_candidate_types.clone()));
            merge_types(narrowed, remaining)
        }
    }
}

/// `---@cast x +T`: add T to decl's path type (`-` / no-operator semantics differ from luals and are not handled yet).
fn collect_decl_cast(
    model: &SemanticModel,
    decl: &SemanticId,
    cast_ptr: &emmylua_parser::LuaAstPtr<emmylua_parser::LuaDocTagCast>,
    path: &mut PathState,
) {
    let Some(chunk) = model.chunk() else {
        return;
    };
    let Some(cast) = cast_ptr.to_node(&chunk) else {
        return;
    };
    let Some(key_expr) = cast.get_key_expr() else {
        return;
    };
    let matches = match &key_expr {
        LuaExpr::NameExpr(name_expr) => {
            model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
        }
        _ => false,
    };
    if !matches {
        return;
    }
    for op_type in cast.get_op_types() {
        let Some(doc_type) = op_type.get_type() else {
            // `-?`: subtraction with no DocType → remove nil.
            if op_type
                .get_op()
                .is_some_and(|op| op.get_op() == BinaryOperator::OpSub)
            {
                path.casts.push(CastOp::Remove(LuaType::Nil));
            }
            continue;
        };
        let ty = if doc_type.get_text().trim() == "?" {
            LuaType::Nil
        } else {
            model.doc_type_lua(doc_type.get_syntax_id())
        };
        if matches!(ty, LuaType::Unknown) {
            continue;
        }
        match op_type.get_op().map(|op| op.get_op()) {
            None => path.casts.push(CastOp::Replace(ty)),
            Some(BinaryOperator::OpAdd) => path.casts.push(CastOp::Add(ty)),
            Some(BinaryOperator::OpSub) => path.casts.push(CastOp::Remove(ty)),
            _ => {}
        }
    }
}

/// `---@cast t.x +T`: after matching a member, add T to the path type.
fn collect_member_cast(
    model: &SemanticModel,
    member: &SemanticId,
    cast_ptr: &emmylua_parser::LuaAstPtr<emmylua_parser::LuaDocTagCast>,
    path: &mut PathState,
) {
    let Some(chunk) = model.chunk() else {
        return;
    };
    let Some(cast) = cast_ptr.to_node(&chunk) else {
        return;
    };
    let Some(key_expr) = cast.get_key_expr() else {
        return;
    };
    let matches = match &key_expr {
        LuaExpr::IndexExpr(index_expr) => {
            model
                .resolve_member(index_expr)
                .and_then(|resolved| resolved.member_id)
                .as_ref()
                == Some(member)
        }
        _ => false,
    };
    if !matches {
        return;
    }
    for op_type in cast.get_op_types() {
        let Some(doc_type) = op_type.get_type() else {
            // `-?`: subtraction with no DocType → remove nil.
            if op_type
                .get_op()
                .is_some_and(|op| op.get_op() == BinaryOperator::OpSub)
            {
                path.casts.push(CastOp::Remove(LuaType::Nil));
            }
            continue;
        };
        let ty = if doc_type.get_text().trim() == "?" {
            LuaType::Nil
        } else {
            model.doc_type_lua(doc_type.get_syntax_id())
        };
        if matches!(ty, LuaType::Unknown) {
            continue;
        }
        match op_type.get_op().map(|op| op.get_op()) {
            None => path.casts.push(CastOp::Replace(ty)),
            Some(BinaryOperator::OpAdd) => path.casts.push(CastOp::Add(ty)),
            Some(BinaryOperator::OpSub) => path.casts.push(CastOp::Remove(ty)),
            _ => {}
        }
    }
}

/// Inline assertion `--[[@as T]]`: when matching the current decl's path, replace the path type directly with T.
fn collect_decl_as_cast(
    model: &SemanticModel,
    _decl: &SemanticId,
    as_ptr: &emmylua_parser::LuaAstPtr<LuaDocTagAs>,
    path: &mut PathState,
) {
    let Some(ty) = as_cast_type(model, as_ptr) else {
        return;
    };
    if !matches!(ty, LuaType::Unknown) {
        path.casts.push(CastOp::Replace(ty));
    }
}

/// Inline assertion `--[[@as T]]`: when matching the current member's path, replace the path type directly with T.
fn collect_member_as_cast(
    model: &SemanticModel,
    _member: &SemanticId,
    as_ptr: &emmylua_parser::LuaAstPtr<LuaDocTagAs>,
    path: &mut PathState,
) {
    let Some(ty) = as_cast_type(model, as_ptr) else {
        return;
    };
    if !matches!(ty, LuaType::Unknown) {
        path.casts.push(CastOp::Replace(ty));
    }
}

fn as_cast_type(
    model: &SemanticModel,
    as_ptr: &emmylua_parser::LuaAstPtr<LuaDocTagAs>,
) -> Option<LuaType> {
    let chunk = model.chunk()?;
    let as_tag = as_ptr.to_node(&chunk)?;
    let doc_type = as_tag.get_type()?;
    Some(model.doc_type_lua(doc_type.get_syntax_id()))
}

/// RHS type of an assignment: if decl is a multi-return slot on this assignment, take the matching return type by `return_index`;
/// otherwise fall back to ordinary expression inference.
fn assigned_value_type(
    model: &SemanticModel,
    tree: &FlowTree,
    decl: &SemanticId,
    flow_id: FlowId,
    assign_pos: TextSize,
    value_syntax: emmylua_parser::LuaSyntaxId,
    target_ty: &LuaType,
) -> LuaType {
    let Some(multi_ref) = tree.get_decl_multi_return_ref_on_flow(decl, assign_pos, flow_id) else {
        return assigned_value_type_with_self_widening(
            model,
            decl,
            value_syntax,
            target_ty,
            model.type_of_expr_at(value_syntax, assign_pos),
        );
    };
    let Some(reference) = &multi_ref.reference else {
        // Non-multi-return source (ordinary assignment): still use `type_of_expr_at` so inline `--[[@as T]]` applies.
        // In `x, y = 1`, y is a missing RHS slot and should get nil instead of reusing the last value.
        if let Some(assign) = flow_assign_stat(model, tree, flow_id)
            && let Some(index) = assignment_target_index(model, &assign, decl)
            && let (_, values) = assign.get_var_and_expr_list()
            && index >= values.len()
        {
            return LuaType::Nil;
        }
        return assigned_value_type_with_self_widening(
            model,
            decl,
            value_syntax,
            target_ty,
            model.type_of_expr_at(value_syntax, assign_pos),
        );
    };
    let Some(chunk) = model.chunk() else {
        return assigned_value_type_with_self_widening(
            model,
            decl,
            value_syntax,
            target_ty,
            model.type_of_expr(value_syntax),
        );
    };
    let Some(call) = reference.call_expr.to_node(&chunk) else {
        return assigned_value_type_with_self_widening(
            model,
            decl,
            value_syntax,
            target_ty,
            model.type_of_expr(value_syntax),
        );
    };
    let expanded =
        model.infer_expr_list_types(&[LuaExpr::CallExpr(call)], Some(reference.return_index + 1));
    expanded
        .get(reference.return_index)
        .map(|(ty, _)| ty.clone())
        .unwrap_or_else(|| {
            if reference.return_index > 0 {
                // When a finite-return call is assigned to multiple variables, slots beyond the return count are nil.
                LuaType::Nil
            } else {
                assigned_value_type_with_self_widening(
                    model,
                    decl,
                    value_syntax,
                    target_ty,
                    model.type_of_expr_at(value_syntax, assign_pos),
                )
            }
        })
}

/// Self-referential arithmetic assignments (`x = x + 1` / `x = x - 1`) should keep the base type even when constant-folded:
/// in loops/runtime the value is not a fixed literal.
fn assigned_value_type_with_self_widening(
    model: &SemanticModel,
    decl: &SemanticId,
    value_syntax: emmylua_parser::LuaSyntaxId,
    target_ty: &LuaType,
    value_ty: LuaType,
) -> LuaType {
    let value_ty = widen_consts_to_target(model, value_ty, target_ty);
    if is_self_dependent_expr(model, value_syntax, decl) && is_const_like(&value_ty) {
        widen_const_type(value_ty)
    } else {
        value_ty
    }
}

fn is_self_dependent_expr(
    model: &SemanticModel,
    value_syntax: emmylua_parser::LuaSyntaxId,
    decl: &SemanticId,
) -> bool {
    let Some(tree) = model.syntax_tree() else {
        return false;
    };
    let Some(node) = value_syntax.to_node_from_root(&tree.get_red_root()) else {
        return false;
    };
    let Some(expr) = LuaExpr::cast(node) else {
        return false;
    };
    let references_decl = |expr: &LuaExpr| -> bool {
        expr.descendants::<emmylua_parser::LuaNameExpr>()
            .any(|name| model.resolve_name(name.get_position()).as_ref() == Some(decl))
    };
    match expr {
        LuaExpr::ParenExpr(paren) => paren
            .get_expr()
            .is_some_and(|inner| references_decl(&inner)),
        LuaExpr::BinaryExpr(_) | LuaExpr::UnaryExpr(_) => references_decl(&expr),
        _ => false,
    }
}

fn is_const_like(ty: &LuaType) -> bool {
    matches!(
        ty,
        LuaType::IntegerConst(_)
            | LuaType::DocIntegerConst(_)
            | LuaType::FloatConst(_)
            | LuaType::StringConst(_)
            | LuaType::DocStringConst(_)
            | LuaType::BooleanConst(_)
            | LuaType::DocBooleanConst(_)
    )
}

/// Look up a field type from a table literal (`model.member_type` does not always preserve literal fields for TableConst).
fn table_member_type(model: &SemanticModel, ty: &LuaType, key: &LuaMemberKey) -> Option<LuaType> {
    let LuaType::TableConst(table) = ty else {
        return model.member_type(ty, key);
    };
    let owner = SemanticId::member(table.file_id, table.value);
    let member_ref = model
        .members_of_owner(&owner)
        .into_iter()
        .find(|member| match key {
            LuaMemberKey::Name(name) => member.name.as_str() == name.as_str(),
            _ => false,
        })?;
    let facts = model.file_facts_of(member_ref.file_id)?;
    let member = facts.member_by_id(&member_ref.id)?;
    let syntax = member.value_syntax?;
    // Prefer the original literal constant, avoiding `"bar"` being projected to broad `string` and losing discriminant conflict detection.
    if let Some(tree) = model.syntax_tree_of(member_ref.file_id)
        && let Some(node) = syntax.to_node_from_root(&tree.get_red_root())
        && let Some(LuaExpr::LiteralExpr(lit)) = LuaExpr::cast(node)
        && let Some(lit_ty) = literal_type(&lit)
    {
        return Some(lit_ty);
    }
    model
        .type_of_member(&member_ref.id)
        .or_else(|| Some(model.type_of_expr(syntax)))
}

/// Table-literal assignments preserve branch narrowing:
/// if `x` was narrowed to `Foo` in a branch, then `x = { kind = "foo" }` should remain `Foo`;
/// if the literal's discriminant field conflicts with `Foo` (`{ kind = "bar" }`), drop the narrowing and return to the declared union.
fn flow_assign_stat(
    model: &SemanticModel,
    tree: &FlowTree,
    flow_id: FlowId,
) -> Option<LuaAssignStat> {
    let node = tree.get_flow_node(flow_id)?;
    let FlowNodeKind::Assignment(assign_ptr) = &node.kind else {
        return None;
    };
    let chunk = model.chunk()?;
    LuaAssignStat::cast(assign_ptr.to_node(&chunk)?.syntax().clone())
}

fn assignment_target_index(
    model: &SemanticModel,
    assign: &LuaAssignStat,
    decl: &SemanticId,
) -> Option<usize> {
    let (vars, _) = assign.get_var_and_expr_list();
    vars.iter().enumerate().find_map(|(index, var)| {
        let emmylua_parser::LuaVarExpr::NameExpr(name_expr) = var else {
            return None;
        };
        if model.resolve_name(name_expr.get_position()).as_ref() == Some(decl) {
            Some(index)
        } else {
            None
        }
    })
}

fn is_empty_table_literal(model: &SemanticModel, ty: &LuaType) -> bool {
    let LuaType::TableConst(table) = ty else {
        return false;
    };
    model
        .members_of_owner(&SemanticId::member(table.file_id, table.value))
        .is_empty()
}

fn class_has_required_named_field(model: &SemanticModel, def: &crate::TypeDef) -> bool {
    fn collect(model: &SemanticModel, def: &crate::TypeDef, visited: &mut Vec<SemanticId>) -> bool {
        if visited.contains(&def.id) {
            return false;
        }
        visited.push(def.id.clone());
        for member_ref in model.members_of_owner(&def.id) {
            let Some(facts) = model.file_facts_of(member_ref.file_id) else {
                continue;
            };
            let Some(member) = facts.member_by_id(&member_ref.id) else {
                continue;
            };
            if !member.is_index_signature && !member.is_nullable {
                return true;
            }
        }
        for super_name in &def.super_names {
            let Some(super_def) = model
                .resolve_type_def_in(def.file_id, super_name.as_str())
                .or_else(|| {
                    model
                        .type_defs_in_scope(crate::TypeScope::Global, super_name.as_str())
                        .into_iter()
                        .next()
                })
            else {
                continue;
            };
            if collect(model, &super_def, visited) {
                return true;
            }
        }
        false
    }
    collect(model, def, &mut Vec::new())
}

fn empty_table_compatible_with(model: &SemanticModel, ty: &LuaType) -> bool {
    match ty {
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .all(|component| empty_table_compatible_with(model, component)),
        LuaType::Ref(id) | LuaType::Def(id) => {
            let Some(def) = model.type_def_of(id) else {
                return false;
            };
            match def.kind {
                crate::TypeDefKind::Alias => model
                    .alias_target(&def)
                    .map(|target| empty_table_compatible_with(model, &target))
                    .unwrap_or(false),
                crate::TypeDefKind::Class => !class_has_required_named_field(model, &def),
                crate::TypeDefKind::Enum => false,
            }
        }
        LuaType::Object(object) => object
            .get_fields()
            .values()
            .all(|field_ty| field_ty.is_nullable()),
        LuaType::Table
        | LuaType::TableConst(_)
        | LuaType::TableGeneric(_)
        | LuaType::Array(_)
        | LuaType::Tuple(_)
        | LuaType::Generic(_) => true,
        _ => false,
    }
}

/// When an empty table literal is used as an "initialization/fallback" assignment, preserve the variable's declared table shape (e.g. `table<K,V>`)
/// rather than degrading to broad `table`. Used only when the variable is not a named class narrowing (Ref/Def), to avoid affecting
/// existing branch-narrowing preservation like `x = {}; x.kind = ...`.
fn empty_table_fallback_type(
    model: &SemanticModel,
    decl: &SemanticId,
    narrowed_before: &LuaType,
    base_before: &LuaType,
) -> Option<LuaType> {
    let mut candidates = vec![narrowed_before.clone()];
    if narrowed_before != base_before {
        candidates.push(base_before.clone());
    }
    // An unannotated local `local playerCache = archiveCache[0]` may have no declared base type in salsa;
    // in that case use the table shape inferred from the declared initializer as the empty-table fallback shape.
    if let Some(facts) = model.file_facts()
        && let Some(decl_info) = facts.decl_by_id(decl)
        && let Some(value_syntax) = decl_info.value_expr_syntax
    {
        let init_ty = model.type_of_expr(value_syntax);
        if !matches!(init_ty, LuaType::Unknown | LuaType::Any) {
            candidates.push(init_ty);
        } else if let Some(index_ty) = index_expr_type_from_initializer(model, value_syntax) {
            candidates.push(index_ty);
        }
    }
    for candidate in candidates {
        let components: Vec<LuaType> = match candidate {
            LuaType::Union(union) => union.into_vec(),
            other => vec![other],
        };
        for component in components {
            if matches!(
                component,
                LuaType::Nil | LuaType::BooleanConst(false) | LuaType::DocBooleanConst(false)
            ) {
                continue;
            }
            if empty_table_compatible_with(model, &component) {
                return Some(component);
            }
        }
    }
    None
}

/// Compute x's declared shape directly from the initializer `local x = prefix[key]`.
/// salsa's `type_of_expr` often returns Unknown for unannotated locals, but generic table indexes can still be resolved
/// through the prefix declaration type and member key.
fn index_expr_type_from_initializer(
    model: &SemanticModel,
    value_syntax: emmylua_parser::LuaSyntaxId,
) -> Option<LuaType> {
    let tree = model.syntax_tree()?;
    let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
    let LuaExpr::IndexExpr(index_expr) = LuaExpr::cast(node)? else {
        return None;
    };
    let prefix = index_expr.get_prefix_expr()?;
    let LuaExpr::NameExpr(name_expr) = prefix else {
        return None;
    };
    let prefix_decl = model.resolve_name(name_expr.get_position())?;
    let prefix_ty = model.type_of_decl(&prefix_decl).unwrap_or(LuaType::Unknown);
    if matches!(prefix_ty, LuaType::Unknown | LuaType::Any) {
        return None;
    }
    let key = match index_expr.get_index_key()? {
        emmylua_parser::LuaIndexKey::Name(name) => LuaMemberKey::Name(name.get_name_text().into()),
        emmylua_parser::LuaIndexKey::String(str) => LuaMemberKey::Name(str.get_value().into()),
        emmylua_parser::LuaIndexKey::Integer(num) => match num.get_number_value() {
            emmylua_parser::NumberResult::Int(i) => LuaMemberKey::Integer(i),
            emmylua_parser::NumberResult::Uint(i) => LuaMemberKey::Integer(i as i64),
            _ => return None,
        },
        emmylua_parser::LuaIndexKey::Idx(i) => LuaMemberKey::Integer(i as i64),
        _ => return None,
    };
    if let Some(ty) = model.member_type(&prefix_ty, &key) {
        return Some(ty);
    }
    // salsa's member query does not handle `table<K,V>` generic indexing; use the built-in generic table semantics to take V here.
    match &prefix_ty {
        LuaType::Generic(generic) if generic.get_base_type_id().get_name() == "table" => {
            let params = generic.get_params();
            if params.len() == 2 {
                return Some(params[1].clone());
            }
            None
        }
        LuaType::TableGeneric(params) if params.len() == 2 => Some(params[1].clone()),
        _ => None,
    }
}

fn coerce_table_assign_to_narrowed(
    model: &SemanticModel,
    decl: &SemanticId,
    value_ty: LuaType,
    narrowed_before: &LuaType,
    base_before: &LuaType,
) -> LuaType {
    if is_empty_table_literal(model, &value_ty)
        && !matches!(narrowed_before, LuaType::Ref(_) | LuaType::Def(_))
        && let Some(table_ty) = empty_table_fallback_type(model, decl, narrowed_before, base_before)
    {
        return table_ty;
    }
    if narrowed_before == base_before {
        // Non-branch narrowing (e.g. assigning an object return directly after `---@type A`) keeps the original assignment type semantics;
        // the member query layer continues to merge declared fields; do not override here.
        return value_ty;
    }
    if !matches!(value_ty, LuaType::TableConst(_) | LuaType::Object(_))
        || !matches!(narrowed_before, LuaType::Ref(_) | LuaType::Def(_))
    {
        return value_ty;
    }
    for field in ["kind", "type"] {
        let key = LuaMemberKey::Name(field.into());
        if let (Some(lit), Some(nar)) = (
            table_member_type(model, &value_ty, &key),
            model.member_type(narrowed_before, &key),
        ) && !types_may_overlap(lit, nar)
        {
            return base_before.clone();
        }
    }
    narrowed_before.clone()
}

/// Resolve the closure signature of the callee in a call. Follows local function alias chains (`local pred = is_string`)
/// until the actual closure definition, so type guards don't lose the signature on alias calls.
fn resolve_callable_closure(
    model: &SemanticModel,
    mut decl: SemanticId,
) -> Option<(FileId, emmylua_parser::LuaSyntaxId)> {
    let mut visited = HashSet::new();
    loop {
        let SemanticId::Decl(decl_key) = &decl else {
            return None;
        };
        let file_id = decl_key.file_id;
        let facts = model.file_facts_of(file_id)?;
        let decl_info = facts.decl_by_id(&decl)?;
        let value_syntax = decl_info.value_expr_syntax?;
        let tree = model.syntax_tree_of(file_id)?;
        let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
        if emmylua_parser::LuaClosureExpr::cast(node.clone()).is_some() {
            return Some((file_id, value_syntax));
        }
        let LuaExpr::NameExpr(name_expr) = LuaExpr::cast(node)? else {
            return None;
        };
        let next_decl = model.resolve_name(name_expr.get_position())?;
        if !visited.insert(next_decl.clone()) {
            return None;
        }
        decl = next_decl;
    }
}

/// Original signature at the call site (already followed through alias chains).
fn call_signature(
    model: &SemanticModel,
    call: &emmylua_parser::LuaCallExpr,
) -> Option<(
    FileId,
    emmylua_parser::LuaSyntaxId,
    crate::salsa_builder::def::signature::Signature,
)> {
    let LuaExpr::NameExpr(callee_name) = call.get_prefix_expr()? else {
        return None;
    };
    let decl = model.resolve_name(callee_name.get_position())?;
    let (file_id, closure_syntax) = resolve_callable_closure(model, decl)?;
    let facts = model.file_facts_of(file_id)?;
    let signature = facts.signature_by_closure(closure_syntax)?.clone();
    Some((file_id, closure_syntax, signature))
}

/// Signature return type for a conditional call guard (cross-file: the decl key carries its own file_id).
fn call_signature_return(
    model: &SemanticModel,
    call: &emmylua_parser::LuaCallExpr,
) -> Option<LuaType> {
    let (file_id, closure_syntax, signature) = call_signature(model, call)?;
    let shell = model.q().signature_return(file_id, closure_syntax)?;
    let generic_names: Vec<smol_str::SmolStr> = signature
        .docs
        .as_ref()
        .map(|docs| docs.generic_params.iter().map(|p| p.name.clone()).collect())
        .unwrap_or_default();
    Some(model.q().type_shell_lua_in(file_id, &shell, &generic_names))
}

/// Correlated return_overload narrowing: the condition references a discriminant variable from the same multi-return call source.
fn correlated_narrowing_for_condition(
    model: &SemanticModel,
    tree: &FlowTree,
    target_decl: &SemanticId,
    cond: &LuaExpr,
    is_true_branch: bool,
    flow_id: FlowId,
) -> Option<Narrowing> {
    let (disc_decl, disc_narrowing) =
        discriminant_from_condition(model, tree, target_decl, cond, is_true_branch)?;
    let condition_position = cond.get_position();
    let (disc_refs, _) =
        tree.get_decl_multi_return_ref_summary_at(&disc_decl, condition_position, flow_id);
    let (target_refs, _) =
        tree.get_decl_multi_return_ref_summary_at(target_decl, condition_position, flow_id);
    // Only do correlated narrowing when both sides' multi-return call sources match exactly; if either side mixes in other call sources, give up.
    // Non-multi-return sources (ordinary assignments) do not join the call-source set and are kept as uncorrelated types by the apply stage.
    let disc_call_ids: HashSet<_> = disc_refs
        .iter()
        .map(|r| r.call_expr.get_syntax_id())
        .collect();
    let target_call_ids: HashSet<_> = target_refs
        .iter()
        .map(|r| r.call_expr.get_syntax_id())
        .collect();
    let shared_call_ids: HashSet<_> = disc_call_ids
        .intersection(&target_call_ids)
        .cloned()
        .collect();
    // Correlated narrowing is unreliable when the discriminant variable mixes in other call sources; extra call sources on the target variable are kept as uncorrelated.
    if disc_call_ids.len() != shared_call_ids.len() {
        return None;
    }

    let mut matching_target_types = Vec::new();
    let mut correlated_candidate_types = Vec::new();

    for disc_ref in &disc_refs {
        let Some(rows) = return_overload_rows_for_call(model, &disc_ref.call_expr) else {
            continue;
        };
        let disc_call_id = disc_ref.call_expr.get_syntax_id();
        // The discriminant variable's base type is the union of discriminant slots across all return_overload rows of that call.
        let disc_base = LuaType::from_vec(
            rows.iter()
                .map(|row| {
                    row.get(disc_ref.return_index)
                        .cloned()
                        .unwrap_or(LuaType::Nil)
                })
                .collect(),
        );
        let disc_narrowed = apply_narrowing(model, disc_base.clone(), &disc_narrowing);
        for target_ref in &target_refs {
            if target_ref.call_expr.get_syntax_id() != disc_call_id {
                continue;
            }
            for row in &rows {
                let disc_slot = row
                    .get(disc_ref.return_index)
                    .cloned()
                    .unwrap_or(LuaType::Nil);
                let target_slot = row
                    .get(target_ref.return_index)
                    .cloned()
                    .unwrap_or(LuaType::Nil);
                // For a variable return slot like `R...` assigned to a single variable, use the base slot type.
                let target_slot = target_slot.get_result_slot_type(0).unwrap_or(target_slot);
                if !correlated_candidate_types.contains(&target_slot) {
                    correlated_candidate_types.push(target_slot.clone());
                }
                if types_may_overlap(disc_slot.clone(), disc_narrowed.clone())
                    && !matching_target_types.contains(&target_slot)
                {
                    matching_target_types.push(target_slot);
                }
            }
        }
    }

    if matching_target_types.is_empty() {
        None
    } else {
        Some(Narrowing::Correlated {
            matching_target_types,
            correlated_candidate_types,
        })
    }
}

/// Find from the condition a discriminant variable that shares the multi-return source with the target, and give the branch's narrowing for that discriminant variable.
fn discriminant_from_condition(
    model: &SemanticModel,
    tree: &FlowTree,
    target_decl: &SemanticId,
    cond: &LuaExpr,
    is_true_branch: bool,
) -> Option<(SemanticId, Narrowing)> {
    if let Some(disc_decl) = find_discriminant_decl(model, tree, target_decl, cond) {
        let narrowing = if is_true_branch {
            direct_narrow_condition(model, &disc_decl, cond, false)
        } else {
            direct_narrow_condition_false(model, &disc_decl, cond, false)
        }?;
        return Some((disc_decl, narrowing));
    }
    if let Some(result) =
        boolean_alias_discriminant_from_condition(model, tree, target_decl, cond, is_true_branch)
    {
        return Some(result);
    }
    type_alias_discriminant_from_condition(model, tree, target_decl, cond, is_true_branch)
}

/// Find in a condition expression the discriminant variable that shares a multi-return source with the target.
fn find_discriminant_decl(
    model: &SemanticModel,
    tree: &FlowTree,
    target_decl: &SemanticId,
    cond: &LuaExpr,
) -> Option<SemanticId> {
    fn check_expr(
        model: &SemanticModel,
        tree: &FlowTree,
        target_decl: &SemanticId,
        expr: &LuaExpr,
    ) -> Option<SemanticId> {
        match expr {
            LuaExpr::NameExpr(name_expr) => {
                if let Some(decl) = model.resolve_name(name_expr.get_position())
                    && decl != *target_decl
                    && tree.has_shared_multi_return_refs(&decl, target_decl)
                {
                    Some(decl)
                } else {
                    None
                }
            }
            LuaExpr::CallExpr(call) => {
                if let Some(args) = call.get_args_list() {
                    for arg in args.get_args() {
                        if let Some(decl) = check_expr(model, tree, target_decl, &arg) {
                            return Some(decl);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    match cond {
        LuaExpr::NameExpr(_) => check_expr(model, tree, target_decl, cond),
        LuaExpr::UnaryExpr(unary) => unary
            .get_expr()
            .and_then(|inner| check_expr(model, tree, target_decl, &inner)),
        LuaExpr::BinaryExpr(binary) => {
            if let Some((left, right)) = binary.get_exprs() {
                if let Some(decl) = check_expr(model, tree, target_decl, &left) {
                    return Some(decl);
                }
                if let Some(decl) = check_expr(model, tree, target_decl, &right) {
                    return Some(decl);
                }
            }
            None
        }
        LuaExpr::CallExpr(_) => check_expr(model, tree, target_decl, cond),
        _ => None,
    }
}

/// Collect all name declarations appearing in a condition expression.
fn collect_condition_name_decls(model: &SemanticModel, expr: &LuaExpr, out: &mut Vec<SemanticId>) {
    match expr {
        LuaExpr::NameExpr(name) => {
            if let Some(decl) = model.resolve_name(name.get_position())
                && !out.contains(&decl)
            {
                out.push(decl);
            }
        }
        LuaExpr::UnaryExpr(unary) => {
            if let Some(inner) = unary.get_expr() {
                collect_condition_name_decls(model, &inner, out);
            }
        }
        LuaExpr::BinaryExpr(binary) => {
            if let Some((left, right)) = binary.get_exprs() {
                collect_condition_name_decls(model, &left, out);
                collect_condition_name_decls(model, &right, out);
            }
        }
        LuaExpr::CallExpr(call) => {
            if let Some(args) = call.get_args_list() {
                for arg in args.get_args() {
                    collect_condition_name_decls(model, &arg, out);
                }
            }
        }
        _ => {}
    }
}

/// Find a literal like `"string" == kind` in a condition, and return the corresponding primitive type name and operator.
fn find_string_literal_for_alias(
    model: &SemanticModel,
    cond: &LuaExpr,
    alias_decl: &SemanticId,
) -> Option<(String, BinaryOperator)> {
    let LuaExpr::BinaryExpr(binary) = cond else {
        return None;
    };
    let op = binary.get_op_token()?.get_op();
    let (left, right) = binary.get_exprs()?;
    let is_alias = |expr: &LuaExpr| -> bool {
        matches!(expr, LuaExpr::NameExpr(name) if model.resolve_name(name.get_position()).as_ref() == Some(alias_decl))
    };
    let lit = if is_alias(&left) {
        if let LuaExpr::LiteralExpr(lit) = &right {
            Some(lit)
        } else {
            None
        }
    } else if is_alias(&right) {
        if let LuaExpr::LiteralExpr(lit) = &left {
            Some(lit)
        } else {
            None
        }
    } else {
        None
    };
    let lit = lit?;
    let LuaLiteralToken::String(str) = lit.get_literal()? else {
        return None;
    };
    Some((str.get_value().to_string(), op))
}

/// Boolean alias like `local failed = ok == false`: `if failed` is equivalent to narrowing `ok` in its true/false branch.
/// Handles only aliases that compare a boolean literal with a discriminant variable sharing a multi-return source.
fn boolean_alias_discriminant_from_condition(
    model: &SemanticModel,
    tree: &FlowTree,
    target_decl: &SemanticId,
    cond: &LuaExpr,
    is_true_branch: bool,
) -> Option<(SemanticId, Narrowing)> {
    let LuaExpr::NameExpr(name_expr) = cond else {
        return None;
    };
    let alias_decl = model.resolve_name(name_expr.get_position())?;
    if alias_decl == *target_decl || tree.has_shared_multi_return_refs(&alias_decl, target_decl) {
        return None;
    }
    let facts = model.file_facts()?;
    let decl_info = facts.decl_by_id(&alias_decl)?;
    let value_syntax = decl_info.value_expr_syntax?;
    let syntax_tree = model.syntax_tree()?;
    let node = value_syntax.to_node_from_root(&syntax_tree.get_red_root())?;
    let LuaExpr::BinaryExpr(binary) = LuaExpr::cast(node)? else {
        return None;
    };
    let op = binary.get_op_token()?.get_op();
    if !matches!(op, BinaryOperator::OpEq | BinaryOperator::OpNe) {
        return None;
    }
    let (left, right) = binary.get_exprs()?;
    let (name_expr, bool_expr) = match (&left, &right) {
        (LuaExpr::NameExpr(name), LuaExpr::LiteralExpr(lit))
            if matches!(lit.get_literal(), Some(LuaLiteralToken::Bool(_))) =>
        {
            (name, lit)
        }
        (LuaExpr::LiteralExpr(lit), LuaExpr::NameExpr(name))
            if matches!(lit.get_literal(), Some(LuaLiteralToken::Bool(_))) =>
        {
            (name, lit)
        }
        _ => return None,
    };
    let LuaLiteralToken::Bool(bool_token) = bool_expr.get_literal()? else {
        return None;
    };
    let disc_decl = model.resolve_name(name_expr.get_position())?;
    if !tree.has_shared_multi_return_refs(&disc_decl, target_decl) {
        return None;
    }
    let bool_val = bool_token.is_true();
    // Alias's own truthiness → the true/false state the discriminant variable should be in.
    let alias_true = is_true_branch;
    let disc_should_be_true = match (op, bool_val, alias_true) {
        (BinaryOperator::OpEq, false, true) => false,
        (BinaryOperator::OpEq, true, true) => true,
        (BinaryOperator::OpNe, false, true) => true,
        (BinaryOperator::OpNe, true, true) => false,
        (BinaryOperator::OpEq, false, false) => true,
        (BinaryOperator::OpEq, true, false) => false,
        (BinaryOperator::OpNe, false, false) => false,
        (BinaryOperator::OpNe, true, false) => true,
        _ => return None,
    };
    let narrowing = if disc_should_be_true {
        Narrowing::Truthy
    } else {
        Narrowing::Falsy
    };
    Some((disc_decl, narrowing))
}

/// Alias `local kind = type(tag)`: `"string" == kind` is equivalent to `type(tag) == "string"`.
fn type_alias_discriminant_from_condition(
    model: &SemanticModel,
    tree: &FlowTree,
    target_decl: &SemanticId,
    cond: &LuaExpr,
    is_true_branch: bool,
) -> Option<(SemanticId, Narrowing)> {
    let mut alias_decls = Vec::new();
    collect_condition_name_decls(model, cond, &mut alias_decls);
    let facts = model.file_facts()?;
    let syntax_tree = model.syntax_tree()?;

    for alias_decl in alias_decls {
        if alias_decl == *target_decl || tree.has_shared_multi_return_refs(&alias_decl, target_decl)
        {
            continue;
        }
        let Some(decl_info) = facts.decl_by_id(&alias_decl) else {
            continue;
        };
        let Some(value_syntax) = decl_info.value_expr_syntax else {
            continue;
        };
        let Some(node) = value_syntax.to_node_from_root(&syntax_tree.get_red_root()) else {
            continue;
        };
        let Some(call) = emmylua_parser::LuaCallExpr::cast(node) else {
            continue;
        };
        let Some(LuaExpr::NameExpr(callee)) = call.get_prefix_expr() else {
            continue;
        };
        if callee.get_name_text().as_deref() != Some("type") {
            continue;
        }
        let Some(arg) = call.get_args_list().and_then(|list| list.get_args().next()) else {
            continue;
        };
        let LuaExpr::NameExpr(arg_name) = arg else {
            continue;
        };
        let Some(real_decl) = model.resolve_name(arg_name.get_position()) else {
            continue;
        };
        if !tree.has_shared_multi_return_refs(&real_decl, target_decl) {
            continue;
        }
        let Some((type_name, op)) = find_string_literal_for_alias(model, cond, &alias_decl) else {
            continue;
        };
        let Some(primitive) = primitive_type_from_name(&type_name) else {
            continue;
        };
        let narrowing = match (is_true_branch, op) {
            (true, BinaryOperator::OpEq) => Narrowing::Replace(primitive),
            (true, BinaryOperator::OpNe) => Narrowing::Remove(primitive),
            (false, BinaryOperator::OpEq) => Narrowing::Remove(primitive),
            (false, BinaryOperator::OpNe) => Narrowing::Replace(primitive),
            _ => continue,
        };
        return Some((real_decl, narrowing));
    }
    None
}

/// Widen RHS literal constants to their base type only when the target variable already has a "primitive union" type.
/// For example, `result: integer|string` assigned `1 | "override"` widens to `integer|string`;
/// unannotated variables or enum/literal types keep the original literal.
fn widen_consts_to_target(model: &SemanticModel, ty: LuaType, target: &LuaType) -> LuaType {
    if matches!(target, LuaType::Unknown | LuaType::Any) {
        return ty;
    }
    let is_primitive = |t: &LuaType| -> bool {
        matches!(
            t,
            LuaType::Integer | LuaType::Number | LuaType::String | LuaType::Boolean
        )
    };
    let target_components = match target {
        LuaType::Union(union) => union.into_vec(),
        other => vec![other.clone()],
    };
    let non_nil_components: Vec<_> = target_components
        .iter()
        .filter(|component| **component != LuaType::Nil)
        .cloned()
        .collect();
    let target_is_primitive_union =
        !non_nil_components.is_empty() && non_nil_components.iter().all(is_primitive);
    if !target_is_primitive_union {
        return ty;
    }
    // In a nullable primitive union (e.g. `string?`), widen literals only when the assignment is compatible with that non-nil primitive;
    // incompatible assignments (`string?` variable assigned `1`) keep the exact literal (which would otherwise drop nil).
    if target_components.contains(&LuaType::Nil) {
        let non_nil_target = LuaType::from_vec(non_nil_components);
        if !model.type_check(&ty, &non_nil_target) {
            return ty;
        }
    }
    widen_const_type(ty)
}

/// After generic binding substitution, widen literal constants to their base type (`1` → `integer`).
fn widen_const_type(ty: LuaType) -> LuaType {
    match ty {
        LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => LuaType::Integer,
        LuaType::FloatConst(_) => LuaType::Number,
        LuaType::StringConst(_) | LuaType::DocStringConst(_) => LuaType::String,
        LuaType::Union(union) => {
            LuaType::from_vec(union.into_vec().into_iter().map(widen_const_type).collect())
        }
        LuaType::Variadic(variadic) => {
            let widened = match variadic.as_ref() {
                crate::VariadicType::Base(base) => {
                    crate::VariadicType::Base(widen_const_type(base.clone()))
                }
                crate::VariadicType::Multi(types) => crate::VariadicType::Multi(
                    types.iter().map(|t| widen_const_type(t.clone())).collect(),
                ),
            };
            LuaType::Variadic(Arc::new(widened))
        }
        _ => ty,
    }
}

/// Get the raw `---@return_overload` rows from the call signature (before generic call instantiation).
fn return_overload_rows_for_call(
    model: &SemanticModel,
    call_ptr: &emmylua_parser::LuaAstPtr<emmylua_parser::LuaCallExpr>,
) -> Option<Vec<Vec<LuaType>>> {
    let chunk = model.chunk()?;
    let call = call_ptr.to_node(&chunk)?;
    let LuaExpr::NameExpr(callee_name) = call.get_prefix_expr()? else {
        return None;
    };
    let decl = model.resolve_name(callee_name.get_position())?;
    let (file_id, closure_syntax) = resolve_callable_closure(model, decl)?;
    let facts = model.file_facts_of(file_id)?;
    let signature = facts.signature_by_closure(closure_syntax)?;
    let docs = signature.docs.as_ref()?;
    if docs.return_overload_rows.is_empty() {
        return None;
    }

    let mut rows: Vec<Vec<LuaType>> = Vec::new();
    let mut index = 0;
    for &len in &docs.return_overload_rows {
        let end = (index + len).min(docs.return_overloads.len());
        rows.push(
            docs.return_overloads[index..end]
                .iter()
                .map(|(_, syntax)| model.doc_type_lua_in(file_id, *syntax, &docs.generic_params))
                .collect(),
        );
        index = end;
    }

    // Generic rows: infer bindings from call-site arguments and substitute them to get instantiated return_overload rows.
    if rows.iter().any(|row| row.iter().any(|ty| ty.contain_tpl()))
        && let Some((_, bindings)) =
            crate::semantic_model::infer::infer_call_with_bindings(model, call.get_syntax_id())
    {
        let instantiated = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|raw_ty| {
                        if raw_ty.contain_tpl() {
                            widen_const_type(crate::semantic_model::infer::unify::substitute(
                                raw_ty, &bindings,
                            ))
                        } else {
                            raw_ty.clone()
                        }
                    })
                    .collect()
            })
            .collect();
        return Some(instantiated);
    }

    Some(rows)
}

/// Member discriminant condition: `x.kind == 'A'` / `'A' == x.kind`.
fn member_discriminant_from_binary(
    model: &SemanticModel,
    left: &LuaExpr,
    right: &LuaExpr,
) -> Option<(SemanticId, LuaMemberKey, LuaType)> {
    let (index_expr, lit_expr) = match (left, right) {
        (LuaExpr::IndexExpr(index), lit @ LuaExpr::LiteralExpr(_)) => (index, lit),
        (lit @ LuaExpr::LiteralExpr(_), LuaExpr::IndexExpr(index)) => (index, lit),
        _ => return None,
    };
    let key = match index_expr.get_index_key()? {
        emmylua_parser::LuaIndexKey::Name(name) => LuaMemberKey::Name(name.get_name_text().into()),
        emmylua_parser::LuaIndexKey::String(str) => LuaMemberKey::Name(str.get_value().into()),
        emmylua_parser::LuaIndexKey::Expr(key_expr) => {
            // Dynamic key `obj[key]`: if the key is narrowed to a single string constant on the flow path, discriminate by that field.
            LuaMemberKey::Name(dynamic_key_string_literal(
                model,
                &key_expr,
                key_expr.get_range().start(),
            )?)
        }
        _ => return None,
    };
    let LuaExpr::LiteralExpr(lit_expr) = lit_expr else {
        return None;
    };
    let literal = literal_type(lit_expr)?;
    let prefix = index_expr.get_prefix_expr()?;
    let LuaExpr::NameExpr(name_expr) = prefix else {
        return None;
    };
    let decl = model.resolve_name(name_expr.get_position())?;
    Some((decl, key, literal))
}

/// Resolve a member key from an index expression; dynamic keys allow a key expression narrowed to a single string constant on the flow path.
fn member_key_from_index_expr(
    model: &SemanticModel,
    index_expr: &emmylua_parser::LuaIndexExpr,
) -> Option<LuaMemberKey> {
    match index_expr.get_index_key()? {
        emmylua_parser::LuaIndexKey::Name(name) => {
            Some(LuaMemberKey::Name(name.get_name_text().into()))
        }
        emmylua_parser::LuaIndexKey::String(str) => {
            Some(LuaMemberKey::Name(str.get_value().into()))
        }
        emmylua_parser::LuaIndexKey::Expr(key_expr) => {
            dynamic_key_string_literal(model, &key_expr, key_expr.get_range().start())
                .map(LuaMemberKey::Name)
        }
        _ => None,
    }
}

/// Resolve a dynamic key value: first take the string constant from the flow type; if the key itself is a table-literal index like `keys[slot]`,
/// look up the literal string value from the table-literal member directly.
fn dynamic_key_string_literal(
    model: &SemanticModel,
    key_expr: &LuaExpr,
    offset: TextSize,
) -> Option<smol_str::SmolStr> {
    if let LuaExpr::NameExpr(_) = key_expr {
        let key_ty = model.type_of_expr_at(key_expr.get_syntax_id(), offset);
        if let LuaType::StringConst(s) | LuaType::DocStringConst(s) = key_ty {
            return Some(s.as_str().into());
        }
    }
    if let LuaExpr::IndexExpr(inner_index) = key_expr {
        return table_literal_index_string(model, &inner_index, offset);
    }
    None
}

/// `keys[slot]`: when keys is a table literal and slot is narrowed to an integer constant, take that field's string literal directly.
fn table_literal_index_string(
    model: &SemanticModel,
    index_expr: &emmylua_parser::LuaIndexExpr,
    offset: TextSize,
) -> Option<smol_str::SmolStr> {
    let inner_key = match index_expr.get_index_key()? {
        emmylua_parser::LuaIndexKey::Expr(expr) => expr,
        _ => return None,
    };
    let LuaExpr::NameExpr(idx_name) = inner_key else {
        return None;
    };
    let idx_decl = model.resolve_name(idx_name.get_position())?;
    let idx_ty = model.type_of_decl_at(&idx_decl, offset);
    let int_key = match idx_ty {
        LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => i,
        _ => return None,
    };
    let prefix = index_expr.get_prefix_expr()?;
    let LuaExpr::NameExpr(prefix_name) = prefix else {
        return None;
    };
    let prefix_decl = model.resolve_name(prefix_name.get_position())?;
    let prefix_ty = model.type_of_decl_at(&prefix_decl, offset);
    let LuaType::TableConst(table) = prefix_ty else {
        return None;
    };
    let owner = SemanticId::member(table.file_id, table.value);
    let members = model.members_of_owner(&owner);
    let member = members.into_iter().find(|member| {
        model
            .file_facts_of(member.file_id)
            .and_then(|facts| facts.member_by_id(&member.id))
            .is_some_and(|m| m.key == LuaMemberKey::Integer(int_key))
    })?;
    let member_file = member.file_id;
    let facts = model.file_facts_of(member_file)?;
    let member_def = facts.member_by_id(&member.id)?;
    let value_syntax = member_def.value_syntax?;
    let tree = model.syntax_tree_of(member_file)?;
    let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
    let literal = emmylua_parser::LuaLiteralExpr::cast(node)?;
    match literal.get_literal()? {
        LuaLiteralToken::String(str_token) => Some(str_token.get_value().into()),
        _ => None,
    }
}

/// Array-length guards like `#arr == n` / `#arr <= n` / `#arr >= n`:
/// narrow the array length on the path to a known upper bound (`Max`) or lower bound (`Min`).
fn array_len_condition(
    model: &SemanticModel,
    decl: &SemanticId,
    cond: &LuaExpr,
    branch_is_true: bool,
) -> Option<Narrowing> {
    if let LuaExpr::ParenExpr(paren) = cond
        && let Some(inner) = paren.get_expr()
    {
        return array_len_condition(model, decl, &inner, branch_is_true);
    }
    if let LuaExpr::UnaryExpr(unary) = cond
        && unary.get_op_token()?.get_op() == UnaryOperator::OpNot
        && let Some(inner) = unary.get_expr()
    {
        return array_len_condition(model, decl, &inner, !branch_is_true);
    }
    let LuaExpr::BinaryExpr(binary) = cond else {
        return None;
    };
    let op = binary.get_op_token()?.get_op();
    let (left, right) = binary.get_exprs()?;
    let (unary_expr, lit_expr) = match (left, right) {
        (LuaExpr::UnaryExpr(unary), LuaExpr::LiteralExpr(lit)) => (unary, lit),
        (LuaExpr::LiteralExpr(lit), LuaExpr::UnaryExpr(unary)) => (unary, lit),
        _ => return None,
    };
    if unary_expr.get_op_token()?.get_op() != UnaryOperator::OpLen {
        return None;
    }
    let LuaExpr::NameExpr(name_expr) = unary_expr.get_expr()? else {
        return None;
    };
    if model.resolve_name(name_expr.get_position()).as_ref() != Some(decl) {
        return None;
    }
    let lit_ty = literal_type(&lit_expr)?;
    let bound = match lit_ty {
        LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => i,
        _ => return None,
    };

    match op {
        BinaryOperator::OpEq if branch_is_true => Some(Narrowing::ArrayLen(bound)),
        BinaryOperator::OpLe if branch_is_true => Some(Narrowing::ArrayLen(bound)),
        BinaryOperator::OpLe => match bound.checked_add(1) {
            Some(next) => Some(Narrowing::ArrayMinLen(next)),
            // `#arr <= i64::MAX` is always true, so the opposite branch is unreachable: use Never to represent the impossible path.
            None => Some(Narrowing::Replace(LuaType::Never)),
        },
        BinaryOperator::OpLt if branch_is_true => bound.checked_sub(1).map(Narrowing::ArrayLen),
        BinaryOperator::OpLt => Some(Narrowing::ArrayMinLen(bound)),
        BinaryOperator::OpGe if branch_is_true => Some(Narrowing::ArrayMinLen(bound)),
        BinaryOperator::OpGe => bound.checked_sub(1).map(Narrowing::ArrayLen),
        BinaryOperator::OpGt if branch_is_true => bound.checked_add(1).map(Narrowing::ArrayMinLen),
        BinaryOperator::OpGt => Some(Narrowing::ArrayLen(bound)),
        _ => None,
    }
}

/// Get a local variable's initializer expression (one level only, does not follow alias chains).
fn local_initializer_expr(
    model: &SemanticModel,
    name_expr: &emmylua_parser::LuaNameExpr,
) -> Option<LuaExpr> {
    let cond_decl = model.resolve_name(name_expr.get_position())?;
    let SemanticId::Decl(decl_key) = &cond_decl else {
        return None;
    };
    let facts = model.file_facts_of(decl_key.file_id)?;
    let info = facts.decl_by_id(&cond_decl)?;
    let value_syntax = info.value_expr_syntax?;
    let tree = model.syntax_tree_of(decl_key.file_id)?;
    let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
    LuaExpr::cast(node)
}

/// One-level inheritance for boolean local variable guards: after `local h = type(ret) == 'string'`,
/// `if h then` can narrow `ret` by h's initializer expression; after `local e = type(ret)`,
/// `if e == 'string'` can also reduce back to `type(ret) == 'string'`.
/// Does not recurse through alias chains.
fn alias_condition_narrowing(
    model: &SemanticModel,
    decl: &SemanticId,
    cond: &LuaExpr,
    branch_is_true: bool,
    has_prior_narrowing: bool,
) -> Option<Narrowing> {
    // `e == 'string'`, where e's initial value is `type(ret)`.
    if let LuaExpr::BinaryExpr(binary) = cond {
        let op = binary.get_op_token()?.get_op();
        if matches!(op, BinaryOperator::OpEq | BinaryOperator::OpNe) {
            let (left, right) = binary.get_exprs()?;
            let (name_expr, lit_expr) = match (left, right) {
                (LuaExpr::NameExpr(name), lit @ LuaExpr::LiteralExpr(_)) => (Some(name), lit),
                (lit @ LuaExpr::LiteralExpr(_), LuaExpr::NameExpr(name)) => (Some(name), lit),
                _ => return None,
            };
            if let Some(name_expr) = name_expr
                && let Some(expr) = local_initializer_expr(model, &name_expr)
            {
                if let LuaExpr::CallExpr(call) = &expr
                    && let Some(LuaExpr::NameExpr(callee_name)) = call.get_prefix_expr()
                    && callee_name.get_name_text().as_deref() == Some("type")
                    && let Some(arg) = call.get_args_list()?.get_args().next()
                {
                    let LuaExpr::NameExpr(arg_name) = arg else {
                        return None;
                    };
                    if model.resolve_name(arg_name.get_position()).as_ref() == Some(decl) {
                        let LuaExpr::LiteralExpr(lit) = lit_expr else {
                            return None;
                        };
                        let Some(LuaLiteralToken::String(str)) = lit.get_literal() else {
                            return None;
                        };
                        let primitive = primitive_type_from_name(&str.get_value())?;
                        let keep_matching = if op == BinaryOperator::OpEq {
                            branch_is_true
                        } else {
                            !branch_is_true
                        };
                        return Some(Narrowing::FilterPrimitive {
                            primitive,
                            keep_matching,
                        });
                    }
                }
            }
        }
    }

    let LuaExpr::NameExpr(name_expr) = cond else {
        return None;
    };
    let expr = local_initializer_expr(model, name_expr)?;
    // One level is enough: alias chains like `ok = h` are not inherited, preventing infinite recursion/over-narrowing.
    if matches!(expr, LuaExpr::NameExpr(_)) {
        return None;
    }
    if branch_is_true {
        direct_narrow_condition(model, decl, &expr, has_prior_narrowing)
    } else {
        direct_narrow_condition_false(model, decl, &expr, has_prior_narrowing)
    }
}

/// True-branch narrowing: `type(x) == 'string'` → string; `x == nil` → nil;
/// bare `x` / `x ~= nil` → truthy.
fn narrow_condition(
    model: &SemanticModel,
    tree: &FlowTree,
    decl: &SemanticId,
    cond_ptr: &emmylua_parser::LuaAstPtr<LuaExpr>,
    flow_id: FlowId,
    has_prior_narrowing: bool,
) -> Option<Narrowing> {
    let chunk = model.chunk()?;
    let cond = LuaExpr::cast(cond_ptr.to_node(&chunk)?.syntax().clone())?;
    if let Some(narrowing) = direct_narrow_condition(model, decl, &cond, has_prior_narrowing) {
        return Some(narrowing);
    }
    if let Some(narrowing) = array_len_condition(model, decl, &cond, true) {
        return Some(narrowing);
    }
    if let Some(narrowing) =
        alias_condition_narrowing(model, decl, &cond, true, has_prior_narrowing)
    {
        return Some(narrowing);
    }
    correlated_narrowing_for_condition(model, tree, decl, &cond, true, flow_id)
}

/// Condition narrowing of a member expression itself: the true/false branches of `t.x == "foo"` narrow `t.x`.
fn narrow_member_condition(
    model: &SemanticModel,
    member: &SemanticId,
    cond_ptr: &emmylua_parser::LuaAstPtr<LuaExpr>,
    branch_is_true: bool,
) -> Option<Narrowing> {
    let chunk = model.chunk()?;
    let cond = LuaExpr::cast(cond_ptr.to_node(&chunk)?.syntax().clone())?;
    if let LuaExpr::CallExpr(call) = &cond {
        if let Some((arg, guard_ty)) = type_guard_call_target(model, call)
            && let LuaExpr::IndexExpr(index_expr) = &arg
            && model
                .resolve_member(index_expr)
                .and_then(|resolved| resolved.member_id)
                .as_ref()
                == Some(member)
        {
            return Some(Narrowing::Replace(guard_ty));
        }
        // Guards like `rawget(t, "x")` / `utils.get(t, "x")` that fetch a member by key and test truthiness:
        // the true branch means that member is non-nil/non-false.
        if let Some(narrowing) = rawget_member_narrowing(model, member, call, branch_is_true) {
            return Some(narrowing);
        }
    }
    if branch_is_true {
        if let LuaExpr::IndexExpr(index_expr) = &cond
            && model
                .resolve_member(index_expr)
                .and_then(|resolved| resolved.member_id)
                .as_ref()
                == Some(member)
        {
            return Some(Narrowing::Truthy);
        }
        if let Some(narrowing) = alias_member_condition_narrowing(model, member, &cond) {
            return Some(narrowing);
        }
    }
    direct_narrow_member_condition(model, member, &cond, branch_is_true)
}

/// One-level inheritance for boolean alias guards: after `local ok = t.x ~= nil`, `if ok` can narrow `t.x`.
/// Alias chains are not recursive (`ok = has_x` is not inherited), preserving wide semantics.
fn alias_member_condition_narrowing(
    model: &SemanticModel,
    member: &SemanticId,
    cond: &LuaExpr,
) -> Option<Narrowing> {
    let LuaExpr::NameExpr(name_expr) = cond else {
        return None;
    };
    let expr = local_initializer_expr(model, name_expr)?;
    if matches!(expr, LuaExpr::NameExpr(_)) {
        return None;
    }
    direct_narrow_member_condition(model, member, &expr, true)
}

/// Whether a call is a rawget-like member truthiness guard: arguments are instance name + string key, and the key matches the current member.
fn rawget_member_narrowing(
    model: &SemanticModel,
    member: &SemanticId,
    call: &emmylua_parser::LuaCallExpr,
    branch_is_true: bool,
) -> Option<Narrowing> {
    if !branch_is_true {
        return None;
    }
    let args: Vec<LuaExpr> = call.get_args_list()?.get_args().collect();
    if args.len() < 2 {
        return None;
    }
    let LuaExpr::NameExpr(receiver) = &args[0] else {
        return None;
    };
    let receiver_decl = model.resolve_name(receiver.get_position())?;
    let key_name = match &args[1] {
        LuaExpr::LiteralExpr(lit) => match lit.get_literal()? {
            LuaLiteralToken::String(str) => str.get_value(),
            _ => return None,
        },
        _ => return None,
    };
    let member_file = match member {
        SemanticId::Member(key) => key.file_id,
        _ => model.file_id(),
    };
    let facts = model.file_facts_of(member_file)?;
    let member_def = facts.member_by_id(member)?;
    if member_def.key.name() != Some(key_name.as_str()) {
        return None;
    }
    let receiver_ty = model
        .type_of_decl(&receiver_decl)
        .unwrap_or(LuaType::Unknown);
    if matches!(receiver_ty, LuaType::Unknown | LuaType::Any) {
        return None;
    }
    // The receiver type must resolve that member (named type/generic instance), preventing unrelated calls from being misapplied to the current member.
    let key = LuaMemberKey::Name(key_name.into());
    model.member_type(&receiver_ty, &key)?;
    Some(Narrowing::NotNil)
}

fn direct_narrow_member_condition(
    model: &SemanticModel,
    member: &SemanticId,
    cond: &LuaExpr,
    branch_is_true: bool,
) -> Option<Narrowing> {
    let LuaExpr::BinaryExpr(binary) = cond else {
        return None;
    };
    let op = binary.get_op_token()?.get_op();
    if !matches!(op, BinaryOperator::OpEq | BinaryOperator::OpNe) {
        return None;
    }
    let (left, right) = binary.get_exprs()?;
    let (index_expr, lit_expr) = match (left, right) {
        (LuaExpr::IndexExpr(index), LuaExpr::LiteralExpr(lit)) => (index, lit),
        (LuaExpr::LiteralExpr(lit), LuaExpr::IndexExpr(index)) => (index, lit),
        _ => return None,
    };
    let resolved = model.resolve_member(&index_expr)?;
    if resolved.member_id.as_ref() != Some(member) {
        return None;
    }
    let lit_ty = literal_type(&lit_expr)?;
    let keep_matching = if branch_is_true {
        op == BinaryOperator::OpEq
    } else {
        op != BinaryOperator::OpEq
    };
    Some(if keep_matching {
        Narrowing::Replace(lit_ty)
    } else {
        Narrowing::Remove(lit_ty)
    })
}

/// Whether the self return_cast receiver is a conditional expression like `a and b or c`.
/// Conditional expressions must be narrowed first before self return_cast is safe; a plain `local m = {}` can be used directly.
fn is_conditional_receiver(model: &SemanticModel, arg: &LuaExpr) -> bool {
    let LuaExpr::NameExpr(name_expr) = arg else {
        return false;
    };
    let Some(decl) = model.resolve_name(name_expr.get_position()) else {
        return false;
    };
    let SemanticId::Decl(decl_key) = &decl else {
        return false;
    };
    let Some(facts) = model.file_facts_of(decl_key.file_id) else {
        return false;
    };
    let Some(decl_info) = facts.decl_by_id(&decl) else {
        return false;
    };
    let Some(value_syntax) = decl_info.value_expr_syntax else {
        return false;
    };
    let Some(tree) = model.syntax_tree_of(decl_key.file_id) else {
        return false;
    };
    let Some(node) = value_syntax.to_node_from_root(&tree.get_red_root()) else {
        return false;
    };
    let Some(expr) = LuaExpr::cast(node) else {
        return false;
    };
    matches!(
        expr,
        LuaExpr::BinaryExpr(binary)
            if matches!(
                binary.get_op_token().map(|token| token.get_op()),
                Some(BinaryOperator::OpAnd | BinaryOperator::OpOr)
            )
    )
}

/// Parse `---@return_cast name Type else Fallback` at the call site.
fn return_cast_for_call(
    model: &SemanticModel,
    call: &emmylua_parser::LuaCallExpr,
) -> Option<(LuaExpr, LuaType, Option<LuaType>, bool)> {
    let prefix = call.get_prefix_expr()?;
    let callee_decl = match &prefix {
        LuaExpr::NameExpr(name) => model.resolve_name(name.get_position())?,
        LuaExpr::IndexExpr(index) => {
            let name = match index.get_index_key()? {
                emmylua_parser::LuaIndexKey::Name(n) => n.get_name_text().to_string(),
                emmylua_parser::LuaIndexKey::String(s) => s.get_value().to_string(),
                _ => return None,
            };
            model
                .resolve_member(index)
                .and_then(|r| r.member_id)
                .or_else(|| super::member::find_self_return_cast_member(model, &name))?
        }
        _ => return None,
    };
    let (callee_file, closure_syntax) = match &callee_decl {
        SemanticId::Decl(key) => {
            let facts = model.file_facts_of(key.file_id)?;
            let decl = facts.decl_by_id(&callee_decl)?;
            (key.file_id, decl.value_expr_syntax?)
        }
        SemanticId::Member(key) => {
            let facts = model.file_facts_of(key.file_id)?;
            let member = facts.member_by_id(&callee_decl)?;
            (key.file_id, member.value_syntax?)
        }
        _ => return None,
    };
    let facts = model.file_facts_of(callee_file)?;
    let signature = facts
        .signatures
        .iter()
        .find(|signature| signature.closure_syntax == closure_syntax)?;
    let docs = signature.docs.as_ref()?;
    let return_cast = docs.return_cast.as_ref()?;
    // In a colon call `obj:m(x)`, self is the implicit receiver and is not in the args list;
    // `---@return_cast self ...` directly refers to the receiver.
    let is_self_cast = return_cast.name == "self" && call.is_colon_call();
    let param_index = if is_self_cast {
        0
    } else {
        signature
            .param_names
            .iter()
            .position(|name| name == &return_cast.name)?
    };
    let args: Vec<LuaExpr> = call.get_args_list()?.get_args().collect();
    let arg = if is_self_cast {
        // In `obj:m()`, self is the receiver, not the whole `obj:m` IndexExpr.
        match &prefix {
            LuaExpr::IndexExpr(index) => index.get_prefix_expr()?,
            other => other.clone(),
        }
    } else {
        args.get(param_index)?.clone()
    };
    let generics = docs.generic_params.as_slice();
    let cast_ty = model.doc_type_lua_in(callee_file, return_cast.cast, generics);
    let fallback_ty = return_cast
        .fallback
        .map(|syntax| model.doc_type_lua_in(callee_file, syntax, generics));
    Some((arg, cast_ty, fallback_ty, is_self_cast))
}

/// return_cast narrowing in boolean comparisons like `call == false` / `call ~= true`.
fn return_cast_bool_binary(
    model: &SemanticModel,
    left: &LuaExpr,
    right: &LuaExpr,
    op: BinaryOperator,
    branch_is_true: bool,
) -> Option<(LuaExpr, LuaType, bool)> {
    let (call, lit) = if let LuaExpr::CallExpr(call) = left {
        (call, right)
    } else if let LuaExpr::CallExpr(call) = right {
        (call, left)
    } else {
        return None;
    };
    let literal_bool = match lit {
        LuaExpr::LiteralExpr(l) => match l.get_literal() {
            Some(LuaLiteralToken::Bool(bool_token)) => Some(bool_token.is_true()),
            _ => None,
        },
        _ => None,
    };
    let literal_is_true = literal_bool?;
    let call_result_true = match op {
        BinaryOperator::OpEq => literal_is_true,
        BinaryOperator::OpNe => !literal_is_true,
        _ => return None,
    };
    let branch_call_true = if branch_is_true {
        call_result_true
    } else {
        !call_result_true
    };
    let (arg, cast_ty, fallback_ty, is_self) = return_cast_for_call(model, call)?;
    let ty = if branch_call_true {
        cast_ty
    } else {
        fallback_ty?
    };
    Some((arg, ty, is_self))
}

/// Boolean comparisons like `typeguard(x) == false` / `typeguard(x) ~= true`: when the comparison result
/// indicates the call itself is true, return the corresponding TypeGuard narrowing.
fn type_guard_bool_binary(
    model: &SemanticModel,
    left: &LuaExpr,
    right: &LuaExpr,
    op: BinaryOperator,
    branch_is_true: bool,
) -> Option<(LuaExpr, LuaType)> {
    let (call, lit) = if let LuaExpr::CallExpr(call) = left {
        (call, right)
    } else if let LuaExpr::CallExpr(call) = right {
        (call, left)
    } else {
        return None;
    };
    let literal_is_true = match lit {
        LuaExpr::LiteralExpr(l) => match l.get_literal() {
            Some(LuaLiteralToken::Bool(bool_token)) => bool_token.is_true(),
            _ => return None,
        },
        _ => return None,
    };
    let call_result_true = match op {
        BinaryOperator::OpEq => literal_is_true,
        BinaryOperator::OpNe => !literal_is_true,
        _ => return None,
    };
    let branch_call_true = if branch_is_true {
        call_result_true
    } else {
        !call_result_true
    };
    if !branch_call_true {
        return None;
    }
    type_guard_call_target(model, call)
}

/// Resolve the true-branch narrowing target from a call annotated `---@return TypeGuard<T>`.
/// Returns (guarded argument expression, narrowed type).
///
/// Deliberately does not fall back to `model.type_of_expr(call)`: type guards are themselves within flow backtracking,
/// so VM call-type inference would query the guarded variable's flow type again, causing nested backtracking across many guards.
/// Instead, project TypeGuard directly from the signature and lightly bind it to the argument corresponding to the generic parameter.
fn type_guard_call_target(
    model: &SemanticModel,
    call: &emmylua_parser::LuaCallExpr,
) -> Option<(LuaExpr, LuaType)> {
    // Prefer signature projection (does not trigger VM flow backtracking), ensuring consecutive guards don't accumulate backtracking from call-type evaluation.
    if let Some((file_id, _closure_syntax, signature)) = call_signature(model, call)
        && let Some(return_ty) = call_signature_return(model, call)
        && let Some(guard_ty) = match return_ty {
            LuaType::TypeGuard(inner) => Some((*inner).clone()),
            LuaType::Generic(generic) if generic.get_base_type_id().get_name() == "TypeGuard" => {
                generic.get_params().first().cloned()
            }
            _ => None,
        }
    {
        let guard_ty = resolve_type_guard_generic(model, file_id, call, &signature, guard_ty);
        let guard_ty = normalize_builtin_type(guard_ty);
        if !matches!(guard_ty, LuaType::Unknown | LuaType::Any) && !guard_ty.contain_tpl() {
            let arg = call.get_args_list()?.get_args().next()?;
            return Some((arg, guard_ty));
        }
    }
    // Fall back to VM binding when signature projection is insufficient (aliases through assignment chains, parameter function types, conditional generic aliases, etc.).
    let vm_ty = model.type_of_expr(call.get_syntax_id());
    let return_ty = if matches!(vm_ty, LuaType::Unknown) {
        call_signature_return(model, call).unwrap_or(vm_ty)
    } else {
        vm_ty
    };
    let guard_ty = match return_ty {
        LuaType::TypeGuard(inner) => Some((*inner).clone()),
        LuaType::Generic(generic) if generic.get_base_type_id().get_name() == "TypeGuard" => {
            generic.get_params().first().cloned()
        }
        _ => None,
    }?;
    let guard_ty = normalize_builtin_type(guard_ty);
    let arg = call.get_args_list()?.get_args().next()?;
    Some((arg, guard_ty))
}

/// If TypeGuard contains a function-level generic (`---@return TypeGuard<T>`), substitute the argument type corresponding to `T` in the signature.
/// Only reads literal / declaration types, so it does not trigger flow backtracking.
fn resolve_type_guard_generic(
    model: &SemanticModel,
    file_id: FileId,
    call: &emmylua_parser::LuaCallExpr,
    signature: &crate::salsa_builder::def::Signature,
    guard_ty: LuaType,
) -> LuaType {
    let (generic_index, generic_name, keep_literal) = match &guard_ty {
        LuaType::TplRef(tpl) => (
            match tpl.get_tpl_id() {
                crate::GenericTplId::Type(idx) | crate::GenericTplId::Func(idx) => idx as usize,
                _ => return guard_ty,
            },
            tpl.get_name().to_string(),
            tpl.is_const() || tpl.get_constraint().is_some(),
        ),
        LuaType::StrTplRef(str_tpl) => {
            let index = match str_tpl.get_tpl_id() {
                crate::GenericTplId::Type(idx) | crate::GenericTplId::Func(idx) => idx as usize,
                _ => return guard_ty,
            };
            (index, str_tpl.get_name().to_string(), false)
        }
        _ => return guard_ty,
    };
    let Some(docs) = signature.docs.as_ref() else {
        return guard_ty;
    };
    let Some(generic_param) = docs.generic_params.get(generic_index) else {
        return guard_ty;
    };
    // Find the parameter that uses this generic (`---@param type \`T\``), then substitute with the corresponding call-site argument.
    let generic_name = if generic_name.is_empty() {
        generic_param.name.as_str()
    } else {
        generic_name.as_str()
    };
    let Some(param_index) =
        signature
            .param_names
            .iter()
            .enumerate()
            .find_map(|(index, param_name)| {
                docs.param_types
                    .iter()
                    .find(|(name, _)| name == param_name)
                    .and_then(|(_, syntax)| {
                        let param_ty =
                            model.doc_type_lua_in(file_id, *syntax, &docs.generic_params);
                        // Type projection maps the same `T` to the same TplRef / StrTplRef.
                        let same_tpl = matches!(
                            &param_ty,
                            LuaType::TplRef(other) if other.get_name() == generic_name
                        ) || matches!(
                            &param_ty,
                            LuaType::StrTplRef(other) if other.get_name() == generic_name
                        ) || param_ty == guard_ty;
                        same_tpl.then_some(index)
                    })
            })
    else {
        return guard_ty;
    };
    let Some(arg) = call
        .get_args_list()
        .and_then(|list| list.get_args().nth(param_index))
    else {
        return guard_ty;
    };
    let arg_ty = match &arg {
        LuaExpr::NameExpr(name_expr) => model
            .resolve_name(name_expr.get_position())
            .and_then(|decl| model.type_of_decl(&decl))
            .unwrap_or_else(|| model.type_of_expr(arg.get_syntax_id())),
        _ => model.type_of_expr(arg.get_syntax_id()),
    };
    if keep_literal {
        arg_ty
    } else {
        crate::semantic_model::infer::vm::widen_const(&arg_ty)
    }
}

/// Whether a call condition may relate to `decl`: an argument names that declaration, or the receiver of a colon call.
/// Used before call-style guard analysis to quickly skip unrelated ordinary calls like `enabled(...)`/`hi(...)`,
/// avoiding repeated VM call inference on large linear CFGs.
fn call_may_narrow_decl(
    model: &SemanticModel,
    call: &emmylua_parser::LuaCallExpr,
    decl: &SemanticId,
) -> bool {
    if call.is_colon_call()
        && let Some(prefix) = call.get_prefix_expr()
    {
        let receiver_matches = match &prefix {
            LuaExpr::NameExpr(name) => {
                model.resolve_name(name.get_position()).as_ref() == Some(decl)
            }
            LuaExpr::IndexExpr(index) => index.get_prefix_expr().is_some_and(|prefix| {
                matches!(&prefix, LuaExpr::NameExpr(name)
                    if model.resolve_name(name.get_position()).as_ref() == Some(decl))
            }),
            _ => false,
        };
        if receiver_matches {
            return true;
        }
    }
    call.get_args_list().is_some_and(|args| {
        args.get_args().any(|arg| {
            matches!(&arg, LuaExpr::NameExpr(name)
                if model.resolve_name(name.get_position()).as_ref() == Some(decl))
        })
    })
}

fn direct_narrow_condition(
    model: &SemanticModel,
    decl: &SemanticId,
    cond: &LuaExpr,
    has_prior_narrowing: bool,
) -> Option<Narrowing> {
    match cond {
        // Call guard `typeguard(x)` / `---@return TypeGuard<T>`: the true branch narrows the argument to T.
        LuaExpr::CallExpr(call) => {
            if !call_may_narrow_decl(model, call, decl) {
                return None;
            }
            // `---@return_cast name Type else Fallback`: narrow the corresponding argument on the true branch.
            if let Some((arg, cast_ty, _, is_self)) = return_cast_for_call(model, call) {
                if (!is_self || has_prior_narrowing || !is_conditional_receiver(model, &arg))
                    && let LuaExpr::NameExpr(arg_name) = &arg
                    && model.resolve_name(arg_name.get_position()).as_ref() == Some(decl)
                {
                    return Some(Narrowing::Replace(cast_ty));
                }
            }
            // Only resolve TypeGuard when the guard call's argument really points to the current decl.
            // Otherwise ordinary conditions like `enabled(...)` fall back to VM call-type inference,
            // triggering repeated/nested flow backtracking for every unrelated condition on large linear fragments.
            let call_targets_decl = call
                .get_args_list()
                .and_then(|list| list.get_args().next())
                .is_some_and(|arg| {
                    matches!(&arg, LuaExpr::NameExpr(arg_name)
                        if model.resolve_name(arg_name.get_position()).as_ref() == Some(decl))
                });
            if call_targets_decl
                && let Some((arg, guard_ty)) = type_guard_call_target(model, call)
                && let LuaExpr::NameExpr(arg_name) = &arg
                && model.resolve_name(arg_name.get_position()).as_ref() == Some(decl)
            {
                return Some(Narrowing::Replace(guard_ty));
            }
            None
        }
        LuaExpr::BinaryExpr(binary) => {
            let op = binary.get_op_token()?.get_op();
            let (left, right) = binary.get_exprs()?;
            // `x.kind == 'A'`: member discriminant narrowing.
            if matches!(op, BinaryOperator::OpEq | BinaryOperator::OpNe)
                && let Some((disc_decl, key, literal)) =
                    member_discriminant_from_binary(model, &left, &right)
                && &disc_decl == decl
            {
                return Some(Narrowing::MemberDiscriminant {
                    key,
                    literal,
                    keep_matching: op == BinaryOperator::OpEq,
                });
            }
            // `isX(v) == false` / `isX(v) ~= true` true branch → return_cast false branch.
            if let Some((arg, ty, is_self)) =
                return_cast_bool_binary(model, &left, &right, op, false)
                && (!is_self || has_prior_narrowing || !is_conditional_receiver(model, &arg))
                && let LuaExpr::NameExpr(arg_name) = &arg
                && model.resolve_name(arg_name.get_position()).as_ref() == Some(decl)
            {
                return Some(Narrowing::Replace(ty));
            }
            // `typeguard(x) == true` / `typeguard(x) ~= false` true branch → TypeGuard true branch.
            if let Some((arg, guard_ty)) = type_guard_bool_binary(model, &left, &right, op, true)
                && let LuaExpr::NameExpr(arg_name) = &arg
                && model.resolve_name(arg_name.get_position()).as_ref() == Some(decl)
            {
                return Some(Narrowing::Replace(guard_ty));
            }
            // `type(x) == 'name'`: type guard.
            if let LuaExpr::CallExpr(call) = &left
                && let Some(LuaExpr::NameExpr(callee)) = call.get_prefix_expr()
                && callee.get_name_text().as_deref() == Some("type")
                && let Some(arg) = call.get_args_list()?.get_args().next()
                && let LuaExpr::NameExpr(arg_name) = arg
                && model.resolve_name(arg_name.get_position()).as_ref() == Some(decl)
                && let LuaExpr::LiteralExpr(lit) = &right
                && let Some(LuaLiteralToken::String(str)) = lit.get_literal()
            {
                let primitive_ty = primitive_type_from_name(&str.get_value())?;
                return Some(Narrowing::FilterPrimitive {
                    primitive: primitive_ty,
                    keep_matching: op == BinaryOperator::OpEq,
                });
            }
            // `x == nil` → nil; `x ~= nil` → truthy (remove nil).
            if let LuaExpr::NameExpr(name_expr) = &left
                && model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
                && matches!(&right, LuaExpr::LiteralExpr(lit) if matches!(lit.get_literal(), Some(LuaLiteralToken::Nil(_))))
            {
                return Some(if op == BinaryOperator::OpEq {
                    Narrowing::Replace(LuaType::Nil)
                } else {
                    Narrowing::NotNil
                });
            }
            // `nil == x` / `nil ~= x`: symmetric handling for right-hand operand.
            if let LuaExpr::NameExpr(name_expr) = &right
                && model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
                && matches!(&left, LuaExpr::LiteralExpr(lit) if matches!(lit.get_literal(), Some(LuaLiteralToken::Nil(_))))
            {
                return Some(if op == BinaryOperator::OpEq {
                    Narrowing::Replace(LuaType::Nil)
                } else {
                    Narrowing::NotNil
                });
            }
            // `x == 1` / `x == "a"` / `x == true` → replace with the literal type on the true branch.
            if let LuaExpr::NameExpr(name_expr) = &left
                && model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
                && let LuaExpr::LiteralExpr(lit) = &right
                && let Some(lit_ty) = literal_type(lit)
            {
                return Some(if op == BinaryOperator::OpEq {
                    Narrowing::Replace(lit_ty)
                } else if op == BinaryOperator::OpNe {
                    Narrowing::Remove(lit_ty)
                } else {
                    return None;
                });
            }
            // Symmetric forms: `1 == x` / `"a" == x` / `true == x`.
            if let LuaExpr::NameExpr(name_expr) = &right
                && model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
                && let LuaExpr::LiteralExpr(lit) = &left
                && let Some(lit_ty) = literal_type(lit)
            {
                return Some(if op == BinaryOperator::OpEq {
                    Narrowing::Replace(lit_ty)
                } else if op == BinaryOperator::OpNe {
                    Narrowing::Remove(lit_ty)
                } else {
                    return None;
                });
            }
            // `x == y` / `x ~= y`: the true branch narrows the target variable to the other variable's current type.
            if matches!(op, BinaryOperator::OpEq | BinaryOperator::OpNe) {
                let other_expr = if let LuaExpr::NameExpr(name_expr) = &left
                    && model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
                {
                    Some(&right)
                } else if let LuaExpr::NameExpr(name_expr) = &right
                    && model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
                {
                    Some(&left)
                } else {
                    None
                };
                if let Some(other_expr) = other_expr {
                    let other_ty = match other_expr {
                        LuaExpr::NameExpr(other_name) => {
                            model.type_of_expr(other_name.get_syntax_id())
                        }
                        LuaExpr::IndexExpr(other_index) => model.type_of_expr_at(
                            other_index.get_syntax_id(),
                            other_index.get_range().start(),
                        ),
                        _ => return None,
                    };
                    if !matches!(other_ty, LuaType::Unknown) {
                        if op == BinaryOperator::OpEq {
                            // When the target is a known non-nullable type (e.g. method `self`), equality with another nullable value
                            // cannot equal nil, so keep only the non-nil components.
                            let decl_base = model.type_of_decl(decl).unwrap_or(LuaType::Unknown);
                            if other_ty.is_nullable() && !decl_base.is_nullable() {
                                return Some(Narrowing::Replace(remove_type(
                                    other_ty,
                                    &LuaType::Nil,
                                )));
                            }
                            return Some(Narrowing::Replace(other_ty));
                        }
                        // `x ~= y` true only tells us x is not equal to some concrete value; when y is a wide type you cannot
                        // simply remove that wide type from x's type (two different strings can also be unequal).
                        if is_singleton_value_type(&other_ty) {
                            return Some(Narrowing::Remove(other_ty));
                        }
                    }
                }
            }
            None
        }
        // `not x` true → x is falsy; `#x` true (numeric for upper bound) → x is non-falsy.
        // `not isX(v)` true → return_cast's false branch (fallback).
        LuaExpr::UnaryExpr(unary) => {
            let op = unary.get_op_token()?.get_op();
            let inner = unary.get_expr()?;
            if op == UnaryOperator::OpNot
                && let LuaExpr::CallExpr(call) = &inner
                && let Some((arg, _, fallback_ty, is_self)) = return_cast_for_call(model, call)
                && (!is_self || has_prior_narrowing || !is_conditional_receiver(model, &arg))
                && let Some(fallback_ty) = fallback_ty
                && let LuaExpr::NameExpr(arg_name) = &arg
                && model.resolve_name(arg_name.get_position()).as_ref() == Some(decl)
            {
                return Some(Narrowing::Replace(fallback_ty));
            }
            if op == UnaryOperator::OpNot
                && let LuaExpr::IndexExpr(index_expr) = &inner
            {
                let LuaExpr::NameExpr(name_expr) = index_expr.get_prefix_expr()? else {
                    return None;
                };
                if model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
                    && let Some(key) = member_key_from_index_expr(model, index_expr)
                {
                    return Some(Narrowing::MemberTruthy {
                        key,
                        keep_matching: false,
                    });
                }
            }
            let LuaExpr::NameExpr(name_expr) = inner else {
                return None;
            };
            if model.resolve_name(name_expr.get_position()).as_ref() != Some(decl) {
                return None;
            }
            match op {
                UnaryOperator::OpNot => Some(Narrowing::Falsy),
                UnaryOperator::OpLen => Some(Narrowing::Truthy),
                _ => None,
            }
        }
        // Bare `x`: true branch → x truthy.
        LuaExpr::NameExpr(name_expr) => {
            if model.resolve_name(name_expr.get_position()).as_ref() == Some(decl) {
                Some(Narrowing::Truthy)
            } else {
                None
            }
        }
        // Dynamic member truthiness: `obj[key]` true branch → filter union by member truthiness.
        LuaExpr::IndexExpr(index_expr) => {
            let LuaExpr::NameExpr(name_expr) = index_expr.get_prefix_expr()? else {
                return None;
            };
            if model.resolve_name(name_expr.get_position()).as_ref() != Some(decl) {
                return None;
            }
            let key = member_key_from_index_expr(model, &index_expr)?;
            Some(Narrowing::MemberTruthy {
                key,
                keep_matching: true,
            })
        }
        _ => None,
    }
}

/// False-branch narrowing: `x == nil` false → remove nil; `not x` false → x true → truthy.
fn narrow_condition_false(
    model: &SemanticModel,
    tree: &FlowTree,
    decl: &SemanticId,
    cond_ptr: &emmylua_parser::LuaAstPtr<LuaExpr>,
    flow_id: FlowId,
    has_prior_narrowing: bool,
) -> Option<Narrowing> {
    let chunk = model.chunk()?;
    let cond = LuaExpr::cast(cond_ptr.to_node(&chunk)?.syntax().clone())?;
    if let Some(narrowing) = direct_narrow_condition_false(model, decl, &cond, has_prior_narrowing)
    {
        return Some(narrowing);
    }
    if let Some(narrowing) = array_len_condition(model, decl, &cond, false) {
        return Some(narrowing);
    }
    if let Some(narrowing) =
        alias_condition_narrowing(model, decl, &cond, false, has_prior_narrowing)
    {
        return Some(narrowing);
    }
    correlated_narrowing_for_condition(model, tree, decl, &cond, false, flow_id)
}

fn direct_narrow_condition_false(
    model: &SemanticModel,
    decl: &SemanticId,
    cond: &LuaExpr,
    has_prior_narrowing: bool,
) -> Option<Narrowing> {
    match cond {
        // `x == nil` false → remove nil.
        LuaExpr::BinaryExpr(binary) => {
            let op = binary.get_op_token()?.get_op();
            let (left, right) = binary.get_exprs()?;
            // `x.kind == 'A'` false: keep components different from the literal.
            if matches!(op, BinaryOperator::OpEq | BinaryOperator::OpNe)
                && let Some((disc_decl, key, literal)) =
                    member_discriminant_from_binary(model, &left, &right)
                && &disc_decl == decl
            {
                return Some(Narrowing::MemberDiscriminant {
                    key,
                    literal,
                    keep_matching: op != BinaryOperator::OpEq,
                });
            }
            // `isX(v) == false` / `isX(v) ~= true` false branch → return_cast true branch.
            if let Some((arg, ty, is_self)) =
                return_cast_bool_binary(model, &left, &right, op, false)
                && (!is_self || has_prior_narrowing || !is_conditional_receiver(model, &arg))
                && let LuaExpr::NameExpr(arg_name) = &arg
                && model.resolve_name(arg_name.get_position()).as_ref() == Some(decl)
            {
                return Some(Narrowing::Replace(ty));
            }
            // `typeguard(x) == false` / `typeguard(x) ~= true` false branch → TypeGuard true branch.
            if let Some((arg, guard_ty)) = type_guard_bool_binary(model, &left, &right, op, false)
                && let LuaExpr::NameExpr(arg_name) = &arg
                && model.resolve_name(arg_name.get_position()).as_ref() == Some(decl)
            {
                return Some(Narrowing::Replace(guard_ty));
            }
            // `type(x) == 'name'` false → keep components incompatible with the primitive;
            // `type(x) ~= 'name'` false → keep components compatible with the primitive.
            if let LuaExpr::CallExpr(call) = &left
                && let Some(LuaExpr::NameExpr(callee)) = call.get_prefix_expr()
                && callee.get_name_text().as_deref() == Some("type")
                && let Some(arg) = call.get_args_list()?.get_args().next()
                && let LuaExpr::NameExpr(arg_name) = arg
                && model.resolve_name(arg_name.get_position()).as_ref() == Some(decl)
                && let LuaExpr::LiteralExpr(lit) = &right
                && let Some(LuaLiteralToken::String(str)) = lit.get_literal()
            {
                let primitive_ty = primitive_type_from_name(&str.get_value())?;
                return Some(Narrowing::FilterPrimitive {
                    primitive: primitive_ty,
                    keep_matching: op != BinaryOperator::OpEq,
                });
            }
            if let LuaExpr::NameExpr(name_expr) = &left
                && model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
            {
                if matches!(&right, LuaExpr::LiteralExpr(lit) if matches!(lit.get_literal(), Some(LuaLiteralToken::Nil(_))))
                {
                    return Some(match op {
                        BinaryOperator::OpEq => Narrowing::NotNil,
                        BinaryOperator::OpNe => Narrowing::Replace(LuaType::Nil),
                        _ => return None,
                    });
                }
                if let LuaExpr::LiteralExpr(lit) = &right
                    && let Some(lit_ty) = literal_type(lit)
                {
                    return Some(match op {
                        BinaryOperator::OpEq => Narrowing::Remove(lit_ty),
                        BinaryOperator::OpNe => Narrowing::Replace(lit_ty),
                        _ => return None,
                    });
                }
            }
            // Symmetric handling for `nil == x` / `nil ~= x` false branches.
            if let LuaExpr::NameExpr(name_expr) = &right
                && model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
                && matches!(&left, LuaExpr::LiteralExpr(lit) if matches!(lit.get_literal(), Some(LuaLiteralToken::Nil(_))))
            {
                return Some(match op {
                    BinaryOperator::OpEq => Narrowing::NotNil,
                    BinaryOperator::OpNe => Narrowing::Replace(LuaType::Nil),
                    _ => return None,
                });
            }
            // Symmetric forms: false branches of `1 == x` / `"a" == x`.
            if let LuaExpr::NameExpr(name_expr) = &right
                && model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
                && let LuaExpr::LiteralExpr(lit) = &left
                && let Some(lit_ty) = literal_type(lit)
            {
                return Some(match op {
                    BinaryOperator::OpEq => Narrowing::Remove(lit_ty),
                    BinaryOperator::OpNe => Narrowing::Replace(lit_ty),
                    _ => return None,
                });
            }
            // `x == y` / `x ~= y` false branch: narrow the target variable by reversing the other variable's current type.
            if matches!(op, BinaryOperator::OpEq | BinaryOperator::OpNe) {
                let other_expr = if let LuaExpr::NameExpr(name_expr) = &left
                    && model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
                {
                    Some(&right)
                } else if let LuaExpr::NameExpr(name_expr) = &right
                    && model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
                {
                    Some(&left)
                } else {
                    None
                };
                if let Some(other_expr) = other_expr {
                    let other_ty = match other_expr {
                        LuaExpr::NameExpr(other_name) => {
                            model.type_of_expr(other_name.get_syntax_id())
                        }
                        LuaExpr::IndexExpr(other_index) => model.type_of_expr_at(
                            other_index.get_syntax_id(),
                            other_index.get_range().start(),
                        ),
                        _ => return None,
                    };
                    if !matches!(other_ty, LuaType::Unknown) {
                        if op == BinaryOperator::OpNe {
                            return Some(Narrowing::Replace(other_ty));
                        }
                        // Similarly, `x == y` false cannot remove y's wide type entirely from x.
                        if is_singleton_value_type(&other_ty) {
                            return Some(Narrowing::Remove(other_ty));
                        }
                    }
                }
            }
            None
        }
        // `not x` false → x true → truthy.
        // `not isX(v)` false → return_cast's true branch (cast).
        LuaExpr::UnaryExpr(unary) => {
            if unary.get_op_token()?.get_op() != UnaryOperator::OpNot {
                return None;
            }
            let inner = unary.get_expr()?;
            if let LuaExpr::CallExpr(call) = &inner
                && let Some((arg, cast_ty, _, is_self)) = return_cast_for_call(model, call)
                && (!is_self || has_prior_narrowing || !is_conditional_receiver(model, &arg))
                && let LuaExpr::NameExpr(arg_name) = &arg
                && model.resolve_name(arg_name.get_position()).as_ref() == Some(decl)
            {
                return Some(Narrowing::Replace(cast_ty));
            }
            if let LuaExpr::CallExpr(call) = &inner
                && let Some((arg, guard_ty)) = type_guard_call_target(model, call)
                && let LuaExpr::NameExpr(arg_name) = &arg
                && model.resolve_name(arg_name.get_position()).as_ref() == Some(decl)
            {
                return Some(Narrowing::Replace(guard_ty));
            }
            if let LuaExpr::IndexExpr(index_expr) = &inner {
                let LuaExpr::NameExpr(name_expr) = index_expr.get_prefix_expr()? else {
                    return None;
                };
                if model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
                    && let Some(key) = member_key_from_index_expr(model, index_expr)
                {
                    return Some(Narrowing::MemberTruthy {
                        key,
                        keep_matching: true,
                    });
                }
            }
            if let LuaExpr::NameExpr(name_expr) = inner
                && model.resolve_name(name_expr.get_position()).as_ref() == Some(decl)
            {
                return Some(Narrowing::Truthy);
            }
            None
        }
        // Bare `x` false → x falsy.
        LuaExpr::NameExpr(name_expr) => {
            if model.resolve_name(name_expr.get_position()).as_ref() == Some(decl) {
                Some(Narrowing::Falsy)
            } else {
                None
            }
        }
        // Dynamic member truthiness false branch: `obj[key]` false → keep components where the member is falsy.
        LuaExpr::IndexExpr(index_expr) => {
            let LuaExpr::NameExpr(name_expr) = index_expr.get_prefix_expr()? else {
                return None;
            };
            if model.resolve_name(name_expr.get_position()).as_ref() != Some(decl) {
                return None;
            }
            let key = member_key_from_index_expr(model, &index_expr)?;
            Some(Narrowing::MemberTruthy {
                key,
                keep_matching: false,
            })
        }
        // `---@return_cast name Type else Fallback`: the false branch prefers fallback;
        // when there is no fallback, remove the cast type (compatible with old behavior).
        LuaExpr::CallExpr(call) => {
            if let Some((arg, cast_ty, fallback_ty, is_self)) = return_cast_for_call(model, call)
                && (!is_self || has_prior_narrowing || !is_conditional_receiver(model, &arg))
                && let LuaExpr::NameExpr(arg_name) = &arg
                && model.resolve_name(arg_name.get_position()).as_ref() == Some(decl)
            {
                if let Some(fallback_ty) = fallback_ty {
                    return Some(Narrowing::Replace(fallback_ty));
                }
                if !matches!(cast_ty, LuaType::Unknown) {
                    return Some(Narrowing::Remove(cast_ty));
                }
            }
            None
        }
        _ => None,
    }
}

/// Remove falsy components (false / nil).
fn remove_falsy(ty: LuaType) -> LuaType {
    let mut types: Vec<LuaType> = Vec::new();
    for component in match ty {
        LuaType::Union(union) => union.into_vec(),
        other => vec![other],
    } {
        if !matches!(
            component,
            LuaType::Nil
                | LuaType::Boolean
                | LuaType::BooleanConst(false)
                | LuaType::DocBooleanConst(false)
        ) {
            types.push(component);
        }
    }
    match types.len() {
        0 => LuaType::Unknown,
        1 => types.pop().expect("len checked"),
        _ => LuaType::Union(crate::LuaUnionType::from_vec(types).into()),
    }
}

/// Remove truthy components (keep only false / nil).
fn remove_truthy(ty: LuaType) -> LuaType {
    let mut types: Vec<LuaType> = Vec::new();
    for component in match ty {
        LuaType::Union(union) => union.into_vec(),
        other => vec![other],
    } {
        if matches!(
            component,
            LuaType::Nil | LuaType::BooleanConst(false) | LuaType::DocBooleanConst(false)
        ) {
            types.push(component);
        }
    }
    match types.len() {
        0 => LuaType::Unknown,
        1 => types.pop().expect("len checked"),
        _ => LuaType::Union(crate::LuaUnionType::from_vec(types).into()),
    }
}

/// Whether `component` should be removed by `removed` (including `integer <: number`, literals <: base types).
fn type_is_removed_by(component: &LuaType, removed: &LuaType) -> bool {
    if component == removed {
        return true;
    }
    matches!(
        (component, removed),
        (
            LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_),
            LuaType::Number,
        ) | (
            LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_),
            LuaType::Integer
        ) | (
            LuaType::StringConst(_) | LuaType::DocStringConst(_),
            LuaType::String
        ) | (
            LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_),
            LuaType::Boolean
        )
    )
}

/// Remove the specified components from a type (when `removed` is a union, remove all its components).
fn remove_type(ty: LuaType, removed: &LuaType) -> LuaType {
    let removed_components: Vec<LuaType> = match removed {
        LuaType::Union(union) => union.into_vec(),
        other => vec![other.clone()],
    };
    let mut types: Vec<LuaType> = Vec::new();
    for component in match ty {
        LuaType::Union(union) => union.into_vec(),
        other => vec![other],
    } {
        if !removed_components
            .iter()
            .any(|removed| type_is_removed_by(&component, removed))
        {
            types.push(component);
        }
    }
    match types.len() {
        0 => LuaType::Unknown,
        1 => types.pop().expect("len checked"),
        _ => LuaType::Union(crate::LuaUnionType::from_vec(types).into()),
    }
}

/// Literal expression → literal type (for comparison guards).
fn literal_type(lit: &emmylua_parser::LuaLiteralExpr) -> Option<LuaType> {
    match lit.get_literal()? {
        LuaLiteralToken::Nil(_) => Some(LuaType::Nil),
        LuaLiteralToken::Bool(bool_token) => Some(LuaType::BooleanConst(bool_token.is_true())),
        LuaLiteralToken::String(str_token) => Some(LuaType::StringConst(
            smol_str::SmolStr::new(str_token.get_value()).into(),
        )),
        LuaLiteralToken::Number(num_token) => match num_token.get_number_value() {
            emmylua_parser::NumberResult::Int(i) => Some(LuaType::IntegerConst(i)),
            _ => Some(LuaType::Number),
        },
        _ => None,
    }
}

/// Whether a type is a "single-value type": `nil`, or a concrete string/integer/boolean constant.
/// Used for variable equality/inequality guards: only when the other side is a single value can false branches / inequality true branches safely "remove that type".
fn is_singleton_value_type(ty: &LuaType) -> bool {
    matches!(
        ty,
        LuaType::Nil
            | LuaType::StringConst(_)
            | LuaType::DocStringConst(_)
            | LuaType::IntegerConst(_)
            | LuaType::DocIntegerConst(_)
            | LuaType::BooleanConst(_)
            | LuaType::DocBooleanConst(_)
    )
}

/// Whether `prefix` is more specific than `base`, suitable for member projection.
/// Note: `type_check` treats source unions as "passes if any component is compatible";
/// here every component of the union must be compatible, avoiding `LeftFoo|LeftBar` projecting to `LeftFoo`.
fn member_type_more_specific(model: &SemanticModel, prefix: &LuaType, base: &LuaType) -> bool {
    if is_singleton_value_type(base) {
        return prefix == base;
    }
    match (prefix, base) {
        (LuaType::Union(prefix_union), LuaType::Union(base_union)) => {
            let base_types = base_union.into_vec();
            prefix_union.into_vec().iter().all(|component| {
                base_types
                    .iter()
                    .any(|base| model.type_check(component, base))
            })
        }
        (LuaType::Union(prefix_union), _) => prefix_union
            .into_vec()
            .iter()
            .all(|component| model.type_check(component, base)),
        (_, LuaType::Union(base_union)) => base_union
            .into_vec()
            .iter()
            .any(|base| model.type_check(prefix, base)),
        _ => model.type_check(prefix, base),
    }
}

/// Primitive type name → `LuaType` (the name returned by `type()`).
fn primitive_type_from_name(name: &str) -> Option<LuaType> {
    Some(match name {
        "nil" => LuaType::Nil,
        "boolean" => LuaType::Boolean,
        "number" => LuaType::Number,
        "string" => LuaType::String,
        "table" => LuaType::Table,
        "function" => LuaType::Function,
        "userdata" => LuaType::Userdata,
        "thread" => LuaType::Thread,
        _ => return None,
    })
}

/// Normalize built-in type references like `Ref("string")` to the corresponding primitive `LuaType`.
fn normalize_builtin_type(ty: LuaType) -> LuaType {
    match &ty {
        LuaType::Ref(id) | LuaType::Def(id) => {
            primitive_type_from_name(id.get_name()).unwrap_or(ty)
        }
        _ => ty,
    }
}

/// Filter a union by `type(x)`'s primitive: `keep_matching=true` keeps components compatible with `primitive`,
/// `false` keeps incompatible components (true branch of `type(x) ~= 'table'`).
fn filter_type_by_primitive(
    model: &SemanticModel,
    ty: LuaType,
    primitive: &LuaType,
    keep_matching: bool,
) -> LuaType {
    if matches!(ty, LuaType::Any | LuaType::Unknown) {
        // `any` should still narrow to `table` in the true branch of `type(x)=='table'`.
        return if keep_matching { primitive.clone() } else { ty };
    }
    let components: Vec<LuaType> = match ty {
        LuaType::Union(union) => union.into_vec(),
        other => vec![other],
    };
    let mut kept = Vec::new();
    for component in components {
        // Aliases need to be expanded to their target union before filtering, otherwise `alias = string|string[]`
        // would incorrectly keep the whole alias in the `type(x)=='string'` branch.
        if let LuaType::Ref(id) | LuaType::Def(id) = &component {
            if let Some(def) = model.type_def_of(id)
                && def.kind == crate::TypeDefKind::Alias
                && let Some(target) = model.alias_target(&def)
            {
                // Single-target aliases (e.g. `---@alias MyFun fun(): string[]`) keep the named alias:
                // expanding directly to DocFunction would lose the "this is an alias" information callers need (e.g. pcall error slots).
                if !matches!(target, LuaType::Union(_) | LuaType::MultiLineUnion(_))
                    && component_matches_primitive(model, &component, primitive)
                {
                    kept.push(component.clone());
                    continue;
                }
                let expanded = filter_type_by_primitive(model, target, primitive, keep_matching);
                match expanded {
                    LuaType::Unknown => {}
                    LuaType::Union(union) => kept.extend(union.into_vec()),
                    other => kept.push(other),
                }
                continue;
            }
        }
        let matched = component_matches_primitive(model, &component, primitive);
        if matched == keep_matching {
            kept.push(component);
        }
    }
    match kept.len() {
        0 => LuaType::Never,
        1 => kept.pop().expect("len checked"),
        _ => LuaType::Union(crate::LuaUnionType::from_vec(kept).into()),
    }
}

/// Member discriminant narrowing: when `x.kind == 'A'`, filter union components by whether their member type overlaps the literal.
fn filter_type_by_member_discriminant(
    model: &SemanticModel,
    ty: LuaType,
    key: &LuaMemberKey,
    literal: &LuaType,
    keep_matching: bool,
) -> LuaType {
    let components: Vec<LuaType> = match ty {
        LuaType::Union(union) => union.into_vec(),
        other => vec![other],
    };
    let mut kept = Vec::new();
    for component in components {
        if let LuaType::Ref(id) | LuaType::Def(id) = &component {
            if let Some(def) = model.type_def_of(id)
                && def.kind == crate::TypeDefKind::Alias
                && let Some(target) = model.alias_target(&def)
            {
                let expanded =
                    filter_type_by_member_discriminant(model, target, key, literal, keep_matching);
                // Member discriminant should keep the original named type (`Ref(A)`) rather than expanding into an anonymous object.
                // `Never` means the alias is completely excluded under this branch.
                if !matches!(expanded, LuaType::Unknown | LuaType::Never) {
                    kept.push(component);
                }
                continue;
            }
        }
        let matched = model
            .member_type(&component, key)
            .map(|member_ty| types_may_overlap(member_ty, literal.clone()))
            .unwrap_or(false);
        if matched == keep_matching {
            kept.push(component);
        }
    }
    match kept.len() {
        0 => LuaType::Never,
        1 => kept.pop().expect("len checked"),
        _ => LuaType::Union(crate::LuaUnionType::from_vec(kept).into()),
    }
}

/// Dynamic member truthiness narrowing: when `obj[key]` is true, keep union components whose member is truthy;
/// when false, keep falsy (nil/false) components.
fn filter_type_by_member_truthiness(
    model: &SemanticModel,
    ty: LuaType,
    key: &LuaMemberKey,
    keep_matching: bool,
) -> LuaType {
    let components: Vec<LuaType> = match ty {
        LuaType::Union(union) => union.into_vec(),
        other => vec![other],
    };
    let mut kept = Vec::new();
    for component in components {
        if let LuaType::Ref(id) | LuaType::Def(id) = &component {
            if let Some(def) = model.type_def_of(id)
                && def.kind == crate::TypeDefKind::Alias
                && let Some(target) = model.alias_target(&def)
            {
                let expanded = filter_type_by_member_truthiness(model, target, key, keep_matching);
                if !matches!(expanded, LuaType::Unknown | LuaType::Never) {
                    kept.push(component);
                }
                continue;
            }
        }
        // Missing member is equivalent to nil: `obj.missing` is false.
        let member_ty = model.member_type(&component, key).unwrap_or(LuaType::Nil);
        let truthy = type_is_truthy(&member_ty);
        if truthy == keep_matching {
            kept.push(component);
        }
    }
    match kept.len() {
        0 => LuaType::Never,
        1 => kept.pop().expect("len checked"),
        _ => LuaType::Union(crate::LuaUnionType::from_vec(kept).into()),
    }
}

fn type_is_truthy(ty: &LuaType) -> bool {
    let components: Vec<LuaType> = match ty {
        LuaType::Union(union) => union.into_vec(),
        other => vec![other.clone()],
    };
    !components.iter().all(|component| {
        matches!(
            component,
            LuaType::Nil | LuaType::BooleanConst(false) | LuaType::DocBooleanConst(false)
        )
    })
}

fn component_matches_primitive(
    model: &SemanticModel,
    component: &LuaType,
    primitive: &LuaType,
) -> bool {
    if component == primitive {
        return true;
    }
    // A function generic parameter `T` projects to `Ref(Global("T"))` with no actual type definition;
    // in that case check the `---@generic T: table` constraint to decide if it counts as table-compatible.
    if let LuaType::Ref(id) | LuaType::Def(id) = component {
        if model.type_def_of(id).is_none()
            && let Some(constraint_ty) = generic_param_constraint_type(model, id.get_name())
        {
            return component_matches_primitive(model, &constraint_ty, primitive);
        }
    }
    if let LuaType::TplRef(tpl) = component {
        if let Some(constraint_ty) = generic_param_constraint_type(model, tpl.get_name()) {
            return component_matches_primitive(model, &constraint_ty, primitive);
        }
    }
    if matches!(
        (component, primitive),
        (
            LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_),
            LuaType::Number | LuaType::Integer,
        ) | (
            LuaType::StringConst(_) | LuaType::DocStringConst(_),
            LuaType::String,
        ) | (
            LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_),
            LuaType::Boolean,
        )
    ) {
        return true;
    }
    // `---@enum` types represent member values (numbers/strings), not runtime tables;
    // even if the `---@enum` table itself is a table, `type(x) == 'table'` cannot keep enum member values.
    if *primitive == LuaType::Table
        && let LuaType::Ref(id) | LuaType::Def(id) = component
        && let Some(def) = model.type_def_of(id)
        && def.kind == crate::TypeDefKind::Enum
    {
        return false;
    }
    model.type_check(component, primitive)
}

/// Find the `---@generic T: ...` constraint type in the current file's function signatures.
fn generic_param_constraint_type(model: &SemanticModel, name: &str) -> Option<LuaType> {
    let signatures = model.signatures()?;
    let file_id = model.file_id();
    for signature in signatures {
        let Some(docs) = &signature.docs else {
            continue;
        };
        if let Some(param) = docs
            .generic_params
            .iter()
            .find(|param| param.name.as_str() == name)
            && let Some(constraint_syntax) = param.constraint
        {
            return Some(model.doc_type_lua_in(file_id, constraint_syntax, &docs.generic_params));
        }
    }
    None
}

/// Whether a condition expression may narrow `decl`: the condition names that declaration, or a discriminant variable sharing
/// its multi-return source (return_overload correlated narrowing). Used to safely skip CFG branches unrelated to the target declaration
/// during branch merges, avoiding exponential expansion on large linear fragments.
fn condition_may_narrow_decl(
    model: &SemanticModel,
    tree: &FlowTree,
    decl: &SemanticId,
    cond_ptr: &emmylua_parser::LuaAstPtr<LuaExpr>,
) -> bool {
    let Some(cond) = flow_condition_expr(model, cond_ptr) else {
        return false;
    };
    let mut decls = Vec::new();
    collect_condition_name_decls(model, &cond, &mut decls);
    decls
        .iter()
        .any(|d| d == decl || tree.has_shared_multi_return_refs(d, decl))
}

/// Compute the "local" nearest common predecessor of a group of branches: each branch follows Single predecessors until the first
/// Multiple or the start. For a simple if/else diamond, the common predecessor is the node containing that Multiple.
/// This differs from `FlowTree::get_nearest_common_antecedent`, which collects all predecessors back to the file start
/// and costs O(chain length) per merge on large linear fragments.
fn local_merge_common(tree: &FlowTree, branches: &[FlowId]) -> Option<FlowId> {
    let common_for = |start: FlowId| -> Option<FlowId> {
        let mut current = start;
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(current) {
                return None;
            }
            let node = tree.get_flow_node(current)?;
            match node.antecedent.as_ref()? {
                FlowAntecedent::Single(next) => current = *next,
                FlowAntecedent::Multiple(_) => return Some(current),
            }
        }
    };
    let first = common_for(*branches.first()?)?;
    branches
        .iter()
        .skip(1)
        .all(|branch| common_for(*branch) == Some(first))
        .then_some(first)
}

/// Whether the linear branch from `start` to `stop` (excluding `stop`, the common predecessor) has no effect on `decl`.
/// No effect means: no assignment to `decl`, no guard/cast that may narrow `decl`, and no call that definitely does not return.
fn decl_branch_is_pure(
    model: &SemanticModel,
    tree: &FlowTree,
    decl: &SemanticId,
    options: TraceOptions,
    start: FlowId,
    stop: FlowId,
) -> bool {
    let mut current = start;
    let mut visited = HashSet::new();
    loop {
        if current == stop {
            return true;
        }
        if !visited.insert(current) {
            return false;
        }
        let Some(node) = tree.get_flow_node(current) else {
            return false;
        };
        match &node.kind {
            FlowNodeKind::Assignment(_) => {
                if options.assignments
                    && tree.get_flow_effects(current).iter().any(|effect| {
                        matches!(effect, FlowEffect::AssignDecl { decl: d, .. } if d == decl)
                    })
                {
                    return false;
                }
            }
            FlowNodeKind::TrueCondition(cond_ptr) | FlowNodeKind::FalseCondition(cond_ptr) => {
                if options.guards && condition_may_narrow_decl(model, tree, decl, cond_ptr) {
                    return false;
                }
            }
            FlowNodeKind::CallExprStat(call_stat_ptr) => {
                if call_stat_returns_never(model, call_stat_ptr) {
                    return false;
                }
            }
            FlowNodeKind::TagCast(_) | FlowNodeKind::AsCast(_) => {
                if options.casts {
                    return false;
                }
            }
            _ => {}
        }
        match node.antecedent.as_ref() {
            Some(FlowAntecedent::Single(next)) => current = *next,
            Some(FlowAntecedent::Multiple(_)) | None => return false,
        }
    }
}

fn walk_decl(
    model: &SemanticModel,
    decl: &SemanticId,
    tree: &FlowTree,
    antecedent: Option<&FlowAntecedent>,
    options: TraceOptions,
    mode: TraceMode,
    visited: &mut HashSet<FlowId>,
    path: &mut PathState,
) -> Option<LuaType> {
    match antecedent {
        // Backtracked to the start: use the declaration type as the base and apply guards / casts collected on the path.
        None => model.type_of_decl(decl).map(|ty| finalize(model, ty, path)),
        Some(FlowAntecedent::Single(next)) => {
            trace_decl(model, decl, tree, *next, options, mode, visited, path)
        }
        Some(FlowAntecedent::Multiple(multi_id)) => {
            // Branch merge: each branch backtracks independently (visited / path must not be shared across branches).
            // FLOW_READ takes the union; ASSIGN_TARGET takes the intersection — casts unique to one branch do not leak after the merge,
            // while casts present on both sides before the merge are retained.
            let branches = tree.get_multi_antecedents(*multi_id)?;
            // If all branches before merging into the next common predecessor have no effect on the target declaration, there is no need to expand
            // branch by branch: they all produce the same type, so just continue along the common predecessor. This avoids 2^N exponential
            // backtracking from large linear `if` fragments (where each branch is just an ordinary call/unrelated guard).
            if let Some(common) = local_merge_common(tree, branches)
                && branches
                    .iter()
                    .all(|branch| decl_branch_is_pure(model, tree, decl, options, *branch, common))
            {
                return trace_decl(model, decl, tree, common, options, mode, visited, path);
            }
            let mut merged = LuaType::Unknown;
            let branch_mode = TraceMode::MergeBranch;
            for branch in branches {
                let mut branch_visited = visited.clone();
                let mut branch_path = path.clone_path();
                if let Some(branch_ty) = trace_decl(
                    model,
                    decl,
                    tree,
                    *branch,
                    options,
                    branch_mode,
                    &mut branch_visited,
                    &mut branch_path,
                ) {
                    if branch_ty.is_never() {
                        continue;
                    }
                    merged = if options.assignments {
                        merge_types(merged, branch_ty)
                    } else {
                        intersect_types(merged, branch_ty)
                    };
                }
            }
            Some(merged)
        }
    }
}

/// Whether the linear branch from `start` to `stop` (excluding `stop`) has no effect on member `member`.
fn member_branch_is_pure(
    tree: &FlowTree,
    member: &SemanticId,
    start: FlowId,
    stop: FlowId,
) -> bool {
    let mut current = start;
    let mut visited = HashSet::new();
    loop {
        if current == stop {
            return true;
        }
        if !visited.insert(current) {
            return false;
        }
        let Some(node) = tree.get_flow_node(current) else {
            return false;
        };
        match &node.kind {
            FlowNodeKind::Assignment(_) => {
                if tree.get_flow_effects(current).iter().any(|effect| {
                    matches!(
                        effect,
                        FlowEffect::AssignMember {
                            member: assigned_member,
                            ..
                        } if assigned_member == member
                    )
                }) {
                    return false;
                }
            }
            FlowNodeKind::TagCast(_) | FlowNodeKind::AsCast(_) => return false,
            _ => {}
        }
        match node.antecedent.as_ref() {
            Some(FlowAntecedent::Single(next)) => current = *next,
            Some(FlowAntecedent::Multiple(_)) | None => return false,
        }
    }
}

fn walk_member(
    model: &SemanticModel,
    member: &SemanticId,
    tree: &FlowTree,
    antecedent: Option<&FlowAntecedent>,
    visited: &mut HashSet<FlowId>,
    path: &mut PathState,
) -> Option<LuaType> {
    match antecedent {
        // Backtracked to the start: use the member's declaration type as the base and apply cast widening collected on the path.
        None => flow_member_value_type(model, member).map(|ty| finalize(model, ty, path)),
        Some(FlowAntecedent::Single(next)) => {
            trace_member(model, member, tree, *next, visited, path)
        }
        Some(FlowAntecedent::Multiple(multi_id)) => {
            let branches = tree.get_multi_antecedents(*multi_id)?;
            if let Some(common) = local_merge_common(tree, branches)
                && branches
                    .iter()
                    .all(|branch| member_branch_is_pure(tree, member, *branch, common))
            {
                return trace_member(model, member, tree, common, visited, path);
            }
            let mut merged = LuaType::Unknown;
            for branch in branches {
                let mut branch_visited = visited.clone();
                let mut branch_path = path.clone_path();
                if let Some(branch_ty) = trace_member(
                    model,
                    member,
                    tree,
                    *branch,
                    &mut branch_visited,
                    &mut branch_path,
                ) {
                    merged = merge_types(merged, branch_ty);
                }
            }
            Some(merged)
        }
    }
}

impl PathState {
    fn clone_path(&self) -> Self {
        Self {
            narrowings: self.narrowings.iter().map(|n| n.clone_ref()).collect(),
            casts: self.casts.clone(),
        }
    }
}

impl Narrowing {
    fn clone_ref(&self) -> Self {
        match self {
            Narrowing::Replace(ty) => Narrowing::Replace(ty.clone()),
            Narrowing::Remove(ty) => Narrowing::Remove(ty.clone()),
            Narrowing::Truthy => Narrowing::Truthy,
            Narrowing::Falsy => Narrowing::Falsy,
            Narrowing::NotNil => Narrowing::NotNil,
            Narrowing::Correlated {
                matching_target_types,
                correlated_candidate_types,
            } => Narrowing::Correlated {
                matching_target_types: matching_target_types.clone(),
                correlated_candidate_types: correlated_candidate_types.clone(),
            },
            Narrowing::FilterPrimitive {
                primitive,
                keep_matching,
            } => Narrowing::FilterPrimitive {
                primitive: primitive.clone(),
                keep_matching: *keep_matching,
            },
            Narrowing::MemberDiscriminant {
                key,
                literal,
                keep_matching,
            } => Narrowing::MemberDiscriminant {
                key: key.clone(),
                literal: literal.clone(),
                keep_matching: *keep_matching,
            },
            Narrowing::MemberTruthy { key, keep_matching } => Narrowing::MemberTruthy {
                key: key.clone(),
                keep_matching: *keep_matching,
            },
            Narrowing::ArrayLen(max) => Narrowing::ArrayLen(*max),
            Narrowing::ArrayMinLen(min) => Narrowing::ArrayMinLen(*min),
        }
    }
}

/// Whether two types may overlap (used for return_overload discriminant slot matching).
fn types_may_overlap(left: LuaType, right: LuaType) -> bool {
    if matches!(left, LuaType::Unknown | LuaType::Any)
        || matches!(right, LuaType::Unknown | LuaType::Any)
    {
        return true;
    }
    let inter = narrow_intersect_types(left, right);
    !inter.is_never() && !matches!(inter, LuaType::Unknown)
}

/// Intersection with primitive subtyping (`integer <: number`, literals <: base types).
fn narrow_intersect_types(left: LuaType, right: LuaType) -> LuaType {
    let base = intersect_types(left.clone(), right.clone());
    if !matches!(base, LuaType::Unknown) {
        return base;
    }
    match (&left, &right) {
        (LuaType::Integer, LuaType::Number) | (LuaType::Number, LuaType::Integer) => {
            LuaType::Integer
        }
        (
            LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_),
            LuaType::Integer | LuaType::Number,
        )
        | (
            LuaType::Integer | LuaType::Number,
            LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_),
        ) => left,
        (LuaType::StringConst(_) | LuaType::DocStringConst(_), LuaType::String)
        | (LuaType::String, LuaType::StringConst(_) | LuaType::DocStringConst(_)) => left,
        (LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_), LuaType::Boolean)
        | (LuaType::Boolean, LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_)) => left,
        _ => base,
    }
}

/// Intersection of two types (`Unknown` is the identity; unions expand and take common components).
fn intersect_types(left: LuaType, right: LuaType) -> LuaType {
    if left == right {
        return left;
    }
    let left_components: Vec<LuaType> = match left {
        LuaType::Unknown => return right,
        LuaType::Union(union) => union.into_vec(),
        other => vec![other],
    };
    let right_components: Vec<LuaType> = match right {
        LuaType::Unknown => return merge_components(left_components),
        LuaType::Union(union) => union.into_vec(),
        other => vec![other],
    };
    let common: Vec<LuaType> = left_components
        .into_iter()
        .filter(|component| right_components.contains(component))
        .collect();
    merge_components(common)
}

fn merge_components(types: Vec<LuaType>) -> LuaType {
    match types.len() {
        0 => LuaType::Unknown,
        1 => types.into_iter().next().expect("len checked"),
        _ => LuaType::Union(crate::LuaUnionType::from_vec(types).into()),
    }
}

/// Union of two types (`Unknown` is the absorbing element; unions expand and deduplicate).
fn merge_types(left: LuaType, right: LuaType) -> LuaType {
    if left == right {
        return left;
    }
    // Unreachable branches do not participate in the union: `never | T = T`.
    if left.is_never() {
        return right;
    }
    if right.is_never() {
        return left;
    }
    let mut candidates: Vec<LuaType> = Vec::new();
    for ty in [left, right] {
        match ty {
            LuaType::Unknown => {}
            LuaType::Union(union) => candidates.extend(union.into_vec()),
            other => candidates.push(other),
        }
    }
    let mut types: Vec<LuaType> = Vec::new();
    for ty in candidates {
        // Wider base types absorb their constant/narrower type subsets (`"a" | string` → `string`).
        if types.iter().any(|existing| type_is_broader(existing, &ty)) {
            continue;
        }
        types.retain(|existing| !type_is_broader(&ty, existing));
        types.push(ty);
    }
    match types.len() {
        0 => LuaType::Unknown,
        1 => types.pop().expect("len checked"),
        _ => LuaType::Union(crate::LuaUnionType::from_vec(types).into()),
    }
}

fn type_is_broader(broad: &LuaType, narrow: &LuaType) -> bool {
    matches!(
        (broad, narrow),
        (LuaType::String, LuaType::StringConst(_))
            | (LuaType::String, LuaType::DocStringConst(_))
            | (LuaType::Integer, LuaType::IntegerConst(_))
            | (LuaType::Integer, LuaType::DocIntegerConst(_))
            | (LuaType::Integer, LuaType::Number)
            | (LuaType::Number, LuaType::Integer)
            | (LuaType::Number, LuaType::IntegerConst(_))
            | (LuaType::Number, LuaType::DocIntegerConst(_))
            | (LuaType::Number, LuaType::FloatConst(_))
            | (LuaType::Boolean, LuaType::BooleanConst(_))
            | (LuaType::Boolean, LuaType::DocBooleanConst(_))
            | (LuaType::Table, LuaType::TableConst(_))
            | (LuaType::Table, LuaType::Array(_))
            | (LuaType::Table, LuaType::Object(_))
            | (LuaType::Table, LuaType::Tuple(_))
            | (LuaType::Function, LuaType::DocFunction(_))
            | (LuaType::Function, LuaType::Signature(_))
    )
}
