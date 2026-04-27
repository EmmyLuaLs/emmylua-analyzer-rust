use smol_str::SmolStr;

use crate::{
    FileId, SalsaDocGenericParamSummary, SalsaDocTypeDefKindSummary, SalsaDocTypeNodeKey,
    WorkspaceId,
    db_index::{LuaTypeDeclId, LuaTypeIdentifier},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompilationTypeDeclScope {
    Global,
    Internal(WorkspaceId),
    Local(FileId),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompilationTypeDeclId {
    pub scope: CompilationTypeDeclScope,
    pub full_name: SmolStr,
}

impl CompilationTypeDeclId {
    pub fn global(name: impl Into<SmolStr>) -> Self {
        Self {
            scope: CompilationTypeDeclScope::Global,
            full_name: name.into(),
        }
    }

    pub fn internal(workspace_id: WorkspaceId, name: impl Into<SmolStr>) -> Self {
        Self {
            scope: CompilationTypeDeclScope::Internal(workspace_id),
            full_name: name.into(),
        }
    }

    pub fn local(file_id: FileId, name: impl Into<SmolStr>) -> Self {
        Self {
            scope: CompilationTypeDeclScope::Local(file_id),
            full_name: name.into(),
        }
    }

    pub fn name(&self) -> &str {
        self.full_name.as_str()
    }

    pub fn simple_name(&self) -> &str {
        self.name().rsplit('.').next().unwrap_or(self.name())
    }
}

impl From<&LuaTypeDeclId> for CompilationTypeDeclId {
    fn from(value: &LuaTypeDeclId) -> Self {
        match value.get_id() {
            LuaTypeIdentifier::Global(name) => Self::global(name.clone()),
            LuaTypeIdentifier::Internal(workspace_id, name) => {
                Self::internal(*workspace_id, name.clone())
            }
            LuaTypeIdentifier::Local(file_id, name) => Self::local(*file_id, name.clone()),
        }
    }
}

impl From<&CompilationTypeDeclId> for LuaTypeDeclId {
    fn from(value: &CompilationTypeDeclId) -> Self {
        match value.scope {
            CompilationTypeDeclScope::Global => LuaTypeDeclId::global(value.name()),
            CompilationTypeDeclScope::Internal(workspace_id) => {
                LuaTypeDeclId::internal(workspace_id, value.name())
            }
            CompilationTypeDeclScope::Local(file_id) => LuaTypeDeclId::local(file_id, value.name()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilationTypeDecl {
    pub file_id: FileId,
    pub id: CompilationTypeDeclId,
    pub simple_name: SmolStr,
    pub kind: SalsaDocTypeDefKindSummary,
    pub syntax_offset: u32,
    pub generic_params: Vec<SalsaDocGenericParamSummary>,
    pub super_type_offsets: Vec<SalsaDocTypeNodeKey>,
    pub super_type_names: Vec<SmolStr>,
    pub value_type_offset: Option<SalsaDocTypeNodeKey>,
}

impl CompilationTypeDecl {
    pub fn namespace(&self) -> Option<&str> {
        self.id
            .name()
            .rfind('.')
            .map(|index| &self.id.name()[..index])
    }

    pub fn full_name(&self) -> &str {
        self.id.name()
    }
}
