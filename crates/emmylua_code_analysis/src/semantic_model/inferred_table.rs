//! Inferred table interfaces share stable contracts, but each reference has its own flow facts.
//!
//! Discover sources without resolving function bodies. First definitions and joins
//! use the same CFG as scalar flow typing; reads and escapes do not close an interface.

use super::flow::contract::{self, Contract, Source};
use super::table_constructor::{TableConstructor, key_type, member_key, table_expr};
use super::type_check::{check_type_subtype, is_compatible};
use super::type_eval::expand_alias_branches;
use super::{SemanticModel, member::MemberInfo};
use crate::semantic_db::flow::{
    FlowEffect, FlowId, FlowNodeKind, LuaClosureId, assignment_sources,
};
use crate::{DeclKind, InFiled, LuaMemberKey, LuaType, SemanticId};
use emmylua_parser::{
    LuaAstNode, LuaComment, LuaDocTagAs, LuaExpr, LuaIndexExpr, LuaIndexKey, LuaLocalStat,
    LuaSyntaxId, LuaTableExpr, LuaVarExpr,
};
use hashbrown::HashMap;
use rowan::TextRange;
use std::rc::Rc;

struct Definition {
    key: LuaIndexKey,
    source: Source,
    flow: Option<FlowId>,
}

pub(crate) struct InferredTable {
    table: LuaTableExpr,
    file_id: crate::FileId,
    decl: Option<SemanticId>,
    closure: LuaClosureId,
    anchor: Option<FlowId>,
    rebindings: Vec<FlowId>,
    definitions: Vec<Definition>,
}

impl InferredTable {
    pub(crate) fn has_local_binding(&self) -> bool {
        self.decl.is_some()
    }

    fn direct_at(&self, model: &SemanticModel, syntax: LuaSyntaxId) -> bool {
        if model.file_id() != self.file_id {
            return false;
        }
        let Some(node) = model
            .syntax_tree()
            .and_then(|tree| syntax.to_node_from_root(&tree.get_red_root()))
        else {
            return false;
        };
        if LuaClosureId::from_node(&node) != self.closure {
            return false;
        }
        let expr = LuaExpr::cast(node);
        let expr = match expr {
            Some(LuaExpr::IndexExpr(index)) => index.get_prefix_expr(),
            expr => expr,
        };
        expr.is_some_and(|expr| self.is_owner(model, expr))
    }

    fn is_owner(&self, model: &SemanticModel, mut expr: LuaExpr) -> bool {
        while let LuaExpr::ParenExpr(paren) = expr {
            let Some(inner) = paren.get_expr() else {
                return false;
            };
            expr = inner;
        }
        matches!(expr, LuaExpr::NameExpr(name) if model.resolve_name(name.get_position()).as_ref() == self.decl.as_ref() && self.decl.is_some())
    }

    fn domains(
        &self,
        model: &SemanticModel,
        filter: Option<&LuaMemberKey>,
    ) -> Vec<(LuaMemberKey, LuaType)> {
        let mut domains = Vec::new();
        for definition in &self.definitions {
            // Resolve only computed keys. Named-member lookup must not infer other values.
            let domain = if let Some(key) = literal_key(&definition.key) {
                if filter.is_some_and(|filter| filter != &key) {
                    continue;
                }
                key.to_index_type()
            } else {
                key_type(model, definition.key.clone()).ok()
            };
            let Some(domain) = domain else {
                continue;
            };
            let key = member_key(&domain).unwrap_or(LuaMemberKey::None);
            if !domains.contains(&(key.clone(), domain.clone())) {
                domains.push((key, domain));
            }
        }
        domains
    }

