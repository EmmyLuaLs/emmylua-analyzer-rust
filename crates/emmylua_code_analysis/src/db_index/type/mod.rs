mod humanize_type;
mod test;
mod type_assert;
mod type_decl;
mod type_ops;
mod types;

use super::traits::LuaIndex;
use crate::{FileId, InFiled};
use emmylua_parser::LuaSyntaxId;
use flagset::FlagSet;
pub use humanize_type::{humanize_type, RenderLevel};
use rowan::TextRange;
use std::collections::HashMap;
pub use type_assert::TypeAssertion;
pub use type_decl::{
    LuaDeclLocation, LuaDeclTypeKind, LuaTypeAttribute, LuaTypeDecl, LuaTypeDeclId,
};
pub use type_ops::TypeOps;
pub use types::*;

#[derive(Debug)]
pub struct LuaTypeIndex {
    file_namespace: HashMap<FileId, String>,
    file_using_namespace: HashMap<FileId, Vec<String>>,
    file_env: HashMap<FileId, String>,
    file_types: HashMap<FileId, Vec<LuaTypeDeclId>>,
    full_name_type_map: HashMap<LuaTypeDeclId, LuaTypeDecl>,
    generic_params: HashMap<LuaTypeDeclId, Vec<(String, Option<LuaType>)>>,
    supers: HashMap<LuaTypeDeclId, Vec<InFiled<LuaType>>>,
    as_force_type: HashMap<InFiled<LuaSyntaxId>, LuaType>,
}

impl LuaTypeIndex {
    pub fn new() -> Self {
        Self {
            file_namespace: HashMap::new(),
            file_using_namespace: HashMap::new(),
            file_env: HashMap::new(),
            file_types: HashMap::new(),
            full_name_type_map: HashMap::new(),
            generic_params: HashMap::new(),
            supers: HashMap::new(),
            as_force_type: HashMap::new(),
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

    pub fn add_file_env(&mut self, file_id: FileId, env: String) {
        self.file_env.insert(file_id, env);
    }

    pub fn get_file_env(&self, file_id: &FileId) -> Option<&String> {
        self.file_env.get(file_id)
    }

    pub fn add_type_decl(
        &mut self,
        file_id: FileId,
        range: TextRange,
        name: String,
        kind: LuaDeclTypeKind,
        attrib: Option<FlagSet<LuaTypeAttribute>>,
    ) -> Result<(), String> {
        let basic_name = name;
        let ns = self.get_file_namespace(&file_id);
        let full_name = ns
            .map(|ns| format!("{}.{}", ns, basic_name))
            .unwrap_or(basic_name.to_string());
        let id = LuaTypeDeclId::new(&full_name);
        self.file_types
            .entry(file_id)
            .or_insert_with(Vec::new)
            .push(id.clone());

        if let Some(decls) = self.full_name_type_map.get_mut(&id) {
            let can_add = match (decls.get_attrib(), attrib) {
                (Some(a), Some(b)) => {
                    a.contains(LuaTypeAttribute::Partial) && b.contains(LuaTypeAttribute::Partial)
                }
                _ => false,
            };

            if !can_add {
                return Err(t!("Type '%{name}' already defined", name = full_name).to_string());
            }

            if let Some(decl_attrib) = &mut decls.attrib {
                *decl_attrib |= attrib.unwrap();
            }

            let location = LuaDeclLocation { file_id, range };
            decls.defined_locations.push(location);
        } else {
            let just_name = if let Some(i) = basic_name.rfind('.') {
                basic_name[i + 1..].to_string()
            } else {
                basic_name.clone()
            };

            let decl = LuaTypeDecl::new(file_id, range, just_name, kind, attrib, id.clone());
            self.full_name_type_map.insert(id, decl);
        }

        Ok(())
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
                if id_name.starts_with(prefix) {
                    let rest_name = id_name.strip_prefix(prefix).unwrap();
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
                    if id_name.starts_with(prefix) {
                        let rest_name = id_name.strip_prefix(prefix).unwrap();
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
                let rest_name = id_name.strip_prefix(prefix).unwrap();
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

    pub fn add_as_force_type(&mut self, syntax_id: InFiled<LuaSyntaxId>, ty: LuaType) {
        self.as_force_type.insert(syntax_id, ty);
    }

    pub fn get_as_force_type(&self, syntax_id: &InFiled<LuaSyntaxId>) -> Option<&LuaType> {
        self.as_force_type.get(syntax_id)
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

        self.as_force_type.retain(|id, _| id.file_id != file_id);
    }

    fn fill_snapshot_info(&self, info: &mut HashMap<String, String>) {
        info.insert(
            "type.file_namespace".to_string(),
            self.file_namespace.len().to_string(),
        );
        info.insert(
            "type.file_using_namespace".to_string(),
            self.file_using_namespace.len().to_string(),
        );
        info.insert(
            "type.file_types".to_string(),
            self.file_types.len().to_string(),
        );
        info.insert(
            "type.full_name_type_map".to_string(),
            self.full_name_type_map.len().to_string(),
        );
        info.insert(
            "type.generic_params".to_string(),
            self.generic_params.len().to_string(),
        );
        info.insert("type.supers".to_string(), self.supers.len().to_string());
        info.insert(
            "type.as_force_type".to_string(),
            self.as_force_type.len().to_string(),
        );
    }
}
