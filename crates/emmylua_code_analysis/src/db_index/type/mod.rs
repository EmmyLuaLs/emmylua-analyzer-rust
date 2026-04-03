mod basic_union;
mod generic_param;
mod humanize_type;
mod test;
mod type_decl;
mod type_ops;
mod type_owner;
mod type_visit_trait;
mod types;

use super::traits::LuaIndex;
use crate::{DbIndex, FileId, InFiled, db_index::WorkspaceId};
pub use basic_union::{BasicTypeKind, BasicTypeUnion};
pub use generic_param::GenericParam;
use hashbrown::{HashMap, HashSet};
pub use humanize_type::{RenderLevel, TypeHumanizer, format_union_type, humanize_type};
pub use type_decl::{
    LuaDeclLocation, LuaDeclTypeKind, LuaTypeDecl, LuaTypeDeclId, LuaTypeFlag, LuaTypeIdentifier,
};
pub use type_ops::TypeOps;
pub(crate) use type_ops::union_type_shallow;
pub use type_owner::{LuaTypeCache, LuaTypeOwner};
pub use type_visit_trait::TypeVisitTrait;
pub use types::*;

#[derive(Debug)]
pub struct LuaTypeIndex {
    file_namespace: HashMap<FileId, String>,
    file_using_namespace: HashMap<FileId, Vec<String>>,
    file_types: HashMap<FileId, Vec<LuaTypeDeclId>>,
    full_name_type_map: HashMap<LuaTypeDeclId, LuaTypeDecl>,
    generic_params: HashMap<LuaTypeDeclId, Vec<GenericParam>>,
    supers: HashMap<LuaTypeDeclId, Vec<InFiled<LuaType>>>,
    types: HashMap<LuaTypeOwner, LuaTypeCache>,
    in_filed_type_owner: HashMap<FileId, HashSet<LuaTypeOwner>>,
    // type name index
    global_name_type_map: HashMap<String, LuaTypeDeclId>,
    internal_name_type_map: HashMap<WorkspaceId, HashMap<String, LuaTypeDeclId>>,
    local_name_type_map: HashMap<FileId, HashMap<String, LuaTypeDeclId>>,
}

impl Default for LuaTypeIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaTypeIndex {
    pub fn new() -> Self {
        Self {
            file_namespace: HashMap::new(),
            file_using_namespace: HashMap::new(),
            file_types: HashMap::new(),
            full_name_type_map: HashMap::new(),
            generic_params: HashMap::new(),
            supers: HashMap::new(),
            types: HashMap::new(),
            in_filed_type_owner: HashMap::new(),
            global_name_type_map: HashMap::new(),
            internal_name_type_map: HashMap::new(),
            local_name_type_map: HashMap::new(),
        }
    }

    pub fn add_file_namespace(&mut self, file_id: FileId, namespace: String) {
        self.file_namespace.insert(file_id, namespace);
    }

    pub fn get_file_namespace(&self, file_id: &FileId) -> Option<&String> {
        self.file_namespace.get(file_id)
    }

    pub fn add_file_using_namespace(&mut self, file_id: FileId, namespace: String) {
        self.file_using_namespace
            .entry(file_id)
            .or_default()
            .push(namespace);
    }

    pub fn get_file_using_namespace(&self, file_id: &FileId) -> Option<&Vec<String>> {
        self.file_using_namespace.get(file_id)
    }

    /// return previous FileId if exist
    pub fn add_type_decl(&mut self, file_id: FileId, type_decl: LuaTypeDecl) {
        let id = type_decl.get_id();
        self.index_type_decl_name(&id);
        self.file_types.entry(file_id).or_default().push(id.clone());

        if let Some(old_decl) = self.full_name_type_map.get_mut(&id) {
            old_decl.merge_decl(type_decl);
        } else {
            self.full_name_type_map.insert(id, type_decl);
        }
    }

