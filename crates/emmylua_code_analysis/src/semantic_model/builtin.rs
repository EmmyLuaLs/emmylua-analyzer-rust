//! Resolve builtin identity from the values that reach a read.
//!
//! This query uses syntax and flow effects only. Type annotations describe a
//! call contract; they do not prove that a value is a standard function.

use super::cache::BuiltinKey;
use super::prelude::*;
use crate::semantic_db::flow::{FlowAntecedent, FlowEffect, FlowId, FlowNodeKind, LuaClosureId};
use emmylua_parser::LuaVarExpr;

enum BuiltinSource {
    Function(SmolStr),
    Aliases(Vec<BuiltinKey>),
}

impl<'db> SemanticModel<'db> {
    /// Match a standard function through the local values that reach this read.
    pub(crate) fn resolves_to_builtin(&self, expr: LuaExpr, builtin: &str) -> Option<()> {
        let query = BuiltinKey::Expression(expr.get_syntax_id());
        let key = (self.file_id(), query.clone());
        if let Some(name) = self.cache.borrow().builtin_function.get(&key) {
            return (name.as_deref() == Some(builtin)).then_some(());
        }

        enum Step {
            Read(BuiltinKey),
            Merge(BuiltinKey, usize),
        }

        let mut pending = vec![Step::Read(query)];
        let mut active = HashSet::new();
        let mut values: Vec<Option<SmolStr>> = Vec::new();
        while let Some(step) = pending.pop() {
            let (query, value) = match step {
                Step::Read(query) => {
                    let key = (self.file_id(), query.clone());
                    if let Some(value) = self.cache.borrow().builtin_function.get(&key) {
                        values.push(value.clone());
                        continue;
                    }
                    if !active.insert(query.clone()) {
                        values.push(None);
                        continue;
                    }
                    let value = match self.builtin_source(&query) {
                        Some(BuiltinSource::Aliases(sources)) => {
                            pending.push(Step::Merge(query, sources.len()));
                            pending.extend(sources.into_iter().rev().map(Step::Read));
                            continue;
                        }
                        Some(BuiltinSource::Function(name)) => Some(name),
                        None => None,
                    };
                    (query, value)
                }
                Step::Merge(query, count) => {
                    let start = values.len() - count;
                    let value = values[start].clone().filter(|name| {
                        values[start..]
                            .iter()
                            .all(|value| value.as_ref() == Some(name))
                    });
                    values.truncate(start);
                    (query, value)
                }
            };
            active.remove(&query);
            self.cache
                .borrow_mut()
                .builtin_function
                .insert((self.file_id(), query), value.clone());
            values.push(value);
        }
        (values.pop()?? == builtin).then_some(())
    }

