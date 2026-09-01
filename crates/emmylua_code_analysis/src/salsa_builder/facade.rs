//! Query facade — crate-internal API for salsa_builder.
//!
//! The public contract is only through `semantic_model::SemanticModel` (the sole public entry);
//! this facade (`SalsaQueries`) is an implementation detail of SemanticModel and stays within the crate.

use std::sync::Arc;

use emmylua_parser::{LuaChunk, LuaParseError, LuaSyntaxId, LuaSyntaxTree};
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;

use crate::salsa_builder::types::LiteralShell;
use crate::{
    GenericTpl, GenericTplId, LuaAliasCallKind, LuaAliasCallType, LuaArrayType, LuaStringTplType,
    LuaTupleStatus, LuaTupleType, VariadicType,
};

use super::SalsaDatabase;
use super::def::{
    ConstructorAttribute, Decl, InternedLuaType, Member, MemberRef, ModuleExport, NameUse,
    SalsaGenericParam, Scope, SemanticId, Signature, TypeDef, TypeScope, TypeVisibility,
};
use super::exports::{FileExports, file_exports};
use super::facts::FileFacts;
use super::query::{
    self, decl_references, decl_type, file_and_config, file_facts, member_keys_of_decl,
    member_keys_of_type, member_type, module_export_type, parse, resolve_name, resolve_type_def,
    signature_return,
};
use super::types::{PrimitiveType, TypeCandidate, TypeShell};
use crate::{
    AsyncState, FileId, InFiled, LuaFunctionType, LuaGenericType, LuaType, LuaTypeDeclId,
    LuaUnionType,
};

/// Owned member list wrapper.
///
/// Keeping an `Arc<[MemberRef]>` internally lets salsa snapshots share member lists
/// without deep cloning. Iterating by value still materializes a `Vec` for callers
/// that need owned `MemberRef` values, mirroring the previous API shape.
#[derive(Clone, Debug, Default)]
pub struct MemberList(Arc<[MemberRef]>);

impl MemberList {
    pub fn as_slice(&self) -> &[MemberRef] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl From<Arc<[MemberRef]>> for MemberList {
    fn from(value: Arc<[MemberRef]>) -> Self {
        Self(value)
    }
}

impl From<Vec<MemberRef>> for MemberList {
    fn from(value: Vec<MemberRef>) -> Self {
        Self(Arc::from(value))
    }
}

impl std::ops::Deref for MemberList {
    type Target = [MemberRef];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl IntoIterator for MemberList {
    type Item = MemberRef;
    type IntoIter = std::vec::IntoIter<MemberRef>;

    #[allow(clippy::unnecessary_to_owned)] // By-value API intentionally yields owned MemberRef.
    fn into_iter(self) -> Self::IntoIter {
        self.0.to_vec().into_iter()
    }
}

/// Owned type-definition list wrapper, mirroring `MemberList`.
#[derive(Clone, Debug, Default)]
pub struct TypeDefList(Arc<[TypeDef]>);

impl TypeDefList {
    pub fn as_slice(&self) -> &[TypeDef] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl From<Arc<[TypeDef]>> for TypeDefList {
    fn from(value: Arc<[TypeDef]>) -> Self {
        Self(value)
    }
}

impl From<Vec<TypeDef>> for TypeDefList {
    fn from(value: Vec<TypeDef>) -> Self {
        Self(Arc::from(value))
    }
}

impl std::ops::Deref for TypeDefList {
    type Target = [TypeDef];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl IntoIterator for TypeDefList {
    type Item = TypeDef;
    type IntoIter = std::vec::IntoIter<TypeDef>;

    #[allow(clippy::unnecessary_to_owned)] // By-value API intentionally yields owned TypeDef.
    fn into_iter(self) -> Self::IntoIter {
        self.0.to_vec().into_iter()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SalsaQueries<'db> {
    db: &'db SalsaDatabase,
}

impl<'db> SalsaQueries<'db> {
    pub fn new(db: &'db SalsaDatabase) -> Self {
        Self { db }
    }

    // ── Files / Syntax ──

    pub fn file_facts(&self, file_id: FileId) -> Option<&'db FileFacts> {
        let (file, config) = file_and_config(self.db, file_id)?;
        Some(file_facts(self.db, file, config))
    }

    pub fn syntax_tree(&self, file_id: FileId) -> Option<&'db LuaSyntaxTree> {
        let (file, config) = file_and_config(self.db, file_id)?;
        Some(parse(self.db, file, config))
    }

