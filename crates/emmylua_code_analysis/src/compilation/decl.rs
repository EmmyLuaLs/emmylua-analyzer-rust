use emmylua_parser::{LuaAstNode, LuaDocType, LuaExpr};
use rowan::TextSize;
use smol_str::SmolStr;
use std::sync::Arc;

use crate::{
    AsyncState, DbIndex, DocTypeInferContext, FileId, LuaDeclId, LuaFunctionType, LuaInferCache,
    LuaGenericType, LuaMemberKey, LuaType, SalsaDeclId, SalsaDeclKindSummary, SalsaDeclSummary,
    SalsaDeclTreeSummary, SalsaDeclTypeInfoSummary, SalsaDocTagFieldKeySummary,
    SalsaDocTypeDefKindSummary,
    SalsaDocTypeNodeKey, SalsaDocTypeRef, SalsaDocVisibilityKindSummary,
    SalsaPropertyKeySummary, SalsaScopeChildSummary, SalsaScopeKindSummary, SalsaScopeSummary,
    SalsaSummaryHost, SalsaSyntaxIdSummary, TypeOps, TypeSubstitutor, VariadicType,
    WorkspaceId, infer_doc_type, infer_expr, instantiate_type_generic,
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
            .flat_map(|generic| {
                generic.params.iter().map(move |param| CompilationGenericParamInfo {
                    name: param.name.to_string(),
                    constraint: infer_compilation_doc_type_ref_with_owner(
                        db,
                        decl.file_id,
                        generic.syntax_offset,
                        param.bound_type.as_ref().map(|info| &info.type_ref),
                    ),
                    default_type: infer_compilation_doc_type_ref_with_owner(
                        db,
                        decl.file_id,
                        generic.syntax_offset,
                        param.default_type.as_ref().map(|info| &info.type_ref),
                    ),
                })
            })
            .collect(),
    )
}

pub fn find_compilation_type_generic_params(
    db: &DbIndex,
    type_decl_id: &crate::LuaTypeDeclId,
) -> Option<Vec<CompilationGenericParamInfo>> {
    let type_name = type_decl_id.get_name();
    let index = db.get_type_def_reverse_index();
    let defs = index.by_name.get(&SmolStr::new(type_name))?;
    for (file_id, type_def) in defs {
        let workspace_id = db.resolve_workspace_id(*file_id).unwrap_or(WorkspaceId::MAIN);
        let projected_type_id = match type_def.visibility {
            SalsaDocVisibilityKindSummary::Private => crate::LuaTypeDeclId::local(*file_id, type_name),
            SalsaDocVisibilityKindSummary::Internal | SalsaDocVisibilityKindSummary::Package => {
                crate::LuaTypeDeclId::internal(workspace_id, type_name)
            }
            SalsaDocVisibilityKindSummary::Public | SalsaDocVisibilityKindSummary::Protected => {
                crate::LuaTypeDeclId::global(type_name)
            }
        };
        if projected_type_id != *type_decl_id {
            continue;
        }

        return Some(
            type_def
                .generic_params
                .iter()
                .map(|param| CompilationGenericParamInfo {
                    name: param.name.to_string(),
                    constraint: param.type_offset.and_then(|type_offset| infer_compilation_doc_type_key_with_owner(
                        db,
                        *file_id,
                        Some(type_def.syntax_offset),
                        type_offset,
                    )),
                    default_type: param.default_type_offset.and_then(|type_offset| infer_compilation_doc_type_key_with_owner(
                        db,
                        *file_id,
                        Some(type_def.syntax_offset),
                        type_offset,
                    )),
                })
                .collect(),
        );
    }

    None
}

