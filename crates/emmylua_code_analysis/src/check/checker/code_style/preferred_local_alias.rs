//! preferred_local_alias — when `local alias = a.b.c` exists, suggests using the alias for later `a.b.c` references.
//!
//! M0 simplification: mutable references are not indexed (cases where `a.b.c = v` invalidates the alias are not yet recognized; will be wired in after member-reference usesite support).

use std::collections::{HashMap, HashSet};

use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaExpr, LuaIndexExpr, LuaLocalStat, LuaSyntaxKind,
    LuaVarExpr, PathTrait,
};
use rowan::TextRange;

use crate::DiagnosticCode;
use crate::LuaType;
use crate::salsa_builder::def::SemanticId;
use crate::semantic_model::SemanticModel;

use super::super::{CheckContext, Checker};

pub struct PreferredLocalAliasChecker;

impl Checker for PreferredLocalAliasChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::PreferredLocalAlias];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let mut alias_set = LocalAliasSet::new();
        let chunk = tree.get_chunk_node();
        for walk in chunk.walk_descendants::<LuaAst>() {
            match walk {
                rowan::WalkEvent::Enter(node) => {
                    if is_scope(&node) {
                        alias_set.push();
                    }
                    match node {
                        LuaAst::LuaLocalStat(local_stat) => {
                            collect_local_alias(&mut alias_set, semantic_model, &local_stat);
                        }
                        LuaAst::LuaAssignStat(assign_stat) => {
                            invalidate_assigned_aliases(
                                &mut alias_set,
                                semantic_model,
                                &assign_stat,
                            );
                        }
                        LuaAst::LuaIndexExpr(index_expr) => {
                            check_index_expr_preference(
                                context,
                                &alias_set,
                                semantic_model,
                                &index_expr,
                            );
                        }
                        _ => {}
                    }
                }
                rowan::WalkEvent::Leave(node) => {
                    if is_scope(&node) {
                        alias_set.pop();
                    }
                }
            }
        }
    }
}

fn is_scope(node: &LuaAst) -> bool {
    matches!(
        node.syntax().kind().into(),
        LuaSyntaxKind::Chunk | LuaSyntaxKind::Block | LuaSyntaxKind::ClosureExpr
    )
}

fn collect_local_alias(
    alias_set: &mut LocalAliasSet,
    semantic_model: &SemanticModel<'_>,
    local_stat: &LuaLocalStat,
) {
    let local_list = local_stat.get_local_name_list().collect::<Vec<_>>();
    let value_exprs = local_stat.get_value_exprs().collect::<Vec<_>>();
    let min_len = local_list.len().min(value_exprs.len());
    for i in 0..min_len {
        let local_name = &local_list[i];
        let value_expr = &value_exprs[i];
        let LuaExpr::IndexExpr(index_expr) = value_expr else {
            continue;
        };
        if !is_only_dot_index_expr(value_expr).unwrap_or(false) {
            continue;
        }
        // M0: only single-level member aliases (`local alias = t.a`); chained (`a.b.c`) is left to chained member resolution.
        let Some(LuaExpr::NameExpr(_)) = index_expr.get_prefix_expr() else {
            continue;
        };
        let Some(access_path) = index_expr.get_access_path() else {
            continue;
        };
        // Root variable declaration + outermost field member declaration (resolve_member: the only member-resolution entry point).
        let Some(ref_var) = find_ref_var_decl_id(semantic_model, value_expr) else {
            continue;
        };
        let Some(resolved) = semantic_model.resolve_member(index_expr) else {
            continue;
        };
        let ref_field = resolved
            .member_id
            .or_else(|| runtime_member_id(semantic_model, index_expr, &resolved.name));
        let Some(name_token) = local_name.get_name_token() else {
            continue;
        };
        let preferred_name = name_token.get_name_text().to_string();
        alias_set.insert(access_path, preferred_name, ref_field, ref_var);
        alias_set.add_disable_check(value_expr.get_range());
    }
}

/// Runtime members that `resolve_member` does not bridge (std table members like `string.gsub`):
/// find the member declaration by name in the declaration file using the prefix TableConst identity.
fn runtime_member_id(
    semantic_model: &SemanticModel<'_>,
    index_expr: &LuaIndexExpr,
    name: &str,
) -> Option<SemanticId> {
    let prefix = index_expr.get_prefix_expr()?;
    let prefix_ty = semantic_model.type_of_expr(prefix.get_syntax_id());
    let LuaType::TableConst(table) = prefix_ty else {
        return None;
    };
    let facts = semantic_model.file_facts_of(table.file_id)?;
    facts
        .members
        .iter()
        .find(|member| member.value_syntax.is_some() && member.key.name() == Some(name))
        .map(|member| member.id.clone())
}

/// Root name declaration of a pure dot-chain expression.
fn find_ref_var_decl_id(semantic_model: &SemanticModel<'_>, expr: &LuaExpr) -> Option<SemanticId> {
    let mut prefix = expr.clone();
    while let LuaExpr::IndexExpr(index_expr) = prefix {
        match index_expr.get_prefix_expr() {
            Some(LuaExpr::NameExpr(name_expr)) => {
                return semantic_model.resolve_name(name_expr.get_position());
            }
            Some(LuaExpr::IndexExpr(prefix_index_expr)) => {
                prefix = LuaExpr::IndexExpr(prefix_index_expr);
            }
            _ => return None,
        }
    }
    None
}

