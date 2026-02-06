mod schema_file;

use std::collections::HashMap;

use url::Url;

use crate::{FileId, LuaIndex};
pub use schema_file::*;

#[derive(Debug)]
pub struct JsonSchemaIndex {
    schema_files: HashMap<Url, JsonSchemaFile>,
}

impl JsonSchemaIndex {
    pub fn new() -> Self {
        Self {
            schema_files: HashMap::new(),
        }
    }

    pub fn get_schema_file(&self, url: &Url) -> Option<&JsonSchemaFile> {
        self.schema_files.get(url)
    }

    pub fn get_schema_file_mut(&mut self, url: &Url) -> Option<&mut JsonSchemaFile> {
        self.schema_files.get_mut(url)
    }

    pub fn add_schema_file(&mut self, url: Url, schema_file: JsonSchemaFile) {
        self.schema_files.insert(url, schema_file);
    }

    pub fn has_need_resolve_schemas(&self) -> bool {
        self.schema_files
            .values()
            .any(|schema_file| matches!(schema_file, JsonSchemaFile::NeedResolve))
    }

    pub fn get_need_resolve_schemas(&self) -> Vec<Url> {
        self.schema_files
            .iter()
            .filter_map(|(url, schema_file)| {
                if let JsonSchemaFile::NeedResolve = schema_file {
                    Some(url.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn reset_rest_schemas(&mut self) {
        for schema_file in self.schema_files.values_mut() {
            if let JsonSchemaFile::NeedResolve = schema_file {
                *schema_file = JsonSchemaFile::BadUrl;
            }
        }
    }
}

impl LuaIndex for JsonSchemaIndex {
    fn remove(&mut self, _file_id: FileId) {
        // TODO remove schema index by file_id
    }

    fn clear(&mut self) {
        // TODO clear all schema index
    }
}
