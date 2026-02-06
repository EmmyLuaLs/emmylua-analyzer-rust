mod declaration;
mod dependency;
mod diagnostic;
mod flow;
mod global;
mod member;
mod metatable;
mod module;
mod operators;
mod property;
mod reference;
mod schema;
mod semantic_decl;
mod signature;
mod traits;
mod r#type;

use std::sync::Arc;

use crate::{Emmyrc, FileId, Vfs};
pub use declaration::*;
pub use dependency::LuaDependencyIndex;
pub use diagnostic::{AnalyzeError, DiagnosticAction, DiagnosticActionKind, DiagnosticIndex};
pub use flow::*;
pub use global::{GlobalId, LuaGlobalIndex};
pub use member::*;
pub use metatable::LuaMetatableIndex;
pub use module::*;
pub use operators::*;
pub use property::*;
pub use reference::*;
pub use schema::*;
pub use semantic_decl::*;
pub use signature::*;
pub use traits::LuaIndex;
pub use r#type::*;

#[derive(Debug)]
pub struct DbIndex {
    decl_index: LuaDeclIndex,
    references_index: LuaReferenceIndex,
    types_index: LuaTypeIndex,
    modules_index: LuaModuleIndex,
    members_index: LuaMemberIndex,
    property_index: LuaPropertyIndex,
    signature_index: LuaSignatureIndex,
    diagnostic_index: DiagnosticIndex,
    operator_index: LuaOperatorIndex,
    flow_index: LuaFlowIndex,
    vfs: Vfs,
    file_dependencies_index: LuaDependencyIndex,
    metatable_index: LuaMetatableIndex,
    global_index: LuaGlobalIndex,
    json_schema_index: JsonSchemaIndex,
    emmyrc: Arc<Emmyrc>,
}

#[allow(unused)]
impl Default for DbIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl DbIndex {
    pub fn new() -> Self {
        Self {
            decl_index: LuaDeclIndex::new(),
            references_index: LuaReferenceIndex::new(),
            types_index: LuaTypeIndex::new(),
            modules_index: LuaModuleIndex::new(),
            members_index: LuaMemberIndex::new(),
            property_index: LuaPropertyIndex::new(),
            signature_index: LuaSignatureIndex::new(),
            diagnostic_index: DiagnosticIndex::new(),
            operator_index: LuaOperatorIndex::new(),
            flow_index: LuaFlowIndex::new(),
            vfs: Vfs::new(),
            file_dependencies_index: LuaDependencyIndex::new(),
            metatable_index: LuaMetatableIndex::new(),
            global_index: LuaGlobalIndex::new(),
            json_schema_index: JsonSchemaIndex::new(),
            emmyrc: Arc::new(Emmyrc::default()),
        }
    }

    pub fn remove_index(&mut self, file_ids: Vec<FileId>) {
        for file_id in file_ids {
            self.remove(file_id);
        }
    }

    pub fn get_metatable_index_mut(&mut self) -> &mut LuaMetatableIndex {
        &mut self.metatable_index
    }

    pub fn get_metatable_index(&self) -> &LuaMetatableIndex {
        &self.metatable_index
    }

    pub fn get_decl_index_mut(&mut self) -> &mut LuaDeclIndex {
        &mut self.decl_index
    }

    pub fn get_reference_index_mut(&mut self) -> &mut LuaReferenceIndex {
        &mut self.references_index
    }

    pub fn get_type_index_mut(&mut self) -> &mut LuaTypeIndex {
        &mut self.types_index
    }

    pub fn get_module_index_mut(&mut self) -> &mut LuaModuleIndex {
        &mut self.modules_index
    }

    pub fn get_member_index_mut(&mut self) -> &mut LuaMemberIndex {
        &mut self.members_index
    }

    pub fn get_property_index_mut(&mut self) -> &mut LuaPropertyIndex {
        &mut self.property_index
    }