    fn builtin_source(&self, query: &BuiltinKey) -> Option<BuiltinSource> {
        let syntax = match query {
            BuiltinKey::Expression(syntax) => syntax,
            BuiltinKey::Flow(decl, flow_id) => return self.builtin_flow_source(decl, *flow_id),
        };
        let root = self.syntax_tree()?.get_red_root();
        let expr = LuaExpr::cast(syntax.to_node_from_root(&root)?)?;
        let name = match expr {
            LuaExpr::ParenExpr(paren) => {
                return Some(BuiltinSource::Aliases(vec![BuiltinKey::Expression(
                    paren.get_expr()?.get_syntax_id(),
                )]));
            }
            LuaExpr::NameExpr(name) => name,
            _ => return None,
        };
        let decl = self.resolve_name(name.get_position());
        let Some(SemanticId::Decl(key)) = &decl else {
            return Some(BuiltinSource::Function(name.get_name_text()?.into()));
        };
        if self.is_std_file(key.file_id) {
            return Some(BuiltinSource::Function(name.get_name_text()?.into()));
        }
        if key.file_id != self.file_id() {
            return None;
        }
        let decl = decl?;
        let info = self.file_facts()?.decl_by_id(&decl)?;
        if !matches!(info.kind, DeclKind::Local { is_iter: false, .. }) {
            return None;
        }
        let tree = self.flow_tree()?;
        if !tree.has_decl_assignment(&decl) {
            return self.builtin_initializer(&decl);
        }

        // Deferred writes and later loop iterations can run before this read.
        // Keep those cases conservative instead of using source order.
        let owner = info.owner_syntax?.to_node_from_root(&root)?;
        let closure = LuaClosureId::from_node(&owner);
        if LuaClosureId::from_node(name.syntax()) != closure
            || name
                .syntax()
                .ancestors()
                .map(|node| LuaSyntaxKind::from(node.kind()))
                .take_while(|kind| *kind != LuaSyntaxKind::ClosureExpr)
                .any(|kind| {
                    matches!(
                        kind,
                        LuaSyntaxKind::WhileStat
                            | LuaSyntaxKind::RepeatStat
                            | LuaSyntaxKind::ForStat
                            | LuaSyntaxKind::ForRangeStat
                    )
                })
        {
            return None;
        }
        let key = (self.file_id(), decl.clone());
        let cached = self.cache.borrow().builtin_local_writes.get(&key).copied();
        let local_writes = if let Some(local_writes) = cached {
            local_writes
        } else {
            let local_writes = tree.decl_assignment_flows(&decl).all(|flow_id| {
                let Some(node) = tree.get_flow_node(flow_id) else {
                    return false;
                };
                node.kind
                    .assignment_syntax()
                    .and_then(|syntax| syntax.to_node_from_root(&root))
                    .is_some_and(|node| LuaClosureId::from_node(&node) == closure)
            });
            self.cache
                .borrow_mut()
                .builtin_local_writes
                .insert(key, local_writes);
            local_writes
        };
        if !local_writes {
            return None;
        }
        let flow_id = tree.get_flow_id_at(name.get_position())?;
        if tree.is_loop_reachable(flow_id) {
            return None;
        }
        let start = flow::skip_own_decl_assign(&decl, &tree, flow_id, name.get_position());
        Some(BuiltinSource::Aliases(vec![BuiltinKey::Flow(decl, start)]))
    }

    fn builtin_initializer(&self, decl: &SemanticId) -> Option<BuiltinSource> {
        let info = self.file_facts()?.decl_by_id(decl)?;
        if info.multi_return_index.is_some_and(|index| index > 0) {
            return None;
        }
        Some(BuiltinSource::Aliases(vec![BuiltinKey::Expression(
            info.value_expr_syntax?,
        )]))
    }

    /// Cache each declaration/flow state, including states between successive
    /// calls. No file-wide branch shortcut is needed for repeated reads.
    fn builtin_flow_source(&self, decl: &SemanticId, flow_id: FlowId) -> Option<BuiltinSource> {
        let tree = self.flow_tree()?;
        let node = tree.get_flow_node(flow_id)?;
        if tree.get_flow_effects(flow_id).iter().any(|effect| {
            matches!(effect, FlowEffect::AssignDecl { decl: assigned, .. } if assigned == decl)
        }) {
            match &node.kind {
                FlowNodeKind::DeclPosition(_) => return self.builtin_initializer(decl),
                FlowNodeKind::Assignment(ptr) => {
                    let root = self.syntax_tree()?.get_red_root();
                    let assign = LuaAssignStat::cast(ptr.get_syntax_id().to_node_from_root(&root)?)?;
                    let (vars, exprs) = assign.get_var_and_expr_list();
                    let mut slots = vars.iter().enumerate().filter_map(|(index, var)| {
                        let LuaVarExpr::NameExpr(name) = var else {
                            return None;
                        };
                        (self.resolve_name(name.get_position()).as_ref() == Some(decl))
                            .then_some(index)
                    });
                    let slot = slots.next()?;
                    if slots.next().is_some() {
                        return None;
                    }
                    // A name or parenthesized name supplies only one value.
                    return Some(BuiltinSource::Aliases(vec![BuiltinKey::Expression(
                        exprs.get(slot)?.get_syntax_id(),
                    )]));
                }
                // A function definition replaces the builtin with a closure.
                _ => return None,
            }
        }
        let sources = match &node.antecedent {
            Some(FlowAntecedent::Single(next)) => vec![BuiltinKey::Flow(decl.clone(), *next)],
            Some(FlowAntecedent::Multiple(id)) => tree
                .get_multi_antecedents(*id)?
                .iter()
                .map(|next| BuiltinKey::Flow(decl.clone(), *next))
                .collect(),
            None => return None,
        };
        (!sources.is_empty()).then_some(BuiltinSource::Aliases(sources))
    }
}