    pub fn chunk(&self, file_id: FileId) -> Option<LuaChunk> {
        self.syntax_tree(file_id).map(|tree| tree.get_chunk_node())
    }

    /// High-level semantic expression type (salsa-tracked, replaces the shared
    /// `SemanticCache::expr_type` map as the memoization layer).
    pub(crate) fn semantic_expr_type(
        &self,
        file_id: FileId,
        expr_syntax: LuaSyntaxId,
    ) -> Option<LuaType> {
        let (file, config) = file_and_config(self.db, file_id)?;
        query::semantic_expr_type(self.db, file, config, expr_syntax).map(|ty| ty.into_inner())
    }

    /// High-level type compatibility check (salsa-tracked).
    pub(crate) fn semantic_type_check(
        &self,
        file_id: FileId,
        source: &LuaType,
        target: &LuaType,
    ) -> bool {
        let Some((file, config)) = file_and_config(self.db, file_id) else {
            return false;
        };
        query::semantic_type_check(
            self.db,
            file,
            config,
            InternedLuaType::new(source.clone()),
            InternedLuaType::new(target.clone()),
        )
    }

    /// High-level member resolution (salsa-tracked).
    pub(crate) fn semantic_resolve_member(
        &self,
        file_id: FileId,
        index_syntax: LuaSyntaxId,
    ) -> Option<query::SalsaResolvedMember> {
        let (file, config) = file_and_config(self.db, file_id)?;
        query::semantic_resolve_member(self.db, file, config, index_syntax)
    }

    /// Whether a doc tag is in `emmyrc.doc.known_tags` (used by unknown_doc_tag checks).
    pub fn is_known_doc_tag(&self, file_id: FileId, name: &str) -> bool {
        let Some((_, config)) = file_and_config(self.db, file_id) else {
            return false;
        };
        config.known_doc_tags(self.db).iter().any(|tag| tag == name)
    }

    pub fn parse_errors(&self, file_id: FileId) -> Option<Vec<LuaParseError>> {
        let tree = self.syntax_tree(file_id)?;
        let errors = tree.get_errors().to_vec();
        if errors.is_empty() {
            None
        } else {
            Some(errors)
        }
    }

    /// Per-file control-flow graph (CFG, for flow-sensitive analysis).
    pub fn flow_tree(&self, file_id: FileId) -> Option<Arc<super::flow::FlowTree>> {
        let (file, config) = file_and_config(self.db, file_id)?;
        Some(super::flow::flow_tree_of(self.db, file, config).clone())
    }

    // ── Declarations ──

    pub fn decls(&self, file_id: FileId) -> Option<&'db [Decl]> {
        self.file_facts(file_id).map(|facts| facts.decls.as_slice())
    }

    pub fn scopes(&self, file_id: FileId) -> Option<&'db [Scope]> {
        self.file_facts(file_id)
            .map(|facts| facts.scopes.as_slice())
    }

    /// `find_decl`: finds a declaration whose name token covers the given offset.
    pub fn decl_by_offset(&self, file_id: FileId, offset: TextSize) -> Option<SemanticId> {
        let facts = self.file_facts(file_id)?;
        facts.decl_at_offset(offset).map(|d| d.id.clone())
    }

    /// Type of a declaration (node-keyed, with cycle convergence; may reference members across files).
    pub fn decl_type(&self, file_id: FileId, decl: SemanticId) -> Option<TypeShell> {
        let (file, config) = file_and_config(self.db, file_id)?;
        Some(decl_type(self.db, file, config, decl))
    }

