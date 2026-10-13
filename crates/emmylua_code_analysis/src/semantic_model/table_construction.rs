//! Stable shapes of fresh local tables, before and after construction seals.
//!
//! The plan records value sources before resolving their field types. Direct
//! function bodies can then use the sealed shape without making shape discovery
//! depend on inference of those same bodies.

use std::rc::Rc;

use emmylua_parser::{
    LuaAstNode, LuaBlock, LuaCallExpr, LuaClosureExpr, LuaComment, LuaDocTagAs, LuaDocTagCast,
    LuaExpr, LuaIndexExpr, LuaIndexKey, LuaLiteralToken, LuaNameExpr, LuaStat, LuaSyntaxId,
    LuaTableExpr, LuaVarExpr,
};
use hashbrown::{HashMap, HashSet};
use rowan::TextRange;

use super::SemanticModel;
use super::member::MemberInfo;
use super::table_constructor::{TableConstructor, key_type, member_key, table_expr};
use super::type_check::{check_type_subtype, is_compatible};
use super::type_eval::expand_alias_branches;
use crate::semantic_db::query::TypeShellHead;
use crate::semantic_db::types::{LiteralShell, PrimitiveType, TypeCandidate, TypeShell};
use crate::{DeclKind, InFiled, LuaMemberKey, LuaSignatureId, LuaType, SemanticId};

#[derive(Clone, Copy)]
enum FieldType {
    Read,
    Contract,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Source {
    value: LuaSyntaxId,
    slot: usize,
    member: Option<SemanticId>,
}

#[derive(Clone, Debug)]
struct Field {
    sources: Vec<Source>,
    optional: bool,
    reassigned: bool,
}

// Previous key domains distinguish a later write from a first write on an
// independent branch. Resolve their types only after the syntax plan is built.
#[derive(Clone, Debug)]
struct Index {
    key: LuaIndexKey,
    source: Source,
    previous: Vec<LuaIndexKey>,
}

#[derive(Clone, Debug, Default)]
struct Shape {
    fields: HashMap<LuaMemberKey, Field>,
    indexes: Vec<Index>,
    sealed: bool,
}

impl Shape {
    fn write(&mut self, key: LuaIndexKey, source: Source) {
        if let Some(key) = literal_key(&key) {
            if let Some(field) = self.fields.get_mut(&key) {
                field.reassigned = true;
                if !self.sealed {
                    field.optional = false;
                }
            } else if !self.sealed {
                self.fields.insert(
                    key,
                    Field {
                        sources: vec![source],
                        optional: false,
                        reassigned: false,
                    },
                );
            }
        } else {
            for field in self.fields.values_mut() {
                field.reassigned = true;
            }
            if !self.sealed && !self.indexes.iter().any(|index| index.source == source) {
                self.indexes.push(Index {
                    key,
                    source,
                    previous: self.indexes.iter().map(|index| index.key.clone()).collect(),
                });
            }
        }
    }

    fn join(paths: Vec<Self>) -> Option<Self> {
        let mut paths = paths.into_iter();
        let mut result = paths.next()?;
        for path in paths {
            for (key, field) in &mut result.fields {
                if let Some(other) = path.fields.get(key) {
                    extend_unique(&mut field.sources, &other.sources);
                    field.optional |= other.optional;
                    field.reassigned |= other.reassigned;
                } else {
                    field.optional = true;
                }
            }
            for (key, mut field) in path.fields {
                if !result.fields.contains_key(&key) {
                    field.optional = true;
                    result.fields.insert(key, field);
                }
            }
            for index in path.indexes {
                if !result.indexes.iter().any(|old| old.source == index.source) {
                    result.indexes.push(index);
                }
            }
            result.sealed |= path.sealed;
        }
        Some(result)
    }
}

fn extend_unique<T: Clone + PartialEq>(into: &mut Vec<T>, from: &[T]) {
    for item in from {
        if !into.contains(item) {
            into.push(item.clone());
        }
    }
}

// Selection keeps sources lazy. Only the final expanding constructor field
// needs its value type to determine which keys it supplies.
struct Entry<'a> {
    key: LuaMemberKey,
    domain: LuaType,
    value: EntryValue<'a>,
}

enum EntryValue<'a> {
    Field(&'a Field),
    Index(Vec<&'a Source>),
    Expanded(LuaType),
}

impl Entry<'_> {
    fn value_type(
        &self,
        construction: &Construction,
        model: &SemanticModel,
        mode: FieldType,
    ) -> LuaType {
        match &self.value {
            EntryValue::Field(field) => {
                // Computed reads cannot trace one field path after a write.
                let mode = if field.reassigned {
                    FieldType::Contract
                } else {
                    mode
                };
                construction.field_type(model, field, mode)
            }
            EntryValue::Index(sources) => LuaType::from_vec(
                sources
                    .iter()
                    .map(|source| construction.source_type(model, source, FieldType::Contract))
                    .collect(),
            ),
            EntryValue::Expanded(typ) => typ.clone(),
        }
    }

    fn into_info(self, construction: &Construction, model: &SemanticModel) -> MemberInfo {
        let id = match &self.value {
            EntryValue::Field(field) => field.sources[0].member.clone(),
            _ => None,
        };
        let is_method = id.as_ref().is_some_and(|id| {
            model
                .file_facts_of(construction.file_id)
                .and_then(|facts| facts.member_by_id(id))
                .is_some_and(|member| member.is_method)
        });
        MemberInfo {
            typ: self.value_type(construction, model, FieldType::Contract),
            index_key_type: matches!(self.key, LuaMemberKey::None).then_some(self.domain),
            key: self.key,
            id,
            file_id: Some(construction.file_id),
            is_method,
        }
    }
}

