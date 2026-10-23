//! Supplied constructor values, or stable inferred fields of a table reference.
//!
//! Field flow affects field reads, not the structural type of the whole table.

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaTableExpr};
use hashbrown::HashSet;
use rowan::TextRange;

use super::SemanticModel;
use super::member::{member_infos_all, member_infos_with_key_all, table_metatable_index_info};
use super::table_constructor::{ConstructorValue, TableConstructor, member_key};
use super::type_check::TypeCheckFailReason;
use crate::{InFiled, LuaMemberKey, LuaType, SemanticId};

pub(crate) struct TableValue<'db> {
    model: SemanticModel<'db>,
    identity: InFiled<TextRange>,
    constructor: TableConstructor,
    is_constructor: bool,
    expression: Option<emmylua_parser::LuaSyntaxId>,
}

impl<'db> TableValue<'db> {
    pub(crate) fn new(
        model: &SemanticModel<'db>,
        identity: &InFiled<TextRange>,
        expression: Option<&InFiled<TextRange>>,
    ) -> Result<Self, TypeCheckFailReason> {
        let model = model.model_for(identity.file_id);
        let table = model
            .syntax_tree()
            .and_then(|tree| {
                tree.get_red_root()
                    .covering_element(identity.value)
                    .into_node()
            })
            .and_then(LuaTableExpr::cast)
            .ok_or(TypeCheckFailReason::Unresolved)?;
        // A literal inside a call contributes to the call's result.
        let is_constructor = expression.is_some_and(|expr| {
            expr.file_id == identity.file_id
                && expr.value.contains_range(identity.value)
                && !table
                    .ancestors::<LuaCallExpr>()
                    .any(|call| expr.value.contains_range(call.get_range()))
        });
        let constructor = TableConstructor::new(&model, &table);
        let expression = expression
            .filter(|expr| expr.file_id == identity.file_id)
            .and_then(|expr| {
                let element = model
                    .syntax_tree()?
                    .get_red_root()
                    .covering_element(expr.value);
                let node = match element {
                    rowan::NodeOrToken::Node(node) => node,
                    rowan::NodeOrToken::Token(token) => token.parent()?,
                };
                Some(emmylua_parser::LuaSyntaxId::from_node(&node))
            });
        Ok(Self {
            model,
            identity: identity.clone(),
            constructor,
            is_constructor,
            expression,
        })
    }