    /// Range of a declaration's name (for goto-def).
    #[allow(unused)]
    pub fn decl_range(&self, file_id: FileId, decl: SemanticId) -> Option<TextRange> {
        self.file_facts(file_id)
            .and_then(|facts| facts.decl_by_id(&decl))
            .map(|decl| decl.name_range)
    }

    // ── Names / References ──

    pub fn name_uses(&self, file_id: FileId) -> Option<&'db [NameUse]> {
        self.file_facts(file_id)
            .map(|facts| facts.name_uses.as_slice())
    }

    /// Name use site → declaration (scope-aware).
    pub fn resolve_name(&self, file_id: FileId, offset: TextSize) -> Option<SemanticId> {
        let (file, config) = file_and_config(self.db, file_id)?;
        resolve_name(self.db, file, config, offset)
    }

    /// Workspace-global declaration (cross-file). The `Decl` key carries its defining file.
    pub fn global_decl(&self, name: &str) -> Option<SemanticId> {
        let workspace = self.db.workspace_input()?;
        let config = self.db.config_input()?;
        query::global_decl_by_name(self.db, workspace, config, SmolStr::new(name))
    }

    /// All references to a declaration.
    pub fn decl_references(&self, file_id: FileId, decl: SemanticId) -> Vec<LuaSyntaxId> {
        let Some((file, config)) = file_and_config(self.db, file_id) else {
            return Vec::new();
        };
        decl_references(self.db, file, config, decl)
    }

    // ── Members ──