pub(crate) struct Construction {
    table: LuaTableExpr,
    file_id: crate::FileId,
    decl: Option<SemanticId>,
    snapshots: HashMap<LuaSyntaxId, Shape>,
    final_shape: Shape,
    deferred_shape: Shape,
    deferred_bodies: Vec<TextRange>,
}

impl Construction {
    pub(crate) fn has_local_binding(&self) -> bool {
        self.decl.is_some()
    }

    fn shape_at(&self, syntax: Option<LuaSyntaxId>) -> &Shape {
        syntax
            .and_then(|syntax| self.snapshots.get(&syntax))
            .unwrap_or_else(|| {
                if syntax.is_some_and(|syntax| {
                    self.deferred_bodies
                        .iter()
                        .any(|body| body.contains_range(syntax.get_range()))
                }) {
                    &self.deferred_shape
                } else {
                    &self.final_shape
                }
            })
    }

    fn source_type(&self, model: &SemanticModel, source: &Source, mode: FieldType) -> LuaType {
        let model = model.model_for(self.file_id);
        let model = &model;
        let facts = model.file_facts();
        let member = source
            .member
            .as_ref()
            .and_then(|id| facts?.member_by_id(id));
        if matches!(mode, FieldType::Contract)
            && let Some(annotation) = member.and_then(|member| member.doc_type_syntax)
        {
            return model.doc_type_lua(annotation);
        }
        let node = model
            .syntax_tree()
            .and_then(|tree| source.value.to_node_from_root(&tree.get_red_root()));
        let typ = if let Some(closure) = node.and_then(LuaClosureExpr::cast)
            && self.decl.is_some()
        {
            // Keep the signature lazy until a call or structural check needs it.
            LuaType::Signature(LuaSignatureId::from_closure(model.file_id(), &closure))
        } else {
            model.type_of_expr_at(source.value, source.value.get_range().start())
        };
        let typ = typ
            .get_result_slot_type(source.slot)
            .unwrap_or(LuaType::Nil);
        if matches!(mode, FieldType::Contract) {
            model.member_contract_type(source.member.as_ref(), typ)
        } else {
            typ
        }
    }

    fn field_type(&self, model: &SemanticModel, field: &Field, mode: FieldType) -> LuaType {
        let mut types: Vec<_> = field
            .sources
            .iter()
            .map(|source| self.source_type(model, source, mode))
            .collect();
        if field.optional {
            types.push(LuaType::Nil);
        }
        LuaType::from_vec(types)
    }

    pub(crate) fn members(
        &self,
        model: &SemanticModel,
        syntax: Option<LuaSyntaxId>,
        filter: Option<&LuaMemberKey>,
    ) -> Vec<MemberInfo> {
        self.member_sources(model, self.shape_at(syntax), filter)
            .into_iter()
            .map(|entry| entry.into_info(self, model))
            .collect()
    }

    fn member_sources<'a>(
        &'a self,
        model: &SemanticModel,
        shape: &'a Shape,
        filter: Option<&LuaMemberKey>,
    ) -> Vec<Entry<'a>> {
        let mut entries = Vec::new();
        for (key, field) in &shape.fields {
            if filter.is_none_or(|filter| filter == key)
                && let Some(domain) = key.to_index_type()
            {
                entries.push(Entry {
                    key: key.clone(),
                    domain,
                    value: EntryValue::Field(field),
                });
            }
        }
        let source_model = model.model_for(self.file_id);
        for index in &shape.indexes {
            let Ok(domain) = key_type(&source_model, index.key.clone()) else {
                continue;
            };
            if index.previous.iter().any(|previous| {
                key_type(&source_model, previous.clone())
                    .is_ok_and(|previous| is_compatible(model, &domain, &previous))
            }) {
                continue;
            }
            if let Some(Entry {
                value: EntryValue::Index(sources),
                ..
            }) = entries
                .iter_mut()
                .find(|entry| matches!(entry.value, EntryValue::Index(_)) && entry.domain == domain)
            {
                sources.push(&index.source);
            } else {
                entries.push(Entry {
                    key: LuaMemberKey::None,
                    domain,
                    value: EntryValue::Index(vec![&index.source]),
                });
            }
        }
        // Only the final unkeyed field can supply entries beyond the syntax plan.
        if filter.is_none_or(|key| matches!(key, LuaMemberKey::Integer(_)))
            && self
                .table
                .get_fields_with_keys()
                .last()
                .is_some_and(|(_, key)| matches!(key, LuaIndexKey::Idx(_)))
        {
            let constructor = TableConstructor::new(&source_model, &self.table);
            if let Some(Ok(expanded)) = constructor.field_entries().last() {
                for (domain, value) in expanded {
                    let key = member_key(&domain).unwrap_or(LuaMemberKey::None);
                    if shape.fields.contains_key(&key)
                        || (!matches!(key, LuaMemberKey::None)
                            && filter.is_some_and(|filter| filter != &key))
                    {
                        continue;
                    }
                    let typ = if matches!(key, LuaMemberKey::None) {
                        value.typ
                    } else {
                        model.member_contract_type(None, value.typ)
                    };
                    entries.push(Entry {
                        key,
                        domain,
                        value: EntryValue::Expanded(typ),
                    });
                }
            }
        }
        entries
    }

    pub(crate) fn entries(
        &self,
        model: &SemanticModel,
        syntax: Option<LuaSyntaxId>,
    ) -> Vec<(LuaType, LuaType)> {
        self.member_sources(model, self.shape_at(syntax), None)
            .into_iter()
            .map(|entry| {
                let typ = entry.value_type(self, model, FieldType::Contract);
                (entry.domain, typ)
            })
            .collect()
    }
}

