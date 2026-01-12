mod generic_param;
mod humanize_type;
mod test;
mod type_decl;
mod type_ops;
mod type_owner;
mod type_visit_trait;
mod types;

use super::traits::LuaIndex;
use crate::{DbIndex, FileId, InFiled, db_index::r#type::type_decl::LuaTypeIdentifier};
pub use generic_param::GenericParam;
pub use humanize_type::{RenderLevel, format_union_type, humanize_type};
use std::collections::{HashMap, HashSet};
pub use type_decl::{LuaDeclLocation, LuaDeclTypeKind, LuaTypeDecl, LuaTypeDeclId, LuaTypeFlag};
pub use type_ops::TypeOps;
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
        self.file_types.entry(file_id).or_default().push(id.clone());

        if let Some(old_decl) = self.full_name_type_map.get_mut(&id) {
            old_decl.merge_decl(type_decl);
        } else {
            self.full_name_type_map.insert(id, type_decl);
        }
    }

    pub fn find_type_decl(&self, file_id: FileId, name: &str) -> Option<&LuaTypeDecl> {
        if let Some(ns) = self.get_file_namespace(&file_id) {
            let full_name = LuaTypeDeclId::global(&format!("{}.{}", ns, name));
            if let Some(decl) = self.full_name_type_map.get(&full_name) {
                return Some(decl);
            }
        }
        if let Some(usings) = self.get_file_using_namespace(&file_id) {
            for ns in usings {
                let full_name = LuaTypeDeclId::global(&format!("{}.{}", ns, name));
                if let Some(decl) = self.full_name_type_map.get(&full_name) {
                    return Some(decl);
                }
            }
        }

        let local_id = LuaTypeDeclId::local(file_id, name);
        if let Some(decl) = self.full_name_type_map.get(&local_id) {
            return Some(decl);
        }

        let global_id = LuaTypeDeclId::global(name);
        self.full_name_type_map.get(&global_id)
    }

    pub fn find_type_decls(
        &self,
        file_id: FileId,
        prefix: &str,
    ) -> HashMap<String, Option<LuaTypeDeclId>> {
        let mut result = HashMap::new();
        let all_type_ids = self.full_name_type_map.keys().collect::<Vec<_>>();
        if let Some(ns) = self.get_file_namespace(&file_id) {
            let prefix = &format!("{}.{}", ns, prefix);
            for id in all_type_ids.clone() {
                let id_name = id.get_name();

                if let Some(rest_name) = id_name.strip_prefix(prefix) {
                    if let Some(i) = rest_name.find('.') {
                        let name = rest_name[..i].to_string();
                        result.entry(name).or_insert(None);
                    } else {
                        result.insert(rest_name.to_string(), Some(id.clone()));
                    }
                }
            }
        }

        if let Some(usings) = self.get_file_using_namespace(&file_id) {
            for ns in usings {
                let prefix = &format!("{}.{}", ns, prefix);
                for id in all_type_ids.clone() {
                    let id_name = id.get_name();

                    if let Some(rest_name) = id_name.strip_prefix(prefix) {
                        if let Some(i) = rest_name.find('.') {
                            let name = rest_name[..i].to_string();
                            result.entry(name).or_insert(None);
                        } else {
                            result.insert(rest_name.to_string(), Some(id.clone()));
                        }
                    }
                }
            }
        }

        for id in all_type_ids {
            let id_name = match id.get_id() {
                LuaTypeIdentifier::Local(f_id, name) => {
                    if f_id != &file_id {
                        continue;
                    }
                    name
                }
                LuaTypeIdentifier::Global(name) => name,
            };
            if id_name.starts_with(prefix)
                && let Some(rest_name) = id_name.strip_prefix(prefix)
            {
                if let Some(i) = rest_name.find('.') {
                    let name = rest_name[..i].to_string();
                    result.entry(name).or_insert(None);
                } else {
                    result.insert(rest_name.to_string(), Some(id.clone()));
                }
            }
        }

        result
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
