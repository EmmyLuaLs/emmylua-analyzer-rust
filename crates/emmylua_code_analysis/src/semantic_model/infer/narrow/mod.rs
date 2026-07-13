#[cfg(test)]
mod test;

use crate::compilation::{
    ConditionNodeId, SalsaDeclTreeSummary, SalsaFlowConditionKindSummary,
    SalsaFlowConditionSummary, SalsaFlowEdgeKindSummary, SalsaFlowEdgeSummary,
    SalsaFlowLoopKindSummary, SalsaFlowNodeRefSummary, SalsaFlowQuerySummary, SalsaSummaryDatabase,
};
use crate::semantic_model::infer::lowered_node_to_lua_type;
use crate::{
    FileId, LuaType, SalsaDocOwnerSummary, SalsaDocTagDataSummary, SalsaDocTagKindSummary,
    SalsaDocTypeLoweredKind, SalsaDocTypeLoweredNode, SalsaDocTypeNodeKey, SalsaDocTypeRef,
    SalsaFlowLoopSummary,
};
use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaChunk, LuaExpr, LuaIndexExpr, LuaLiteralExpr, LuaLiteralToken,
    LuaNameExpr, UnaryOperator,
};
use rowan::TextSize;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub(crate) struct FlowNarrow {
    condition_offset: ConditionNodeId,
    is_true_branch: bool,
}

enum ConditionEffect {
    Truthy,
    Falsy,
    EqLiteral(LuaType),
    NeqLiteral(LuaType),
    TypeGuard(LuaType),
}

#[derive(Debug, Clone)]
pub(crate) struct BlockPred {
    kind: SalsaFlowEdgeKindSummary,
    source: SalsaFlowNodeRefSummary,
}

pub(crate) struct FlowIndex {
    pub(crate) block_to_branch_clause: HashMap<TextSize, (TextSize, usize)>,
    pub(crate) block_to_parent: HashMap<TextSize, TextSize>,
    pub(crate) entry_to_branch_clauses: HashMap<TextSize, Vec<TextSize>>,
    pub(crate) body_to_is_repeat: HashMap<TextSize, bool>,
    pub(crate) stmt_enclosing_block: HashMap<TextSize, TextSize>,
    pub(crate) block_preds: HashMap<TextSize, Vec<BlockPred>>,
    pub(crate) loop_condition_offsets: HashMap<TextSize, Option<ConditionNodeId>>,
}

impl FlowIndex {
    pub(crate) fn new(query: &SalsaFlowQuerySummary, loops: &[SalsaFlowLoopSummary]) -> Self {
        let mut block_to_branch_clause = HashMap::new();
        let mut block_to_parent = HashMap::new();
        let mut entry_to_branch_clauses: HashMap<TextSize, Vec<TextSize>> = HashMap::new();
        let mut stmt_enclosing_block = HashMap::new();
        let mut block_preds: HashMap<TextSize, Vec<BlockPred>> = HashMap::new();
        let mut body_to_is_repeat = HashMap::new();
        let mut loop_condition_offsets: HashMap<TextSize, Option<ConditionNodeId>> = HashMap::new();

        for l in loops {
            if let Some(body) = l.block_offset {
                body_to_is_repeat.insert(body, l.kind == SalsaFlowLoopKindSummary::Repeat);
            }
            loop_condition_offsets.insert(l.syntax_offset, l.condition_node_offset);
        }

        for branch in &query.branch_links {
            for (clause_idx, &clause_block) in branch.clause_block_offsets.iter().enumerate() {
                block_to_branch_clause.insert(clause_block, (branch.branch_offset, clause_idx));
            }
            if let Some(entry) = branch.entry_block_offset {
                for &clause_block in &branch.clause_block_offsets {
                    block_to_parent.insert(clause_block, entry);
                }
                entry_to_branch_clauses
                    .entry(entry)
                    .or_default()
                    .extend(branch.clause_block_offsets.iter().copied());
            }
        }

        // Build statement→block and block→predecessor indices
        for edge in &query.edges {
            match (&edge.from, &edge.to, &edge.kind) {
                (SalsaFlowNodeRefSummary::Block(b), SalsaFlowNodeRefSummary::Statement(s), _) => {
                    stmt_enclosing_block.insert(*s, *b);
                }
                (_, SalsaFlowNodeRefSummary::Block(b), kind) => {
                    block_preds.entry(*b).or_default().push(BlockPred {
                        kind: kind.clone(),
                        source: edge.from.clone(),
                    });
                }
                _ => {}
            }
        }

        Self {
            block_to_branch_clause,
            block_to_parent,
            entry_to_branch_clauses,
            body_to_is_repeat,
            stmt_enclosing_block,
            block_preds,
            loop_condition_offsets,
        }
    }
}

fn find_correlated_siblings(
    tree: &SalsaDeclTreeSummary,
    var_name: &str,
) -> Option<Vec<(SmolStr, u32)>> {
    let our_decl = tree.decls.iter().find(|d| d.name == var_name)?;
    let call_sid = our_decl.source_call_syntax_id?;
    let siblings: Vec<(SmolStr, u32)> = tree
        .decls
        .iter()
        .filter(|d| {
            d.name != var_name
                && d.source_call_syntax_id
                    .is_some_and(|sid| sid.start_offset == call_sid.start_offset)
        })
        .map(|d| (d.name.clone(), d.value_result_index as u32))
        .collect();
    if siblings.is_empty() {
        None
    } else {
        Some(siblings)
    }
}