/// The same key-domain rules govern reads, writes, and field diagnostics.
pub(crate) struct Access<'a, 'db> {
    model: &'a SemanticModel<'db>,
    construction: Rc<Construction>,
    syntax: Option<LuaSyntaxId>,
    key: Option<LuaType>,
}

impl Access<'_, '_> {
    fn shape(&self) -> &Shape {
        self.construction.shape_at(self.syntax)
    }

    fn field(&self) -> Option<&Field> {
        self.shape().fields.get(&member_key(self.key.as_ref()?)?)
    }

    fn constructor_value(&self) -> Option<LuaType> {
        let key = member_key(self.key.as_ref()?)?;
        let construction = &self.construction;
        TableConstructor::new(
            &self.model.model_for(construction.file_id),
            &construction.table,
        )
        .member_value(&key)
        .ok()
        .flatten()
        .map(|value| value.typ)
    }

    fn read_mode(&self) -> FieldType {
        // Deferred function bodies read the sealed contracts.
        if self.construction.decl.is_some()
            && self
                .syntax
                .is_none_or(|syntax| !self.construction.snapshots.contains_key(&syntax))
        {
            FieldType::Contract
        } else {
            FieldType::Read
        }
    }

    fn selects(&self, entry: &Entry<'_>) -> bool {
        self.key
            .as_ref()
            .is_some_and(|key| key_domains_overlap(self.model, key, &entry.domain))
    }

    pub(crate) fn read(&self) -> LuaType {
        if let Some(field) = self.field() {
            return self
                .construction
                .field_type(self.model, field, self.read_mode());
        }
        if let Some(typ) = self.constructor_value() {
            return typ;
        }
        let Some(key) = &self.key else {
            return LuaType::Unknown;
        };
        if key.is_unknown() {
            return LuaType::Unknown;
        }
        let mut types = Vec::new();
        let mut present_keys = Vec::new();
        for entry in self
            .construction
            .member_sources(self.model, self.shape(), None)
            .into_iter()
            .filter(|entry| self.selects(entry))
        {
            let typ = entry.value_type(&self.construction, self.model, self.read_mode());
            if !matches!(entry.key, LuaMemberKey::None) && !typ.is_nullable() {
                present_keys.push(entry.domain.clone());
            }
            types.push(typ);
        }
        let covered = !present_keys.is_empty()
            && expand_alias_branches(self.model, key).is_some_and(|key| {
                !matches!(key, LuaType::Any | LuaType::Unknown)
                    && check_type_subtype(self.model, &key, &LuaType::from_vec(present_keys))
                        .is_ok()
            });
        // Required named fields prove presence. Index signatures only describe values.
        if !covered {
            types.push(LuaType::Nil);
        }
        LuaType::from_vec(types)
    }

    pub(crate) fn has_exact_key(&self) -> bool {
        self.key.as_ref().and_then(member_key).is_some()
    }

    pub(crate) fn member_id(&self) -> Option<SemanticId> {
        self.field()?.sources.first()?.member.clone()
    }

    pub(crate) fn has_field(&self) -> bool {
        self.field().is_some()
            || self.key.as_ref().is_none_or(|key| key.is_unknown())
            || self.construction.member_sources(self.model, self.shape(), None)
                .iter().any(|entry| self.selects(entry))
            // A final expanded return can supply an explicit nil slot.
            || (matches!(self.key.as_ref().and_then(member_key), Some(LuaMemberKey::Integer(_)))
                && self.constructor_value().is_some())
    }

    /// None rejects the write. Accepted writes resolve contracts only when consumed.
    pub(crate) fn write_targets(&self) -> Option<impl Iterator<Item = LuaType> + '_> {
        let entries = self
            .construction
            .member_sources(self.model, self.shape(), None);
        // Overlap selects contracts; coverage decides whether a sealed write is allowed.
        if self.shape().sealed
            && self.key.as_ref().is_some_and(|key| {
                let domains = entries
                    .iter()
                    .map(|entry| entry.domain.clone())
                    .collect::<Vec<_>>();
                domains.is_empty() || !is_compatible(self.model, key, &LuaType::from_vec(domains))
            })
        {
            return None;
        }
        Some(
            entries
                .into_iter()
                .filter(|entry| self.selects(entry))
                .map(|entry| entry.value_type(&self.construction, self.model, FieldType::Contract)),
        )
    }
}

