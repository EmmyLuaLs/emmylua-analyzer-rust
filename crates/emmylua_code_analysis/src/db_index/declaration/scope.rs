use rowan::{TextRange, TextSize};

use crate::FileId;

use super::decl::LuaDeclId;

#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy)]
pub enum LuaScopeKind {
    Normal,
    Repeat,
    LocalOrAssignStat,
    ForRange,
    FuncStat,
    // defined in function xxx:aaa() end
    MethodStat,
}

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub struct LuaScope {
    parent: Option<LuaScopeId>,
    children: Vec<ScopeOrDeclId>,
    range: TextRange,
    kind: LuaScopeKind,
    id: LuaScopeId,
}

impl LuaScope {
    pub fn new(range: TextRange, kind: LuaScopeKind, id: LuaScopeId) -> Self {
        Self {
            parent: None,
            children: Vec::new(),
            range,
            kind,
            id,
        }
    }

    pub fn add_decl(&mut self, decl: LuaDeclId) {
        self.children.push(ScopeOrDeclId::Decl(decl));
    }

    pub fn add_child(&mut self, child: LuaScopeId) {
        self.children.push(ScopeOrDeclId::Scope(child));
    }

    pub fn get_parent(&self) -> Option<LuaScopeId> {
        self.parent
    }

    pub(crate) fn set_parent(&mut self, parent: Option<LuaScopeId>) {
        self.parent = parent;
    }

    pub fn get_children(&self) -> &[ScopeOrDeclId] {
        &self.children
    }

    pub fn get_range(&self) -> TextRange {
        self.range
    }

    pub fn get_kind(&self) -> LuaScopeKind {
        self.kind
    }

    pub fn get_position(&self) -> TextSize {
        self.range.start()
    }

    pub fn get_id(&self) -> LuaScopeId {
        self.id
    }
}

#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy)]
pub struct LuaScopeId {
    pub file_id: FileId,
    pub id: u32,
}

#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy)]
pub enum ScopeOrDeclId {
    Scope(LuaScopeId),
    Decl(LuaDeclId),
}

impl From<LuaDeclId> for ScopeOrDeclId {
    fn from(decl_id: LuaDeclId) -> Self {
        Self::Decl(decl_id)
    }
}

impl From<LuaScopeId> for ScopeOrDeclId {
    fn from(scope_id: LuaScopeId) -> Self {
        Self::Scope(scope_id)
    }
}

impl From<&LuaDeclId> for ScopeOrDeclId {
    fn from(decl_id: &LuaDeclId) -> Self {
        Self::Decl(*decl_id)
    }
}

impl From<&LuaScopeId> for ScopeOrDeclId {
    fn from(scope_id: &LuaScopeId) -> Self {
        Self::Scope(*scope_id)
    }
}