    fn own_member_value(
        &self,
        key: &LuaMemberKey,
    ) -> Result<Option<ConstructorValue<'_>>, TypeCheckFailReason> {
        if !self.is_constructor
            && let Some(construction) = super::table_construction::of(&self.model, &self.identity)
        {
            return self.model.require_complete(|| {
                construction
                    .members(&self.model, self.expression, Some(key))
                    .into_iter()
                    .find(|info| info.index_key_type.is_none())
                    .map(|info| ConstructorValue {
                        typ: info.typ,
                        expr: None,
                    })
            });
        }
        let initial = self.constructor.member_value(key)?;
        if self.is_constructor {
            return Ok(initial);
        }
        let has_initial = initial.as_ref().is_some_and(|value| !value.typ.is_nil());
        let owner = SemanticId::member(self.identity.file_id, self.identity.value);
        // Keep the constructor field's contract when it supplies a value.
        // Otherwise, use the assignment that adds the field to this table.
        let member = self.model.require_complete(|| {
            member_infos_with_key_all(
                &self.model,
                &LuaType::TableConst(self.identity.clone()),
                key,
            )
            .into_iter()
            .find(|info| {
                info.id.as_ref().is_some_and(|id| {
                    self.model
                        .file_facts_of(self.identity.file_id)
                        .and_then(|facts| facts.member_by_id(id))
                        .is_some_and(|member| {
                            if has_initial {
                                member.owner == owner
                            } else {
                                member.owner != owner
                            }
                        })
                })
            })
        })?;
        let value = if has_initial {
            initial
        } else {
            member
                .as_ref()
                .map(|info| ConstructorValue {
                    typ: info.typ.clone(),
                    expr: None,
                })
                .or(initial)
        };
        Ok(value.map(|value| ConstructorValue {
            typ: self
                .model
                .member_contract_type(member.as_ref().and_then(|info| info.id.as_ref()), value.typ),
            expr: None,
        }))
    }

    pub(crate) fn member_value(
        &self,
        key: &LuaMemberKey,
    ) -> Result<Option<ConstructorValue<'_>>, TypeCheckFailReason> {
        let actual = self.own_member_value(key)?;
        if self.is_constructor
            || actual
                .as_ref()
                .is_some_and(|value| !value.typ.is_nullable())
        {
            return Ok(actual);
        }
        // __index supplies reads of absent fields; it does not add dictionary entries.
        let mut types = Vec::new();
        if let Some(value) = &actual {
            push_present_types(&mut types, &value.typ);
        }
        let mut identity = self.identity.clone();
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(identity.clone()) {
                return Err(TypeCheckFailReason::Unresolved);
            }
            let model = self.model.model_for(identity.file_id);
            let index = model.require_complete(|| table_metatable_index_info(&model, &identity))?;
            match index.map(|info| info.typ) {
                Some(LuaType::TableConst(table)) => {
                    let prototype = Self::new(&self.model, &table, None)?;
                    let value = prototype.own_member_value(key)?;
                    if let Some(value) = value {
                        push_present_types(&mut types, &value.typ);
                        if !value.typ.is_nullable() {
                            break;
                        }
                    }
                    identity = table;
                }
                Some(LuaType::DocFunction(function)) => {
                    types.push(
                        function
                            .get_ret()
                            .get_result_slot_type(0)
                            .unwrap_or(LuaType::Nil),
                    );
                    break;
                }
                Some(index) => {
                    types.push(
                        model
                            .require_complete(|| model.member_type(&index, key))?
                            .unwrap_or(LuaType::Nil),
                    );
                    break;
                }
                None => {
                    if types.is_empty() {
                        return Ok(actual);
                    }
                    types.push(LuaType::Nil);
                    break;
                }
            }
        }
        Ok(Some(ConstructorValue {
            typ: LuaType::from_vec(types),
            expr: None,
        }))
    }

    pub(crate) fn field_entries(
        &self,
    ) -> impl Iterator<Item = Result<(LuaType, ConstructorValue<'_>), TypeCheckFailReason>> {
        if !self.is_constructor
            && let Some(construction) = super::table_construction::of(&self.model, &self.identity)
        {
            let entries = self
                .model
                .require_complete(|| construction.entries(&self.model, self.expression));
            return match entries {
                Ok(entries) => entries
                    .into_iter()
                    .map(|(key, typ)| Ok((key, ConstructorValue { typ, expr: None })))
                    .collect::<Vec<_>>(),
                Err(err) => vec![Err(err)],
            }
            .into_iter();
        }
        let entries = self
            .constructor
            .field_entries()
            .flat_map(|entries| match entries {
                Ok(entries) => entries.into_iter().map(Ok).collect(),
                Err(err) => vec![Err(err)],
            });
        if self.is_constructor {
            return entries.collect::<Vec<_>>().into_iter();
        }
        let mut keys = HashSet::new();
        let mut values = Vec::new();
        for entry in entries {
            match entry {
                Ok((key, value)) => {
                    if let Some(member) = member_key(&key) {
                        keys.insert(member);
                    } else {
                        values.push(Ok((key, value)));
                    }
                }
                Err(err) => values.push(Err(err)),
            }
        }
        match self.model.require_complete(|| {
            member_infos_all(&self.model, &LuaType::TableConst(self.identity.clone()))
                .into_iter()
                .map(|info| info.key)
        }) {
            Ok(members) => keys.extend(members),
            Err(err) => values.push(Err(err)),
        }
        for key in keys {
            match self.own_member_value(&key) {
                Ok(Some(value)) if value.typ != LuaType::Nil => {
                    if let Some(key) = key.to_index_type() {
                        values.push(Ok((key, value)));
                    }
                }
                Ok(_) => {}
                Err(err) => values.push(Err(err)),
            }
        }
        values.into_iter()
    }
}

fn push_present_types(types: &mut Vec<LuaType>, typ: &LuaType) {
    match typ {
        LuaType::Union(union) => types.extend(union.iter().filter(|ty| !ty.is_nil()).cloned()),
        LuaType::Nil => {}
        typ => types.push(typ.clone()),
    }
}