pub(crate) fn of(model: &SemanticModel, table: &InFiled<TextRange>) -> Option<Rc<Construction>> {
    let key = table.clone();
    if let Some(result) = model.cache.borrow().table_construction.get(&key) {
        return result.clone();
    }
    let model = model.model_for(table.file_id);
    let result = build(&model, table.value).map(Rc::new);
    model
        .cache
        .borrow_mut()
        .table_construction
        .insert(key, result.clone());
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
        model.type_of_decl_assign_target_at(&decl, name.get_position())
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
    let construction = of(model, table)?;
    Some(Access {
        syntax: (model.file_id() == construction.file_id).then_some(syntax),
        key: key.map(super::table_constructor::normalize_key_type),
        model,
        construction,
    })
}

fn build(model: &SemanticModel, range: TextRange) -> Option<Construction> {
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
    // A nested constructor within an inferred local keeps its own initial
    // structure. It does not start another construction phase through a field.
    if decl.is_none() {
        let parent = table
            .syntax()
            .parent()?
            .ancestors()
            .find_map(LuaTableExpr::cast)?;
        of(model, &InFiled::new(model.file_id(), parent.get_range()))?;
    }
    let sources: HashMap<_, _> = facts
        .members
        .iter()
        .filter_map(|member| Some((member.value_syntax?, member.id.clone())))
        .collect();
    let reassigned = tree
        .get_red_root()
        .descendants()
        .filter_map(LuaStat::cast)
        .flat_map(|stat| match stat {
            LuaStat::AssignStat(stat) => stat.get_var_and_expr_list().0,
            LuaStat::FuncStat(stat) => stat.get_func_name().into_iter().collect(),
            _ => Vec::new(),
        })
        .filter_map(|var| match var {
            LuaVarExpr::NameExpr(name) => model.resolve_name(name.get_position()),
            _ => None,
        })
        .collect();
    let casted = tree
        .get_red_root()
        .descendants()
        .filter_map(LuaDocTagCast::cast)
        .filter_map(|cast| match cast.get_key_expr()? {
            LuaExpr::NameExpr(name) => model.resolve_name(name.get_position()),
            _ => None,
        })
        .collect();
    let mut shape = Shape::default();
    for (field, key) in table.get_fields_with_keys() {
        if let Some(value) = field.get_value_expr() {
            shape.write(
                key,
                Source {
                    value: value.get_syntax_id(),
                    slot: 0,
                    member: sources.get(&value.get_syntax_id()).cloned(),
                },
            );
        }
    }
    let mut construction = Construction {
        table,
        file_id: model.file_id(),
        decl: decl.map(|decl| decl.id.clone()),
        snapshots: HashMap::new(),
        final_shape: shape.clone(),
        deferred_shape: shape.clone(),
        deferred_bodies: Vec::new(),
    };
    construction.final_shape.sealed = true;
    // Only a direct local initializer can grow fields. A nested constructor
    // already has its stable constructor shape.
    let Some(_) = decl else {
        return Some(construction);
    };
    let stat = construction
        .table
        .ancestors::<emmylua_parser::LuaLocalStat>()
        .next()?;
    if stat.descendants::<LuaDocTagAs>().any(|tag| {
        tag.get_parent::<LuaComment>()
            .and_then(|comment| comment.get_owner())
            .and_then(|owner| {
                LuaExpr::cast(owner.syntax().clone()).or_else(|| owner.children::<LuaExpr>().last())
            })
            .and_then(|expr| table_expr(&expr))
            .is_some_and(|table| table == construction.table)
    }) {
        return None;
    }
    let block = stat.ancestors::<LuaBlock>().next()?;
    let mut planner = Planner {
        model,
        construction,
        sources,
        reassigned,
        casted,
        exits: Vec::new(),
        seals: Vec::new(),
        returns: Vec::new(),
        breaks: Vec::new(),
        continues: Vec::new(),
    };
    let statements = block
        .children::<LuaStat>()
        .filter(|node| node.get_position() > stat.get_position())
        .collect();
    if let Some(shape) = planner.statements(statements, shape) {
        planner.exits.push(shape);
    }
    // Deferred bodies can run at any sealing point. A caller receives only the
    // shapes on paths that return this table, not unrelated aliases or uses.
    let sealed_shape = Shape::join(planner.seals)
        .or_else(|| Shape::join(planner.exits))
        .unwrap_or_default();
    planner.construction.deferred_shape = sealed_shape.clone();
    planner.construction.deferred_shape.sealed = true;
    planner.construction.deferred_bodies = block
        .descendants::<LuaClosureExpr>()
        .map(|closure| closure.get_range())
        .collect();
    planner.construction.final_shape = Shape::join(planner.returns).unwrap_or(sealed_shape);
    planner.construction.final_shape.sealed = true;
    Some(planner.construction)
}