    pub fn get_signature_index_mut(&mut self) -> &mut LuaSignatureIndex {
        &mut self.signature_index
    }

    pub fn get_diagnostic_index_mut(&mut self) -> &mut DiagnosticIndex {
        &mut self.diagnostic_index
    }

    pub fn get_operator_index_mut(&mut self) -> &mut LuaOperatorIndex {
        &mut self.operator_index
    }

    pub fn get_flow_index_mut(&mut self) -> &mut LuaFlowIndex {
        &mut self.flow_index
    }

    pub fn get_decl_index(&self) -> &LuaDeclIndex {
        &self.decl_index
    }

    pub fn get_reference_index(&self) -> &LuaReferenceIndex {
        &self.references_index
    }

    pub fn get_type_index(&self) -> &LuaTypeIndex {
        &self.types_index
    }

    pub fn get_module_index(&self) -> &LuaModuleIndex {
        &self.modules_index
    }

    pub fn get_member_index(&self) -> &LuaMemberIndex {
        &self.members_index
    }

    pub fn get_property_index(&self) -> &LuaPropertyIndex {
        &self.property_index
    }

    pub fn get_signature_index(&self) -> &LuaSignatureIndex {
        &self.signature_index
    }

    pub fn get_diagnostic_index(&self) -> &DiagnosticIndex {
        &self.diagnostic_index
    }

    pub fn get_operator_index(&self) -> &LuaOperatorIndex {
        &self.operator_index
    }

    pub fn get_flow_index(&self) -> &LuaFlowIndex {
        &self.flow_index
    }

    pub fn get_vfs(&self) -> &Vfs {
        &self.vfs
    }

    pub fn get_vfs_mut(&mut self) -> &mut Vfs {
        &mut self.vfs
    }

    pub fn get_file_dependencies_index(&self) -> &LuaDependencyIndex {
        &self.file_dependencies_index
    }

    pub fn get_file_dependencies_index_mut(&mut self) -> &mut LuaDependencyIndex {
        &mut self.file_dependencies_index
    }

    pub fn get_global_index(&self) -> &LuaGlobalIndex {
        &self.global_index
    }

    pub fn get_global_index_mut(&mut self) -> &mut LuaGlobalIndex {
        &mut self.global_index
    }

    pub fn get_json_schema_index(&self) -> &JsonSchemaIndex {
        &self.json_schema_index
    }

    pub fn get_json_schema_index_mut(&mut self) -> &mut JsonSchemaIndex {
        &mut self.json_schema_index
    }

    pub fn update_config(&mut self, config: Arc<Emmyrc>) {
        self.vfs.update_config(config.clone());
        self.modules_index.update_config(config.clone());
        self.emmyrc = config;
    }

    pub fn get_emmyrc(&self) -> &Emmyrc {
        &self.emmyrc
    }
}

impl LuaIndex for DbIndex {
    fn remove(&mut self, file_id: FileId) {
        self.decl_index.remove(file_id);
        self.references_index.remove(file_id);
        self.types_index.remove(file_id);
        self.modules_index.remove(file_id);
        self.members_index.remove(file_id);
        self.property_index.remove(file_id);
        self.signature_index.remove(file_id);
        self.diagnostic_index.remove(file_id);
        self.operator_index.remove(file_id);
        self.flow_index.remove(file_id);
        self.file_dependencies_index.remove(file_id);
        self.metatable_index.remove(file_id);
        self.global_index.remove(file_id);
        self.json_schema_index.remove(file_id);
    }

    fn clear(&mut self) {
        self.decl_index.clear();
        self.references_index.clear();
        self.types_index.clear();
        self.modules_index.clear();
        self.members_index.clear();
        self.property_index.clear();
        self.signature_index.clear();
        self.diagnostic_index.clear();
        self.operator_index.clear();
        self.flow_index.clear();
        self.file_dependencies_index.clear();
        self.metatable_index.clear();
        self.global_index.clear();
        self.json_schema_index.clear();
    }
}