    fn state(
        &self,
        model: &SemanticModel,
        domain: &LuaType,
        syntax: Option<LuaSyntaxId>,
    ) -> Contract {
        let model = model.model_for(self.file_id);
        let mut initial = Contract::absent();
        let mut writes = HashMap::new();
        for definition in &self.definitions {
            if key_type(&model, definition.key.clone()).ok().as_ref() != Some(domain) {
                continue;
            }
            if let Some(flow) = definition.flow {
                writes.insert(flow, definition.source.clone());
            } else {
                initial.write(&model, definition.source.clone());
            }
        }
        let Some(anchor) = self.anchor else {
            return initial;
        };
        let states = contract::infer(
            &model,
            (
                self.file_id,
                SemanticId::member(self.file_id, self.table.get_range()),
                domain.clone(),
            ),
            anchor,
            initial.clone(),
            &writes,
            self.decl.as_ref(),
        );
        let Some(tree) = model.flow_tree() else {
            return initial;
        };
        if let Some(syntax) = syntax
            && self.direct_at(&model, syntax)
        {
            return tree
                .get_flow_id(syntax)
                .or_else(|| tree.get_flow_id_at(syntax.get_range().start()))
                .and_then(|id| states.get(id.0 as usize))
                .cloned()
                .flatten()
                .unwrap_or(initial);
        }
        Contract::join(
            tree.normal_exits(self.closure)
                .iter()
                .chain(&self.rebindings)
                .filter_map(|id| states.get(id.0 as usize).cloned().flatten()),
        )
        .unwrap_or(initial)
    }

    fn source_id(&self, key: &LuaMemberKey) -> Option<SemanticId> {
        self.definitions
            .iter()
            .find(|definition| literal_key(&definition.key).as_ref() == Some(key))?
            .source
            .member
            .clone()
    }

    /// The completed stable interface is independent of the reference and use position.
    fn member_entries(
        &self,
        model: &SemanticModel,
        filter: Option<&LuaMemberKey>,
    ) -> Vec<(LuaType, MemberInfo)> {
        let source_model = model.model_for(self.file_id);
        let mut members = Vec::new();
        for (key, domain) in self.domains(&source_model, filter) {
            let state = self.state(model, &domain, None);
            if !state.is_defined() {
                continue;
            }
            let id = self.source_id(&key);
            let is_method = id.as_ref().is_some_and(|id| {
                source_model
                    .file_facts()
                    .and_then(|facts| facts.member_by_id(id))
                    .is_some_and(|member| member.is_method)
            });
            members.push((
                domain,
                MemberInfo {
                    typ: state.stable_type(),
                    key,
                    id,
                    file_id: Some(self.file_id),
                    is_method,
                },
            ));
        }
        // A final list field can expand a multiple return. Other values stay lazy.
        if filter.is_none_or(|key| matches!(key, LuaMemberKey::Integer(_)))
            && self
                .table
                .get_fields_with_keys()
                .last()
                .is_some_and(|(_, key)| matches!(key, LuaIndexKey::Idx(_)))
        {
            if let Some(Ok(expanded)) = TableConstructor::new(&source_model, &self.table)
                .field_entries()
                .last()
            {
                for (domain, value) in expanded {
                    let key = member_key(&domain).unwrap_or(LuaMemberKey::None);
                    if members.iter().any(|(_, member)| member.key == key)
                        || filter.is_some_and(|filter| !key.is_none() && filter != &key)
                    {
                        continue;
                    }
                    members.push((
                        domain,
                        MemberInfo {
                            typ: source_model.member_contract_type(None, value.typ),
                            key,
                            id: None,
                            file_id: Some(self.file_id),
                            is_method: false,
                        },
                    ));
                }
            }
        }
        members
    }

    pub(crate) fn members(
        &self,
        model: &SemanticModel,
        filter: Option<&LuaMemberKey>,
    ) -> Vec<MemberInfo> {
        self.member_entries(model, filter)
            .into_iter()
            .filter_map(|(_, member)| (!member.key.is_none()).then_some(member))
            .collect()
    }

    pub(crate) fn entries(&self, model: &SemanticModel) -> Vec<(LuaType, LuaType)> {
        self.member_entries(model, None)
            .into_iter()
            .map(|(domain, member)| (domain, member.typ))
            .collect()
    }
}

/// Key selection is shared by reads, write contracts, and field diagnostics.
pub(crate) struct Access<'a, 'db> {
    model: &'a SemanticModel<'db>,
    inferred: Rc<InferredTable>,
    syntax: Option<LuaSyntaxId>,
    key: Option<LuaType>,
}