struct Planner<'a, 'db> {
    model: &'a SemanticModel<'db>,
    construction: Construction,
    sources: HashMap<LuaSyntaxId, SemanticId>,
    reassigned: HashSet<SemanticId>,
    casted: HashSet<SemanticId>,
    exits: Vec<Shape>,
    seals: Vec<Shape>,
    returns: Vec<Shape>,
    breaks: Vec<Shape>,
    continues: Vec<Shape>,
}

impl Planner<'_, '_> {
    fn seal(&mut self, shape: &mut Shape) {
        if !shape.sealed {
            shape.sealed = true;
            self.seals.push(shape.clone());
        }
    }

    fn source(&self, value: &LuaExpr) -> Source {
        Source {
            value: value.get_syntax_id(),
            slot: 0,
            member: self.sources.get(&value.get_syntax_id()).cloned(),
        }
    }

    fn is_function(&self, source: &Source) -> bool {
        if self
            .model
            .syntax_tree()
            .and_then(|tree| source.value.to_node_from_root(&tree.get_red_root()))
            .and_then(LuaClosureExpr::cast)
            .is_some()
        {
            return true;
        }
        // A copied or annotated function has no closure at the write. The shell
        // query identifies its callable category without resolving bodies through
        // this construction plan.
        source
            .member
            .as_ref()
            .and_then(|member| {
                self.model
                    .analysis()
                    .member_type(self.model.file_id(), member.clone())
            })
            .is_some_and(|shell| {
                self.model
                    .analysis()
                    .type_shell_heads(self.model.file_id(), &shell)
                    .iter()
                    .any(|head| {
                        matches!(
                            head.candidate,
                            TypeCandidate::Function(_)
                                | TypeCandidate::Primitive(PrimitiveType::Function)
                        )
                    })
            })
    }

    fn is_owner(&self, expr: &LuaExpr) -> bool {
        match expr {
            LuaExpr::ParenExpr(paren) => paren.get_expr().is_some_and(|expr| self.is_owner(&expr)),
            LuaExpr::NameExpr(name) => {
                self.model.resolve_name(name.get_position()).as_ref()
                    == self.construction.decl.as_ref()
            }
            _ => false,
        }
    }

    /// Use literal initializer chains and declared primitive key domains only.
    /// Full expression inference could re-enter this unfinished construction.
    fn read_key(&self, key: &LuaIndexKey) -> Option<LuaType> {
        let LuaIndexKey::Expr(mut expr) = key.clone() else {
            return key_type(self.model, key.clone()).ok();
        };
        let tree = self.model.syntax_tree()?;
        let facts = self.model.file_facts()?;
        let mut visited = HashSet::new();
        let mut declared = None;
        loop {
            expr = match expr {
                LuaExpr::ParenExpr(paren) => paren.get_expr()?,
                LuaExpr::NameExpr(name) => {
                    let id = self.model.resolve_name(name.get_position())?;
                    if self.casted.contains(&id) || !visited.insert(id.clone()) {
                        return None;
                    }
                    let decl = facts.decl_by_id(&id)?;
                    if declared.is_none()
                        && (decl.doc_type_syntax.is_some() || matches!(decl.kind, DeclKind::Param))
                    {
                        declared = self
                            .model
                            .analysis()
                            .decl_type(self.model.file_id(), id.clone())
                            .and_then(|shell| shallow_key_type(self.model, &shell));
                    }
                    if self.reassigned.contains(&id)
                        || !matches!(decl.kind, DeclKind::Local { is_iter: false, .. })
                        || decl.multi_return_index.is_some_and(|slot| slot != 0)
                    {
                        return declared;
                    }
                    let Some(initializer) = decl.value_expr_syntax else {
                        return declared;
                    };
                    LuaExpr::cast(initializer.to_node_from_root(&tree.get_red_root())?)?
                }
                LuaExpr::LiteralExpr(literal) => {
                    return match literal.get_literal()? {
                        LuaLiteralToken::String(value) => Some(LuaType::StringConst(
                            smol_str::SmolStr::new(value.get_value()).into(),
                        )),
                        LuaLiteralToken::Number(value) => {
                            key_type(self.model, LuaIndexKey::Integer(value)).ok()
                        }
                        _ => declared,
                    };
                }
                _ => return declared,
            };
        }
    }

