use hashbrown::HashMap;
use rowan::TextSize;
use std::sync::Arc;

use crate::{
    FileId, SalsaDeclId, SalsaDeclKindSummary, SalsaDeclSummary, SalsaDeclTreeSummary,
    SalsaScopeChildSummary, SalsaScopeKindSummary, SalsaScopeSummary,
};

use super::{CompilationIndexContext, FileBackedIndex};

#[derive(Debug, Clone)]
pub struct CompilationDeclTree {
    summary: Arc<SalsaDeclTreeSummary>,
}

impl CompilationDeclTree {
    pub fn new(summary: Arc<SalsaDeclTreeSummary>) -> Self {
        Self { summary }
    }

    pub fn file_id(&self) -> FileId {
        self.summary.file_id
    }

    pub fn summary(&self) -> &SalsaDeclTreeSummary {
        self.summary.as_ref()
    }

    pub fn get_decl(&self, decl_id: &SalsaDeclId) -> Option<&SalsaDeclSummary> {
        self.summary.decls.iter().find(|decl| &decl.id == decl_id)
    }

    pub fn get_decls(&self) -> &[SalsaDeclSummary] {
        &self.summary.decls
    }

    pub fn get_scope(&self, scope_id: u32) -> Option<&SalsaScopeSummary> {
        self.summary.scopes.get(scope_id as usize)
    }

    pub fn get_scopes(&self) -> &[SalsaScopeSummary] {
        &self.summary.scopes
    }

    pub fn find_local_decl(&self, name: &str, position: TextSize) -> Option<&SalsaDeclSummary> {
        let scope = self.find_scope(position)?;
        let mut result = None;
        self.walk_up(scope, position, 0, &mut |decl_id| {
            let Some(decl) = self.get_decl(&decl_id) else {
                return false;
            };
            if decl.name == name {
                result = Some(decl);
                return true;
            }
            false
        });
        result
    }

    pub fn get_env_decls(&self, position: TextSize) -> Option<Vec<SalsaDeclId>> {
        let scope = self.find_scope(position)?;
        let mut result = Vec::new();
        self.walk_up(scope, position, 0, &mut |decl_id| {
            if let Some(decl) = self.get_decl(&decl_id)
                && !matches!(decl.kind, SalsaDeclKindSummary::ImplicitSelf)
            {
                result.push(decl.id);
            }
            false
        });
        Some(result)
    }

    fn find_scope(&self, position: TextSize) -> Option<&SalsaScopeSummary> {
        if self.summary.scopes.is_empty() {
            return None;
        }

        let mut scope = self.summary.scopes.first()?;
        loop {
            let child_scope = scope
                .children
                .iter()
                .filter_map(|child| match child {
                    SalsaScopeChildSummary::Scope(child_id) => self.get_scope(*child_id),
                    SalsaScopeChildSummary::Decl(_) => None,
                })
                .find(|child_scope| {
                    child_scope.start_offset <= position && position <= child_scope.end_offset
                });
            let Some(child_scope) = child_scope else {
                break;
            };
            scope = child_scope;
        }

        Some(scope)
    }

    fn walk_up<F>(&self, scope: &SalsaScopeSummary, start_pos: TextSize, level: usize, f: &mut F)
    where
        F: FnMut(SalsaDeclId) -> bool,
    {
        match scope.kind {
            SalsaScopeKindSummary::LocalOrAssignStat | SalsaScopeKindSummary::ForRange
                if level == 0 =>
            {
                if let Some(parent_id) = scope.parent
                    && let Some(parent_scope) = self.get_scope(parent_id)
                {
                    self.walk_up(parent_scope, scope.start_offset, level, f);
                }
            }
            SalsaScopeKindSummary::Repeat if level == 0 => {
                if let Some(SalsaScopeChildSummary::Scope(scope_id)) = scope.children.first()
                    && let Some(child_scope) = self.get_scope(*scope_id)
                {
                    self.walk_up(child_scope, start_pos, level, f);
                    return;
                }
                self.base_walk_up(scope, start_pos, level, f);
            }
            _ => self.base_walk_up(scope, start_pos, level, f),
        }
    }

    fn base_walk_up<F>(
        &self,
        scope: &SalsaScopeSummary,
        start_pos: TextSize,
        level: usize,
        f: &mut F,
    ) where
        F: FnMut(SalsaDeclId) -> bool,
    {
        let cur_index = scope.children.iter().rposition(|child| match child {
            SalsaScopeChildSummary::Decl(decl_id) => self
                .get_decl(&decl_id)
                .is_some_and(|decl| decl.start_offset < start_pos),
            SalsaScopeChildSummary::Scope(scope_id) => self
                .get_scope(*scope_id)
                .is_some_and(|child_scope| child_scope.start_offset < start_pos),
        });

        if let Some(cur_index) = cur_index {
            for index in (0..=cur_index).rev() {
                let Some(child) = scope.children.get(index) else {
                    continue;
                };

                match child {
                    SalsaScopeChildSummary::Decl(decl_id) => {
                        if f(*decl_id) {
                            return;
                        }
                    }
                    SalsaScopeChildSummary::Scope(scope_id) => {
                        let Some(child_scope) = self.get_scope(*scope_id) else {
                            continue;
                        };
                        if self.walk_over_scope(child_scope, f) {
                            return;
                        }
                    }
                }
            }
        }

        if let Some(parent_id) = scope.parent
            && let Some(parent_scope) = self.get_scope(parent_id)
        {
            self.walk_up(parent_scope, start_pos, level + 1, f);
        }
    }

    fn walk_over_scope<F>(&self, scope: &SalsaScopeSummary, f: &mut F) -> bool
    where
        F: FnMut(SalsaDeclId) -> bool,
    {
        match scope.kind {
            SalsaScopeKindSummary::LocalOrAssignStat
            | SalsaScopeKindSummary::FuncStat
            | SalsaScopeKindSummary::MethodStat => {
                for child in &scope.children {
                    if let SalsaScopeChildSummary::Decl(decl_id) = child
                        && f(*decl_id)
                    {
                        return true;
                    }
                }
            }
            _ => {}
        }

        false
    }
}

#[derive(Debug, Default)]
pub struct CompilationDeclIndex {
    decl_trees: HashMap<FileId, CompilationDeclTree>,
}

impl CompilationDeclIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_decl_tree(&self, file_id: FileId) -> Option<&CompilationDeclTree> {
        self.decl_trees.get(&file_id)
    }

    pub fn get_decl(&self, decl_id: &SalsaDeclId, file_id: FileId) -> Option<&SalsaDeclSummary> {
        self.get_decl_tree(file_id)?.get_decl(decl_id)
    }

    pub fn remove(&mut self, file_id: FileId) {
        self.decl_trees.remove(&file_id);
    }

    pub fn clear(&mut self) {
        self.decl_trees.clear();
    }

    fn rebuild_file(&mut self, summary: &crate::SalsaSummaryHost, file_id: FileId) {
        let Some(decl_tree) = summary.file().decl_tree(file_id) else {
            return;
        };

        self.decl_trees
            .insert(file_id, CompilationDeclTree::new(decl_tree));
    }
}

impl FileBackedIndex for CompilationDeclIndex {
    fn remove_file(&mut self, file_id: FileId) {
        CompilationDeclIndex::remove(self, file_id);
    }

    fn rebuild_file(&mut self, ctx: &CompilationIndexContext<'_>, file_id: FileId) {
        CompilationDeclIndex::rebuild_file(self, ctx.summary, file_id);
    }

    fn clear(&mut self) {
        CompilationDeclIndex::clear(self);
    }
}
