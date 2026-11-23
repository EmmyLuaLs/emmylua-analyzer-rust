//! Resolve builtin identity from the values that reach a read.
//!
//! This query uses syntax and flow effects only. Type annotations describe a
//! call contract; they do not prove that a value is a standard function.

use super::cache::BuiltinKey;
use super::prelude::*;
use crate::semantic_db::flow::{FlowAntecedent, FlowEffect, FlowId, FlowNodeKind, LuaClosureId};
use emmylua_parser::LuaVarExpr;

type BuiltinRead = (FileId, BuiltinKey);

enum BuiltinSource {
    Function(SmolStr),
    Aliases(Vec<BuiltinRead>),
}

impl BuiltinSource {
    fn alias(file_id: FileId, key: BuiltinKey) -> Self {
        Self::Aliases(vec![(file_id, key)])
    }
}

impl<'db> SemanticModel<'db> {
    /// Match a standard function through the values that reach this read.
    pub(crate) fn resolves_to_builtin(&self, expr: LuaExpr, builtin: &str) -> Option<()> {
        let query = (self.file_id(), BuiltinKey::Expression(expr.get_syntax_id()));
        if let Some(name) = self.cache.borrow().builtin_function.get(&query) {
            return (name.as_deref() == Some(builtin)).then_some(());
        }

        let mut pending = vec![query.clone()];
        let mut visited = HashSet::new();
        let mut dependents: HashMap<BuiltinRead, Vec<BuiltinRead>> = HashMap::new();
        // No entry means unresolved. None is a known replacement or a conflict.
        let mut origins: HashMap<BuiltinRead, Option<SmolStr>> = HashMap::new();
        let mut changed = Vec::new();
        while let Some(read) = pending.pop() {
            if !visited.insert(read.clone()) {
                continue;
            }
            let cached = self.cache.borrow().builtin_function.get(&read).cloned();
            let origin = match cached {
                Some(origin) => origin,
                None => match self.model_for(read.0).builtin_source(&read.1) {
                    Some(BuiltinSource::Aliases(sources)) if !sources.is_empty() => {
                        for source in sources {
                            dependents
                                .entry(source.clone())
                                .or_default()
                                .push(read.clone());
                            pending.push(source);
                        }
                        continue;
                    }
                    Some(BuiltinSource::Function(name)) => Some(name),
                    _ => None,
                },
            };
            origins.insert(read.clone(), origin);
            changed.push(read);
        }

        // Propagate origins through aliases and loop edges until they agree.
        // Each read can change only from unresolved to one builtin, then unknown.
        while let Some(read) = changed.pop() {
            let origin = origins[&read].clone();
            for dependent in dependents.get(&read).into_iter().flatten() {
                let merged = match origins.get(dependent) {
                    None => origin.clone(),
                    Some(Some(current)) if origin.as_ref() == Some(current) => {
                        Some(current.clone())
                    }
                    _ => None,
                };
                if origins.get(dependent) != Some(&merged) {
                    origins.insert(dependent.clone(), merged);
                    changed.push(dependent.clone());
                }
            }
        }
        let matches = origins
            .get(&query)
            .is_some_and(|origin| origin.as_deref() == Some(builtin));
        self.cache.borrow_mut().builtin_function.extend(origins);
        matches.then_some(())
    }