fn is_only_dot_index_expr(expr: &LuaExpr) -> Option<bool> {
    let mut index_expr = match expr {
        LuaExpr::IndexExpr(index_expr) => index_expr.clone(),
        _ => return Some(false),
    };
    loop {
        let index_token = index_expr.get_index_token()?;
        if !index_token.is_dot() {
            return Some(false);
        }
        match index_expr.get_prefix_expr() {
            Some(LuaExpr::NameExpr(_)) => return Some(true),
            Some(LuaExpr::IndexExpr(prefix_index_expr)) => {
                index_expr = prefix_index_expr;
            }
            _ => return Some(false),
        }
    }
}

#[derive(Debug)]
struct LocalAliasSet {
    local_alias_stack: Vec<HashMap<String, LocalAliasInfo>>,
    disable_check: HashSet<TextRange>,
}

#[derive(Debug)]
struct LocalAliasInfo {
    ref_var: SemanticId,
    ref_field: Option<SemanticId>,
    preferred_name: String,
}

impl LocalAliasSet {
    fn new() -> Self {
        LocalAliasSet {
            local_alias_stack: vec![HashMap::new()],
            disable_check: HashSet::new(),
        }
    }

    fn push(&mut self) {
        self.local_alias_stack.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.local_alias_stack.pop();
    }

    fn insert(
        &mut self,
        access_path: String,
        preferred_name: String,
        ref_field: Option<SemanticId>,
        ref_var: SemanticId,
    ) {
        if let Some(map) = self.local_alias_stack.last_mut() {
            map.insert(
                access_path,
                LocalAliasInfo {
                    ref_var,
                    ref_field,
                    preferred_name,
                },
            );
        }
    }

    fn get(&self, access_path: &str) -> Option<&LocalAliasInfo> {
        for map in self.local_alias_stack.iter().rev() {
            if let Some(item) = map.get(access_path) {
                return Some(item);
            }
        }
        None
    }

    fn add_disable_check(&mut self, range: TextRange) {
        self.disable_check.insert(range);
    }

    fn remove_by_field(&mut self, field: &SemanticId) {
        for map in &mut self.local_alias_stack {
            map.retain(|_, info| info.ref_field.as_ref() != Some(field));
        }
    }

    fn is_disable_check(&self, range: &TextRange) -> bool {
        self.disable_check.contains(range)
    }
}

/// If an aliased member is later assigned, the alias becomes invalid so the suggestion is removed.
fn invalidate_assigned_aliases(
    alias_set: &mut LocalAliasSet,
    semantic_model: &SemanticModel<'_>,
    assign_stat: &LuaAssignStat,
) {
    let (vars, _) = assign_stat.get_var_and_expr_list();
    for var in vars {
        let LuaVarExpr::IndexExpr(index_expr) = var else {
            continue;
        };
        let Some(resolved) = semantic_model.resolve_member(&index_expr) else {
            continue;
        };
        if let Some(member_id) = resolved.member_id {
            alias_set.remove_by_field(&member_id);
        }
    }
}

fn check_index_expr_preference(
    context: &mut CheckContext<'_>,
    alias_set: &LocalAliasSet,
    semantic_model: &SemanticModel<'_>,
    index_expr: &LuaIndexExpr,
) {
    if alias_set.is_disable_check(&index_expr.get_range()) {
        return;
    }
    let expr = LuaExpr::IndexExpr(index_expr.clone());
    if !is_only_dot_index_expr(&expr).unwrap_or(false) {
        return;
    }
    // M0: only single-level member references.
    if !matches!(index_expr.get_prefix_expr(), Some(LuaExpr::NameExpr(_))) {
        return;
    }
    let Some(access_path) = index_expr.get_access_path() else {
        return;
    };
    let Some(alias_info) = alias_set.get(&access_path) else {
        return;
    };
    // Same root variable?
    let var_expr = first_name_expr(index_expr);
    let Some(var_expr) = var_expr else {
        return;
    };
    if !semantic_model.is_reference_to(
        rowan::NodeOrToken::Node(var_expr.syntax().clone()),
        &alias_info.ref_var,
    ) {
        return;
    }
    // Same outermost field?
    if let Some(ref_field) = &alias_info.ref_field {
        let Some(resolved) = semantic_model.resolve_member(index_expr) else {
            return;
        };
        let check_field = resolved
            .member_id
            .or_else(|| runtime_member_id(semantic_model, index_expr, &resolved.name));
        if check_field.as_ref() != Some(ref_field) {
            return;
        }
    }
    context.add_diagnostic(
        DiagnosticCode::PreferredLocalAlias,
        index_expr.get_range(),
        t!(
            "Prefer use local alias variable `%{name}`",
            name = alias_info.preferred_name
        ),
    );
}

fn first_name_expr(index_expr: &LuaIndexExpr) -> Option<LuaExpr> {
    let mut index_expr = index_expr.clone();
    loop {
        match index_expr.get_prefix_expr() {
            Some(LuaExpr::NameExpr(name_expr)) => return Some(LuaExpr::NameExpr(name_expr)),
            Some(LuaExpr::IndexExpr(prefix_index_expr)) => {
                index_expr = prefix_index_expr;
            }
            _ => return None,
        }
    }
}