    /// File's exported facts (cross-file consumption entry: reads only the defining file, not its function bodies).
    pub fn file_exports(&self, file_id: FileId) -> Option<&'db FileExports> {
        let (file, config) = file_and_config(self.db, file_id)?;
        Some(file_exports(self.db, file, config))
    }

    pub fn members(&self, file_id: FileId) -> Option<&'db [Member]> {
        self.file_facts(file_id)
            .map(|facts| facts.members.as_slice())
    }

    /// Type of a member (node-keyed, with cycle convergence; may resolve value expressions across files).
    pub fn member_type(&self, file_id: FileId, member: SemanticId) -> Option<TypeShell> {
        let (file, config) = file_and_config(self.db, file_id)?;
        Some(member_type(self.db, file, config, member))
    }

    /// Index expression (by syntax position) → `(owner, name)` member reference (for deprecated etc. checks).
    pub fn member_ref_of_index(
        &self,
        file_id: FileId,
        index_syntax: LuaSyntaxId,
    ) -> Option<(SemanticId, SmolStr)> {
        let (file, config) = file_and_config(self.db, file_id)?;
        let facts = file_facts(self.db, file, config);
        let tree = parse(self.db, file, config);
        let expr = query::find_expr_by_syntax_id(tree, &index_syntax)?;
        let emmylua_parser::LuaExpr::IndexExpr(index_expr) = expr else {
            return None;
        };
        query::member_ref_from_index_expr(facts, &index_expr)
    }

    /// Direct member names of a local declaration (completion candidates).
    #[allow(unused)]
    pub fn member_keys_of_decl(&self, file_id: FileId, decl: SemanticId) -> Vec<SmolStr> {
        let Some((file, config)) = file_and_config(self.db, file_id) else {
            return Vec::new();
        };
        member_keys_of_decl(self.db, file, config, decl)
    }

    /// Member keys of a named type (including parent types, completion candidates).
    #[allow(unused)]
    pub fn member_keys_of_type(&self, file_id: FileId, type_def: SemanticId) -> Vec<SmolStr> {
        let Some((file, config)) = file_and_config(self.db, file_id) else {
            return Vec::new();
        };
        member_keys_of_type(self.db, file, config, type_def)
    }

    /// Cross-file merged member keys (completion candidates): runtime members (Name/Decl keys) + `@field` (TypeDef keys).
    pub fn member_keys_of_owner(&self, owner: SemanticId) -> Vec<SmolStr> {
        let Some(workspace) = self.db.workspace_input() else {
            return Vec::new();
        };
        let Some(config) = self.db.config_input() else {
            return Vec::new();
        };
        query::member_keys_of_owner(self.db, workspace, config, owner)
    }

    // ── Phase 2: Workspace member associations ──

    /// Resolve `Name("a.b")` to its real definition (global type/variable/member chain). `Decl`/`TypeDef`/`Member` are returned as-is.
    pub fn resolve_owner(&self, owner: SemanticId) -> Option<SemanticId> {
        let workspace = self.db.workspace_input()?;
        let config = self.db.config_input()?;
        query::resolve_owner(self.db, workspace, config, owner)
    }

    /// Resolve an owner to a set of identities (dual identity: same-named type + runtime value decl).
    pub fn resolve_owner_set(&self, owner: SemanticId) -> Vec<SemanticId> {
        let Some(workspace) = self.db.workspace_input() else {
            return Vec::new();
        };
        let Some(config) = self.db.config_input() else {
            return Vec::new();
        };
        query::resolve_owner_set(self.db, workspace, config, owner)
    }

    /// Members of an owner identified by `SemanticId` (cross-file).
    pub fn members_of_owner(&self, owner: SemanticId) -> MemberList {
        let Some(workspace) = self.db.workspace_input() else {
            return MemberList::default();
        };
        let Some(config) = self.db.config_input() else {
            return MemberList::default();
        };
        MemberList::from(query::members_of_owner(self.db, workspace, config, owner))
    }

    /// Constructor attribute for a type definition (from `meta("Class")` factory `---@[constructor("init")]`).
    pub fn constructor_attribute_of_type(
        &self,
        type_def: SemanticId,
    ) -> Option<ConstructorAttribute> {
        let workspace = self.db.workspace_input()?;
        let config = self.db.config_input()?;
        query::constructor_attribute_of_type(self.db, workspace, config, type_def)
    }

    // ── Signatures ──

    pub fn signatures(&self, file_id: FileId) -> Option<&'db [Signature]> {
        self.file_facts(file_id)
            .map(|facts| facts.signatures.as_slice())
    }

    /// Function return type (doc annotation first, otherwise scan function body returns; may reference members across files).
    pub fn signature_return(
        &self,
        file_id: FileId,
        closure_syntax: LuaSyntaxId,
    ) -> Option<TypeShell> {
        let (file, config) = file_and_config(self.db, file_id)?;
        Some(signature_return(self.db, file, config, closure_syntax))
    }

    /// Per-slot function return types (preserves multiple return values).
    pub fn signature_returns(
        &self,
        file_id: FileId,
        closure_syntax: LuaSyntaxId,
    ) -> Option<Vec<TypeShell>> {
        let (file, config) = file_and_config(self.db, file_id)?;
        Some(query::signature_returns(
            self.db,
            file,
            config,
            closure_syntax,
        ))
    }

    /// Type of a function's `param_index`-th parameter (`---@param` annotation + generic bindings).
    pub fn param_type(
        &self,
        file_id: FileId,
        closure_syntax: LuaSyntaxId,
        param_index: usize,
    ) -> Option<TypeShell> {
        let (file, config) = file_and_config(self.db, file_id)?;
        Some(query::param_type(
            self.db,
            file,
            config,
            closure_syntax,
            param_index,
        ))
    }

    // ── Modules ──

    pub fn module_export(&self, file_id: FileId) -> Option<&'db ModuleExport> {
        self.file_facts(file_id).map(|facts| &facts.module_export)
    }

    /// Type of a module's exported value (for cross-file require resolution).
    pub fn module_export_type(&self, file_id: FileId) -> Option<TypeShell> {
        let (file, config) = file_and_config(self.db, file_id)?;
        Some(module_export_type(self.db, file, config))
    }

    /// Module name → module file (require resolution).
    pub fn module_file_of(&self, module_name: &str) -> Option<FileId> {
        let workspace = self.db.workspace_input()?;
        let config = self.db.config_input()?;
        query::module_file_of(self.db, workspace, config, SmolStr::new(module_name))
    }

    // ── Types (named, scoped) ──

    /// Resolve a named type in the current file scope (same-file Private → Internal → Global).
    pub fn resolve_type_def(&self, file_id: FileId, name: &str) -> Option<TypeDef> {
        let (file, config) = file_and_config(self.db, file_id)?;
        let workspace = self.db.workspace_input()?;
        resolve_type_def(self.db, workspace, config, file, SmolStr::new(name))
    }

    /// Resolve **all definition locations** of a named type in the current file scope (for duplicate-type checks).
    pub fn type_def_locations(&self, file_id: FileId, name: &str) -> Vec<TypeDef> {
        let Some((file, config)) = file_and_config(self.db, file_id) else {
            return Vec::new();
        };
        let Some(workspace) = self.db.workspace_input() else {
            return Vec::new();
        };
        query::resolve_type_def_locations(self.db, workspace, config, file, SmolStr::new(name))
            .to_vec()
    }

    /// All type definitions for a scope + full name (cross-file, for member queries / inheritance chains).
    pub fn type_defs_in_scope(&self, scope: TypeScope, full_name: &str) -> TypeDefList {
        let Some(workspace) = self.db.workspace_input() else {
            return TypeDefList::default();
        };
        let Some(config) = self.db.config_input() else {
            return TypeDefList::default();
        };
        TypeDefList::from(query::type_defs_in_scope(
            self.db,
            workspace,
            config,
            scope,
            SmolStr::new(full_name),
        ))
    }

    // ── Projection: TypeShell → LuaType ──

    /// Declared type projected to a concrete `LuaType` (consumed by semantic_model / diagnostics).
    pub fn decl_type_lua(&self, file_id: FileId, decl: SemanticId) -> Option<LuaType> {
        let shell = self.decl_type(file_id, decl)?;
        Some(self.project_shell(file_id, &shell))
    }

    /// Project a `TypeShell` to a concrete `LuaType` (for semantic_model type checks).
    pub fn type_shell_lua(&self, file_id: FileId, shell: &TypeShell) -> LuaType {
        self.project_shell(file_id, shell)
    }

    /// Projection (with function generic context: `Generic("T")` → `TplRef`).
    pub fn type_shell_lua_in(
        &self,
        file_id: FileId,
        shell: &TypeShell,
        generic_names: &[SmolStr],
    ) -> LuaType {
        self.project_shell_in(file_id, shell, generic_names)
    }

    /// Doc type node (by syntax position) → projected `LuaType` (for generic constraints / default value resolution).
    pub fn doc_type_lua(
        &self,
        file_id: FileId,
        type_syntax: LuaSyntaxId,
        generics: &[SalsaGenericParam],
    ) -> LuaType {
        let Some((file, config)) = file_and_config(self.db, file_id) else {
            return LuaType::Unknown;
        };
        let workspace = self.db.workspace_input();
        let shell = query::lower_doc_type(self.db, workspace, file, config, type_syntax, generics);
        let generic_names: Vec<SmolStr> = generics.iter().map(|g| g.name.clone()).collect();
        self.type_shell_lua_in(file_id, &shell, &generic_names)
    }

    fn project_shell(&self, file_id: FileId, shell: &TypeShell) -> LuaType {
        self.project_shell_in(file_id, shell, &[])
    }

    /// Projection (with function generic context: `Generic("T")` inside `fun<T>` params → `TplRef`).
    fn project_shell_in(
        &self,
        file_id: FileId,
        shell: &TypeShell,
        generic_names: &[SmolStr],
    ) -> LuaType {
        let mut types: Vec<LuaType> = Vec::new();
        for candidate in &shell.candidates {
            let typ = match candidate {
                TypeCandidate::Primitive(p) => primitive_lua_type(*p),
                TypeCandidate::Named(name) => self.resolve_named(file_id, name),
                // Generic parameter reference: inside function context → TplRef (bindable during unify); otherwise fall back to a global name.
                TypeCandidate::Generic(name) => {
                    if let Some(index) = generic_names.iter().position(|n| n == name) {
                        LuaType::TplRef(Arc::new(GenericTpl::new(
                            GenericTplId::Type(index as u32),
                            name.clone(),
                            None,
                            None,
                            false,
                            None,
                        )))
                    } else {
                        LuaType::Ref(LuaTypeDeclId::global(name))
                    }
                }
                // Structured function type → DocFunction (params/return recursively projected, generic names as context).
                TypeCandidate::Function(fun) => {
                    // Nested fun without its own generics inherits the outer context (`fun(item: T)` inside `fun<T>`).
                    let context: &[SmolStr] = if fun.generic_params.is_empty() {
                        generic_names
                    } else {
                        &fun.generic_params
                    };
                    let params = fun
                        .params
                        .iter()
                        .enumerate()
                        .map(|(index, param)| {
                            let name = fun
                                .param_names
                                .get(index)
                                .map(|name| name.to_string())
                                .unwrap_or_default();
                            (name, Some(self.project_shell_in(file_id, param, context)))
                        })
                        .collect();
                    let ret = if fun.returns_multi.len() > 1 {
                        LuaType::Variadic(Arc::new(VariadicType::Multi(
                            fun.returns_multi
                                .iter()
                                .map(|r| self.project_shell_in(file_id, r, context))
                                .collect(),
                        )))
                    } else {
                        self.project_shell_in(file_id, &fun.returns, context)
                    };
                    let generic_params: Vec<GenericTpl> = fun
                        .generic_params
                        .iter()
                        .enumerate()
                        .map(|(index, name)| {
                            GenericTpl::new(
                                GenericTplId::Type(index as u32),
                                name.clone(),
                                None,
                                None,
                                false,
                                None,
                            )
                        })
                        .collect();
                    let async_state = match fun.async_state {
                        1 => AsyncState::Async,
                        2 => AsyncState::Sync,
                        _ => AsyncState::None,
                    };
                    LuaType::DocFunction(Arc::new(LuaFunctionType::new(
                        async_state,
                        fun.is_colon_define,
                        fun.is_variadic,
                        params,
                        ret,
                        Some(generic_params),
                    )))
                }
                // Anonymous table literal: projected to TableConst (preserves synthetic identity (file, range) for member queries).
                TypeCandidate::Table(table_id) => LuaType::TableConst(InFiled::new(
                    FileId::new(table_id.file_id),
                    TextRange::new(TextSize::from(table_id.start), TextSize::from(table_id.end)),
                )),
                // Array: recursively project the base type.
                TypeCandidate::Array(base) => {
                    LuaType::Array(Arc::new(LuaArrayType::from_base_type(
                        self.project_shell_in(file_id, base, generic_names),
                    )))
                }
                TypeCandidate::Variadic(base) => LuaType::Variadic(Arc::new(VariadicType::Base(
                    self.project_shell_in(file_id, base, generic_names),
                ))),
                TypeCandidate::Tuple(types) => LuaType::Tuple(Arc::new(LuaTupleType::new(
                    types
                        .iter()
                        .map(|ty| self.project_shell_in(file_id, ty, generic_names))
                        .collect(),
                    LuaTupleStatus::DocResolve,
                ))),
                TypeCandidate::Literal(literal) => match literal {
                    LiteralShell::String(value) => LuaType::StringConst(value.clone().into()),
                    LiteralShell::Integer(value) => LuaType::IntegerConst(*value),
                    LiteralShell::Float(bits) => LuaType::FloatConst(f64::from_bits(*bits)),
                    LiteralShell::Boolean(value) => LuaType::BooleanConst(*value),
                    LiteralShell::Nil => LuaType::Nil,
                },
                // Generic instantiation → Generic (base type + args recursively projected, reusing outer context).
                TypeCandidate::GenericInstance(ins) => {
                    let name = &ins.name;
                    let args = &ins.args;
                    let base_id = self.resolve_named_id(file_id, name);
                    let params: Vec<LuaType> = args
                        .iter()
                        .map(|arg| self.project_shell_in(file_id, arg, generic_names))
                        .collect();
                    // `TypeGuard<T>` is a built-in type guard: projected to `LuaType::TypeGuard(T)`,
                    // which flow analysis uses to narrow the argument in `if isX(v) then`.
                    if name == "TypeGuard"
                        && let Some(inner) = params.first()
                    {
                        LuaType::TypeGuard(Arc::new(inner.clone()))
                    } else if name == "std.Unpack" {
                        LuaType::Call(Arc::new(LuaAliasCallType::new(
                            LuaAliasCallKind::Unpack,
                            params,
                        )))
                    } else {
                        LuaType::Generic(Arc::new(LuaGenericType::new(base_id, params)))
                    }
                }
                // String template type → StrTplRef (string arguments replace placeholder names).
                TypeCandidate::StrTpl(str_tpl) => {
                    let prefix = &str_tpl.prefix;
                    let name = &str_tpl.name;
                    let suffix = &str_tpl.suffix;
                    let tpl_index = str_tpl.tpl_index;
                    let tpl_id = match tpl_index {
                        Some(index) => GenericTplId::Type(index),
                        None => GenericTplId::ConditionalInfer(0),
                    };
                    LuaType::StrTplRef(Arc::new(LuaStringTplType::new(
                        prefix, name, tpl_id, suffix, None,
                    )))
                }
                TypeCandidate::ModuleRef(module_file_id) => LuaType::ModuleRef(*module_file_id),
                TypeCandidate::Recursive => LuaType::Unknown,
            };
            if !types.contains(&typ) {
                types.push(typ);
            }
        }
        match types.len() {
            0 => LuaType::Unknown,
            1 => types.pop().expect("len checked"),
            _ => LuaType::Union(LuaUnionType::from_vec(types).into()),
        }
    }

    pub(crate) fn resolve_named(&self, file_id: FileId, name: &str) -> LuaType {
        // Built-in doc names (any/unknown/...) do not fall back to a global type named "any".
        match name {
            "any" => LuaType::Any,
            "unknown" => LuaType::Unknown,
            "never" => LuaType::Never,
            "userdata" => LuaType::Userdata,
            "thread" => LuaType::Thread,
            "io" => LuaType::Io,
            "self" => LuaType::SelfInfer,
            "global" => LuaType::Global,
            "int" | "integer" => LuaType::Integer,
            "number" | "float" => LuaType::Number,
            "string" => LuaType::String,
            "table" => LuaType::Table,
            "function" => LuaType::Function,
            "boolean" => LuaType::Boolean,
            "nil" => LuaType::Nil,
            _ => LuaType::Ref(self.resolve_named_id(file_id, name)),
        }
    }

    pub(crate) fn resolve_named_id(&self, file_id: FileId, name: &str) -> LuaTypeDeclId {
        if let Some(def) = self.resolve_type_def(file_id, name) {
            match def.visibility {
                TypeVisibility::Public => LuaTypeDeclId::global(&def.full_name),
                _ => LuaTypeDeclId::file(def.file_id, &def.full_name),
            }
        } else {
            LuaTypeDeclId::global(name)
        }
    }
}

fn primitive_lua_type(primitive: PrimitiveType) -> LuaType {
    match primitive {
        PrimitiveType::Nil => LuaType::Nil,
        PrimitiveType::Boolean => LuaType::Boolean,
        PrimitiveType::Integer => LuaType::Integer,
        PrimitiveType::Number => LuaType::Number,
        PrimitiveType::String => LuaType::String,
        PrimitiveType::Table => LuaType::Table,
        PrimitiveType::Function => LuaType::Function,
        PrimitiveType::EmptyObject => LuaType::Object(Arc::new(
            crate::LuaObjectType::new_with_fields(Default::default(), Vec::new()),
        )),
    }
}
