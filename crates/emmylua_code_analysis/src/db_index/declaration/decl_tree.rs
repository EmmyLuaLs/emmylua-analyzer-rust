use hashbrown::HashMap;

use super::{LuaDeclId, decl, scope};
use crate::{FileId, db_index::LuaMemberId};
use decl::LuaDecl;
use rowan::{TextRange, TextSize};
use scope::{LuaScope, LuaScopeId, LuaScopeKind, ScopeOrDeclId};

#[derive(Debug)]
pub struct LuaDeclarationTree {
    file_id: FileId,
    decls: HashMap<LuaDeclId, LuaDecl>,
    scopes: Vec<LuaScope>,
}

impl LuaDeclarationTree {
    pub fn new(file_id: FileId) -> Self {
        Self {
            file_id,
            decls: HashMap::new(),
            scopes: Vec::new(),
        }
    }

    pub fn file_id(&self) -> FileId {
        self.file_id
    }

    pub fn find_local_decl(&self, name: &str, position: TextSize) -> Option<&LuaDecl> {
        let scope = self.find_scope(position)?;
        let mut result: Option<&LuaDecl> = None;
        self.visit_visible_decls(scope, position, true, &mut |decl_id| {
            match decl_id {
                ScopeOrDeclId::Decl(decl_id) => {
                    let decl = match self.get_decl(&decl_id) {
                        Some(decl) => decl,
                        None => return false,
                    };
                    if decl.get_name() == name {
                        result = Some(decl);
                        return true;
                    }
                }
                ScopeOrDeclId::Scope(_) => {}
            }
            false
        });
        result
    }

    pub fn get_env_decls(&self, position: TextSize) -> Option<Vec<LuaDeclId>> {
        let scope = self.find_scope(position)?;
        let mut result = Vec::new();
        self.visit_visible_decls(scope, position, true, &mut |decl_id| {
            match decl_id {
                ScopeOrDeclId::Decl(decl_id) => {
                    if let Some(decl) = self.get_decl(&decl_id)
                        && !decl.is_implicit_self()
                    {
                        result.push(decl.get_id());
                    }
                }
                ScopeOrDeclId::Scope(_) => {}
            }
            false
        });
        Some(result)
    }

    fn find_scope(&self, position: TextSize) -> Option<&LuaScope> {
        if self.scopes.is_empty() {
            return None;
        }
        let mut scope = self.scopes.first()?;

        loop {
            let child_scope = scope
                .get_children()
                .iter()
                .filter_map(|child| match child {
                    ScopeOrDeclId::Scope(child_id) => self.scopes.get(child_id.id as usize),
                    ScopeOrDeclId::Decl(_) => None,
                })
                .find(|child_scope| child_scope.get_range().contains(position));
            if child_scope.is_none() {
                break;
            }
            scope = child_scope?;
        }

        Some(scope)
    }

    fn visit_visible_decls<F>(
        &self,
        scope: &LuaScope,
        position: TextSize,
        is_entry: bool,
        f: &mut F,
    ) where
        F: FnMut(ScopeOrDeclId) -> bool,
    {
        let search_scope = if is_entry {
            match scope.get_kind() {
                LuaScopeKind::LocalOrAssignStat => {
                    let cutoff = scope.get_position();
                    if let Some(parent_id) = scope.get_parent() {
                        if let Some(parent) = self.get_scope(&parent_id) {
                            self.visit_visible_decls(parent, cutoff, false, f);
                            return;
                        }
                    }
                    false
                }
                LuaScopeKind::Repeat => {
                    if let Some(ScopeOrDeclId::Scope(child_id)) = scope.get_children().first() {
                        if let Some(child) = self.get_scope(child_id) {
                            self.visit_visible_decls(child, position, true, f);
                            return;
                        }
                    }
                    false
                }
                LuaScopeKind::ForRange => {
                    if let Some(parent_id) = scope.get_parent() {
                        if let Some(parent) = self.get_scope(&parent_id) {
                            self.visit_visible_decls(parent, position, false, f);
                            return;
                        }
                    }
                    false
                }
                _ => true,
            }
        } else {
            if scope.get_kind() == LuaScopeKind::LocalOrAssignStat {
                let cutoff = scope.get_position();
                if let Some(parent_id) = scope.get_parent() {
                    if let Some(parent) = self.get_scope(&parent_id) {
                        self.visit_visible_decls(parent, cutoff, false, f);
                        return;
                    }
                }
                return;
            }
            if scope.get_kind() == LuaScopeKind::Repeat {
                if let Some(ScopeOrDeclId::Scope(child_id)) = scope.get_children().first() {
                    if let Some(body) = self.get_scope(child_id) {
                        if self.search_scope_children(body, position, f) {
                            return;
                        }
                    }
                }
            }
            true
        };

        if search_scope {
            if self.search_scope_children(scope, position, f) {
                return;
            }
        }

        if let Some(parent_id) = scope.get_parent() {
            if let Some(parent) = self.get_scope(&parent_id) {
                self.visit_visible_decls(parent, position, false, f);
            }
        }
    }

