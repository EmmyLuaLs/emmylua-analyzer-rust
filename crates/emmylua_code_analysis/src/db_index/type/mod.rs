mod humanize_type;
mod test;
mod type_assert;
mod type_decl;
mod type_ops;
mod type_owner;
mod types;

use super::traits::LuaIndex;
use crate::{FileId, InFiled};
pub use humanize_type::{format_union_type, humanize_type, RenderLevel};
use std::collections::{HashMap, HashSet};
pub use type_assert::TypeAssertion;
pub use type_decl::{
    LuaDeclLocation, LuaDeclTypeKind, LuaTypeAttribute, LuaTypeDecl, LuaTypeDeclId,
};
pub use type_ops::TypeOps;
pub use type_owner::{LuaTypeCache, LuaTypeOwner};
pub use types::*;

#[derive(Debug)]
pub struct LuaTypeIndex {
    file_namespace: HashMap<FileId, String>,
    file_using_namespace: HashMap<FileId, Vec<String>>,
    file_types: HashMap<FileId, Vec<LuaTypeDeclId>>,
    full_name_type_map: HashMap<LuaTypeDeclId, LuaTypeDecl>,
    generic_params: HashMap<LuaTypeDeclId, Vec<(String, Option<LuaType>)>>,
    supers: HashMap<LuaTypeDeclId, Vec<InFiled<LuaType>>>,
    types: HashMap<LuaTypeOwner, LuaTypeCache>,
    in_filed_type_owner: HashMap<FileId, HashSet<LuaTypeOwner>>,
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
            .or_insert_with(Vec::new)
            .push(namespace);
    }

    pub fn get_file_using_namespace(&self, file_id: &FileId) -> Option<&Vec<String>> {
        self.file_using_namespace.get(file_id)
    }

    pub fn add_type_decl(&mut self, file_id: FileId, type_decl: LuaTypeDecl) {
        let id = type_decl.get_id();
        self.file_types
            .entry(file_id)
            .or_insert_with(Vec::new)
            .push(id.clone());

        if let Some(old_decl) = self.full_name_type_map.get_mut(&id) {
            old_decl.merge_decl(type_decl);
        } else {
            self.full_name_type_map.insert(id, type_decl);
        }
    }

    pub fn find_type_decl(&self, file_id: FileId, name: &str) -> Option<&LuaTypeDecl> {
        if let Some(ns) = self.get_file_namespace(&file_id) {
            let full_name = LuaTypeDeclId::new(&format!("{}.{}", ns, name));
            if let Some(decl) = self.full_name_type_map.get(&full_name) {
                return Some(decl);
            }
        }
        if let Some(usings) = self.get_file_using_namespace(&file_id) {
            for ns in usings {
                let full_name = LuaTypeDeclId::new(&format!("{}.{}", ns, name));
                if let Some(decl) = self.full_name_type_map.get(&full_name) {
                    return Some(decl);
                }
            }
        }

        let id = LuaTypeDeclId::new(name);
        self.full_name_type_map.get(&id)
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
                        if !result.contains_key(&name) {
                            result.insert(name, None);
                        }
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
                            if !result.contains_key(&name) {
                                result.insert(name, None);
                            }
                        } else {
                            result.insert(rest_name.to_string(), Some(id.clone()));
                        }
                    }
                }
            }
        }

        for id in all_type_ids {
            let id_name = id.get_name();
            if id_name.starts_with(prefix) {
                if let Some(rest_name) = id_name.strip_prefix(prefix) {
                    if let Some(i) = rest_name.find('.') {
                        let name = rest_name[..i].to_string();
                        if !result.contains_key(&name) {
                            result.insert(name, None);
                        }
                    } else {
                        result.insert(rest_name.to_string(), Some(id.clone()));
                    }
                }
            }
        }

        result
    }

    pub fn add_generic_params(
        &mut self,
        decl_id: LuaTypeDeclId,
        params: Vec<(String, Option<LuaType>)>,
    ) {
        self.generic_params.insert(decl_id, params);
    }

    pub fn get_generic_params(
        &self,
        decl_id: &LuaTypeDeclId,
    ) -> Option<&Vec<(String, Option<LuaType>)>> {
        self.generic_params.get(decl_id)
    }

    pub fn add_super_type(&mut self, decl_id: LuaTypeDeclId, file_id: FileId, super_type: LuaType) {
        self.supers
            .entry(decl_id)
            .or_insert_with(Vec::new)
            .push(InFiled::new(file_id, super_type));
    }

    pub fn get_super_types(&self, decl_id: &LuaTypeDeclId) -> Option<Vec<LuaType>> {
        if let Some(supers) = self.supers.get(decl_id) {
            Some(supers.iter().map(|s| s.value.clone()).collect())
        } else {
            None
        }
    }

    pub fn get_type_decl(&self, decl_id: &LuaTypeDeclId) -> Option<&LuaTypeDecl> {
        self.full_name_type_map.get(decl_id)
    }

    pub fn get_all_types(&self) -> Vec<&LuaTypeDecl> {
        self.full_name_type_map.values().collect()
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
            .or_insert_with(HashSet::new)
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