    fn index_type_decl_name(&mut self, decl_id: &LuaTypeDeclId) {
        match decl_id.get_id() {
            LuaTypeIdentifier::Global(name) => {
                self.global_name_type_map
                    .entry(name.to_string())
                    .or_insert_with(|| decl_id.clone());
            }
            LuaTypeIdentifier::Internal(workspace_id, name) => {
                self.internal_name_type_map
                    .entry(*workspace_id)
                    .or_default()
                    .entry(name.to_string())
                    .or_insert_with(|| decl_id.clone());
            }
            LuaTypeIdentifier::Local(file_id, name) => {
                self.local_name_type_map
                    .entry(*file_id)
                    .or_default()
                    .entry(name.to_string())
                    .or_insert_with(|| decl_id.clone());
            }
        }
    }

    fn remove_type_decl_name(&mut self, decl_id: &LuaTypeDeclId) {
        match decl_id.get_id() {
            LuaTypeIdentifier::Global(name) => {
                self.global_name_type_map.remove(name.as_str());
            }
            LuaTypeIdentifier::Internal(workspace_id, name) => {
                let should_remove_workspace =
                    if let Some(type_names) = self.internal_name_type_map.get_mut(workspace_id) {
                        type_names.remove(name.as_str());
                        type_names.is_empty()
                    } else {
                        false
                    };
                if should_remove_workspace {
                    self.internal_name_type_map.remove(workspace_id);
                }
            }
            LuaTypeIdentifier::Local(file_id, name) => {
                let should_remove_file =
                    if let Some(type_names) = self.local_name_type_map.get_mut(file_id) {
                        type_names.remove(name.as_str());
                        type_names.is_empty()
                    } else {
                        false
                    };
                if should_remove_file {
                    self.local_name_type_map.remove(file_id);
                }
            }
        }
    }