    fn search_scope_children<F>(&self, scope: &LuaScope, position: TextSize, f: &mut F) -> bool
    where
        F: FnMut(ScopeOrDeclId) -> bool,
    {
        let children = scope.get_children();

        // Find the rightmost child whose position is before `position`.
        let cut = children.iter().rposition(|child| match child {
            ScopeOrDeclId::Decl(decl_id) => decl_id.position < position,
            ScopeOrDeclId::Scope(scope_id) => self
                .get_scope(scope_id)
                .is_some_and(|s| s.get_position() < position),
        });

        let Some(cut) = cut else {
            return false;
        };

        // Walk children in reverse source order (closest first).
        for i in (0..=cut).rev() {
            match children.get(i) {
                Some(ScopeOrDeclId::Decl(decl_id)) => {
                    if f((*decl_id).into()) {
                        return true;
                    }
                }
                Some(ScopeOrDeclId::Scope(child_id)) => {
                    if let Some(child) = self.get_scope(child_id) {
                        if self.visit_child_scope(child, f) {
                            return true;
                        }
                    }
                }
                None => continue,
            }
        }

        false
    }

    fn visit_child_scope<F>(&self, scope: &LuaScope, f: &mut F) -> bool
    where
        F: FnMut(ScopeOrDeclId) -> bool,
    {
        match scope.get_kind() {
            LuaScopeKind::FuncStat | LuaScopeKind::MethodStat => {
                for child in scope.get_children() {
                    if let ScopeOrDeclId::Decl(decl_id) = child
                        && f(decl_id.into())
                    {
                        return true;
                    }
                }
                false
            }
            LuaScopeKind::LocalOrAssignStat => {
                for child in scope.get_children() {
                    if let ScopeOrDeclId::Decl(decl_id) = child
                        && f(decl_id.into())
                    {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    pub fn add_decl(&mut self, decl: LuaDecl) -> LuaDeclId {
        let decl_id = decl.get_id();
        self.decls.insert(decl_id, decl);
        decl_id
    }

    #[allow(unused)]
    pub fn get_decl_mut(&mut self, decl_id: LuaDeclId) -> Option<&mut LuaDecl> {
        self.decls.get_mut(&decl_id)
    }

    pub fn get_decl(&self, decl_id: &LuaDeclId) -> Option<&LuaDecl> {
        self.decls.get(decl_id)
    }

    pub fn create_scope(&mut self, range: TextRange, kind: LuaScopeKind) -> LuaScopeId {
        let id = self.scopes.len() as u32;
        let scope_id = LuaScopeId {
            file_id: self.file_id,
            id,
        };

        let scope = LuaScope::new(range, kind, scope_id);
        self.scopes.push(scope);
        scope_id
    }

    pub fn add_decl_to_scope(&mut self, scope_id: LuaScopeId, decl_id: LuaDeclId) {
        if let Some(scope) = self.scopes.get_mut(scope_id.id as usize) {
            scope.add_decl(decl_id);
        }
    }

    pub fn add_child_scope(&mut self, parent_id: LuaScopeId, child_id: LuaScopeId) {
        if let Some(parent) = self.scopes.get_mut(parent_id.id as usize) {
            parent.add_child(child_id);
        }
        if let Some(child) = self.scopes.get_mut(child_id.id as usize) {
            child.set_parent(Some(parent_id));
        }
    }

    pub fn get_root_scope(&self) -> Option<&LuaScope> {
        self.scopes.first()
    }

    pub fn get_scope(&self, scope_id: &LuaScopeId) -> Option<&LuaScope> {
        self.scopes.get(scope_id.id as usize)
    }

    pub fn get_decls(&self) -> &HashMap<LuaDeclId, LuaDecl> {
        &self.decls
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LuaDeclOrMemberId {
    Decl(LuaDeclId),
    Member(LuaMemberId),
}

impl LuaDeclOrMemberId {
    pub fn as_decl_id(&self) -> Option<LuaDeclId> {
        match self {
            LuaDeclOrMemberId::Decl(decl_id) => Some(*decl_id),
            LuaDeclOrMemberId::Member(_) => None,
        }
    }

    pub fn as_member_id(&self) -> Option<LuaMemberId> {
        match self {
            LuaDeclOrMemberId::Decl(_) => None,
            LuaDeclOrMemberId::Member(member_id) => Some(*member_id),
        }
    }

    pub fn get_position(&self) -> TextSize {
        match self {
            LuaDeclOrMemberId::Decl(decl_id) => decl_id.position,
            LuaDeclOrMemberId::Member(member_id) => member_id.get_position(),
        }
    }
}