    fn builtin_source(&self, query: &BuiltinKey) -> Option<BuiltinSource> {
        let syntax = match query {
            BuiltinKey::Expression(syntax) => syntax,
            BuiltinKey::Flow(decl, flow_id) => return self.builtin_flow_source(decl, *flow_id),
            BuiltinKey::Declaration(decl, start) => return self.builtin_decl_source(decl, *start),
            BuiltinKey::InitialGlobal(name) => return Some(BuiltinSource::Function(name.clone())),
        };
        let root = self.syntax_tree()?.get_red_root();
        let expr = LuaExpr::cast(syntax.to_node_from_root(&root)?)?;
        let name = match expr {
            LuaExpr::ParenExpr(paren) => {
                return Some(BuiltinSource::alias(
                    self.file_id(),
                    BuiltinKey::Expression(paren.get_expr()?.get_syntax_id()),
                ));
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
        let file_id = key.file_id;
        let decl = decl?;
        let info = self.file_facts_of(file_id)?.decl_by_id(&decl)?;
        if matches!(info.kind, DeclKind::Global) {
            return Some(BuiltinSource::alias(
                file_id,
                BuiltinKey::Declaration(decl, None),
            ));
        }
        if file_id != self.file_id() || !matches!(info.kind, DeclKind::Local { is_iter: false, .. })
        {
            return None;
        }
        let tree = self.flow_tree()?;
        if !tree.has_decl_assignment(&decl) {
            return self.builtin_initializer(&decl);
        }

        let owner = info.owner_syntax?.to_node_from_root(&root)?;
        let closure = LuaClosureId::from_node(&owner);
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
        if !local_writes || LuaClosureId::from_node(name.syntax()) != closure {
            // A deferred read or write can observe any value of this local.
            return Some(BuiltinSource::alias(
                self.file_id(),
                BuiltinKey::Declaration(decl, None),
            ));
        }
        let flow_id = tree.get_flow_id_at(name.get_position())?;
        let start = flow::skip_own_decl_assign(&decl, &tree, flow_id, name.get_position());
        if tree.is_loop_reachable(flow_id)
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
            // The point value alone omits possible writes from later iterations.
            return Some(BuiltinSource::alias(
                self.file_id(),
                BuiltinKey::Declaration(decl, Some(start)),
            ));
        }
        Some(BuiltinSource::alias(
            self.file_id(),
            BuiltinKey::Flow(decl, start),
        ))
    }

    fn builtin_decl_source(
        &self,
        decl: &SemanticId,
        start: Option<FlowId>,
    ) -> Option<BuiltinSource> {
        let info = self.file_facts()?.decl_by_id(decl)?;
        let mut sources = Vec::new();
        let decls = if matches!(info.kind, DeclKind::Global) {
            // Globals start with the standard value. Every workspace write
            // must preserve that origin, including writes from other files.
            sources.push((self.file_id(), BuiltinKey::InitialGlobal(info.name.clone())));
            self.global_decls_named_for_file(self.file_id(), info.name.as_str())
        } else {
            let initial = match start {
                Some(start) => BuiltinKey::Flow(decl.clone(), start),
                None => {
                    if info.multi_return_index.is_some_and(|index| index > 0) {
                        return None;
                    }
                    BuiltinKey::Expression(info.value_expr_syntax?)
                }
            };
            sources.push((self.file_id(), initial));
            vec![decl.clone()]
        };
        for decl in decls {
            let SemanticId::Decl(key) = &decl else {
                return None;
            };
            let model = self.model_for(key.file_id);
            sources.extend(
                model
                    .flow_tree()?
                    .decl_assignment_flows(&decl)
                    .map(|flow| (key.file_id, BuiltinKey::Flow(decl.clone(), flow))),
            );
        }
        Some(BuiltinSource::Aliases(sources))
    }

    fn builtin_initializer(&self, decl: &SemanticId) -> Option<BuiltinSource> {
        let info = self.file_facts()?.decl_by_id(decl)?;
        if info.multi_return_index.is_some_and(|index| index > 0) {
            return None;
        }
        Some(BuiltinSource::alias(
            self.file_id(),
            BuiltinKey::Expression(info.value_expr_syntax?),
        ))
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
                    return Some(BuiltinSource::alias(self.file_id(), BuiltinKey::Expression(
                        exprs.get(slot)?.get_syntax_id(),
                    )));
                }
                // A function definition replaces the builtin with a closure.
                _ => return None,
            }
        }
        let sources = match &node.antecedent {
            Some(FlowAntecedent::Single(next)) => {
                vec![(self.file_id(), BuiltinKey::Flow(decl.clone(), *next))]
            }
            Some(FlowAntecedent::Multiple(id)) => tree
                .get_multi_antecedents(*id)?
                .iter()
                .map(|next| (self.file_id(), BuiltinKey::Flow(decl.clone(), *next)))
                .collect(),
            None => return None,
        };
        (!sources.is_empty()).then_some(BuiltinSource::Aliases(sources))
    }
}
