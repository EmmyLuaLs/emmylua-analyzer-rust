use std::collections::HashSet;

use emmylua_parser::{LuaAstNode, LuaCallExprStat, LuaExpr, LuaSyntaxId, LuaSyntaxNode};

use super::{
    AliasContext, FlowAntecedent, FlowEffect, FlowNodeKind, LuaType, SemanticId, SemanticModel,
    local_alias_path, unparenthesized,
};

#[derive(PartialEq, Eq)]
enum TableIdentity {
    Allocation(LuaSyntaxId),
    Declaration(SemanticId),
}

/// Find the metatable at this use. Nil represents a plain table and None
/// means the expression has no tracked metatable.
pub(crate) fn metatable_type_at(model: &SemanticModel, expr: &LuaExpr) -> Option<LuaType> {
    let expr = unparenthesized(expr.clone())?;
    if let Some((_, metatable)) = setmetatable_arguments(model, &expr) {
        return Some(metatable_value(model, metatable));
    }
    if matches!(expr, LuaExpr::TableExpr(_)) {
        return Some(LuaType::Nil);
    }

    let root = model.syntax_tree()?.get_red_root();
    let identity = table_identity(model, &root, expr.clone())?;
    let tree = model.flow_tree()?;
    let start = tree.get_flow_id(expr.get_syntax_id())?;
    let mut pending = vec![start];
    let mut visited = HashSet::new();
    let mut attachments = Vec::new();
    let mut has_unattached_path = false;

    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        let node = tree.get_flow_node(id)?;
        if node.kind.is_unreachable() {
            continue;
        }
        // The flow graph also records calls evaluated in conditions and assertions.
        let effect_values = tree
            .get_flow_effects(id)
            .iter()
            .filter_map(|effect| match effect {
                FlowEffect::AssignDecl { value_syntax, .. }
                | FlowEffect::AssignMember { value_syntax, .. } => Some(*value_syntax),
                FlowEffect::Guard { cond, .. } => Some(cond.get_syntax_id()),
                _ => None,
            });
        let call = match &node.kind {
            FlowNodeKind::CallExprStat(ptr) => ptr
                .get_syntax_id()
                .to_node_from_root(&root)
                .and_then(LuaCallExprStat::cast)
                .and_then(|stat| stat.get_call_expr())
                .map(LuaExpr::CallExpr),
            _ => None,
        };
        let values = call.into_iter().chain(
            effect_values
                .clone()
                .rev()
                .filter_map(|syntax| LuaExpr::cast(syntax.to_node_from_root(&root)?)),
        );
        let attached = values
            .filter_map(unparenthesized)
            .filter_map(|expr| setmetatable_arguments(model, &expr))
            .find_map(|(table, meta)| {
                (table_identity(model, &root, table).as_ref() == Some(&identity))
                    .then(|| metatable_value(model, meta))
            });
        if let Some(ty) = attached {
            if !attachments.contains(&ty) {
                attachments.push(ty);
            }
            continue;
        }
        // A fresh table starts out plain, including on a later loop iteration.
        if matches!(identity, TableIdentity::Allocation(syntax)
            if effect_values.clone().any(|value| value == syntax))
        {
            has_unattached_path = true;
            continue;
        }

        match node.antecedent {
            Some(FlowAntecedent::Single(next)) => pending.push(next),
            Some(FlowAntecedent::Multiple(next)) => {
                pending.extend(tree.get_multi_antecedents(next)?);
            }
            None => has_unattached_path = true,
        }
    }

    if attachments.is_empty() {
        return matches!(identity, TableIdentity::Allocation(_)).then_some(LuaType::Nil);
    }
    if has_unattached_path {
        // A parameter or an opaque initializer can already carry a metatable.
        let incoming = match identity {
            TableIdentity::Allocation(_) => LuaType::Nil,
            TableIdentity::Declaration(_) => LuaType::Unknown,
        };
        if !attachments.contains(&incoming) {
            attachments.push(incoming);
        }
    }
    Some(LuaType::from_vec(attachments))
}

fn table_identity(
    model: &SemanticModel,
    root: &LuaSyntaxNode,
    mut expr: LuaExpr,
) -> Option<TableIdentity> {
    let facts = model.file_facts()?;
    let mut visited = HashSet::new();
    loop {
        expr = unparenthesized(expr)?;
        match &expr {
            LuaExpr::TableExpr(table) => {
                return Some(TableIdentity::Allocation(table.get_syntax_id()));
            }
            LuaExpr::CallExpr(_) => expr = setmetatable_arguments(model, &expr)?.0,
            LuaExpr::NameExpr(_) => {
                let ((decl, keys), _) =
                    local_alias_path(model, &expr, AliasContext::new(model, &expr)?)?;
                if !keys.is_empty() || !visited.insert(decl.clone()) {
                    return None;
                }
                let initializer = facts
                    .decl_by_id(&decl)?
                    .value_expr_syntax
                    .and_then(|syntax| LuaExpr::cast(syntax.to_node_from_root(root)?))
                    .and_then(unparenthesized);
                match initializer {
                    Some(init @ LuaExpr::TableExpr(_)) => expr = init,
                    Some(init) => {
                        if let Some((table, _)) = setmetatable_arguments(model, &init) {
                            expr = table;
                        } else {
                            return Some(TableIdentity::Declaration(decl));
                        }
                    }
                    None => return Some(TableIdentity::Declaration(decl)),
                }
            }
            _ => return None,
        }
    }
}

fn setmetatable_arguments(model: &SemanticModel, expr: &LuaExpr) -> Option<(LuaExpr, LuaExpr)> {
    let LuaExpr::CallExpr(call) = expr else {
        return None;
    };
    model.resolves_to_builtin(call.get_prefix_expr()?, "setmetatable")?;
    let mut args = call.get_args_list()?.get_args();
    Some((args.next()?, args.next()?))
}

fn metatable_value(model: &SemanticModel, expr: LuaExpr) -> LuaType {
    model
        .type_of_expr_node(&expr)
        .get_result_slot_type(0)
        .unwrap_or(LuaType::Unknown)
}
