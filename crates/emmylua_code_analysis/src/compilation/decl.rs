use emmylua_parser::{LuaAstNode, LuaDocType};
use rowan::TextSize;
use std::sync::Arc;

use crate::{
    AsyncState, DbIndex, DocTypeInferContext, FileId, LuaDeclId, LuaFunctionType, LuaType,
    SalsaDeclId, SalsaDeclKindSummary, SalsaDeclSummary, SalsaDeclTreeSummary,
    SalsaDeclTypeInfoSummary, SalsaDocTypeRef, SalsaScopeChildSummary, SalsaScopeKindSummary,
    SalsaScopeSummary, SalsaSummaryHost, SalsaSyntaxIdSummary, VariadicType, infer_doc_type,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilationGenericParamInfo {
    pub name: String,
    pub constraint: Option<LuaType>,
    pub default_type: Option<LuaType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilationDeclInfo {
    pub file_id: FileId,
    pub decl_id: LuaDeclId,
    pub summary: SalsaDeclSummary,
    pub decl_type: Option<SalsaDeclTypeInfoSummary>,
}

fn project_decl_info(
    db: &DbIndex,
    file_id: FileId,
    summary: SalsaDeclSummary,
) -> CompilationDeclInfo {
    let decl_type = db.get_summary_db().types().decl(file_id, summary.id);

    CompilationDeclInfo {
        file_id,
        decl_id: LuaDeclId::new(file_id, summary.id.as_position()),
        summary,
        decl_type,
    }
}

pub fn find_compilation_decl_by_position(
    db: &DbIndex,
    file_id: FileId,
    position: TextSize,
) -> Option<CompilationDeclInfo> {
    let decl_tree = db.get_summary_db().file().decl_tree(file_id)?;
    let summary = decl_tree
        .decls
        .iter()
        .find(|decl| decl.id.as_position() == position)?
        .clone();

    Some(project_decl_info(db, file_id, summary))
}

pub fn find_compilation_decl_by_syntax_id(
    db: &DbIndex,
    file_id: FileId,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<CompilationDeclInfo> {
    let summary = db
        .get_summary_db()
        .file()
        .decl_by_syntax_id(file_id, syntax_id)?;
    Some(project_decl_info(db, file_id, summary))
}

pub fn find_compilation_param_generic_params(
    db: &DbIndex,
    decl: &CompilationDeclInfo,
) -> Option<Vec<CompilationGenericParamInfo>> {
    let signature_offset = match decl.summary.kind {
        SalsaDeclKindSummary::Param {
            signature_offset, ..
        } => signature_offset,
        _ => return None,
    };

    let signature = db
        .get_summary_db()
        .semantic()
        .target()
        .signature_explain(decl.file_id, signature_offset)?;

    Some(
        signature
            .generics
            .iter()
            .flat_map(|generic| generic.params.iter())
            .map(|param| CompilationGenericParamInfo {
                name: param.name.to_string(),
                constraint: infer_compilation_doc_type_ref(
                    db,
                    decl.file_id,
                    param.bound_type.as_ref().map(|info| &info.type_ref),
                ),
                default_type: infer_compilation_doc_type_ref(
                    db,
                    decl.file_id,
                    param.default_type.as_ref().map(|info| &info.type_ref),
                ),
            })
            .collect(),
    )
}

pub fn build_compilation_signature_doc_function(
    db: &DbIndex,
    file_id: FileId,
    signature_offset: TextSize,
) -> Option<Arc<LuaFunctionType>> {
    let signature = db
        .get_summary_db()
        .semantic()
        .target()
        .signature_explain(file_id, signature_offset)?;

    let params = signature
        .params
        .iter()
        .map(|param| {
            (
                param.name.to_string(),
                infer_compilation_doc_type_ref(
                    db,
                    file_id,
                    param.doc_type.as_ref().map(|info| &info.type_ref),
                ),
            )
        })
        .collect();
    let return_type = compilation_return_type(
        signature
            .returns
            .iter()
            .flat_map(|return_info| return_info.items.iter())
            .filter_map(|item| {
                infer_compilation_doc_type_ref(db, file_id, Some(&item.doc_type.type_ref))
            })
            .collect(),
    );
    let is_variadic = signature
        .signature
        .params
        .iter()
        .any(|param| param.is_vararg);

    Some(Arc::new(LuaFunctionType::new(
        AsyncState::None,
        signature.signature.is_method,
        is_variadic,
        params,
        return_type,
    )))
}

fn compilation_return_type(mut row: Vec<LuaType>) -> LuaType {
    match row.len() {
        0 => LuaType::Nil,
        1 => row.pop().unwrap_or(LuaType::Nil),
        _ => LuaType::Variadic(VariadicType::Multi(row).into()),
    }
}

fn infer_compilation_doc_type_ref(
    db: &DbIndex,
    file_id: FileId,
    type_ref: Option<&SalsaDocTypeRef>,
) -> Option<LuaType> {
    let type_key = match type_ref? {
        SalsaDocTypeRef::Node(type_key) => *type_key,
        SalsaDocTypeRef::Incomplete => return None,
    };
    let resolved = db
        .get_summary_db()
        .doc()
        .resolved_type_by_key(file_id, type_key)?;
    let syntax_tree = db.get_vfs().get_syntax_tree(&file_id)?;
    let root = syntax_tree.get_red_root();
    let doc_type = LuaDocType::cast(
        resolved
            .doc_type
            .syntax_id
            .to_lua_syntax_id()
            .to_node_from_root(&root)?,
    )?;

    Some(infer_doc_type(
        DocTypeInferContext::new(db, file_id),
        &doc_type,
    ))
}

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

#[derive(Debug, Clone, Copy)]
pub struct CompilationDeclIndex<'a> {
    summary: &'a SalsaSummaryHost,
}

impl<'a> CompilationDeclIndex<'a> {
    pub fn new(summary: &'a SalsaSummaryHost) -> Self {
        Self { summary }
    }

    pub fn get_decl_tree(&self, file_id: FileId) -> Option<CompilationDeclTree> {
        Some(CompilationDeclTree::new(
            self.summary.file().decl_tree(file_id)?,
        ))
    }

    pub fn get_decl(&self, decl_id: &SalsaDeclId, file_id: FileId) -> Option<SalsaDeclSummary> {
        self.get_decl_tree(file_id)?.get_decl(decl_id).cloned()
    }
}
