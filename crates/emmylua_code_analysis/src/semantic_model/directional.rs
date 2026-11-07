//! Separate read/write contracts for transformed properties.
use super::{
    SemanticModel,
    member::{self, MemberInfo},
    type_eval::with_expanded_alias,
};
use crate::{LuaMemberKey, LuaType, MemberAccess, SemanticId, TypeDefKind};
use emmylua_parser::{LuaAstNode, LuaIndexExpr, LuaIndexKey};

impl SemanticModel<'_> {
    /// Return the storage type checked by assignment.
    /// Expression-local assertions do not redefine a directional setter contract.
    pub fn assignment_target_type(&self, expr: &emmylua_parser::LuaExpr) -> LuaType {
        if let emmylua_parser::LuaExpr::IndexExpr(index) = expr
            && let Some((_, write)) = self.directional_index(index)
        {
            return write.typ;
        }
        self.type_of_expr(expr.get_syntax_id())
    }

    pub fn directional_declaration(&self, id: &SemanticId) -> Option<(MemberInfo, MemberInfo)> {
        let SemanticId::Member(key) = id else {
            return None;
        };
        let facts = self.file_facts_of(key.file_id)?;
        let member = facts.member_by_id(id)?;
        let def = facts.type_def_by_id(&member.owner)?;
        self.directional_member(&self.type_def_ref(def), &member.key)
    }

    /// Return the read type and the union of setter overloads, with generic substitutions.
    /// The write identity remains the first setter declaration for navigation.
    pub fn directional_member(
        &self,
        prefix: &LuaType,
        key: &LuaMemberKey,
    ) -> Option<(MemberInfo, MemberInfo)> {
        let (read, writes) = self.directional_member_views(prefix, key)?;
        let mut write = writes.first()?.clone();
        // A setter overload accepts a value matching any declared write contract.
        // Keep the first declaration identity for navigation, but expose every type.
        write.typ = LuaType::from_vec(writes.into_iter().map(|info| info.typ).collect());
        Some((read, write))
    }

    /// Preserve every setter declaration for contextual callback signature selection.
    pub(super) fn directional_member_views(
        &self,
        prefix: &LuaType,
        key: &LuaMemberKey,
    ) -> Option<(MemberInfo, Vec<MemberInfo>)> {
        let read = self.member_info(prefix, key)?;
        let (writes, directional) = self.member_write_views(prefix, key, &read)?;
        // Leave wholly ordinary unions on the existing inference/flow path.
        directional.then_some((read, writes))
    }

    /// Collect writer types and whether any component declares a direction.
    /// A failed projection stays unresolved rather than falling back to its getter.
    fn member_write_views(
        &self,
        prefix: &LuaType,
        key: &LuaMemberKey,
        read: &MemberInfo,
    ) -> Option<(Vec<MemberInfo>, bool)> {
        let type_id = match prefix {
            LuaType::Ref(id) | LuaType::Def(id) => Some(id.clone()),
            LuaType::Generic(generic) => Some(generic.get_base_type_id()),
            _ => None,
        };
        if type_id
            .and_then(|id| self.type_def_of(&id))
            .is_some_and(|def| def.kind == TypeDefKind::Alias)
        {
            return with_expanded_alias(self, prefix, |expanded| {
                self.member_write_views(expanded, key, read)
            })
            .flatten();
        }
        if let LuaType::Union(union) = prefix {
            let mut writes = Vec::new();
            let mut directional = false;
            for component in union.iter() {
                // Missing members contribute nothing, as in ordinary union lookup.
                let Some(component_read) = self.member_info(&component, key) else {
                    continue;
                };
                let (component_writes, component_directional) =
                    self.member_write_views(&component, key, &component_read)?;
                directional |= component_directional;
                writes.extend(component_writes);
            }
            return Some((writes, directional));
        }
        let Some(SemanticId::Member(id)) = read.id.as_ref() else {
            return Some((vec![read.clone()], false));
        };
        let facts = self.file_facts_of(id.file_id)?;
        let field = facts.member_by_id(read.id.as_ref()?)?;
        if field.access == MemberAccess::ReadWrite {
            return Some((vec![read.clone()], false));
        }
        let mut writers = facts
            .members_with_access(field, MemberAccess::Write)
            .collect::<Vec<_>>();
        if writers.is_empty() {
            writers.push(field);
        }
        let writes = writers
            .into_iter()
            .map(|writer| member::member_info_for_declaration(self, prefix, key, &writer.id))
            .collect::<Option<Vec<_>>>()?;
        Some((writes, true))
    }

    pub fn directional_index(&self, index: &LuaIndexExpr) -> Option<(MemberInfo, MemberInfo)> {
        let prefix = self.type_of_expr(index.get_prefix_expr()?.get_syntax_id());
        let key = match index.get_index_key()? {
            LuaIndexKey::Name(n) => LuaMemberKey::Name(n.get_name_text().into()),
            LuaIndexKey::String(s) => LuaMemberKey::Name(s.get_value().into()),
            LuaIndexKey::Integer(n) => match n.get_number_value() {
                emmylua_parser::NumberResult::Int(n) => LuaMemberKey::Integer(n),
                _ => return None,
            },
            _ => return None,
        };
        self.directional_member(&prefix, &key)
    }
}