fn find_enclosing_block(flow_index: &FlowIndex, target_offset: TextSize) -> Option<TextSize> {
    flow_index.stmt_enclosing_block.get(&target_offset).copied()
}

fn add_narrow(
    narrows: &mut Vec<FlowNarrow>,
    condition_offset: ConditionNodeId,
    is_true_branch: bool,
) {
    if !narrows
        .iter()
        .any(|n| n.condition_offset == condition_offset)
    {
        narrows.push(FlowNarrow {
            condition_offset,
            is_true_branch,
        });
    }
}

pub fn collect_dominating_conditions(
    flow_query: &SalsaFlowQuerySummary,
    target_offset: TextSize,
) -> Vec<FlowNarrow> {
    let flow_index = FlowIndex::new(flow_query, &[]);
    let Some(enclosing_block) = find_enclosing_block(&flow_index, target_offset) else {
        return vec![];
    };
    let mut all_narrows = vec![];
    let mut current = enclosing_block;
    loop {
        let narrows = collect_conditions_for_block(flow_query, &flow_index, current);
        all_narrows.extend(narrows);
        if let Some(parent) = flow_index.block_to_parent.get(&current).copied() {
            current = parent;
        } else {
            break;
        }
    }
    all_narrows
}

fn clause_condition(
    query: &SalsaFlowQuerySummary,
    branch_offset: TextSize,
    clause_idx: usize,
) -> Option<ConditionNodeId> {
    let branch = query
        .branch_links
        .iter()
        .find(|b| b.branch_offset == branch_offset)?;
    if clause_idx >= branch.clause_block_offsets.len() {
        return None;
    }
    let clause_block = branch.clause_block_offsets[clause_idx];
    let block_node = SalsaFlowNodeRefSummary::Block(clause_block);
    let expected_kind = if clause_idx == 0 {
        SalsaFlowEdgeKindSummary::ConditionTrue
    } else {
        SalsaFlowEdgeKindSummary::ConditionFalse
    };
    query
        .edges
        .iter()
        .find(|e| e.to == block_node && e.kind == expected_kind)
        .and_then(|e| match e.from {
            SalsaFlowNodeRefSummary::Condition(offset) => Some(offset),
            _ => None,
        })
}

fn find_condition_expr<'a>(
    chunk: &'a LuaChunk,
    condition_summary: &SalsaFlowConditionSummary,
) -> Option<LuaExpr> {
    let node = condition_summary
        .syntax_id
        .to_node_from_root(&chunk.syntax())?;
    LuaExpr::cast(node)
}

fn collect_leaf_conditions(
    conditions: &[SalsaFlowConditionSummary],
    chunk: &LuaChunk,
    node_offset: ConditionNodeId,
    var_name: &str,
    is_true_branch: bool,
) -> Vec<ConditionEffect> {
    let cond = conditions.iter().find(|c| c.node_offset == node_offset);
    let Some(cond) = cond else {
        return vec![];
    };

    match cond.kind {
        SalsaFlowConditionKindSummary::Expr => {
            analyze_leaf_condition(chunk, cond, var_name, is_true_branch)
        }
        SalsaFlowConditionKindSummary::And => {
            if is_true_branch {
                let mut results = Vec::new();
                if let Some(left) = cond.left_condition_offset {
                    results.extend(collect_leaf_conditions(
                        conditions, chunk, left, var_name, true,
                    ));
                }
                if let Some(right) = cond.right_condition_offset {
                    results.extend(collect_leaf_conditions(
                        conditions, chunk, right, var_name, true,
                    ));
                }
                results
            } else {
                vec![]
            }
        }
        SalsaFlowConditionKindSummary::Or => {
            if !is_true_branch {
                let mut results = Vec::new();
                if let Some(left) = cond.left_condition_offset {
                    results.extend(collect_leaf_conditions(
                        conditions, chunk, left, var_name, false,
                    ));
                }
                if let Some(right) = cond.right_condition_offset {
                    results.extend(collect_leaf_conditions(
                        conditions, chunk, right, var_name, false,
                    ));
                }
                results
            } else {
                vec![]
            }
        }
    }
}

