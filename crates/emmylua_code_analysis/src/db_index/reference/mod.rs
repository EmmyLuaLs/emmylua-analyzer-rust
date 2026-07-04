mod file_reference;
mod string_reference;

use hashbrown::{HashMap, HashSet};

use emmylua_parser::LuaSyntaxId;
pub use file_reference::{DeclReference, DeclReferenceCell, FileReference};
use rowan::TextRange;
use smol_str::SmolStr;
use string_reference::StringReference;

use super::{LuaDeclId, LuaMemberKey, LuaTypeDeclId, traits::LuaIndex};
use crate::{FileId, InFiled, LuaClosureId};

#[derive(Debug)]
pub struct LuaReferenceIndex {
    file_references: HashMap<FileId, FileReference>,
    index_reference: HashMap<LuaMemberKey, HashMap<FileId, HashSet<LuaSyntaxId>>>,
    global_references: HashMap<SmolStr, HashMap<FileId, HashSet<LuaSyntaxId>>>,
    string_references: HashMap<FileId, StringReference>,
    type_references: HashMap<FileId, HashMap<LuaTypeDeclId, HashSet<TextRange>>>,
    label_references: HashMap<FileId, FileLabelReferences>,
}

impl Default for LuaReferenceIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaReferenceIndex {
    pub fn new() -> Self {
        Self {
            file_references: HashMap::new(),
            index_reference: HashMap::new(),
            global_references: HashMap::new(),
            string_references: HashMap::new(),
            type_references: HashMap::new(),
            label_references: HashMap::new(),
        }
    }

    pub fn add_decl_reference(
        &mut self,
        decl_id: LuaDeclId,
        file_id: FileId,
        range: TextRange,
        is_write: bool,
    ) {
        self.file_references
            .entry(file_id)
            .or_default()
            .add_decl_reference(decl_id, range, is_write);
    }

    pub fn add_global_reference(&mut self, name: &str, file_id: FileId, syntax_id: LuaSyntaxId) {
        let key = SmolStr::new(name);
        self.global_references
            .entry(key)
            .or_default()
            .entry(file_id)
            .or_default()
            .insert(syntax_id);
    }

    pub fn add_index_reference(
        &mut self,
        key: LuaMemberKey,
        file_id: FileId,
        syntax_id: LuaSyntaxId,
    ) {
        self.index_reference
            .entry(key)
            .or_default()
            .entry(file_id)
            .or_default()
            .insert(syntax_id);
    }

    pub fn add_string_reference(&mut self, file_id: FileId, string: &str, range: TextRange) {
        self.string_references
            .entry(file_id)
            .or_insert_with(StringReference::new)
            .add_string_reference(string, range);
    }

    pub fn add_type_reference(
        &mut self,
        file_id: FileId,
        type_decl_id: LuaTypeDeclId,
        range: TextRange,
    ) {
        self.type_references
            .entry(file_id)
            .or_default()
            .entry(type_decl_id)
            .or_default()
            .insert(range);
    }

    pub fn add_label_declaration(
        &mut self,
        file_id: FileId,
        closure_id: LuaClosureId,
        name: &str,
        range: TextRange,
    ) {
        self.label_references
            .entry(file_id)
            .or_default()
            .add_declaration(closure_id, name, range);
    }

    pub fn add_label_reference(
        &mut self,
        file_id: FileId,
        closure_id: LuaClosureId,
        name: &str,
        range: TextRange,
    ) {
        self.label_references
            .entry(file_id)
            .or_default()
            .add_reference(closure_id, name, range);
    }

    pub fn get_local_reference(&self, file_id: &FileId) -> Option<&FileReference> {
        self.file_references.get(file_id)
    }

    pub fn create_local_reference(&mut self, file_id: FileId) {
        self.file_references.entry(file_id).or_default();
    }

    pub fn get_decl_references(
        &self,
        file_id: &FileId,
        decl_id: &LuaDeclId,
    ) -> Option<&DeclReference> {
        self.file_references
            .get(file_id)?
            .get_decl_references(decl_id)
    }

    pub fn get_var_reference_decl(&self, file_id: &FileId, range: TextRange) -> Option<LuaDeclId> {
        self.file_references.get(file_id)?.get_decl_id(&range)
    }

    pub fn get_decl_references_map(
        &self,
        file_id: &FileId,
    ) -> Option<&HashMap<LuaDeclId, DeclReference>> {
        self.file_references
            .get(file_id)
            .map(|file_reference| file_reference.get_decl_references_map())
    }

    pub fn get_global_file_references(
        &self,
        name: &str,
        file_id: FileId,
    ) -> Option<Vec<LuaSyntaxId>> {
        let results = self
            .global_references
            .get(name)?
            .iter()
            .filter_map(|(source_file_id, syntax_ids)| {
                if file_id == *source_file_id {
                    Some(syntax_ids.iter())
                } else {
                    None
                }
            })
            .flatten()
            .copied()
            .collect();

        Some(results)
    }

    pub fn get_global_references(&self, name: &str) -> Option<Vec<InFiled<LuaSyntaxId>>> {
        let results = self
            .global_references
            .get(name)?
            .iter()
            .flat_map(|(file_id, syntax_ids)| {
                syntax_ids
                    .iter()
                    .map(|syntax_id| InFiled::new(*file_id, *syntax_id))
            })
            .collect();

        Some(results)
    }