pub fn infer_compilation_type_alias_origin(
    db: &DbIndex,
    type_decl_id: &crate::LuaTypeDeclId,
    substitutor: Option<&TypeSubstitutor>,
) -> Option<LuaType> {
    let type_name = type_decl_id.get_name();
    let index = db.get_type_def_reverse_index();
    let defs = index.by_name.get(&SmolStr::new(type_name))?;
    for (file_id, type_def) in defs {
        if type_def.kind != SalsaDocTypeDefKindSummary::Alias {
            continue;
        }

        let workspace_id = db.resolve_workspace_id(*file_id).unwrap_or(WorkspaceId::MAIN);
        let projected_type_id = match type_def.visibility {
            SalsaDocVisibilityKindSummary::Private => crate::LuaTypeDeclId::local(*file_id, type_name),
            SalsaDocVisibilityKindSummary::Internal | SalsaDocVisibilityKindSummary::Package => {
                crate::LuaTypeDeclId::internal(workspace_id, type_name)
            }
            SalsaDocVisibilityKindSummary::Public | SalsaDocVisibilityKindSummary::Protected => {
                crate::LuaTypeDeclId::global(type_name)
            }
        };
        if projected_type_id != *type_decl_id {
            continue;
        }

        let value_type_offset = type_def.value_type_offset?;
        let mut origin = infer_compilation_doc_type_key_with_owner(
            db,
            *file_id,
            Some(type_def.syntax_offset),
            value_type_offset,
        )?;

        if let Some(substitutor) = substitutor
            && !type_def.generic_params.is_empty()
        {
            origin = instantiate_type_generic(db, &origin, substitutor);
        }

        return Some(origin);
    }

    None
}

