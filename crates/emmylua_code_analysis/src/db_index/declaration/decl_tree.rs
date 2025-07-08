use std::collections::HashMap;

use super::{decl, scope, LuaDeclId};
use crate::{
    db_index::LuaMemberId, infer_node_semantic_decl, DbIndex, FileId, LuaInferCache,
    LuaSemanticDeclId, SemanticDeclLevel,
};
use decl::LuaDecl;
use emmylua_parser::{
    LuaAstNode, LuaChunk, LuaExpr, LuaFuncStat, LuaNameExpr, LuaSyntaxId, LuaSyntaxKind, LuaVarExpr,
};
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
        self.walk_up(scope, position, 0, &mut |decl_id| {
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
                ScopeOrDeclId::Scope(scope) => {
                    let scope = match self.scopes.get(scope.id as usize) {
                        Some(scope) => scope,
                        None => return false,
                    };

                    // self found method stat, donot return decl id
                    if scope.get_kind() == LuaScopeKind::MethodStat && name == "self" {
                        return true;
                    }
                }
            }

            false
        });
        result
    }

    pub fn get_env_decls(&self, position: TextSize) -> Option<Vec<LuaDeclId>> {
        let scope = self.find_scope(position)?;
        let mut result = Vec::new();
        self.walk_up(scope, position, 0, &mut |decl_id| {
            match decl_id {
                ScopeOrDeclId::Decl(decl_id) => {
                    result.push(decl_id);
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
        let mut scope = match self.scopes.get(0) {
            Some(scope) => scope,
            None => return None,
        };

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

    fn base_walk_up<F>(&self, scope: &LuaScope, start_pos: TextSize, level: usize, f: &mut F)
    where
        F: FnMut(ScopeOrDeclId) -> bool,
    {
        let cur_index = scope.get_children().iter().rposition(|child| match child {
            ScopeOrDeclId::Decl(decl_id) => decl_id.position < start_pos,
            ScopeOrDeclId::Scope(scope_id) => {
                let child_scope = match self.scopes.get(scope_id.id as usize) {
                    Some(scope) => scope,
                    None => return false,
                };
                child_scope.get_position() < start_pos
            }
        });

        if let Some(cur_index) = cur_index {
            for i in (0..=cur_index).rev() {
                let scope_or_id = match scope.get_children().get(i) {
                    Some(scope_or_id) => scope_or_id,
                    None => continue,
                };

                match scope_or_id {
                    ScopeOrDeclId::Decl(decl_id) => {
                        if f(decl_id.into()) {
                            return;
                        }
                    }
                    ScopeOrDeclId::Scope(scope_id) => {
                        let child_scope = match self.scopes.get(scope_id.id as usize) {
                            Some(scope) => scope,
                            None => continue,
                        };

                        if self.walk_over_scope(child_scope, f) {
                            return;
                        }
                    }
                }
            }
        }

        if let Some(parent_id) = scope.get_parent() {
            let parent_scope = match self.scopes.get(parent_id.id as usize) {
                Some(scope) => scope,
                None => return,
            };
            self.walk_up(parent_scope, start_pos, level + 1, f);
        }
    }

    /// Walks up the scope tree and calls `f` for each declaration.
    fn walk_up<F>(&self, scope: &LuaScope, start_pos: TextSize, level: usize, f: &mut F)
    where
        F: FnMut(ScopeOrDeclId) -> bool,
    {
        match scope.get_kind() {
            LuaScopeKind::LocalOrAssignStat => {
                let parent = scope.get_parent();
                if let Some(parent) = parent {
                    let parent_scope = match self.scopes.get(parent.id as usize) {
                        Some(scope) => scope,
                        None => return,
                    };
                    self.walk_up(parent_scope, scope.get_position(), level, f);
                }
            }
            LuaScopeKind::Repeat => {
                if level == 0 {
                    if let Some(ScopeOrDeclId::Scope(scope_id)) = scope.get_children().get(0) {
                        let scope = match self.scopes.get(scope_id.id as usize) {
                            Some(scope) => scope,
                            None => return,
                        };
                        self.walk_up(scope, start_pos, level, f);
                        return;
                    }
                }

                self.base_walk_up(scope, start_pos, level, f);
            }
            LuaScopeKind::ForRange => {
                if level == 0 {
                    let parent = scope.get_parent();
                    if let Some(parent) = parent {
                        let parent_scope = match self.scopes.get(parent.id as usize) {
                            Some(scope) => scope,
                            None => return,
                        };
                        self.walk_up(parent_scope, start_pos, level, f);
                    }
                } else {
                    self.base_walk_up(scope, start_pos, level, f);
                }
            }
            _ => {
                self.base_walk_up(scope, start_pos, level, f);
            }
        }
    }

    fn walk_over_scope<F>(&self, scope: &LuaScope, f: &mut F) -> bool
    where
        F: FnMut(ScopeOrDeclId) -> bool,
    {
        match scope.get_kind() {
            LuaScopeKind::LocalOrAssignStat | LuaScopeKind::FuncStat | LuaScopeKind::MethodStat => {
                for child in scope.get_children() {
                    if let ScopeOrDeclId::Decl(decl_id) = child {
                        if f(decl_id.into()) {
                            return true;
                        }
                    }
                }
            }
            _ => {}
        }

        f(scope.get_id().into())
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
        self.decls.get(&decl_id)
    }

    pub fn create_scope(&mut self, range: TextRange, kind: LuaScopeKind) -> LuaScopeId {
        let id = self.scopes.len() as u32;
        let scope_id = LuaScopeId {
            file_id: self.file_id,
            id,
        };

        let scope = LuaScope::new(range, kind, scope_id.clone());
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

    pub fn find_self_decl(
        &self,
        db: &DbIndex,
        cache: &mut LuaInferCache,
        name_expr: LuaNameExpr,
    ) -> Option<LuaDeclOrMemberId> {
        let position = name_expr.get_position();
        let scope = self.find_scope(position)?;
        let mut result: Option<LuaDeclOrMemberId> = None;
        let root = name_expr.ancestors::<LuaChunk>().next()?;

        self.walk_up(scope, position, 0, &mut |decl_id| {
            match decl_id {
                ScopeOrDeclId::Decl(decl_id) => {
                    let decl = match self.get_decl(&decl_id) {
                        Some(decl) => decl,
                        None => return false,
                    };
                    if decl.get_name() == "self" {
                        result = Some(LuaDeclOrMemberId::Decl(decl_id));
                        return true;
                    }
                }
                ScopeOrDeclId::Scope(scope_id) => {
                    if let Some(scope) = self.scopes.get(scope_id.id as usize) {
                        if scope.get_kind() == LuaScopeKind::MethodStat {
                            let range = scope.get_range();
                            let syntax_id = LuaSyntaxId::new(LuaSyntaxKind::FuncStat.into(), range);
                            let id = self.find_self_decl_id(&db, cache, &root, syntax_id);
                            if id.is_some() {
                                result = id;
                                return true;
                            }
                        }
                    }
                }
            }

            false
        });
        result
    }

    fn find_self_decl_id(
        &self,
        db: &DbIndex,
        cache: &mut LuaInferCache,
        root: &LuaChunk,
        syntax_id: LuaSyntaxId,
    ) -> Option<LuaDeclOrMemberId> {
        let node = syntax_id.to_node_from_root(&root.syntax())?;
        let func_stat = LuaFuncStat::cast(node)?;
        let func_name = func_stat.get_func_name()?;

        if let LuaVarExpr::IndexExpr(index_name) = func_name {
            let prefix = index_name.get_prefix_expr()?;
            match prefix {
                LuaExpr::NameExpr(prefix_name) => {
                    let name = prefix_name.get_name_text()?;
                    let decl = self.find_local_decl(&name, prefix_name.get_position());
                    if let Some(decl) = decl {
                        return Some(LuaDeclOrMemberId::Decl(decl.get_id()));
                    }

                    let id = db.get_global_index().resolve_global_decl_id(db, &name)?;
                    return Some(LuaDeclOrMemberId::Decl(id));
                }
                LuaExpr::IndexExpr(prefix_index) => {
                    let semantic_id = infer_node_semantic_decl(
                        db,
                        cache,
                        prefix_index.syntax().clone(),
                        SemanticDeclLevel::NoTrace,
                    )?;

                    match semantic_id {
                        LuaSemanticDeclId::Member(member_id) => {
                            return Some(LuaDeclOrMemberId::Member(member_id));
                        }
                        LuaSemanticDeclId::LuaDecl(decl_id) => {
                            return Some(LuaDeclOrMemberId::Decl(decl_id));
                        }
                        _ => {}
                    }
                }
                _ => return None,
            }
        }

        None
    }

    pub fn get_root_scope(&self) -> Option<&LuaScope> {
        self.scopes.get(0)
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