impl Access<'_, '_> {
    fn direct(&self) -> bool {
        self.syntax
            .is_some_and(|syntax| self.inferred.direct_at(self.model, syntax))
    }

    fn selected(&self) -> Vec<(LuaMemberKey, LuaType)> {
        let model = self.model.model_for(self.inferred.file_id);
        self.inferred
            .domains(&model, None)
            .into_iter()
            .filter(|(_, domain)| {
                self.key
                    .as_ref()
                    .is_some_and(|key| key_domains_overlap(self.model, key, domain))
            })
            .collect()
    }

    pub(crate) fn read(&self) -> LuaType {
        let Some(key) = &self.key else {
            return LuaType::Unknown;
        };
        if key.is_unknown() {
            return LuaType::Unknown;
        }
        let direct = self.direct();
        let source_model = self.model.model_for(self.inferred.file_id);
        let mut types = Vec::new();
        let mut present = Vec::new();
        for (member, domain) in self.selected() {
            let state = self
                .inferred
                .state(self.model, &domain, self.syntax.filter(|_| direct));
            let typ = if direct {
                state.read_type(&source_model)
            } else {
                state.stable_type()
            };
            if !member.is_none() && !typ.is_nullable() {
                present.push(domain);
            }
            types.push(typ);
        }
        if let Some(member) = member_key(key)
            && types.is_empty()
        {
            if let Ok(Some(value)) =
                TableConstructor::new(&source_model, &self.inferred.table).member_value(&member)
            {
                return value.typ;
            }
        }
        let covered = !present.is_empty()
            && expand_alias_branches(self.model, key).is_some_and(|key| {
                !matches!(key, LuaType::Any | LuaType::Unknown)
                    && check_type_subtype(self.model, &key, &LuaType::from_vec(present)).is_ok()
            });
        if !covered {
            types.push(LuaType::Nil);
        }
        LuaType::from_vec(types)
    }

    pub(crate) fn has_exact_key(&self) -> bool {
        self.key.as_ref().and_then(member_key).is_some()
    }
    pub(crate) fn member_id(&self) -> Option<SemanticId> {
        self.inferred.source_id(&member_key(self.key.as_ref()?)?)
    }
    pub(crate) fn has_field(&self) -> bool {
        self.key.as_ref().is_none_or(|key| key.is_unknown())
            || self.selected().iter().any(|(_, domain)| {
                self.inferred
                    .state(self.model, domain, self.syntax.filter(|_| self.direct()))
                    .is_defined()
            })
            || self.key.as_ref().and_then(member_key).is_some_and(|key| {
                matches!(key, LuaMemberKey::Integer(_))
                    && TableConstructor::new(
                        &self.model.model_for(self.inferred.file_id),
                        &self.inferred.table,
                    )
                    .member_value(&key)
                    .ok()
                    .flatten()
                    .is_some()
            })
    }

    pub(crate) fn annotation_mismatch(&self) -> Option<(LuaType, LuaType)> {
        if !self.direct() {
            return None;
        }
        let syntax = self.syntax?;
        let node = self.model.syntax_tree()?.get_red_root();
        let index = syntax
            .to_node_from_root(&node)
            .and_then(LuaIndexExpr::cast)?;
        let member = self
            .model
            .file_facts()?
            .member_at_offset(index.get_index_key()?.get_range()?.start())?;
        let annotation = self.model.doc_type_lua(member.doc_type_syntax?);
        let domain = self.key.as_ref()?;
        let previous = self.inferred.state(self.model, domain, self.syntax).typ?;
        (!is_compatible(self.model, &annotation, &previous)
            || !is_compatible(self.model, &previous, &annotation))
        .then_some((previous, annotation))
    }

    pub(crate) fn write_targets(&self) -> Option<impl Iterator<Item = LuaType> + '_> {
        let direct = self.direct();
        let states: Vec<_> = self
            .selected()
            .into_iter()
            .map(|(_, domain)| {
                let state =
                    self.inferred
                        .state(self.model, &domain, self.syntax.filter(|_| direct));
                (domain, state)
            })
            .collect();
        if !direct
            && self.key.as_ref().is_some_and(|key| {
                let domains = states
                    .iter()
                    .filter(|(_, state)| state.is_defined())
                    .map(|(domain, _)| domain.clone())
                    .collect::<Vec<_>>();
                domains.is_empty() || !is_compatible(self.model, key, &LuaType::from_vec(domains))
            })
        {
            return None;
        }
        Some(states.into_iter().filter_map(|(_, state)| state.typ))
    }
}

