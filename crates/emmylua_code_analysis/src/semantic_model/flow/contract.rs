//! First definitions establish write contracts; later writes only change read values.

use super::super::{
    SemanticModel,
    cache::{CacheEntry, QueryCacheScope},
};
use crate::semantic_db::flow::{FlowAntecedent, FlowEffect, FlowId, FlowNodeKind};
use crate::{LuaSignatureId, LuaType, SemanticId};
use emmylua_parser::{LuaAstNode, LuaClosureExpr, LuaSyntaxId};
use hashbrown::{HashMap, HashSet};
use std::collections::VecDeque;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Source {
    pub value: LuaSyntaxId,
    pub slot: usize,
    pub member: Option<SemanticId>,
}

impl Source {
    pub fn value_type(&self, model: &SemanticModel) -> LuaType {
        let node = model
            .syntax_tree()
            .and_then(|tree| self.value.to_node_from_root(&tree.get_red_root()));
        let typ = if let Some(closure) = node.and_then(LuaClosureExpr::cast) {
            super::apply_expr_casts(
                model,
                self.value,
                LuaType::Signature(LuaSignatureId::from_closure(model.file_id(), &closure)),
                super::ExprCastMode::AsOnly,
            )
        } else {
            model.type_of_expr_at(self.value, self.value.get_range().start())
        };
        typ.get_result_slot_type(self.slot).unwrap_or(LuaType::Nil)
    }