    fn build_qualified_name<'a>(
        qualified_name: &'a mut String,
        namespace: &str,
        name: &str,
    ) -> &'a str {
        qualified_name.clear();
        qualified_name.reserve(namespace.len() + name.len() + 1);
        qualified_name.push_str(namespace);
        qualified_name.push('.');
        qualified_name.push_str(name);
        qualified_name.as_str()
    }

    pub fn find_type_decl(
        &self,
        file_id: FileId,
        name: &str,
        workspace_id: Option<WorkspaceId>,
    ) -> Option<&LuaTypeDecl> {
        let mut qualified_name = String::new();
        if let Some(ns) = self.get_file_namespace(&file_id) {
            Self::build_qualified_name(&mut qualified_name, ns, name);
            if let Some(decl) =
                self.find_scoped_type_decl_by_name(file_id, workspace_id, &qualified_name, false)
            {
                return Some(decl);
            }
        }
        if let Some(usings) = self.get_file_using_namespace(&file_id) {
            for ns in usings {
                Self::build_qualified_name(&mut qualified_name, ns, name);
                if let Some(decl) = self.find_scoped_type_decl_by_name(
                    file_id,
                    workspace_id,
                    &qualified_name,
                    false,
                ) {
                    return Some(decl);
                }
            }
        }

        self.find_scoped_type_decl_by_name(file_id, workspace_id, name, true)
    }

    pub fn find_type_decls(
        &self,
        file_id: FileId,
        prefix: &str,
        workspace_id: Option<WorkspaceId>,
    ) -> HashMap<String, Option<LuaTypeDeclId>> {
        let mut result = HashMap::new();
        let all_type_ids = self.full_name_type_map.keys().collect::<Vec<_>>();
        let mut qualified_prefix = String::new();
        if let Some(ns) = self.get_file_namespace(&file_id) {
            Self::build_qualified_name(&mut qualified_prefix, ns, prefix);
            self.collect_type_decl_matches(
                &mut result,
                &all_type_ids,
                file_id,
                workspace_id,
                &qualified_prefix,
            );
        }

        if let Some(usings) = self.get_file_using_namespace(&file_id) {
            for ns in usings {
                Self::build_qualified_name(&mut qualified_prefix, ns, prefix);
                self.collect_type_decl_matches(
                    &mut result,
                    &all_type_ids,
                    file_id,
                    workspace_id,
                    &qualified_prefix,
                );
            }
        }

        self.collect_type_decl_matches(&mut result, &all_type_ids, file_id, workspace_id, prefix);

        result
    }

    fn find_scoped_type_decl_by_name(
        &self,
        file_id: FileId,
        workspace_id: Option<WorkspaceId>,
        name: &str,
        allow_local: bool,
    ) -> Option<&LuaTypeDecl> {
        if allow_local {
            if let Some(decl) = self
                .local_name_type_map
                .get(&file_id)
                .and_then(|type_names| type_names.get(name))
                .and_then(|decl_id| self.full_name_type_map.get(decl_id))
            {
                return Some(decl);
            }
        }

        if let Some(workspace_id) = workspace_id {
            if let Some(decl) = self
                .internal_name_type_map
                .get(&workspace_id)
                .and_then(|type_names| type_names.get(name))
                .and_then(|decl_id| self.full_name_type_map.get(decl_id))
            {
                return Some(decl);
            }
        }

        self.global_name_type_map
            .get(name)
            .and_then(|decl_id| self.full_name_type_map.get(decl_id))
    }

    fn collect_type_decl_matches(
        &self,
        result: &mut HashMap<String, Option<LuaTypeDeclId>>,
        all_type_ids: &[&LuaTypeDeclId],
        file_id: FileId,
        workspace_id: Option<WorkspaceId>,
        prefix: &str,
    ) {
        for id in all_type_ids {
            let id_name = match id.get_id() {
                LuaTypeIdentifier::Global(name) => name.as_str(),
                LuaTypeIdentifier::Internal(owner_workspace_id, name) => {
                    if workspace_id == Some(*owner_workspace_id) {
                        name.as_str()
                    } else {
                        continue;
                    }
                }
                LuaTypeIdentifier::Local(owner_file_id, name) => {
                    if *owner_file_id == file_id {
                        name.as_str()
                    } else {
                        continue;
                    }
                }
            };
            if id_name.starts_with(prefix)
                && let Some(rest_name) = id_name.strip_prefix(prefix)
            {
                if let Some(i) = rest_name.find('.') {
                    let name = rest_name[..i].to_string();
                    result.entry(name).or_insert(None);
                } else {
                    result.insert(rest_name.to_string(), Some((*id).clone()));
                }
            }
        }
    }

    pub fn add_generic_params(&mut self, decl_id: LuaTypeDeclId, params: Vec<GenericParam>) {
        self.generic_params.insert(decl_id, params);
    }

    pub fn get_generic_params(&self, decl_id: &LuaTypeDeclId) -> Option<&Vec<GenericParam>> {
        self.generic_params.get(decl_id)
    }

    pub fn add_super_type(&mut self, decl_id: LuaTypeDeclId, file_id: FileId, super_type: LuaType) {
        self.supers
            .entry(decl_id)
            .or_default()
            .push(InFiled::new(file_id, super_type));
    }

    pub fn get_super_types(&self, decl_id: &LuaTypeDeclId) -> Option<Vec<LuaType>> {
        self.supers
            .get(decl_id)
            .map(|supers| supers.iter().map(|s| s.value.clone()).collect())
    }

    pub fn get_super_types_iter(
        &self,
        decl_id: &LuaTypeDeclId,
    ) -> Option<impl Iterator<Item = &LuaType> + '_> {
        self.supers
            .get(decl_id)
            .map(|supers| supers.iter().map(|s| &s.value))
    }

    /// Get all direct subclasses of a given type
    /// Returns a vector of type declarations that directly inherit from the given type
    pub fn get_sub_types(&self, decl_id: &LuaTypeDeclId) -> Vec<&LuaTypeDecl> {
        let mut sub_types = Vec::new();

        // Iterate through all types and check their super types
        for (type_id, supers) in &self.supers {
            for super_filed in supers {
                // Check if this super type references our target type
                if let LuaType::Ref(super_id) = &super_filed.value {
                    if super_id == decl_id {
                        // Found a subclass
                        if let Some(sub_decl) = self.full_name_type_map.get(type_id) {
                            sub_types.push(sub_decl);
                        }
                        break; // No need to check other supers of this type
                    }
                }
            }
        }

        sub_types
    }

    /// Get all subclasses (direct and indirect) of a given type recursively
    /// Returns a vector of type declarations in the inheritance hierarchy
    pub fn get_all_sub_types(&self, decl_id: &LuaTypeDeclId) -> Vec<&LuaTypeDecl> {
        let mut all_sub_types = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = vec![decl_id.clone()];

        while let Some(current_id) = queue.pop() {
            if !visited.insert(current_id.clone()) {
                continue;
            }

            // Find direct subclasses of current_id
            let direct_subs = self.get_sub_types(&current_id);
            for sub_decl in direct_subs {
                let sub_id = sub_decl.get_id();
                if !visited.contains(&sub_id) {
                    all_sub_types.push(sub_decl);
                    queue.push(sub_id);
                }
            }
        }

        all_sub_types
    }

    pub fn get_type_decl(&self, decl_id: &LuaTypeDeclId) -> Option<&LuaTypeDecl> {
        self.full_name_type_map.get(decl_id)
    }

    pub fn get_all_types(&self) -> Vec<&LuaTypeDecl> {
        self.full_name_type_map.values().collect()
    }

    pub fn get_file_namespaces(&self) -> Vec<String> {
        self.file_namespace
            .values()
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn get_type_decl_mut(&mut self, decl_id: &LuaTypeDeclId) -> Option<&mut LuaTypeDecl> {
        self.full_name_type_map.get_mut(decl_id)
    }

    pub fn bind_type(&mut self, owner: LuaTypeOwner, cache: LuaTypeCache) {
        if self.types.contains_key(&owner) {
            return;
        }
        self.types.insert(owner.clone(), cache);
        self.in_filed_type_owner
            .entry(owner.get_file_id())
            .or_default()
            .insert(owner);
    }

    pub fn get_type_cache(&self, owner: &LuaTypeOwner) -> Option<&LuaTypeCache> {
        self.types.get(owner)
    }
}