pub(crate) fn of(model: &SemanticModel, table: &InFiled<TextRange>) -> Option<Rc<InferredTable>> {
    if let Some(result) = model.cache.borrow().inferred_tables.get(table) {
        return result.clone();
    }
    let source_model = model.model_for(table.file_id);
    let result = build(&source_model, table.value).map(Rc::new);
    model
        .cache
        .borrow_mut()
        .inferred_tables
        .insert(table.clone(), result.clone());
    result
}

pub(crate) fn access<'a, 'db>(
    model: &'a SemanticModel<'db>,
    index: &LuaIndexExpr,
) -> Option<Access<'a, 'db>> {
    let mut prefix = index.get_prefix_expr()?;
    while let LuaExpr::ParenExpr(paren) = &prefix {
        prefix = paren.get_expr()?;
    }
    let typ = if let LuaExpr::NameExpr(name) = &prefix {
        let decl = model.resolve_name(name.get_position())?;
        // The flow type selects the current table after a rebind.
        model.type_of_decl_at(&decl, name.get_position())
    } else {
        model.type_of_expr_at(prefix.get_syntax_id(), prefix.get_position())
    };
    access_with_types(
        model,
        index.get_syntax_id(),
        &typ,
        index
            .get_index_key()
            .and_then(|key| key_type(model, key).ok()),
    )
}

pub(crate) fn access_with_types<'a, 'db>(
    model: &'a SemanticModel<'db>,
    syntax: LuaSyntaxId,
    prefix: &LuaType,
    key: Option<LuaType>,
) -> Option<Access<'a, 'db>> {
    let LuaType::TableConst(table) = prefix else {
        return None;
    };
    let inferred = of(model, table)?;
    Some(Access {
        syntax: (model.file_id() == inferred.file_id).then_some(syntax),
        key: key.map(super::table_constructor::normalize_key_type),
        model,
        inferred,
    })
}