    pub fn get_index_references(&self, key: &LuaMemberKey) -> Option<Vec<InFiled<LuaSyntaxId>>> {
        let results = self
            .index_reference
            .get(key)?
            .iter()
            .flat_map(|(file_id, syntax_ids)| {
                syntax_ids
                    .iter()
                    .map(|syntax_id| InFiled::new(*file_id, *syntax_id))
            })
            .collect();

        Some(results)
    }

    pub fn get_string_references(&self, string_value: &str) -> Vec<InFiled<TextRange>> {
        self.string_references
            .iter()
            .flat_map(|(file_id, string_reference)| {
                string_reference
                    .get_string_references(string_value)
                    .into_iter()
                    .map(|range| InFiled::new(*file_id, range))
            })
            .collect()
    }

    pub fn get_type_references(
        &self,
        type_decl_id: &LuaTypeDeclId,
    ) -> Option<Vec<InFiled<TextRange>>> {
        let results = self
            .type_references
            .iter()
            .flat_map(|(file_id, type_references)| {
                type_references
                    .get(type_decl_id)
                    .into_iter()
                    .flatten()
                    .map(|range| InFiled::new(*file_id, *range))
            })
            .collect();

        Some(results)
    }

    pub fn get_label_definition(
        &self,
        file_id: &FileId,
        closure_id: LuaClosureId,
        name: &str,
    ) -> Option<TextRange> {
        self.label_references
            .get(file_id)?
            .get_definition(closure_id, name)
    }

    pub fn get_label_references(
        &self,
        file_id: &FileId,
        closure_id: LuaClosureId,
        name: &str,
    ) -> Option<Vec<TextRange>> {
        self.label_references
            .get(file_id)?
            .get_references(closure_id, name)
    }
}

impl LuaIndex for LuaReferenceIndex {
    fn remove(&mut self, file_id: FileId) {
        self.file_references.remove(&file_id);
        self.string_references.remove(&file_id);
        self.type_references.remove(&file_id);
        self.label_references.remove(&file_id);
        let mut to_be_remove = Vec::new();
        for (key, references) in self.index_reference.iter_mut() {
            references.remove(&file_id);
            if references.is_empty() {
                to_be_remove.push(key.clone());
            }
        }

        for key in to_be_remove {
            self.index_reference.remove(&key);
        }

        let mut to_be_remove = Vec::new();
        for (key, references) in self.global_references.iter_mut() {
            references.remove(&file_id);
            if references.is_empty() {
                to_be_remove.push(key.clone());
            }
        }

        for key in to_be_remove {
            self.global_references.remove(&key);
        }
    }

    fn clear(&mut self) {
        self.file_references.clear();
        self.string_references.clear();
        self.index_reference.clear();
        self.global_references.clear();
        self.type_references.clear();
        self.label_references.clear();
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct LabelKey {
    closure_id: LuaClosureId,
    name: SmolStr,
}

impl LabelKey {
    fn new(closure_id: LuaClosureId, name: &str) -> Self {
        Self {
            closure_id,
            name: SmolStr::new(name),
        }
    }

    fn is_match(&self, closure_id: LuaClosureId, name: &str) -> bool {
        self.closure_id == closure_id && self.name.as_str() == name
    }
}

#[derive(Debug, Default)]
struct FileLabelReferences {
    labels: Vec<LabelReferences>,
}

impl FileLabelReferences {
    fn add_declaration(&mut self, closure_id: LuaClosureId, name: &str, range: TextRange) {
        let key = LabelKey::new(closure_id, name);
        self.get_or_create_label(key).declaration = Some(range);
    }

    fn add_reference(&mut self, closure_id: LuaClosureId, name: &str, range: TextRange) {
        let key = LabelKey::new(closure_id, name);
        let label = self.get_or_create_label(key);
        if !label.references.contains(&range) {
            label.references.push(range);
        }
    }

    fn get_definition(&self, closure_id: LuaClosureId, name: &str) -> Option<TextRange> {
        self.get_label(closure_id, name)?.declaration
    }

    fn get_references(&self, closure_id: LuaClosureId, name: &str) -> Option<Vec<TextRange>> {
        let label = self.get_label(closure_id, name)?;
        let mut ranges =
            Vec::with_capacity(label.references.len() + usize::from(label.declaration.is_some()));

        if let Some(declaration) = label.declaration {
            ranges.push(declaration);
        }

        ranges.extend(label.references.iter().copied());

        Some(ranges)
    }

    fn get_or_create_label(&mut self, key: LabelKey) -> &mut LabelReferences {
        if let Some(index) = self.labels.iter().position(|label| label.key == key) {
            return &mut self.labels[index];
        }

        self.labels.push(LabelReferences {
            key,
            declaration: None,
            references: Vec::new(),
        });
        self.labels.last_mut().expect("label was just inserted")
    }

    fn get_label(&self, closure_id: LuaClosureId, name: &str) -> Option<&LabelReferences> {
        self.labels
            .iter()
            .find(|label| label.key.is_match(closure_id, name))
    }
}

#[derive(Debug)]
struct LabelReferences {
    key: LabelKey,
    declaration: Option<TextRange>,
    references: Vec<TextRange>,
}