impl LuaIndex for LuaTypeIndex {
    fn remove(&mut self, file_id: FileId) {
        self.file_namespace.remove(&file_id);
        self.file_using_namespace.remove(&file_id);
        if let Some(type_id_list) = self.file_types.remove(&file_id) {
            for id in type_id_list {
                let mut remove_type = false;
                if let Some(decl) = self.full_name_type_map.get_mut(&id) {
                    decl.get_mut_locations()
                        .retain(|loc| loc.file_id != file_id);
                    if decl.get_mut_locations().is_empty() {
                        self.full_name_type_map.remove(&id);
                        remove_type = true;
                    }
                }

                if let Some(supers) = self.supers.get_mut(&id) {
                    supers.retain(|s| s.file_id != file_id);
                    if supers.is_empty() {
                        self.supers.remove(&id);
                    }
                }

                if remove_type {
                    self.remove_type_decl_name(&id);
                    self.generic_params.remove(&id);
                }
            }
        }

        if let Some(type_owners) = self.in_filed_type_owner.remove(&file_id) {
            for type_owner in type_owners {
                self.types.remove(&type_owner);
            }
        }
    }

    fn clear(&mut self) {
        self.file_namespace.clear();
        self.file_using_namespace.clear();
        self.file_types.clear();
        self.full_name_type_map.clear();
        self.generic_params.clear();
        self.supers.clear();
        self.types.clear();
        self.in_filed_type_owner.clear();
        self.global_name_type_map.clear();
        self.internal_name_type_map.clear();
        self.local_name_type_map.clear();
    }
}

pub fn get_real_type<'a>(db: &'a DbIndex, typ: &'a LuaType) -> Option<&'a LuaType> {
    get_real_type_with_depth(db, typ, 0)
}

fn get_real_type_with_depth<'a>(
    db: &'a DbIndex,
    typ: &'a LuaType,
    depth: u32,
) -> Option<&'a LuaType> {
    const MAX_RECURSION_DEPTH: u32 = 10;

    if depth >= MAX_RECURSION_DEPTH {
        return Some(typ);
    }

    match typ {
        LuaType::Ref(type_decl_id) => {
            let type_decl = db.get_type_index().get_type_decl(type_decl_id)?;
            if type_decl.is_alias() {
                return get_real_type_with_depth(db, type_decl.get_alias_ref()?, depth + 1);
            }
            Some(typ)
        }
        _ => Some(typ),
    }
}

// 第一个参数是否不应该视为 self
pub fn first_param_may_not_self(typ: &LuaType) -> bool {
    if typ.is_table()
        || matches!(
            typ,
            LuaType::TplRef(_) | LuaType::StrTplRef(_) | LuaType::Any
        )
    {
        return true;
    }

    if let LuaType::Union(u) = typ {
        return u.into_vec().iter().any(first_param_may_not_self);
    }
    false
}