fn build(model: &SemanticModel, range: TextRange) -> Option<InferredTable> {
    let facts = model.file_facts()?;
    if facts.is_meta {
        return None;
    }
    let tree = model.syntax_tree()?;
    let table = tree
        .get_red_root()
        .covering_element(range)
        .into_node()
        .and_then(LuaTableExpr::cast)?;
    let mut value = LuaExpr::TableExpr(table.clone());
    while let Some(parent) = value
        .syntax()
        .parent()
        .and_then(emmylua_parser::LuaParenExpr::cast)
    {
        value = LuaExpr::ParenExpr(parent);
    }
    let decl = facts.decl_by_value_range(value.get_range());
    if decl.is_some_and(|decl| {
        !matches!(decl.kind, DeclKind::Local { .. })
            || decl.doc_type_syntax.is_some()
            || decl
                .owner_syntax
                .and_then(|owner| facts.type_def_by_owner_syntax(owner))
                .is_some()
    }) {
        return None;
    }
    if decl.is_none() {
        let parent = table
            .syntax()
            .parent()?
            .ancestors()
            .find_map(LuaTableExpr::cast)?;
        of(model, &InFiled::new(model.file_id(), parent.get_range()))?;
    }
    let closure = LuaClosureId::from_node(table.syntax());
    let mut inferred = InferredTable {
        table,
        file_id: model.file_id(),
        decl: decl.map(|decl| decl.id.clone()),
        closure,
        anchor: None,
        rebindings: Vec::new(),
        definitions: Vec::new(),
    };
    for (field, key) in inferred.table.get_fields_with_keys() {
        if let Some(value) = field.get_value_expr() {
            let member = facts
                .members
                .iter()
                .find(|member| member.value_syntax == Some(value.get_syntax_id()))
                .map(|member| member.id.clone());
            inferred.definitions.push(Definition {
                key,
                source: Source {
                    value: value.get_syntax_id(),
                    slot: 0,
                    member,
                },
                flow: None,
            });
        }
    }
    let Some(_) = decl else {
        return Some(inferred);
    };
    let stat = inferred.table.ancestors::<LuaLocalStat>().next()?;
    if stat.descendants::<LuaDocTagAs>().any(|tag| {
        tag.get_parent::<LuaComment>()
            .and_then(|comment| comment.get_owner())
            .and_then(|owner| {
                LuaExpr::cast(owner.syntax().clone()).or_else(|| owner.children::<LuaExpr>().last())
            })
            .and_then(|expr| table_expr(&expr))
            .is_some_and(|table| table == inferred.table)
    }) {
        return None;
    }
    let flow = model.flow_tree()?;
    inferred.anchor = flow.decl_position_flow(stat.get_position());
    let chunk = model.chunk()?;
    for node in flow.nodes() {
        // Effects cover resolved members only. Syntax discovery also includes
        // dynamic keys, and checks that the original binding is the direct prefix.
        let definitions = match &node.kind {
            FlowNodeKind::Assignment(ptr) => {
                let assign = ptr.to_node(&chunk)?;
                if LuaClosureId::from_node(assign.syntax()) == closure
                    && flow.get_flow_effects(node.id).iter().any(|effect| {
                        matches!(effect, FlowEffect::AssignDecl { decl, .. } if Some(decl) == inferred.decl.as_ref())
                    })
                {
                    inferred.rebindings.push(node.id);
                }
                let (vars, values) = assign.get_var_and_expr_list();
                assignment_sources(vars, &values)
                    .filter_map(|(var, value, slot)| {
                        let LuaVarExpr::IndexExpr(index) = var else {
                            return None;
                        };
                        Some((index, value.clone(), slot))
                    })
                    .collect::<Vec<_>>()
            }
            FlowNodeKind::ImplFunc(ptr) => {
                let stat = ptr.to_node(&chunk)?;
                match (stat.get_func_name(), stat.get_closure()) {
                    (Some(LuaVarExpr::IndexExpr(index)), Some(closure)) => {
                        vec![(index, LuaExpr::ClosureExpr(closure), 0)]
                    }
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        };
        for (index, value, slot) in definitions {
            if LuaClosureId::from_node(index.syntax()) != closure
                || !index
                    .get_prefix_expr()
                    .is_some_and(|expr| inferred.is_owner(model, expr))
            {
                continue;
            }
            let Some(key) = index.get_index_key() else {
                continue;
            };
            let member = facts
                .member_at_offset(key.get_range().unwrap_or_else(|| index.get_range()).start())
                .map(|member| member.id.clone());
            inferred.definitions.push(Definition {
                key,
                source: Source {
                    value: value.get_syntax_id(),
                    slot,
                    member,
                },
                flow: Some(node.id),
            });
        }
    }
    Some(inferred)
}

fn key_domains_overlap(model: &SemanticModel, left: &LuaType, right: &LuaType) -> bool {
    if is_compatible(model, left, right) || is_compatible(model, right, left) {
        return true;
    }
    let (Some(left), Some(right)) = (
        expand_alias_branches(model, left),
        expand_alias_branches(model, right),
    ) else {
        return true;
    };
    let alternatives = |typ| match typ {
        LuaType::Union(union) => union.into_vec(),
        typ => vec![typ],
    };
    let left = alternatives(left);
    let right = alternatives(right);
    left.iter().any(|left| {
        right
            .iter()
            .any(|right| is_compatible(model, left, right) || is_compatible(model, right, left))
    })
}

fn literal_key(key: &LuaIndexKey) -> Option<LuaMemberKey> {
    match key {
        LuaIndexKey::Expr(_) => None,
        key => Some(match key {
            LuaIndexKey::Idx(index) => LuaMemberKey::Integer(*index as i64),
            LuaIndexKey::Integer(number) => match number.get_number_value() {
                emmylua_parser::NumberResult::Int(value) => LuaMemberKey::Integer(value),
                _ => return None,
            },
            _ => LuaMemberKey::Name(key.get_path_part().into()),
        }),
    }
}