    fn never_returns(&self, call: &LuaCallExpr) -> bool {
        let Some(mut prefix) = call.get_prefix_expr() else {
            return false;
        };
        while let LuaExpr::ParenExpr(paren) = prefix {
            let Some(inner) = paren.get_expr() else {
                return false;
            };
            prefix = inner;
        }
        // Shells have no assignment flow. Only a stable named callee can prove
        // that this statement cannot reach a later construction use.
        let LuaExpr::NameExpr(name) = prefix else {
            return false;
        };
        let Some(id) = self.model.resolve_name(name.get_position()) else {
            return false;
        };
        if self.reassigned.contains(&id) || self.casted.contains(&id) {
            return false;
        }
        let Some(shell) = self
            .model
            .analysis()
            .decl_type(self.model.file_id(), id.clone())
        else {
            return false;
        };
        let never = |heads: Vec<TypeShellHead>| {
            !heads.is_empty()
                && heads.iter().all(
                    |head| matches!(&head.candidate, TypeCandidate::Named(name) if name == "never"),
                )
        };
        let heads = self
            .model
            .analysis()
            .type_shell_heads(self.model.file_id(), &shell);
        match heads.as_slice() {
            [
                TypeShellHead {
                    candidate: TypeCandidate::Primitive(PrimitiveType::Function),
                    ..
                },
            ] => {
                // An exact closure has one signature. Read its shallow result
                // slots directly, without selecting a call-query candidate.
                let Some(tree) = self.model.syntax_tree() else {
                    return false;
                };
                let closure = self
                    .model
                    .file_facts()
                    .and_then(|facts| facts.decl_by_id(&id))
                    .and_then(|decl| decl.value_expr_syntax)
                    .and_then(|syntax| syntax.to_node_from_root(&tree.get_red_root()))
                    .and_then(LuaClosureExpr::cast);
                closure
                    .and_then(|closure| {
                        self.model
                            .analysis()
                            .signature_returns(self.model.file_id(), closure.get_syntax_id())
                    })
                    .and_then(|slots| slots.into_iter().next())
                    .is_some_and(|slot| {
                        never(
                            self.model
                                .analysis()
                                .type_shell_heads(self.model.file_id(), &slot),
                        )
                    })
            }
            candidates => {
                !candidates.is_empty()
                    && candidates
                        .iter()
                        .all(|head| never(self.model.analysis().type_shell_return_heads(head)))
            }
        }
    }

    fn snapshot(&mut self, syntax: LuaSyntaxId, shape: &Shape) {
        self.construction.snapshots.insert(syntax, shape.clone());
    }

    fn snapshot_index(&mut self, index: &LuaIndexExpr, shape: &Shape) {
        let snapshot = if let Some(key) = index.get_index_key().as_ref().and_then(literal_key) {
            Shape {
                fields: shape
                    .fields
                    .get(&key)
                    .map(|field| (key, field.clone()))
                    .into_iter()
                    .collect(),
                indexes: shape.indexes.clone(),
                sealed: shape.sealed,
            }
        } else {
            shape.clone()
        };
        self.construction
            .snapshots
            .insert(index.get_syntax_id(), snapshot);
    }

    fn expressions(&mut self, expressions: impl IntoIterator<Item = LuaExpr>, shape: &mut Shape) {
        for expr in expressions {
            self.expression(expr, shape, false);
        }
    }

    fn expression(&mut self, expr: LuaExpr, shape: &mut Shape, own_function: bool) {
        let mut pending = vec![(expr, own_function)];
        while let Some((expr, exempt)) = pending.pop() {
            match &expr {
                LuaExpr::ClosureExpr(closure) => {
                    if !exempt
                        && closure
                            .descendants::<LuaNameExpr>()
                            .any(|name| self.is_owner(&LuaExpr::NameExpr(name)))
                    {
                        self.seal(shape);
                    }
                }
                LuaExpr::IndexExpr(index)
                    if index
                        .get_prefix_expr()
                        .is_some_and(|expr| self.is_owner(&expr)) =>
                {
                    self.snapshot_index(index, shape);
                    if let Some(key) = index.get_index_key() {
                        let read_key = self.read_key(&key);
                        let selects =
                            |domain: Option<LuaType>| {
                                read_key.as_ref().zip(domain.as_ref()).is_none_or(
                                    |(key, domain)| key_domains_overlap(self.model, key, domain),
                                )
                            };
                        // An established field has its own contract. Dynamic
                        // writes must respect it, including when it is data.
                        let function_index = read_key
                            .as_ref()
                            .and_then(member_key)
                            .is_none_or(|key| !shape.fields.contains_key(&key))
                            && shape.indexes.iter().any(|index| {
                                self.is_function(&index.source)
                                    && selects(self.read_key(&index.key))
                            });
                        let reads_function = shape.fields.iter().any(|(key, field)| {
                            selects(key.to_index_type())
                                && field.sources.iter().any(|source| self.is_function(source))
                        }) || function_index;
                        if reads_function {
                            self.seal(shape);
                        }
                        if let LuaIndexKey::Expr(key) = key {
                            pending.push((key, false));
                        }
                    }
                }
                LuaExpr::NameExpr(_) if self.is_owner(&expr) => {
                    self.seal(shape);
                    self.snapshot(expr.get_syntax_id(), shape);
                }
                _ => pending.extend(
                    expression_children(&expr)
                        .into_iter()
                        .map(|expr| (expr, false))
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev(),
                ),
            }
        }
    }