    fn annotation(&self, model: &SemanticModel) -> Option<LuaType> {
        let syntax = self
            .member
            .as_ref()
            .and_then(|id| model.file_facts()?.member_by_id(id)?.doc_type_syntax)?;
        Some(model.doc_type_lua(syntax))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Contract {
    pub typ: Option<LuaType>,
    placeholder: bool,
    values: Vec<(Source, bool)>,
    missing: bool,
}

impl Contract {
    pub fn absent() -> Self {
        Self {
            missing: true,
            ..Self::default()
        }
    }

    pub fn write(&mut self, model: &SemanticModel, source: Source) {
        if self.typ.is_none() {
            let annotation = source.annotation(model);
            let typ = annotation
                .clone()
                .unwrap_or_else(|| source.value_type(model));
            if typ.is_nil() && annotation.is_none() {
                self.placeholder = true;
            } else {
                let typ = if annotation.is_some() {
                    typ
                } else {
                    super::super::types::widen_assignment_type(typ)
                };
                self.typ = Some(if self.placeholder {
                    LuaType::from_vec(vec![typ, LuaType::Nil])
                } else {
                    typ
                });
            }
        }
        self.values = vec![(source, false)];
        self.missing = false;
    }

    pub fn join(paths: impl IntoIterator<Item = Self>) -> Option<Self> {
        let mut paths = paths.into_iter();
        let mut result = paths.next()?;
        for path in paths {
            result.typ = match (result.typ, path.typ) {
                (Some(left), Some(right)) => Some(LuaType::from_vec(vec![left, right])),
                (Some(typ), None) | (None, Some(typ)) => {
                    Some(LuaType::from_vec(vec![typ, LuaType::Nil]))
                }
                (None, None) => None,
            };
            result.placeholder |= path.placeholder;
            result.missing |= path.missing;
            for source in path.values {
                if !result.values.contains(&source) {
                    result.values.push(source);
                }
            }
        }
        Some(result)
    }

    pub fn stable_type(&self) -> LuaType {
        self.typ.clone().unwrap_or(LuaType::Nil)
    }

    pub fn read_type(&self, model: &SemanticModel) -> LuaType {
        let mut types = self
            .values
            .iter()
            .map(|(source, non_nil)| {
                let typ = source.value_type(model);
                if *non_nil {
                    super::super::types::remove_nil_from_type(typ)
                } else {
                    typ
                }
            })
            .collect::<Vec<_>>();
        if self.missing {
            types.push(LuaType::Nil);
        }
        LuaType::from_vec(types)
    }

    pub fn is_defined(&self) -> bool {
        !self.values.is_empty()
    }
}

pub(crate) type States = Rc<Vec<Option<Contract>>>;
pub(crate) type Key = (crate::FileId, SemanticId, LuaType);

/// Solve on the existing CFG. The queue also handles backward goto edges.
/// Sources stay lazy: an established contract does not inspect later RHS types.
pub(crate) fn infer(
    model: &SemanticModel,
    key: Key,
    anchor: FlowId,
    initial: Contract,
    writes: &HashMap<FlowId, Source>,
    binding: Option<&SemanticId>,
) -> States {
    if let Some(entry) = model.cache.borrow().contracts.get(&key) {
        if let Some(result) = entry.get(&model.cache.borrow()) {
            return result.clone();
        }
        return Rc::new(Vec::new());
    }
    let _scope = QueryCacheScope::new(&model.cache);
    let revision = model.cache.borrow().revision();
    model
        .cache
        .borrow_mut()
        .contracts
        .insert(key.clone(), CacheEntry::InProgress);
    let target = match binding {
        Some(owner) => super::super::table_constructor::member_key(&key.2)
            .map(|key| (owner.clone(), vec![key])),
        None => Some((key.1.clone(), Vec::new())),
    };
    let result = Rc::new(solve(
        model,
        anchor,
        initial,
        writes,
        binding,
        target.as_ref(),
    ));
    let complete = model.cache.borrow().is_complete(revision);
    model
        .cache
        .borrow_mut()
        .contracts
        .insert(key, CacheEntry::finish(result.clone(), complete));
    result
}

fn solve(
    model: &SemanticModel,
    anchor: FlowId,
    initial: Contract,
    writes: &HashMap<FlowId, Source>,
    binding: Option<&SemanticId>,
    target: Option<&(SemanticId, Vec<crate::LuaMemberKey>)>,
) -> Vec<Option<Contract>> {
    let Some(tree) = model.flow_tree() else {
        return Vec::new();
    };
    let mut successors = vec![Vec::new(); tree.node_count() as usize];
    for node in tree.nodes() {
        match node.antecedent {
            Some(FlowAntecedent::Single(id)) => successors[id.0 as usize].push(node.id),
            Some(FlowAntecedent::Multiple(index)) => {
                for id in tree.get_multi_antecedents(index).unwrap_or_default() {
                    successors[id.0 as usize].push(node.id);
                }
            }
            None => {}
        }
    }
    let mut states = vec![None; tree.node_count() as usize];
    let mut queue = VecDeque::from([anchor]);
    let mut queued = vec![false; tree.node_count() as usize];
    queued[anchor.0 as usize] = true;
    while let Some(id) = queue.pop_front() {
        queued[id.0 as usize] = false;
        let node = &tree.nodes()[id.0 as usize];
        let mut state = if id == anchor {
            Some(initial.clone())
        } else {
            let incoming: Vec<_> = match node.antecedent {
                Some(FlowAntecedent::Single(id)) => vec![id],
                Some(FlowAntecedent::Multiple(index)) => tree
                    .get_multi_antecedents(index)
                    .unwrap_or_default()
                    .to_vec(),
                None => Vec::new(),
            };
            Contract::join(
                incoming
                    .into_iter()
                    .filter_map(|id| states[id.0 as usize].clone()),
            )
        };
        if let Some(state) = &mut state
            && let Some(source) = writes.get(&id)
        {
            state.write(model, source.clone());
        }
        if let FlowNodeKind::CallExprStat(ptr) = &node.kind
            && super::call_stat_returns_never(model, ptr)
        {
            state = None;
        }
        if let FlowNodeKind::TrueCondition(ptr) | FlowNodeKind::FalseCondition(ptr) = &node.kind
            && let Some(cond) = super::flow_condition_expr(model, ptr)
        {
            let truthy = matches!(node.kind, FlowNodeKind::TrueCondition(_));
            if super::condition_branch_unreachable(model, &cond, truthy) {
                state = None;
            } else if let (Some(state), Some(target)) = (&mut state, target)
                && super::narrow_index_condition(model, target, cond, truthy).is_some()
            {
                state.missing = false;
                let mut seen = HashSet::new();
                state.values.retain_mut(|(source, non_nil)| {
                    *non_nil = true;
                    seen.insert(source.clone())
                });
            }
        }
        if states[id.0 as usize] != state {
            states[id.0 as usize] = state;
            // A rebind completes the old table value's interface. It cannot
            // introduce members inferred for its replacement into old aliases.
            if id != anchor
                && binding.is_some_and(|binding| {
                    tree.get_flow_effects(id).iter().any(|effect| {
                        matches!(effect, FlowEffect::AssignDecl { decl, .. } if decl == binding)
                    })
                })
            {
                continue;
            }
            for next in &successors[id.0 as usize] {
                if !queued[next.0 as usize] {
                    queued[next.0 as usize] = true;
                    queue.push_back(*next);
                }
            }
        }
    }
    states
}

/// Nil local initializers defer the same first-definition rule as nil fields.
pub(super) fn decl_target(
    model: &SemanticModel,
    decl: &SemanticId,
    offset: rowan::TextSize,
) -> Option<LuaType> {
    let id = model.flow_tree()?.get_flow_id_at(offset)?;
    let facts = model.file_facts()?;
    let info = facts.decl_by_id(decl)?;
    if info.doc_type_syntax.is_some()
        || !matches!(info.kind, crate::DeclKind::Local { .. })
        || !model.type_of_decl(decl)?.is_nil()
    {
        return None;
    }
    let owner = info.owner_syntax?;
    let tree = model.flow_tree()?;
    let anchor = tree.decl_position_flow(owner.get_range().start())?;
    let mut writes = HashMap::new();
    for node in tree.nodes() {
        if node.id == anchor {
            continue;
        }
        for effect in tree.get_flow_effects(node.id) {
            if let FlowEffect::AssignDecl {
                decl: assigned,
                value_syntax,
                result_slot,
            } = effect
                && assigned == decl
            {
                writes.insert(
                    node.id,
                    Source {
                        value: *value_syntax,
                        slot: *result_slot,
                        member: None,
                    },
                );
            }
        }
    }
    let initial = Contract {
        placeholder: true,
        missing: true,
        ..Contract::default()
    };
    let states = infer(
        model,
        (model.file_id(), decl.clone(), LuaType::Unknown),
        anchor,
        initial,
        &writes,
        None,
    );
    states
        .get(id.0 as usize)?
        .as_ref()
        .map(|state| state.typ.clone().unwrap_or(LuaType::Unknown))
}