pub fn build_compilation_signature_doc_function(
    db: &DbIndex,
    file_id: FileId,
    signature_offset: TextSize,
) -> Option<Arc<LuaFunctionType>> {
    let signature = db
        .get_summary_db()
        .doc()
        .signature()
        .explain(file_id, signature_offset)?;
    let signature_owner_offset = signature_generic_owner_offset(&signature)
        .unwrap_or(signature.signature.syntax_offset);

    let params = signature
        .params
        .iter()
        .map(|param| {
            (
                param.name.to_string(),
                infer_compilation_doc_type_ref_with_owner(
                    db,
                    file_id,
                    signature_owner_offset,
                    param.doc_type.as_ref().map(|info| &info.type_ref),
                ),
            )
        })
        .collect();
    let return_type = compilation_return_type(
        signature
            .returns
            .iter()
            .flat_map(|return_info| {
                return_info.items.iter().filter_map(move |item| {
                    infer_compilation_doc_type_ref_with_owner(
                        db,
                        file_id,
                        signature_owner_offset,
                        Some(&item.doc_type.type_ref),
                    )
                })
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

pub fn infer_compilation_decl_type(db: &DbIndex, decl: &CompilationDeclInfo) -> Option<LuaType> {
    if let Some(summary) = db
        .get_summary_db()
        .semantic()
        .file()
        .decl_summary(decl.file_id, decl.summary.id)
    {
        if let Some(decl_type) =
            infer_compilation_decl_type_info(db, decl.file_id, &summary.decl_type)
        {
            return Some(decl_type);
        }

        if let Some(value_shell_type) = infer_compilation_doc_type_keys(
            db,
            decl.file_id,
            &summary.value_shell.candidate_type_offsets,
        ) {
            return Some(value_shell_type);
        }
    }

    decl.decl_type
        .as_ref()
        .and_then(|decl_type| infer_compilation_decl_type_info(db, decl.file_id, decl_type))
}

pub(crate) fn decl_initializer_type(
    db: &DbIndex,
    decl: &CompilationDeclInfo,
) -> Option<LuaType> {
    let expr_syntax_id = decl.decl_type.as_ref()?.value_expr_syntax_id?;
    let syntax_tree = db.get_vfs().get_syntax_tree(&decl.file_id)?;
    let root = syntax_tree.get_red_root();
    let expr = LuaExpr::cast(expr_syntax_id.to_lua_syntax_id().to_node_from_root(&root)?)?;
    let mut cache = LuaInferCache::new(decl.file_id, Default::default());

    infer_expr(db, &mut cache, expr).ok()
}

pub(crate) fn decl_value_shell_type(db: &DbIndex, decl: &CompilationDeclInfo) -> Option<LuaType> {
    let value_shell = db
        .get_summary_db()
        .semantic()
        .file()
        .decl_value_shell(decl.file_id, decl.summary.id)?;

    infer_compilation_doc_type_keys(db, decl.file_id, &value_shell.candidate_type_offsets)
}

pub fn infer_compilation_type_property_type(
    db: &DbIndex,
    type_decl_id: &crate::LuaTypeDeclId,
    key: &LuaMemberKey,
) -> Option<LuaType> {
    let type_name = type_decl_id.get_name();
    let index = db.get_type_def_reverse_index();
    let mut resolved = Vec::new();
    let defs = index.by_name.get(&SmolStr::new(type_name));
    if let Some(defs) = defs {
        for (file_id, type_def) in defs {
            let owner_offset = Some(type_def.syntax_offset);
            let properties = db
                .get_summary_db()
                .file()
                .properties_for_type(*file_id, type_name.into())
                .unwrap_or_default();

            for property in properties {
                let key_matches = match (&property.key, key) {
                    (SalsaPropertyKeySummary::Name(field_name), LuaMemberKey::Name(name)) => {
                        field_name == name
                    }
                    (SalsaPropertyKeySummary::Integer(field_index), LuaMemberKey::Integer(index)) => {
                        field_index == index
                    }
                    _ => false,
                };
                if !key_matches {
                    continue;
                }

                let Some(doc_type_offset) = property.doc_type_offset else {
                    continue;
                };

                let Some(mut typ) = infer_compilation_doc_type_key_with_owner(
                    db,
                    *file_id,
                    owner_offset,
                    doc_type_offset,
                ) else {
                    continue;
                };
                if property.is_nullable && !typ.is_nullable() {
                    typ = TypeOps::Union.apply(db, &typ, &LuaType::Nil);
                }
                resolved.push(typ);
            }

            if resolved.is_empty()
                && let Some(doc_summary) = db.get_summary_db().doc().summary(*file_id)
            {
                for field in &doc_summary.fields {
                    if field.owner != type_def.owner {
                        continue;
                    }

                    let key_matches = match (&field.key, key) {
                        (Some(SalsaDocTagFieldKeySummary::Name(field_name)), LuaMemberKey::Name(name))
                        | (Some(SalsaDocTagFieldKeySummary::String(field_name)), LuaMemberKey::Name(name)) => {
                            field_name == name
                        }
                        (Some(SalsaDocTagFieldKeySummary::Integer(field_index)), LuaMemberKey::Integer(index)) => {
                            field_index == index
                        }
                        _ => false,
                    };
                    if !key_matches {
                        continue;
                    }

                    let Some(type_offset) = field.type_offset else {
                        continue;
                    };
                    let Some(mut typ) = infer_compilation_doc_type_key_with_owner(
                        db,
                        *file_id,
                        Some(type_def.syntax_offset),
                        type_offset,
                    ) else {
                        continue;
                    };
                    if field.is_nullable && !typ.is_nullable() {
                        typ = TypeOps::Union.apply(db, &typ, &LuaType::Nil);
                    }
                    resolved.push(typ);
                }
            }
        }
    }

    let mut types = resolved.into_iter();
    let first = types.next()?;
    Some(types.fold(first, |acc, ty| TypeOps::Union.apply(db, &acc, &ty)))
}

pub fn infer_compilation_type_super_types(
    db: &DbIndex,
    type_decl_id: &crate::LuaTypeDeclId,
) -> Option<Vec<LuaType>> {
    let type_name = type_decl_id.get_name();
    let index = db.get_type_def_reverse_index();
    let mut resolved = Vec::new();
    if let Some(defs) = index.by_name.get(&SmolStr::new(type_name)) {
        for (file_id, type_def) in defs {
            for super_type_offset in &type_def.super_type_offsets {
                let Some(typ) = infer_compilation_doc_type_key_with_owner(
                    db,
                    *file_id,
                    Some(type_def.syntax_offset),
                    *super_type_offset,
                ) else {
                    continue;
                };
                resolved.push(typ);
            }
        }
    }

    if resolved.is_empty() {
        None
    } else {
        Some(resolved)
    }
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
    infer_compilation_doc_type_ref_with_owner(db, file_id, None, type_ref)
}

fn infer_compilation_doc_type_ref_with_owner(
    db: &DbIndex,
    file_id: FileId,
    owner_offset: impl Into<Option<TextSize>>,
    type_ref: Option<&SalsaDocTypeRef>,
) -> Option<LuaType> {
    let type_key = match type_ref? {
        SalsaDocTypeRef::Node(type_key) => *type_key,
        SalsaDocTypeRef::Incomplete => return None,
    };

    infer_compilation_doc_type_key_with_owner(db, file_id, owner_offset, type_key)
}

pub(crate) fn infer_compilation_doc_type_key_with_owner(
    db: &DbIndex,
    file_id: FileId,
    owner_offset: impl Into<Option<TextSize>>,
    type_key: SalsaDocTypeNodeKey,
) -> Option<LuaType> {
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

    let context = owner_offset
        .into()
        .map(|offset| DocTypeInferContext::new(db, file_id).with_type_owner_offset(offset))
        .unwrap_or_else(|| DocTypeInferContext::new(db, file_id));

    Some(infer_doc_type(context, &doc_type))
}

fn signature_generic_owner_offset(
    signature: &crate::SalsaSignatureExplainSummary,
) -> Option<TextSize> {
    signature
        .doc_owners
        .first()
        .map(|owner| owner.owner_offset)
        .or_else(|| signature.generics.first().map(|generic| generic.syntax_offset))
}

fn infer_compilation_doc_type_keys(
    db: &DbIndex,
    file_id: FileId,
    type_keys: &[SalsaDocTypeNodeKey],
) -> Option<LuaType> {
    let mut combined = None;
    for type_key in type_keys {
        let ty = infer_compilation_doc_type_ref(db, file_id, Some(&SalsaDocTypeRef::Node(*type_key)))?;
        combined = Some(match combined {
            Some(existing) => TypeOps::Union.apply(db, &existing, &ty),
            None => ty,
        });
    }

    combined
}

fn infer_compilation_decl_type_info(
    db: &DbIndex,
    file_id: FileId,
    decl_type: &SalsaDeclTypeInfoSummary,
) -> Option<LuaType> {
    if let Some(explicit_type) =
        infer_compilation_doc_type_keys(db, file_id, &decl_type.explicit_type_offsets)
    {
        return Some(crate::complete_type_generic_args_in_type(db, &explicit_type));
    }

    let mut named_types = decl_type
        .named_type_names
        .iter()
        .map(|name| infer_compilation_named_type(db, file_id, name));
    let first = named_types.next()?;
    Some(crate::complete_type_generic_args_in_type(
        db,
        &named_types.fold(first, |acc, ty| TypeOps::Union.apply(db, &acc, &ty)),
    ))
}

fn infer_compilation_named_type(db: &DbIndex, file_id: FileId, name: &str) -> LuaType {
    if let Some(type_def) = db
        .get_summary_db()
        .doc()
        .type_def_by_name(file_id, SmolStr::new(name))
    {
        let workspace_id = db.resolve_workspace_id(file_id).unwrap_or(WorkspaceId::MAIN);
        let type_id = match type_def.visibility {
            SalsaDocVisibilityKindSummary::Private => crate::LuaTypeDeclId::local(file_id, name),
            SalsaDocVisibilityKindSummary::Internal
            | SalsaDocVisibilityKindSummary::Package => {
                crate::LuaTypeDeclId::internal(workspace_id, name)
            }
            SalsaDocVisibilityKindSummary::Public
            | SalsaDocVisibilityKindSummary::Protected => crate::LuaTypeDeclId::global(name),
        };

        if type_def.generic_params.is_empty() {
            return LuaType::Ref(type_id);
        }

        let mut generic_args = Vec::with_capacity(type_def.generic_params.len());
        for param in &type_def.generic_params {
            let Some(default_type_offset) = param.default_type_offset else {
                return LuaType::Any;
            };
            let Some(default_type) = infer_compilation_doc_type_ref(
                db,
                file_id,
                Some(&SalsaDocTypeRef::Node(default_type_offset)),
            ) else {
                return LuaType::Any;
            };
            generic_args.push(default_type);
        }

        return LuaType::Generic(LuaGenericType::new(type_id, generic_args).into());
    }

    LuaType::Ref(crate::LuaTypeDeclId::global(name))
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
pub(crate) struct CompilationDeclIndex<'a> {
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