    fn write(&mut self, index: LuaIndexExpr, value: Option<(LuaExpr, usize)>, shape: &mut Shape) {
        let direct = index
            .get_prefix_expr()
            .is_some_and(|expr| self.is_owner(&expr));
        if !direct {
            if index
                .descendants::<LuaNameExpr>()
                .any(|name| self.is_owner(&LuaExpr::NameExpr(name)))
            {
                self.seal(shape);
            }
            self.snapshot_index(&index, shape);
            return;
        }
        if let Some(LuaIndexKey::Expr(key)) = index.get_index_key() {
            self.expression(key, shape, false);
        }
        self.snapshot_index(&index, shape);
        if let (Some(key), Some((value, slot))) = (index.get_index_key(), value) {
            let mut source = self.source(&value);
            source.slot = slot;
            let member = SemanticId::member(self.model.file_id(), index.get_range());
            if self
                .model
                .file_facts()
                .is_some_and(|facts| facts.member_by_id(&member).is_some())
            {
                source.member = Some(member);
            }
            shape.write(key, source);
        }
    }

    fn statements(&mut self, statements: Vec<LuaStat>, mut shape: Shape) -> Option<Shape> {
        for stat in statements {
            // Named casts preceding a statement end construction before the cast.
            for cast in stat
                .syntax()
                .prev_sibling()
                .into_iter()
                .flat_map(|node| node.descendants().filter_map(LuaDocTagCast::cast))
            {
                if cast.get_key_expr().is_some_and(|expr| self.is_owner(&expr)) {
                    self.seal(&mut shape);
                }
            }
            match stat {
                LuaStat::IfStat(stat) => {
                    self.expressions(stat.get_condition_expr(), &mut shape);
                    let mut paths = Vec::new();
                    if let Some(block) = stat.get_block()
                        && let Some(path) = self.block(block, shape.clone())
                    {
                        paths.push(path);
                    }
                    for clause in stat.get_else_if_clause_list() {
                        self.expressions(clause.get_condition_expr(), &mut shape);
                        if let Some(block) = clause.get_block()
                            && let Some(path) = self.block(block, shape.clone())
                        {
                            paths.push(path);
                        }
                    }
                    if let Some(block) =
                        stat.get_else_clause().and_then(|clause| clause.get_block())
                    {
                        if let Some(path) = self.block(block, shape) {
                            paths.push(path);
                        }
                    } else {
                        paths.push(shape);
                    }
                    shape = Shape::join(paths)?;
                }
                LuaStat::DoStat(stat) => {
                    if let Some(block) = stat.get_block() {
                        shape = self.block(block, shape)?;
                    }
                }
                LuaStat::WhileStat(stat) => {
                    let condition = stat.get_condition_expr();
                    self.expressions(condition.clone(), &mut shape);
                    match condition.and_then(literal_truthiness) {
                        Some(false) => {}
                        Some(true) => {
                            shape = self.loop_block(stat.get_block(), shape, true, false)?;
                        }
                        None => {
                            shape = self.loop_block(stat.get_block(), shape, false, true)?;
                        }
                    }
                }
                LuaStat::ForStat(stat) => {
                    self.expressions(stat.get_iter_expr(), &mut shape);
                    shape = self.loop_block(stat.get_block(), shape, false, true)?;
                }
                LuaStat::ForRangeStat(stat) => {
                    self.expressions(stat.get_expr_list(), &mut shape);
                    shape = self.loop_block(stat.get_block(), shape, false, true)?;
                }
                LuaStat::RepeatStat(stat) => {
                    shape = self.loop_block(stat.get_block(), shape, true, true)?;
                    self.expressions(stat.get_condition_expr(), &mut shape);
                }
                LuaStat::CallExprStat(stat) => {
                    self.expressions(stat.children::<LuaExpr>(), &mut shape);
                    if stat
                        .get_call_expr()
                        .is_some_and(|call| call.is_error() || self.never_returns(&call))
                    {
                        self.exits.push(shape);
                        return None;
                    }
                }
                LuaStat::AssignStat(stat) => {
                    let (vars, values) = stat.get_var_and_expr_list();
                    for (i, value) in values.iter().enumerate() {
                        let own_function = matches!(vars.get(i), Some(LuaVarExpr::IndexExpr(index)) if index.get_prefix_expr().is_some_and(|prefix| self.is_owner(&prefix)));
                        self.expression(value.clone(), &mut shape, own_function);
                    }
                    for (i, var) in vars.into_iter().enumerate() {
                        match var {
                            LuaVarExpr::IndexExpr(index) => {
                                let value =
                                    values.get(i).cloned().map(|value| (value, 0)).or_else(|| {
                                        values
                                            .last()
                                            .cloned()
                                            .map(|value| (value, i + 1 - values.len()))
                                    });
                                self.write(index, value, &mut shape)
                            }
                            LuaVarExpr::NameExpr(name) => {
                                if self.is_owner(&LuaExpr::NameExpr(name)) {
                                    self.seal(&mut shape);
                                }
                            }
                        }
                    }
                }
                LuaStat::FuncStat(stat) => {
                    if let Some(LuaVarExpr::IndexExpr(index)) = stat.get_func_name() {
                        let own = index
                            .get_prefix_expr()
                            .is_some_and(|prefix| self.is_owner(&prefix));
                        if let Some(closure) = stat.get_closure() {
                            self.expression(LuaExpr::ClosureExpr(closure.clone()), &mut shape, own);
                            self.write(index, Some((LuaExpr::ClosureExpr(closure), 0)), &mut shape);
                        }
                    }
                }
                LuaStat::ReturnStat(stat) => {
                    let expressions: Vec<_> = stat.get_expr_list().collect();
                    let returns_table = expressions.iter().any(|expr| {
                        self.is_owner(expr)
                            || super::flow::stable_access_path(self.model, expr).is_some_and(
                                |(owner, keys)| {
                                    Some(&owner) == self.construction.decl.as_ref()
                                        && keys.is_empty()
                                },
                            )
                    });
                    self.expressions(expressions, &mut shape);
                    if returns_table {
                        self.returns.push(shape.clone());
                    }
                    self.exits.push(shape);
                    return None;
                }
                LuaStat::BreakStat(_) => {
                    self.breaks.push(shape);
                    return None;
                }
                LuaStat::ContinueStat(_) => {
                    self.continues.push(shape);
                    return None;
                }
                LuaStat::GotoStat(_) | LuaStat::LabelStat(_) => {
                    self.seal(&mut shape);
                }
                _ => self.expressions(stat.children::<LuaExpr>(), &mut shape),
            }
        }
        Some(shape)
    }