fn analyze_leaf_condition(
    chunk: &LuaChunk,
    condition_summary: &SalsaFlowConditionSummary,
    var_name: &str,
    is_true_branch: bool,
) -> Vec<ConditionEffect> {
    let expr = match find_condition_expr(chunk, condition_summary) {
        Some(e) => e,
        None => return vec![],
    };

    // Pattern: name
    if let LuaExpr::NameExpr(name_expr) = &expr {
        if let Some(token) = name_expr.get_name_token() {
            if token.get_name_text() == var_name {
                return single_effect(is_true_branch);
            }
        }
    }

    // Pattern: not name
    if let LuaExpr::UnaryExpr(unary) = &expr {
        if let Some(op) = unary.get_op_token() {
            if op.get_op() == UnaryOperator::OpNot {
                return analyze_not_inner(chunk, unary, var_name, is_true_branch);
            }
        }
    }

    // Pattern: obj.field
    if let LuaExpr::IndexExpr(idx_expr) = &expr {
        if matches_var_index_prefix(idx_expr, var_name) {
            return single_effect(is_true_branch);
        }
    }

    // Pattern: name == literal or name ~= literal
    if let LuaExpr::BinaryExpr(binary) = &expr {
        if let Some(op_token) = binary.get_op_token() {
            let op = op_token.get_op();
            if matches!(op, BinaryOperator::OpEq | BinaryOperator::OpNe) {
                if let Some((left, right)) = binary.get_exprs() {
                    if let Some(lit) = match_var_eq_literal(var_name, &left, &right) {
                        return eq_effect(op == BinaryOperator::OpEq, is_true_branch, lit);
                    }
                    if let Some(lit) = match_var_eq_literal(var_name, &right, &left) {
                        return eq_effect(op == BinaryOperator::OpEq, is_true_branch, lit);
                    }
                }
            }
        }
    }

    // Pattern: type(name) == "typename"
    if let LuaExpr::BinaryExpr(binary) = &expr {
        if let Some(op_token) = binary.get_op_token() {
            if op_token.get_op() == BinaryOperator::OpEq {
                if let Some((left, right)) = binary.get_exprs() {
                    if let Some(type_name) = match_type_guard(var_name, &left, &right) {
                        return single_guard(is_true_branch, type_name);
                    }
                    if let Some(type_name) = match_type_guard(var_name, &right, &left) {
                        return single_guard(is_true_branch, type_name);
                    }
                }
            }
        }
    }

    // Pattern: #name > 0  =>  name is non-empty/truthy
    //          #name == 0 =>  name is empty/falsy
    if let LuaExpr::BinaryExpr(binary) = &expr {
        if let Some(op_token) = binary.get_op_token() {
            let op = op_token.get_op();
            if matches!(
                op,
                BinaryOperator::OpGt
                    | BinaryOperator::OpGe
                    | BinaryOperator::OpLt
                    | BinaryOperator::OpLe
                    | BinaryOperator::OpEq
                    | BinaryOperator::OpNe
            ) {
                if let Some((left, right)) = binary.get_exprs() {
                    if let Some(len_effect) =
                        match_len_cmp(var_name, &left, &right, op, is_true_branch)
                    {
                        return len_effect;
                    }
                    if let Some(len_effect) =
                        match_len_cmp(var_name, &right, &left, op, is_true_branch)
                    {
                        return len_effect;
                    }
                }
            }
        }
    }

    // Pattern: name > literal, name < literal, name >= literal, etc.
    //          Comparison implies the variable is truthy (non-nil comparable).
    if let LuaExpr::BinaryExpr(binary) = &expr {
        if let Some(op_token) = binary.get_op_token() {
            let op = op_token.get_op();
            if matches!(
                op,
                BinaryOperator::OpGt
                    | BinaryOperator::OpGe
                    | BinaryOperator::OpLt
                    | BinaryOperator::OpLe
            ) {
                if let Some((left, right)) = binary.get_exprs() {
                    if is_name_ref(&left, var_name) && expr_to_lua_type(&right).is_some() {
                        return single_effect(is_true_branch);
                    }
                    if is_name_ref(&right, var_name) && expr_to_lua_type(&left).is_some() {
                        return single_effect(is_true_branch);
                    }
                }
            }
        }
    }

    vec![]
}

fn is_name_ref(expr: &LuaExpr, name: &str) -> bool {
    match expr {
        LuaExpr::NameExpr(name_expr) => name_expr
            .get_name_token()
            .is_some_and(|t| t.get_name_text() == name),
        _ => false,
    }
}

fn single_effect(is_true_branch: bool) -> Vec<ConditionEffect> {
    vec![if is_true_branch {
        ConditionEffect::Truthy
    } else {
        ConditionEffect::Falsy
    }]
}

fn eq_effect(is_eq: bool, is_true_branch: bool, lit: LuaType) -> Vec<ConditionEffect> {
    let kind = if is_eq {
        if is_true_branch {
            ConditionEffect::EqLiteral(lit)
        } else {
            ConditionEffect::NeqLiteral(lit)
        }
    } else if is_true_branch {
        ConditionEffect::NeqLiteral(lit)
    } else {
        ConditionEffect::EqLiteral(lit)
    };
    vec![kind]
}

fn single_guard(is_true_branch: bool, guard_ty: LuaType) -> Vec<ConditionEffect> {
    vec![if is_true_branch {
        ConditionEffect::TypeGuard(guard_ty)
    } else {
        ConditionEffect::Falsy
    }]
}

fn match_len_cmp(
    var_name: &str,
    len_side: &LuaExpr,
    num_side: &LuaExpr,
    op: BinaryOperator,
    is_true_branch: bool,
) -> Option<Vec<ConditionEffect>> {
    let var_expr = extract_len_var(len_side, var_name)?;
    let zero = is_literal_zero(num_side)?;

    let is_non_empty = match op {
        BinaryOperator::OpGt if zero => true,
        BinaryOperator::OpGe if zero => true,
        BinaryOperator::OpLt if zero => false,
        BinaryOperator::OpLe if zero => false,
        BinaryOperator::OpNe if zero => true,
        BinaryOperator::OpEq if zero => false,
        _ => return None,
    };

    let effective = if is_true_branch {
        is_non_empty
    } else {
        !is_non_empty
    };
    Some(single_effect(effective))
}

fn extract_len_var<'a>(expr: &'a LuaExpr, var_name: &str) -> Option<&'a LuaExpr> {
    let LuaExpr::UnaryExpr(unary) = expr else {
        return None;
    };
    let op = unary.get_op_token()?;
    if op.get_op() != UnaryOperator::OpLen {
        return None;
    }
    let inner = unary.get_expr()?;
    let LuaExpr::NameExpr(name_expr) = &inner else {
        return None;
    };
    let token = name_expr.get_name_token()?;
    if token.get_name_text() != var_name {
        return None;
    }
    Some(expr)
}