    fn block(&mut self, block: LuaBlock, shape: Shape) -> Option<Shape> {
        self.statements(block.children::<LuaStat>().collect(), shape)
    }

    fn loop_block(
        &mut self,
        block: Option<LuaBlock>,
        shape: Shape,
        entered: bool,
        condition_can_exit: bool,
    ) -> Option<Shape> {
        let old_breaks = std::mem::take(&mut self.breaks);
        let old_continues = std::mem::take(&mut self.continues);
        let mut paths = Vec::new();
        if !entered {
            paths.push(shape.clone());
        }
        if let Some(block) = block {
            if let Some(shape) = self.block(block, shape) {
                if condition_can_exit {
                    paths.push(shape);
                } else {
                    self.exits.push(shape);
                }
            }
        } else if condition_can_exit {
            paths.push(shape);
        } else {
            self.exits.push(shape);
        }
        paths.append(&mut self.breaks);
        if condition_can_exit {
            paths.append(&mut self.continues);
        } else {
            self.exits.append(&mut self.continues);
        }
        self.breaks = old_breaks;
        self.continues = old_continues;
        Shape::join(paths)
    }
}

// Project primitive outer alternatives only, without resolving tables or functions.
fn shallow_key_type(model: &SemanticModel, shell: &TypeShell) -> Option<LuaType> {
    let types: Option<Vec<_>> = model
        .analysis()
        .type_shell_heads(model.file_id(), shell)
        .into_iter()
        .map(|head| {
            Some(match head.candidate {
                TypeCandidate::Primitive(PrimitiveType::String) => LuaType::String,
                TypeCandidate::Primitive(PrimitiveType::Integer) => LuaType::Integer,
                TypeCandidate::Primitive(PrimitiveType::Number) => LuaType::Number,
                TypeCandidate::Primitive(PrimitiveType::Boolean) => LuaType::Boolean,
                TypeCandidate::Primitive(PrimitiveType::Nil) => LuaType::Nil,
                TypeCandidate::Literal(LiteralShell::String(value)) => {
                    LuaType::StringConst(value.into())
                }
                TypeCandidate::Literal(LiteralShell::Integer(value)) => {
                    LuaType::IntegerConst(value)
                }
                TypeCandidate::Literal(LiteralShell::Float(bits)) => {
                    let value = f64::from_bits(bits);
                    crate::semantic_db::types::exact_integer(value)
                        .map(LuaType::IntegerConst)
                        .unwrap_or(LuaType::FloatConst(value))
                }
                TypeCandidate::Literal(LiteralShell::Boolean(value)) => {
                    LuaType::BooleanConst(value)
                }
                TypeCandidate::Literal(LiteralShell::Nil) => LuaType::Nil,
                _ => return None,
            })
        })
        .collect();
    let types = types?;
    (!types.is_empty()).then(|| LuaType::from_vec(types))
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

// Only literal conditions can prove that the loop enters or cannot exit.
fn literal_truthiness(mut expr: LuaExpr) -> Option<bool> {
    loop {
        expr = match expr {
            LuaExpr::ParenExpr(paren) => paren.get_expr()?,
            LuaExpr::LiteralExpr(literal) => {
                return match literal.get_literal()? {
                    LuaLiteralToken::Bool(value) => Some(value.is_true()),
                    LuaLiteralToken::Nil(_) => Some(false),
                    LuaLiteralToken::String(_) | LuaLiteralToken::Number(_) => Some(true),
                    _ => None,
                };
            }
            _ => return None,
        };
    }
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

// Calls and constructors contain expression lists and field wrappers between
// the expression and its operands. Stop at each operand; closures are handled
// by the caller without walking their bodies as construction statements.
fn expression_children(expr: &LuaExpr) -> Vec<LuaExpr> {
    let mut pending: Vec<_> = expr
        .syntax()
        .children()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let mut result = Vec::new();
    while let Some(node) = pending.pop() {
        if let Some(expr) = LuaExpr::cast(node.clone()) {
            result.push(expr);
        } else {
            pending.extend(node.children().collect::<Vec<_>>().into_iter().rev());
        }
    }
    result
}