fn is_literal_zero(expr: &LuaExpr) -> Option<bool> {
    match expr {
        LuaExpr::LiteralExpr(lit) => match lit.get_literal()? {
            LuaLiteralToken::Number(n) => match n.get_number_value() {
                emmylua_parser::NumberResult::Int(0) => Some(true),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

fn analyze_not_inner(
    _chunk: &LuaChunk,
    unary: &emmylua_parser::LuaUnaryExpr,
    var_name: &str,
    is_true_branch: bool,
) -> Vec<ConditionEffect> {
    let inner = match unary.get_expr() {
        Some(e) => e,
        None => return vec![],
    };

    // not name → reversed polarity
    if let LuaExpr::NameExpr(name_expr) = &inner {
        if let Some(token) = name_expr.get_name_token() {
            if token.get_name_text() == var_name {
                return single_effect(!is_true_branch);
            }
        }
    }

    // not obj.field → reversed polarity
    if let LuaExpr::IndexExpr(idx_expr) = &inner {
        if matches_var_index_prefix(idx_expr, var_name) {
            return single_effect(!is_true_branch);
        }
    }

    vec![]
}

fn matches_var_index_prefix(idx_expr: &LuaIndexExpr, var_name: &str) -> bool {
    let prefix = match idx_expr.get_prefix_expr() {
        Some(p) => p,
        None => return false,
    };
    let LuaExpr::NameExpr(name_expr) = &prefix else {
        return false;
    };
    let Some(token) = name_expr.get_name_token() else {
        return false;
    };
    token.get_name_text() == var_name
}

fn match_var_eq_literal(var_name: &str, var_expr: &LuaExpr, lit_expr: &LuaExpr) -> Option<LuaType> {
    if let LuaExpr::NameExpr(name_expr) = var_expr {
        if let Some(token) = name_expr.get_name_token() {
            if token.get_name_text() != var_name {
                return None;
            }
        } else {
            return None;
        }
    } else {
        return None;
    }

    expr_to_lua_type(lit_expr)
}

fn match_type_guard(var_name: &str, call_side: &LuaExpr, lit_side: &LuaExpr) -> Option<LuaType> {
    let LuaExpr::CallExpr(call) = call_side else {
        return None;
    };
    let prefix = call.get_prefix_expr()?;
    let LuaExpr::NameExpr(prefix_name) = &prefix else {
        return None;
    };
    let prefix_token = prefix_name.get_name_token()?;
    if prefix_token.get_name_text() != "type" {
        return None;
    }

    let mut args = call.get_args_list()?.get_args();
    let arg = args.next()?;
    let LuaExpr::NameExpr(arg_name) = &arg else {
        return None;
    };
    let arg_token = arg_name.get_name_token()?;
    if arg_token.get_name_text() != var_name {
        return None;
    }

    lua_type_from_type_name_literal(lit_side)
}

fn lua_type_from_type_name_literal(expr: &LuaExpr) -> Option<LuaType> {
    let LuaExpr::LiteralExpr(lit) = expr else {
        return None;
    };
    let token = lit.get_literal()?;
    match token {
        LuaLiteralToken::String(s) => {
            let val = s.get_value();
            match val.as_ref() {
                "nil" => Some(LuaType::Nil),
                "boolean" => Some(LuaType::Boolean),
                "number" => Some(LuaType::Number),
                "string" => Some(LuaType::String),
                "table" => Some(LuaType::Table),
                "function" => Some(LuaType::Function),
                "thread" => Some(LuaType::Thread),
                "integer" => Some(LuaType::Integer),
                _ => None,
            }
        }
        _ => None,
    }
}

fn expr_to_lua_type(expr: &LuaExpr) -> Option<LuaType> {
    match expr {
        LuaExpr::LiteralExpr(lit) => match lit.get_literal()? {
            LuaLiteralToken::String(s) => {
                Some(LuaType::StringConst(SmolStr::new(s.get_value()).into()))
            }
            LuaLiteralToken::Number(n) => match n.get_number_value() {
                emmylua_parser::NumberResult::Int(i) => Some(LuaType::IntegerConst(i)),
                emmylua_parser::NumberResult::Float(f) => Some(LuaType::FloatConst(f)),
                _ => Some(LuaType::Number),
            },
            LuaLiteralToken::Bool(b) => Some(LuaType::BooleanConst(b.is_true())),
            _ => Some(LuaType::Nil),
        },
        LuaExpr::NameExpr(name) => {
            let token = name.get_name_token()?;
            let text = token.get_name_text();
            match text.as_ref() {
                "nil" => Some(LuaType::Nil),
                "true" => Some(LuaType::BooleanConst(true)),
                "false" => Some(LuaType::BooleanConst(false)),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(crate) fn narrow_remove_falsy(ty: LuaType) -> LuaType {
    match ty {
        LuaType::Nil => LuaType::Unknown,
        LuaType::BooleanConst(false) => LuaType::Unknown,
        LuaType::Boolean => LuaType::BooleanConst(true),
        LuaType::Union(u) => {
            let types = u.into_vec();
            let mut new_types = Vec::new();
            for t in types.iter() {
                match t {
                    LuaType::Nil => {}
                    LuaType::BooleanConst(false) => {}
                    LuaType::Boolean => {
                        new_types.push(LuaType::BooleanConst(true));
                    }
                    _ => {
                        new_types.push(t.clone());
                    }
                }
            }
            LuaType::from_vec(new_types)
        }
        _ => ty,
    }
}

pub fn narrow_to_falsy(ty: LuaType) -> LuaType {
    match &ty {
        LuaType::Boolean => LuaType::BooleanConst(false),
        LuaType::Union(u) => {
            let falsy: Vec<_> = u
                .into_vec()
                .iter()
                .filter_map(|t| match t {
                    LuaType::Nil => Some(LuaType::Nil),
                    LuaType::BooleanConst(false) => Some(LuaType::BooleanConst(false)),
                    _ => None,
                })
                .collect();
            if falsy.is_empty() {
                LuaType::Never
            } else {
                LuaType::from_vec(falsy)
            }
        }
        LuaType::Nil | LuaType::BooleanConst(false) => ty.clone(),
        _ => LuaType::Never,
    }
}

fn narrow_intersect(ty: LuaType, target: LuaType) -> LuaType {
    if ty == target {
        return ty;
    }
    match &ty {
        LuaType::Union(u) => {
            let types = u.into_vec();
            let matching: Vec<_> = types
                .iter()
                .filter(|t| can_be(*t, &target))
                .cloned()
                .collect();
            if matching.is_empty() {
                target
            } else if matching.len() == 1 {
                matching.into_iter().next().expect("matching.len() == 1")
            } else {
                LuaType::from_vec(matching)
            }
        }
        _ => {
            if can_be(&ty, &target) {
                ty
            } else {
                target
            }
        }
    }
}

fn can_be(ty: &LuaType, target: &LuaType) -> bool {
    match (ty, target) {
        (LuaType::String, LuaType::String)
        | (LuaType::Integer, LuaType::Integer)
        | (LuaType::Number, LuaType::Number)
        | (LuaType::Boolean, LuaType::Boolean)
        | (LuaType::Function, LuaType::Function)
        | (LuaType::Table, LuaType::Table)
        | (LuaType::TableConst(_), LuaType::TableConst(_))
        | (LuaType::TableConst(_), LuaType::Table)
        | (LuaType::Table, LuaType::TableConst(_))
        | (LuaType::Nil, LuaType::Nil) => true,
        (LuaType::StringConst(_), LuaType::String)
        | (LuaType::StringConst(_), LuaType::StringConst(_))
        | (LuaType::IntegerConst(_), LuaType::Integer)
        | (LuaType::IntegerConst(_), LuaType::IntegerConst(_))
        | (LuaType::FloatConst(_), LuaType::Number)
        | (LuaType::FloatConst(_), LuaType::FloatConst(_))
        | (LuaType::BooleanConst(_), LuaType::Boolean)
        | (LuaType::BooleanConst(_), LuaType::BooleanConst(_)) => true,
        (a, b) => a == b,
    }
}

fn narrow_remove(ty: LuaType, target: LuaType) -> LuaType {
    match ty {
        LuaType::Union(u) => {
            let mut types: Vec<_> = u.into_vec();
            types.retain(|t| *t != target);
            if types.is_empty() {
                LuaType::Unknown
            } else if types.len() == 1 {
                types.into_iter().next().expect("types.len() == 1")
            } else {
                LuaType::from_vec(types)
            }
        }
        _ => {
            if ty == target {
                LuaType::Unknown
            } else {
                ty
            }
        }
    }
}

fn resolve_call_narrow(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    _chunk: &LuaChunk,
    call: &emmylua_parser::LuaCallExpr,
    var_name: &str,
    is_true_branch: bool,
) -> Option<ConditionEffect> {
    let call_offset = call.syntax().text_range().start();
    let call_explain = db.doc().signature().call_explain(file_id, call_offset)?;
    let sig = call_explain.resolved_signature.as_ref()?;
    let sig_file = sig.signature.file_id;

    // 1. Check return_cast via the resolved signature's owner
    let sig_owner = SalsaDocOwnerSummary {
        kind: sig.signature.owner_kind.clone(),
        syntax_offset: Some(sig.signature.owner_offset),
    };
    if let Some(return_casts) = db.doc().tags_for_owner(sig_file, sig_owner.clone()) {
        for tag in return_casts {
            if tag.kind != SalsaDocTagKindSummary::ReturnCast {
                continue;
            }
            let Some(tag_name) = tag.name() else { continue };
            if tag_name != var_name {
                continue;
            }
            if let SalsaDocTagDataSummary::NameTypeOffsets { type_offsets, .. } = &tag.data {
                let idx = if is_true_branch { 0 } else { 1 };
                let key = type_offsets.get(idx).or_else(|| type_offsets.first())?;
                if let Some(resolved) = db.doc().resolved_type_by_key(sig_file, *key) {
                    if let Some(cast_ty) = lowered_node_to_lua_type(&resolved.lowered) {
                        return Some(ConditionEffect::TypeGuard(cast_ty));
                    }
                }
            }
        }
    }

    // 2. Check TypeGuard return type from the resolved signature
    for ret in &call_explain.returns {
        for item in &ret.items {
            let Some(ref lowered) = item.doc_type.lowered else {
                continue;
            };
            if !is_type_guard_lowered(lowered) {
                continue;
            }
            if let Some(first_arg) = call.get_args_list().and_then(|al| al.get_args().next()) {
                if let LuaExpr::NameExpr(name_expr) = &first_arg {
                    if name_expr
                        .get_name_token()
                        .is_some_and(|t| t.get_name_text() == var_name)
                    {
                        if let Some(guard_ty) = extract_guard_type(db, file_id, lowered) {
                            return Some(ConditionEffect::TypeGuard(guard_ty));
                        }
                    }
                }
            }
        }
    }

    None
}

fn is_type_guard_lowered(node: &SalsaDocTypeLoweredNode) -> bool {
    matches!(&node.kind, SalsaDocTypeLoweredKind::Generic { .. })
}

fn extract_guard_type(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    node: &SalsaDocTypeLoweredNode,
) -> Option<LuaType> {
    match &node.kind {
        SalsaDocTypeLoweredKind::Generic { arg_types, .. } => {
            let first_param = arg_types.first()?;
            match first_param {
                SalsaDocTypeRef::Node(key) => {
                    if let Some(resolved) = db.doc().resolved_type_by_key(file_id, *key) {
                        lowered_node_to_lua_type(&resolved.lowered)
                    } else if let Some(lowered) = db.doc().lowered_type_by_key(file_id, *key) {
                        lowered_node_to_lua_type(&lowered)
                    } else {
                        None
                    }
                }
                SalsaDocTypeRef::Incomplete => None,
            }
        }
        _ => None,
    }
}

pub fn narrow_local_at_point(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    chunk: &LuaChunk,
    target_offset: TextSize,
    var_name: &str,
    base_type: LuaType,
    decl_tree: Option<&SalsaDeclTreeSummary>,
) -> LuaType {
    let Some(flow_query) = db.flow().query(file_id) else {
        return base_type;
    };

    let flow_summary = match db.flow().summary(file_id) {
        Some(fs) => fs,
        None => return base_type,
    };

    let flow_index = FlowIndex::new(&flow_query, &flow_summary.loops);
    let conditions = &flow_summary.conditions;

    let Some(enclosing_block) = find_enclosing_block(&flow_index, target_offset) else {
        return base_type;
    };

    let mut narrowed = narrow_at_block(
        chunk,
        &flow_query,
        conditions,
        &flow_index,
        enclosing_block,
        var_name,
        base_type.clone(),
    );

    // Assignment propagation: walk backward through statements in the enclosing block.
    // If any statement before the target assigns to var_name, narrow by the RHS type.
    if let Some(block) = flow_summary
        .blocks
        .iter()
        .find(|b| b.syntax_offset == enclosing_block)
    {
        for &stmt_offset in block.statement_offsets.iter().rev() {
            if stmt_offset >= target_offset {
                continue;
            }
            if let Some(assign_type) = find_assignment_type(chunk, stmt_offset, var_name) {
                narrowed = narrow_intersect(narrowed, assign_type);
                break;
            }
        }
    }

    // Correlated narrow: if a sibling variable (shared call origin) is condition-checked,
    // propagate the narrowing effect to our variable.
    // Also handles return_overload: when a discriminant variable narrows,
    // filter overload rows and narrow sibling return values accordingly.
    if let Some(tree) = decl_tree {
        if let Some(siblings) = find_correlated_siblings(tree, var_name) {
            let all_narrows = collect_dominating_conditions(&flow_query, target_offset);
            let call_sid = tree
                .decls
                .iter()
                .find(|d| d.name == var_name)
                .and_then(|d| d.source_call_syntax_id);
            let call_explain = call_sid
                .and_then(|sid| db.doc().signature().call_explain(file_id, sid.start_offset));
            let overload_rows = call_explain
                .as_ref()
                .map(|ce| &ce.overload_returns)
                .filter(|r| !r.is_empty());

            for (sib_name, sib_slot) in &siblings {
                // If we have overload rows, use them for precise filtering.
                // Otherwise fall back to simple effect propagation from sibling.
                if let Some(rows) = overload_rows {
                    if let Some(our_decl) = tree.decls.iter().find(|d| d.name == var_name) {
                        let our_slot = our_decl.value_result_index as u32;
                        // Filter overload rows: for each row, check if the sibling
                        // condition effects are compatible with the row's discriminant type.
                        let filtered: Vec<LuaType> = rows
                            .iter()
                            .filter(|row| {
                                let disc_ty = row
                                    .items
                                    .get(*sib_slot as usize)
                                    .and_then(|item| item.doc_type.lowered.as_ref())
                                    .and_then(|lt| lowered_node_to_lua_type(lt));
                                let Some(disc_ty) = disc_ty else { return false };
                                let mut all_valid = true;
                                for narrow in &all_narrows {
                                    let effects = collect_leaf_conditions(
                                        conditions,
                                        chunk,
                                        narrow.condition_offset,
                                        sib_name,
                                        narrow.is_true_branch,
                                    );
                                    let mut test_ty = disc_ty.clone();
                                    for effect in effects {
                                        test_ty = apply_effect(test_ty, effect);
                                    }
                                    if is_erased(&test_ty) {
                                        all_valid = false;
                                        break;
                                    }
                                }
                                all_valid
                            })
                            .filter_map(|row| {
                                row.items
                                    .get(our_slot as usize)
                                    .and_then(|item| item.doc_type.lowered.as_ref())
                                    .and_then(|lt| lowered_node_to_lua_type(lt))
                            })
                            .collect();
                        if !filtered.is_empty() {
                            narrowed = LuaType::from_vec(filtered);
                        }
                    }
                } else {
                    for narrow in &all_narrows {
                        let effects = collect_leaf_conditions(
                            conditions,
                            chunk,
                            narrow.condition_offset,
                            sib_name,
                            narrow.is_true_branch,
                        );
                        for effect in effects {
                            narrowed = apply_effect(narrowed, effect);
                        }
                    }
                }
            }
        }
    }

    // Call-based narrow: TypeGuard return types + return_cast annotations.
    // When the condition is a call expression like `if is_string(x) then`,
    // narrow the argument based on the function's return type / return_cast.
    for narrow in &collect_dominating_conditions(&flow_query, target_offset) {
        let cond = flow_summary
            .conditions
            .iter()
            .find(|c| c.node_offset == narrow.condition_offset);
        let Some(cond) = cond else { continue };
        if cond.kind != SalsaFlowConditionKindSummary::Expr {
            continue;
        }
        let Some(expr) = find_condition_expr(chunk, cond) else {
            continue;
        };
        if let LuaExpr::CallExpr(call) = &expr {
            if let Some(effect) =
                resolve_call_narrow(db, file_id, chunk, call, var_name, narrow.is_true_branch)
            {
                narrowed = apply_effect(narrowed, effect);
            }
        }
    }
    // Loop exit for sibling statements: when the target is in the same block
    // but AFTER a while/for/repeat statement, apply loop exit narrow.
    for l in &flow_summary.loops {
        let Some(body_block) = l.block_offset else {
            continue;
        };
        if body_block >= target_offset {
            continue;
        }
        let loop_syntax = l.syntax_offset;
        let is_after_loop = target_offset > loop_syntax;
        if !is_after_loop {
            continue;
        }
        if let Some(cond_offset) = flow_index
            .loop_condition_offsets
            .get(&l.syntax_offset)
            .copied()
            .flatten()
        {
            let is_repeat = l.kind == SalsaFlowLoopKindSummary::Repeat;
            let exit_is = is_repeat; // repeat exits on true, while/for on false
            let effects =
                collect_leaf_conditions(conditions, chunk, cond_offset, var_name, exit_is);
            for effect in effects {
                narrowed = apply_effect(narrowed, effect);
            }
        }
    }
    // ---@cast annotation support: find casts targeting this variable and apply them
    if let Some(cast_tags) = db
        .doc()
        .tags_for_kind(file_id, SalsaDocTagKindSummary::Cast)
    {
        let mut closest_cast_syntax_offset: TextSize = TextSize::from(0u32);
        let mut closest_cast_type_offsets: Vec<SalsaDocTypeNodeKey> = Vec::new();
        let mut has_cast = false;
        for tag in &cast_tags {
            let Some(name) = tag.name() else { continue };
            if name != var_name {
                continue;
            }
            if tag.syntax_offset > target_offset {
                continue;
            }
            if let SalsaDocTagDataSummary::NameTypeOffsets { type_offsets, .. } = &tag.data {
                if !has_cast || tag.syntax_offset > closest_cast_syntax_offset {
                    closest_cast_syntax_offset = tag.syntax_offset;
                    closest_cast_type_offsets = type_offsets.clone();
                    has_cast = true;
                }
            }
        }
        if has_cast {
            if let Some(key) = closest_cast_type_offsets.first().copied() {
                if let Some(resolved) = db.doc().resolved_type_by_key(file_id, key) {
                    if let Some(cast_ty) = lowered_node_to_lua_type(&resolved.lowered) {
                        narrowed = cast_ty;
                    }
                }
            }
        }
    }

    narrowed
}

fn narrow_at_block(
    chunk: &LuaChunk,
    flow_query: &SalsaFlowQuerySummary,
    conditions: &[SalsaFlowConditionSummary],
    flow_index: &FlowIndex,
    block_offset: TextSize,
    var_name: &str,
    base_type: LuaType,
) -> LuaType {
    let root_base = base_type.clone();
    let mut visited: HashSet<TextSize> = HashSet::new();
    let mut block_results: HashMap<TextSize, LuaType> = HashMap::new();
    narrow_at_block_inner(
        chunk,
        flow_query,
        conditions,
        flow_index,
        block_offset,
        var_name,
        base_type,
        root_base,
        &mut visited,
        &mut block_results,
        true,
    )
}

fn narrow_at_block_inner(
    chunk: &LuaChunk,
    flow_query: &SalsaFlowQuerySummary,
    conditions: &[SalsaFlowConditionSummary],
    flow_index: &FlowIndex,
    block_offset: TextSize,
    var_name: &str,
    base_type: LuaType,
    root_base: LuaType,
    visited: &mut HashSet<TextSize>,
    block_results: &mut HashMap<TextSize, LuaType>,
    is_root: bool,
) -> LuaType {
    if !visited.insert(block_offset) {
        return block_results
            .get(&block_offset)
            .cloned()
            .unwrap_or(base_type);
    }
    let mut narrowed = base_type;

    let mut narrows = collect_conditions_for_block(flow_query, flow_index, block_offset);

    let mut went_to_parent = false;
    let mut next_block = block_offset;

    for narrow in &narrows {
        let effects = collect_leaf_conditions(
            conditions,
            chunk,
            narrow.condition_offset,
            var_name,
            narrow.is_true_branch,
        );
        for effect in effects {
            narrowed = apply_effect(narrowed, effect);
        }
    }

    if is_root {
        if let Some(clause_blocks) = flow_index.entry_to_branch_clauses.get(&block_offset) {
            if clause_blocks.len() > 1 {
                let mut merged = root_base.clone();
                for &clause_block in clause_blocks {
                    if clause_has_error(flow_query, flow_index, clause_block) {
                        continue;
                    }
                    let clause_type = narrow_at_block_inner(
                        chunk,
                        flow_query,
                        conditions,
                        flow_index,
                        clause_block,
                        var_name,
                        root_base.clone(),
                        root_base.clone(),
                        visited,
                        block_results,
                        false,
                    );
                    merged = union_types(merged, clause_type);
                }
                narrowed = merged;
            }
        }
    }

    if let Some(parent) = flow_index.block_to_parent.get(&block_offset).copied() {
        next_block = parent;
        went_to_parent = true;
    }

    if went_to_parent {
        let result = narrow_at_block_inner(
            chunk,
            flow_query,
            conditions,
            flow_index,
            next_block,
            var_name,
            narrowed,
            root_base,
            visited,
            block_results,
            false,
        );
        block_results.insert(block_offset, result.clone());
        result
    } else {
        block_results.insert(block_offset, narrowed.clone());
        narrowed
    }
}

fn clause_has_error(
    query: &SalsaFlowQuerySummary,
    flow_index: &FlowIndex,
    block: TextSize,
) -> bool {
    query.edges.iter().any(|e| {
        e.kind == SalsaFlowEdgeKindSummary::StatementToTerminal
            && is_stmt_in(flow_index, &e.from, block)
    })
}

fn is_stmt_in(flow_index: &FlowIndex, node: &SalsaFlowNodeRefSummary, block: TextSize) -> bool {
    match node {
        SalsaFlowNodeRefSummary::Statement(s) => {
            flow_index.stmt_enclosing_block.get(s).copied() == Some(block)
        }
        _ => false,
    }
}

fn collect_conditions_for_block(
    flow_query: &SalsaFlowQuerySummary,
    flow_index: &FlowIndex,
    block_offset: TextSize,
) -> Vec<FlowNarrow> {
    let mut narrows = Vec::new();
    let Some(preds) = flow_index.block_preds.get(&block_offset) else {
        return narrows;
    };

    for pred in preds {
        match pred.kind {
            SalsaFlowEdgeKindSummary::ConditionTrue => {
                if let SalsaFlowNodeRefSummary::Condition(offset) = pred.source {
                    add_narrow(&mut narrows, offset, true);
                }
            }
            SalsaFlowEdgeKindSummary::ConditionFalse => {
                if let SalsaFlowNodeRefSummary::Condition(offset) = pred.source {
                    add_narrow(&mut narrows, offset, false);
                }
            }
            SalsaFlowEdgeKindSummary::BranchToClause => {
                if let SalsaFlowNodeRefSummary::Branch(branch_offset) = pred.source {
                    if let Some((_, clause_idx)) =
                        flow_index.block_to_branch_clause.get(&block_offset)
                    {
                        if let Some(condition_offset) =
                            clause_condition(flow_query, branch_offset, *clause_idx)
                        {
                            let is_true = *clause_idx == 0;
                            add_narrow(&mut narrows, condition_offset, is_true);
                        }
                    }
                }
            }
            SalsaFlowEdgeKindSummary::LoopToBody => {
                if let SalsaFlowNodeRefSummary::Loop(loop_offset) = pred.source {
                    // repeat loops check condition at the END — don't narrow inside body
                    let is_repeat = flow_index
                        .body_to_is_repeat
                        .get(&block_offset)
                        .copied()
                        .unwrap_or(false);
                    if !is_repeat {
                        let loop_condition = flow_index
                            .loop_condition_offsets
                            .get(&loop_offset)
                            .copied()
                            .flatten();
                        if let Some(cond_offset) = loop_condition {
                            add_narrow(&mut narrows, cond_offset, true);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    narrows
}

fn find_assignment_type(
    chunk: &LuaChunk,
    stmt_offset: TextSize,
    var_name: &str,
) -> Option<LuaType> {
    use emmylua_parser::{LuaStat, LuaVarExpr};
    for stat in chunk.descendants::<LuaStat>() {
        if stat.syntax().text_range().start() != stmt_offset {
            continue;
        }
        match &stat {
            LuaStat::LocalStat(local) => {
                let names = local.get_local_name_list();
                let exprs: Vec<LuaExpr> = local.get_value_exprs().collect();
                for (i, name) in names.enumerate() {
                    if let Some(token) = name.get_name_token() {
                        if token.get_name_text() == var_name {
                            if let Some(expr) = exprs.get(i) {
                                return expr_to_lua_type(expr);
                            }
                            return Some(LuaType::Any);
                        }
                    }
                }
            }
            LuaStat::AssignStat(assign) => {
                let (vars, values) = assign.get_var_and_expr_list();
                for (i, var) in vars.iter().enumerate() {
                    if let LuaVarExpr::NameExpr(name_expr) = var {
                        if let Some(token) = name_expr.get_name_token() {
                            if token.get_name_text() == var_name {
                                if let Some(expr) = values.get(i) {
                                    return expr_to_lua_type(expr);
                                }
                                return Some(LuaType::Any);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        break;
    }
    None
}

fn is_erased(ty: &LuaType) -> bool {
    matches!(ty, LuaType::Never | LuaType::Unknown)
}

fn apply_effect(narrowed: LuaType, effect: ConditionEffect) -> LuaType {
    match effect {
        ConditionEffect::Truthy => narrow_remove_falsy(narrowed),
        ConditionEffect::Falsy => narrow_to_falsy(narrowed),
        ConditionEffect::EqLiteral(lit) => narrow_intersect(narrowed, lit),
        ConditionEffect::NeqLiteral(lit) => narrow_remove(narrowed, lit),
        ConditionEffect::TypeGuard(guard_ty) => narrow_intersect(narrowed, guard_ty),
    }
}

fn union_types(a: LuaType, b: LuaType) -> LuaType {
    if a == b {
        return a;
    }
    let mut all = Vec::new();
    push_type(&mut all, a);
    push_type(&mut all, b);
    LuaType::from_vec(all)
}

fn push_type(into: &mut Vec<LuaType>, ty: LuaType) {
    match ty {
        LuaType::Union(u) => into.extend(u.into_vec()),
        other => into.push(other),
    }
}
