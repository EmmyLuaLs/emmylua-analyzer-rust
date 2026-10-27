//! Pure Semantic doc model: projects the data needed for documentation generation
//! (modules / types / members / globals / function signatures / type rendering)
//! directly from `SemanticDatabase` / `SemanticModel` / `FileFacts`.
//!
//! It bypasses the old `DbIndex` index and property/signature structures; `LuaType`
//! is just the value type projected by the Semantic facade, and rendering is done here.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use emmylua_code_analysis::{
    Decl, DeclKind, EmmyLuaAnalysis, FileId, LuaAliasCallKind, LuaConditionalType, LuaFunctionType,
    LuaGenericType, LuaMappedType, LuaMemberKey, LuaObjectType, LuaTupleType, LuaType,
    LuaTypeDeclId, Member, ModuleExport, RenderLevel, SemanticId, SemanticModel, Signature,
    TypeDef, TypeDefKind, TypeVisibility, VariadicType,
};
use emmylua_parser::{
    LuaAstNode, LuaCommentOwner, LuaDocDescriptionOwner, LuaDocTagField, LuaDocTagParam,
    LuaDocTagReturn, LuaExpr, LuaStat, LuaSyntaxId, LuaTableField, VisibilityKind,
};
use rowan::TextRange;

/// Type definition kind (used for doc page categorization).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocTypeKind {
    Class,
    Enum,
    Alias,
}

impl DocTypeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DocTypeKind::Class => "class",
            DocTypeKind::Enum => "enum",
            DocTypeKind::Alias => "alias",
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum DocTypeKey {
    Global(String),
    Internal(u32, String),
    File(FileId, String),
}

impl DocTypeKey {
    pub fn from_lua_id(id: &LuaTypeDeclId) -> Self {
        match id.get_id() {
            emmylua_code_analysis::LuaTypeIdentifier::Global(name) => {
                DocTypeKey::Global(name.to_string())
            }
            emmylua_code_analysis::LuaTypeIdentifier::Internal(workspace, name) => {
                DocTypeKey::Internal(workspace.id, name.to_string())
            }
            emmylua_code_analysis::LuaTypeIdentifier::File(file_id, name) => {
                DocTypeKey::File(*file_id, name.to_string())
            }
        }
    }

    pub fn to_lua_id(&self) -> LuaTypeDeclId {
        match self {
            DocTypeKey::Global(name) => LuaTypeDeclId::global(name),
            DocTypeKey::Internal(workspace, name) => {
                LuaTypeDeclId::internal(emmylua_code_analysis::WorkspaceId { id: *workspace }, name)
            }
            DocTypeKey::File(file_id, name) => LuaTypeDeclId::file(*file_id, name),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DocLoc {
    pub file: PathBuf,
    pub line: usize,
}

/// Property annotations attached to a declaration, type or member.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocProperty {
    pub visibility: Option<VisibilityKind>,
    pub deprecated: bool,
    /// Doc-comment text attached to the owner (prose before the `---@` tags).
    pub description: Option<String>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum DocOwner {
    Type(SemanticId),
    Member(SemanticId),
    Decl(SemanticId),
    /// Anonymous module export (e.g. `return {}`) with no reusable declaration identity.
    Expr(FileId, TextRange),
}

#[derive(Debug, Clone)]
pub struct DocGeneric {
    pub name: String,
    pub constraint: Option<LuaType>,
}

#[derive(Debug, Clone)]
pub struct DocFnParam {
    pub name: String,
    pub ty: Option<LuaType>,
}

/// A documented parameter of a function signature.
#[derive(Debug, Clone)]
pub struct DocSignatureParam {
    pub name: String,
    pub ty: Option<LuaType>,
    pub description: Option<String>,
}

/// A documented return value of a function signature.
#[derive(Debug, Clone)]
pub struct DocSignatureReturn {
    pub name: Option<String>,
    pub ty: LuaType,
    pub description: Option<String>,
}

/// Additional function info projected from Semantic `Signature`/`SignatureDoc`.
#[derive(Debug, Clone)]
pub struct DocSignature {
    pub overloads: Vec<LuaType>,
    pub nodiscard: Option<String>,
    /// Parameters in declaration order (types and descriptions only when documented).
    pub params: Vec<DocSignatureParam>,
    /// Explicit `---@return` rows (never inferred returns).
    pub returns: Vec<DocSignatureReturn>,
}

/// Directly renderable function view (prefers the type's own `DocFunction`, then fills in signature details).
#[derive(Debug, Clone)]
pub struct DocFunctionInfo {
    pub params: Vec<DocFnParam>,
    pub returns: Vec<LuaType>,
    pub overloads: Vec<LuaType>,
    pub generics: Vec<DocGeneric>,
    pub is_async: bool,
    pub is_method: bool,
    pub is_nodiscard: bool,
    pub nodiscard_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DocMember {
    pub name: String,
    pub key: LuaMemberKey,
    pub ty: LuaType,
    pub property: DocProperty,
    pub is_method: bool,
    pub loc: Option<DocLoc>,
    pub signature: Option<DocSignature>,
}

impl DocMember {
    /// Named fields use dot access; integer keys and index signatures use bracket access.
    pub fn qualified_name(&self, owner_name: &str) -> String {
        if self.key.is_name() {
            format!("{owner_name}.{}", self.name)
        } else {
            format!("{owner_name}{}", self.name)
        }
    }
}

#[derive(Debug, Clone)]
pub struct DocType {
    /// Type key used by the facade projection (matches the id in `LuaType::Ref/Def`).
    pub key: DocTypeKey,
    pub def_ids: Vec<SemanticId>,
    pub name: String,
    pub full_name: String,
    pub kind: DocTypeKind,
    pub property: DocProperty,
    pub generics: Vec<DocGeneric>,
    pub alias_type: Option<LuaType>,
    pub bases: Vec<LuaType>,
    pub locations: Vec<DocLoc>,
    pub members: Vec<DocMember>,
}

#[derive(Debug, Clone)]
pub struct DocModule {
    pub name: String,
    pub path: Option<PathBuf>,
    pub namespace: Option<String>,
    pub usings: Vec<String>,
    pub export_type: LuaType,
    /// Owner of runtime members (`local M = {}` → `Decl(M)`).
    pub owner: Option<SemanticId>,
    pub property: DocProperty,
}

#[derive(Debug, Clone)]
pub struct DocGlobal {
    pub name: String,
    pub ty: LuaType,
    /// Owner of runtime members (`global_table.foo` → `Name("global_table")`).
    pub owner: Option<SemanticId>,
    pub property: DocProperty,
    pub loc: Option<DocLoc>,
    pub signature: Option<DocSignature>,
}

pub struct DocModel {
    pub types: Vec<DocType>,
    pub modules: Vec<DocModule>,
    pub globals: Vec<DocGlobal>,
    types_by_key: HashMap<DocTypeKey, usize>,
    module_name_by_file: HashMap<FileId, String>,
    members_by_owner: HashMap<SemanticId, Vec<DocMember>>,
}

impl DocModel {
    pub fn build(analysis: &EmmyLuaAnalysis) -> Self {
        let db = &analysis.db;

        let mut file_ids = db.file_ids();
        file_ids.sort();
        let mut main_file_ids: HashSet<FileId> = db.main_workspace_file_ids().into_iter().collect();
        // When main_root is not set, Semantic falls back to all files; old doc behavior also used all main workspace files.
        if db.main_root().is_none() {
            main_file_ids.extend(file_ids.iter().copied());
        }

        let mut file_paths = HashMap::new();
        let mut module_name_by_file = HashMap::new();

        // First pass: collect facts.
        let mut type_builders: HashMap<DocTypeKey, Vec<TypeDef>> = HashMap::new();
        let mut all_type_defs: Vec<TypeDef> = Vec::new();
        #[allow(clippy::type_complexity)]
        let mut member_sources: HashMap<SemanticId, Vec<(FileId, Member)>> = HashMap::new();
        let mut signature_sources: HashMap<LuaSyntaxId, (FileId, Signature)> = HashMap::new();
        let mut global_sources: Vec<(FileId, Decl)> = Vec::new();
        let mut module_sources: Vec<ModuleSource> = Vec::new();
        let mut properties: HashMap<DocOwner, DocProperty> = HashMap::new();
        let mut decls_by_owner_syntax: HashMap<LuaSyntaxId, Vec<SemanticId>> = HashMap::new();

        for file_id in file_ids {
            let Some(path) = db.file_path(file_id) else {
                continue;
            };
            file_paths.insert(file_id, path.clone());
            if main_file_ids.contains(&file_id)
                && let Some(name) = db.module_name_of(file_id)
            {
                module_name_by_file.insert(file_id, name);
            }

            let model = SemanticModel::new(db, file_id);
            let Some(facts) = model.file_facts() else {
                continue;
            };

            for def in &facts.type_defs {
                all_type_defs.push(def.clone());
                if !main_file_ids.contains(&file_id) {
                    continue;
                }
                let key = type_def_key(def);
                type_builders.entry(key).or_default().push(def.clone());
                properties.insert(
                    DocOwner::Type(def.id.clone()),
                    DocProperty {
                        visibility: type_visibility(def.visibility),
                        deprecated: def.deprecated,
                        description: def
                            .owner_syntax
                            .and_then(|owner| doc_comment_description(&model, file_id, owner)),
                    },
                );
            }

            for member in &facts.members {
                member_sources
                    .entry(member.owner.clone())
                    .or_default()
                    .push((file_id, member.clone()));
                properties.insert(
                    DocOwner::Member(member.id.clone()),
                    DocProperty {
                        visibility: Some(member.visibility),
                        deprecated: member.deprecated,
                        description: member_description(&model, member),
                    },
                );
            }

            for signature in &facts.signatures {
                signature_sources.insert(signature.closure_syntax, (file_id, signature.clone()));
            }

            for decl in &facts.decls {
                properties.insert(
                    DocOwner::Decl(decl.id.clone()),
                    DocProperty {
                        visibility: None,
                        deprecated: decl.deprecated,
                        description: decl
                            .owner_syntax
                            .and_then(|owner| doc_comment_description(&model, file_id, owner)),
                    },
                );
                if let Some(owner_syntax) = decl.owner_syntax {
                    decls_by_owner_syntax
                        .entry(owner_syntax)
                        .or_default()
                        .push(decl.id.clone());
                }
                if matches!(decl.kind, DeclKind::Global) && main_file_ids.contains(&file_id) {
                    global_sources.push((file_id, decl.clone()));
                }
            }

            if main_file_ids.contains(&file_id) {
                module_sources.push(ModuleSource {
                    file_id,
                    module_export: facts.module_export.clone(),
                    namespace: facts.namespace.as_ref().map(|s| s.to_string()),
                    usings: facts.usings.iter().map(|s| s.to_string()).collect(),
                });
            }
        }

        // Project signatures.
        let mut signatures: HashMap<LuaSyntaxId, DocSignature> = HashMap::new();
        for (syntax, (file_id, signature)) in &signature_sources {
            signatures.insert(
                *syntax,
                project_signature(&SemanticModel::new(db, *file_id), signature),
            );
        }

        // Project members (cross-file owner -> member).
        let mut member_by_id: HashMap<SemanticId, DocMember> = HashMap::new();
        let mut members_by_owner: HashMap<SemanticId, Vec<DocMember>> = HashMap::new();
        for (owner, sources) in &member_sources {
            for (file_id, member) in sources {
                if member_by_id.contains_key(&member.id) {
                    continue;
                }
                let model = SemanticModel::new(db, *file_id);
                let signature = member
                    .value_syntax
                    .as_ref()
                    .and_then(|syntax| signatures.get(syntax));
                let Some(doc) = project_member(&model, member, &file_paths, signature, &signatures)
                else {
                    continue;
                };
                member_by_id.insert(member.id.clone(), doc.clone());
                members_by_owner.entry(owner.clone()).or_default().push(doc);
            }
        }

        // Finalize types.
        let mut types_by_key = HashMap::new();
        let mut types = Vec::new();
        for (key, mut defs) in type_builders {
            defs.sort_by(|a, b| (a.file_id, a.name.as_str()).cmp(&(b.file_id, b.name.as_str())));
            let first = defs.first().expect("type defs non-empty").clone();
            let model = SemanticModel::new(db, first.file_id);
            let kind = match first.kind {
                TypeDefKind::Class => DocTypeKind::Class,
                TypeDefKind::Enum => DocTypeKind::Enum,
                TypeDefKind::Alias => DocTypeKind::Alias,
            };
            let mut member_owners: Vec<SemanticId> =
                defs.iter().map(|def| def.id.clone()).collect();
            for def in &defs {
                if let Some(owner_syntax) = def.owner_syntax
                    && let Some(decls) = decls_by_owner_syntax.get(&owner_syntax)
                {
                    member_owners.extend(
                        decls
                            .iter()
                            .filter(|decl| decl_belongs_to_type(&model, decl, &defs))
                            .cloned(),
                    );
                }
            }
            let mut members: Vec<DocMember> = Vec::new();
            let mut seen_members = HashSet::new();
            for owner in &member_owners {
                if let Some(list) = members_by_owner.get(owner) {
                    for member in list {
                        if seen_members.insert((member.key.clone(), member.name.clone())) {
                            members.push(member.clone());
                        }
                    }
                }
            }
            members.sort_by(|a, b| a.name.cmp(&b.name));

            let mut bases = model.base_types(&first.type_decl_id());
            bases.dedup();

            let locations = defs
                .iter()
                .filter_map(|def| make_loc(&model, &file_paths, def.file_id, def.name_range))
                .collect();

            let property = properties
                .get(&DocOwner::Type(first.id.clone()))
                .cloned()
                .unwrap_or_default();

            let doc = DocType {
                key: key.clone(),
                def_ids: defs.iter().map(|def| def.id.clone()).collect(),
                name: first.name.to_string(),
                full_name: first.full_name.to_string(),
                kind,
                property,
                generics: first
                    .generic_params
                    .iter()
                    .map(|param| DocGeneric {
                        name: param.name.to_string(),
                        constraint: param.constraint.map(|syntax| {
                            model.doc_type_lua_in(first.file_id, syntax, &first.generic_params)
                        }),
                    })
                    .collect(),
                alias_type: first.alias_type.map(|syntax| {
                    model.doc_type_lua_in(first.file_id, syntax, &first.generic_params)
                }),
                bases,
                locations,
                members,
            };
            types.push(doc);
        }
        types.sort_by(|a, b| a.full_name.cmp(&b.full_name));
        for (idx, doc) in types.iter().enumerate() {
            types_by_key.insert(doc.key.clone(), idx);
        }

        // Finalize globals.
        let mut globals = Vec::new();
        for (file_id, decl) in &global_sources {
            let model = SemanticModel::new(db, *file_id);

            let ty = model.type_of_decl(&decl.id).unwrap_or(LuaType::Unknown);
            globals.push(DocGlobal {
                name: decl.name.to_string(),
                property: properties
                    .get(&DocOwner::Decl(decl.id.clone()))
                    .cloned()
                    .unwrap_or_default(),
                loc: make_loc(&model, &file_paths, *file_id, decl.name_range),
                signature: decl
                    .value_expr_syntax
                    .and_then(|syntax| signatures.get(&syntax).cloned()),
                owner: Some(SemanticId::name(decl.name.clone())),
                ty,
            });
        }
        globals.sort_by(|a, b| a.name.cmp(&b.name));

        // Finalize modules.
        let mut modules = Vec::new();
        for source in module_sources {
            let model = SemanticModel::new(db, source.file_id);
            let export = match &source.module_export {
                ModuleExport::Decl { decl, .. } => {
                    let ty = model.type_of_decl(decl).unwrap_or(LuaType::Unknown);
                    Some((
                        slot0(ty),
                        Some(DocOwner::Decl(decl.clone())),
                        Some(decl.clone()),
                    ))
                }
                ModuleExport::Global { name } => {
                    let decl = global_sources
                        .iter()
                        .find(|(fid, decl)| *fid == source.file_id && decl.name == *name)
                        .map(|(_, decl)| decl.id.clone());
                    let ty = decl
                        .as_ref()
                        .and_then(|decl| model.type_of_decl(decl))
                        .unwrap_or(LuaType::Unknown);
                    Some((
                        slot0(ty),
                        decl.clone().map(DocOwner::Decl),
                        Some(SemanticId::name(name.clone())),
                    ))
                }
                ModuleExport::Expr { value_syntax } => {
                    let ty = slot0(model.type_of_expr(*value_syntax));
                    Some((
                        ty,
                        Some(DocOwner::Expr(source.file_id, value_syntax.get_range())),
                        Some(SemanticId::member(source.file_id, value_syntax.get_range())),
                    ))
                }
                ModuleExport::None => None,
            };
            let Some((export_type, export_owner, member_owner)) = export else {
                continue;
            };

            let mut property = export_owner
                .as_ref()
                .and_then(|owner| properties.get(owner).cloned())
                .unwrap_or_default();
            if property.description.is_none()
                && let ModuleExport::Expr { value_syntax } = &source.module_export
            {
                property.description =
                    value_owner_description(&model, source.file_id, *value_syntax);
            }
            let module_name = module_name_by_file
                .get(&source.file_id)
                .cloned()
                .or_else(|| {
                    file_paths
                        .get(&source.file_id)
                        .and_then(|p| fallback_module_name(p))
                });
            modules.push(DocModule {
                name: module_name.unwrap_or_else(|| "main".to_string()),
                path: file_paths.get(&source.file_id).cloned(),
                namespace: source.namespace,
                usings: source.usings,
                export_type,
                owner: member_owner,
                property,
            });
        }
        modules.sort_by(|a, b| a.name.cmp(&b.name));

        DocModel {
            types,
            modules,
            globals,
            types_by_key,
            module_name_by_file,
            members_by_owner,
        }
    }

    pub fn type_name(&self, id: &LuaTypeDeclId) -> String {
        let key = DocTypeKey::from_lua_id(id);
        self.types_by_key
            .get(&key)
            .and_then(|idx| self.types.get(*idx))
            .map(|ty| ty.full_name.clone())
            .unwrap_or_else(|| key.to_lua_id().get_name().to_string())
    }

    pub fn type_by_key(&self, key: &DocTypeKey) -> Option<&DocType> {
        self.types_by_key
            .get(key)
            .and_then(|idx| self.types.get(*idx))
    }

    pub fn module_name(&self, file_id: FileId) -> Option<&str> {
        self.module_name_by_file.get(&file_id).map(|s| s.as_str())
    }

    /// Returns the members owned by a value type (class members / table literal members).
    pub fn members_of_type(&self, ty: &LuaType) -> Vec<DocMember> {
        let mut owners: Vec<SemanticId> = Vec::new();
        match ty {
            LuaType::Ref(id) | LuaType::Def(id) => {
                if let Some(doc_type) = self.type_by_key(&DocTypeKey::from_lua_id(id)) {
                    owners.extend(doc_type.def_ids.iter().cloned());
                }
            }
            LuaType::TableConst(table) => {
                owners.push(SemanticId::member(table.file_id, table.value));
            }
            LuaType::Instance(instance) => {
                let range = instance.get_range();
                owners.push(SemanticId::member(range.file_id, range.value));
            }
            _ => {}
        }

        let mut seen = HashSet::new();
        let mut members = Vec::new();
        for owner in owners {
            if let Some(list) = self.members_by_owner.get(&owner) {
                for member in list {
                    if seen.insert((member.key.clone(), member.name.clone())) {
                        members.push(member.clone());
                    }
                }
            }
        }
        members.sort_by(|a, b| a.name.cmp(&b.name));
        members
    }

    /// Returns the members owned by an explicit semantic owner.
    pub fn members_of_owner(&self, owner: &SemanticId) -> Vec<DocMember> {
        let mut members = self
            .members_by_owner
            .get(owner)
            .cloned()
            .unwrap_or_default();
        members.sort_by(|a, b| a.name.cmp(&b.name));
        members
    }

    /// Members of a module: runtime members (`function M.add`) plus type members
    /// (`---@field` on the exported class), deduplicated by name.
    pub fn members_for_module(&self, module: &DocModule) -> Vec<DocMember> {
        let mut members = module
            .owner
            .as_ref()
            .map(|owner| self.members_of_owner(owner))
            .unwrap_or_default();
        members.extend(self.members_of_type(&module.export_type));
        dedupe_members(members)
    }

    /// Members of a global table: runtime members (`function global_table.foo`)
    /// plus table-literal members, deduplicated by name.
    pub fn members_for_global(&self, global: &DocGlobal) -> Vec<DocMember> {
        let mut members = global
            .owner
            .as_ref()
            .map(|owner| self.members_of_owner(owner))
            .unwrap_or_default();
        members.extend(self.members_of_type(&global.ty));
        dedupe_members(members)
    }

    /// Function view: uses the type's own params/returns, and signature facts fill overload/async/method/nodiscard.
    pub fn function_info(
        &self,
        ty: &LuaType,
        signature: Option<&DocSignature>,
    ) -> Option<DocFunctionInfo> {
        let (params, returns, generics, is_async, is_method) = match ty {
            LuaType::DocFunction(func) => (
                func.get_params()
                    .iter()
                    .map(|(name, ty)| DocFnParam {
                        name: name.clone(),
                        ty: ty.clone(),
                    })
                    .collect::<Vec<_>>(),
                expand_returns(func.get_ret()),
                func.get_generic_params()
                    .iter()
                    .map(|param| DocGeneric {
                        name: param.get_name().to_string(),
                        constraint: param.get_constraint().cloned(),
                    })
                    .collect(),
                func.get_async_state() == emmylua_code_analysis::AsyncState::Async,
                func.is_colon_define(),
            ),
            LuaType::Function => {
                return Some(DocFunctionInfo {
                    params: Vec::new(),
                    returns: Vec::new(),
                    overloads: Vec::new(),
                    generics: Vec::new(),
                    is_async: false,
                    is_method: false,
                    is_nodiscard: false,
                    nodiscard_message: None,
                });
            }
            _ => return None,
        };

        Some(DocFunctionInfo {
            params,
            returns,
            overloads: signature
                .map(|sig| sig.overloads.clone())
                .unwrap_or_default(),
            generics,
            is_async,
            is_method,
            is_nodiscard: signature.is_some_and(|sig| sig.nodiscard.is_some()),
            nodiscard_message: signature.and_then(|sig| sig.nodiscard.clone()),
        })
    }

    pub fn render_type(&self, ty: &LuaType, level: RenderLevel) -> String {
        let mut renderer = TypeTextRenderer {
            model: self,
            level,
            depth: 0,
            visited: HashSet::new(),
        };
        renderer.render(ty)
    }
}

struct ModuleSource {
    file_id: FileId,
    module_export: ModuleExport,
    namespace: Option<String>,
    usings: Vec<String>,
}

fn slot0(ty: LuaType) -> LuaType {
    ty.get_result_slot_type(0).unwrap_or(ty)
}

fn fallback_module_name(path: &Path) -> Option<String> {
    if path.extension().is_some_and(|ext| ext == "lua") {
        return path.file_stem()?.to_str().map(|s| s.to_string());
    }
    None
}

fn type_def_key(def: &TypeDef) -> DocTypeKey {
    match def.visibility {
        TypeVisibility::Public => DocTypeKey::Global(def.full_name.to_string()),
        TypeVisibility::Internal | TypeVisibility::Private => {
            DocTypeKey::File(def.file_id, def.full_name.to_string())
        }
    }
}

fn type_visibility(visibility: TypeVisibility) -> Option<VisibilityKind> {
    Some(match visibility {
        TypeVisibility::Public => VisibilityKind::Public,
        TypeVisibility::Internal => VisibilityKind::Internal,
        TypeVisibility::Private => VisibilityKind::Private,
    })
}

fn make_loc(
    model: &SemanticModel<'_>,
    file_paths: &HashMap<FileId, PathBuf>,
    file_id: FileId,
    range: TextRange,
) -> Option<DocLoc> {
    let file = file_paths.get(&file_id)?.clone();
    let line = model.document(file_id).and_then(|document| {
        document
            .get_line_col(range.start())
            .map(|(line, _)| line + 1)
    });
    Some(DocLoc {
        file,
        line: line.unwrap_or_default(),
    })
}

fn project_member(
    model: &SemanticModel<'_>,
    member: &Member,
    file_paths: &HashMap<FileId, PathBuf>,
    signature: Option<&DocSignature>,
    signatures: &HashMap<LuaSyntaxId, DocSignature>,
) -> Option<DocMember> {
    let file_id = member_file_id(&member.id);
    let name = if let Some(syntax) = member.index_key_syntax {
        let document = model.document(file_id)?;
        format!("[{}]", document.get_text_slice(syntax.get_range()).trim())
    } else {
        match &member.key {
            LuaMemberKey::Name(name) => name.to_string(),
            LuaMemberKey::Integer(index) => format!("[{index}]"),
            LuaMemberKey::None => return None,
        }
    };
    let mut ty = model.type_of_member(&member.id).unwrap_or(LuaType::Unknown);
    let mut signature = signature.cloned();
    let mut description = member_description(model, member);

    let unresolved = matches!(
        ty,
        LuaType::Unknown | LuaType::TplRef(_) | LuaType::Generic(_) | LuaType::TableGeneric(_)
    ) || match &ty {
        LuaType::Ref(id) | LuaType::Def(id) => model.type_def_of(id).is_none(),
        _ => false,
    };
    if ty.is_function() || unresolved {
        let resolved = resolve_member_docs(model, member, signatures);
        if signature.is_none() {
            signature = resolved.signature;
        }
        // Aliases (`exports.every = exports.all`) infer as a type parameter;
        // adopt the documented closure behind the alias. Bare `function` types
        // carry no parameter list, so prefer the resolved closure there too.
        if let Some(resolved_ty) = resolved.ty
            && resolved_ty.is_function()
            && (!ty.is_function() || matches!(ty, LuaType::Function))
        {
            ty = resolved_ty;
        }
        if description.is_none() {
            description = resolved.description;
        }
    }

    let key_range = member.id.member_key_range().unwrap_or_else(|| {
        member
            .value_syntax
            .map(|syntax| syntax.get_range())
            .unwrap_or_default()
    });
    Some(DocMember {
        name,
        key: member.key.clone(),
        ty,
        property: DocProperty {
            visibility: Some(member.visibility),
            deprecated: member.deprecated,
            description,
        },
        is_method: member.is_method,
        loc: make_loc(model, file_paths, file_id, key_range),
        signature,
    })
}

fn member_file_id(id: &SemanticId) -> FileId {
    match id {
        SemanticId::Member(key) => key.file_id,
        SemanticId::Decl(key) => key.file_id,
        SemanticId::Signature(key) => key.file_id,
        _ => FileId::new(0),
    }
}

/// Documentation recovered for a member whose function signature is not stored
/// directly on the member value (export wrappers, aliases, `@field` functions).
#[derive(Debug, Default)]
struct ResolvedDocs {
    signature: Option<DocSignature>,
    ty: Option<LuaType>,
    description: Option<String>,
}

/// Recovers the signature/description of a documented function behind a member.
///
/// A member value is often a wrapper call (`exports.each = export1(each)`) or a
/// reference to a local function, while the `---@param`/`---@return` tags live
/// on the local function. Follow one or two levels of names, members and call
/// arguments to find the documented closure.
fn resolve_member_docs(
    model: &SemanticModel<'_>,
    member: &Member,
    signatures: &HashMap<LuaSyntaxId, DocSignature>,
) -> ResolvedDocs {
    let Some(value_syntax) = member.value_syntax else {
        return ResolvedDocs::default();
    };
    // A direct function literal carries its own signature; the caller already
    // has it.
    if signatures.contains_key(&value_syntax) {
        return ResolvedDocs::default();
    }

    // `---@field add fun(...)` has no runtime value; fall back to a
    // same-named declaration with a documented closure.
    if member_field_tag(model, member).is_some() {
        return resolve_field_member_fallback(model, member, signatures).unwrap_or_default();
    }

    resolve_syntax_docs(model, value_syntax, signatures, 3).unwrap_or_default()
}

fn resolve_field_member_fallback(
    model: &SemanticModel<'_>,
    member: &Member,
    signatures: &HashMap<LuaSyntaxId, DocSignature>,
) -> Option<ResolvedDocs> {
    let LuaMemberKey::Name(name) = &member.key else {
        return None;
    };
    resolve_named_docs(model, name, signatures, 2)
}

/// Finds a documented function by name in the current file. Used for aliases
/// (`exports.every = exports.all` → `all`) and for `@field fun(...)` members
/// that mirror a local function declaration.
fn resolve_named_docs(
    model: &SemanticModel<'_>,
    name: &str,
    signatures: &HashMap<LuaSyntaxId, DocSignature>,
    depth: usize,
) -> Option<ResolvedDocs> {
    if depth == 0 {
        return None;
    }
    let facts = model.file_facts()?;
    for decl in &facts.decls {
        if decl.name == name
            && let Some(docs) = resolve_id_docs(model, &decl.id, signatures, depth - 1)
        {
            return Some(docs);
        }
    }
    for member in &facts.members {
        if let LuaMemberKey::Name(member_name) = &member.key
            && member_name == name
            && let Some(docs) = resolve_id_docs(model, &member.id, signatures, depth - 1)
        {
            return Some(docs);
        }
    }
    None
}

fn resolve_syntax_docs(
    model: &SemanticModel<'_>,
    syntax: LuaSyntaxId,
    signatures: &HashMap<LuaSyntaxId, DocSignature>,
    depth: usize,
) -> Option<ResolvedDocs> {
    if depth == 0 {
        return None;
    }
    if let Some(signature) = signatures.get(&syntax).cloned() {
        return Some(ResolvedDocs {
            signature: Some(signature),
            ..Default::default()
        });
    }

    let tree = model.syntax_tree_of(model.file_id())?;
    let node = syntax.to_node_from_root(&tree.get_red_root())?;
    let expr = LuaExpr::cast(node)?;
    match expr {
        LuaExpr::NameExpr(name) => {
            let id = model.resolve_name(name.get_range().start())?;
            resolve_id_docs(model, &id, signatures, depth - 1)
        }
        LuaExpr::IndexExpr(index) => {
            if let Some(resolved) = model.resolve_member(&index)
                && let Some(id) = resolved.member_id
                && let Some(docs) = resolve_id_docs(model, &id, signatures, depth - 1)
            {
                return Some(docs);
            }
            // Aliases such as `exports.every = exports.all` may resolve to a
            // class member without a value; fall back to the name after the dot.
            let key = index.get_index_key()?;
            let name = key.get_path_part();
            resolve_named_docs(model, &name, signatures, depth - 1)
        }
        LuaExpr::CallExpr(call) => {
            let args = call.get_args_list()?;
            for arg in args.get_args() {
                if let Some(docs) =
                    resolve_syntax_docs(model, arg.get_syntax_id(), signatures, depth - 1)
                {
                    return Some(docs);
                }
            }
            None
        }
        _ => None,
    }
}

fn resolve_id_docs(
    model: &SemanticModel<'_>,
    id: &SemanticId,
    signatures: &HashMap<LuaSyntaxId, DocSignature>,
    depth: usize,
) -> Option<ResolvedDocs> {
    let file_id = member_file_id(id);
    let model = model.model_for(file_id);
    let facts = model.file_facts()?;
    let (value_syntax, owner_syntax, ty) = match id {
        SemanticId::Decl(_) => {
            let decl = facts.decl_by_id(id)?;
            (
                decl.value_expr_syntax,
                decl.owner_syntax,
                model.type_of_decl(id),
            )
        }
        SemanticId::Member(_) => {
            let member = facts.member_by_id(id)?;
            (member.value_syntax, None, model.type_of_member(id))
        }
        _ => return None,
    };
    let value_syntax = value_syntax?;
    if let Some(signature) = signatures.get(&value_syntax).cloned() {
        let description = owner_syntax
            .and_then(|owner| doc_comment_description(&model, file_id, owner))
            .or_else(|| value_owner_description(&model, file_id, value_syntax));
        return Some(ResolvedDocs {
            signature: Some(signature),
            ty,
            description,
        });
    }
    resolve_syntax_docs(&model, value_syntax, signatures, depth.saturating_sub(1))
}

/// Returns the `@field` tag that owns a member value, when the member is
/// declared with `---@field name type`.
fn member_field_tag(model: &SemanticModel<'_>, member: &Member) -> Option<LuaDocTagField> {
    let file_id = member_file_id(&member.id);
    let tree = model.syntax_tree_of(file_id)?;
    let node = member
        .value_syntax?
        .to_node_from_root(&tree.get_red_root())?;
    node.ancestors().find_map(LuaDocTagField::cast)
}

/// Doc comment attached to the statement that owns a member.
fn member_description(model: &SemanticModel<'_>, member: &Member) -> Option<String> {
    let file_id = member_file_id(&member.id);
    let tree = model.syntax_tree_of(file_id)?;
    let node = member
        .value_syntax?
        .to_node_from_root(&tree.get_red_root())?;

    // `---@field gen fun.Generator @The underlying generator function`
    if let Some(field_tag) = node.ancestors().find_map(LuaDocTagField::cast) {
        return field_tag
            .get_description()
            .map(|desc| preprocess_description(&desc.get_description_text()))
            .filter(|text| !text.is_empty());
    }

    for ancestor in node.ancestors().skip(1) {
        // A table-field key/value does not own a doc comment of its own.
        if LuaTableField::cast(ancestor.clone()).is_some() {
            return None;
        }
        if let Some(stat) = LuaStat::cast(ancestor) {
            let comment = stat.get_left_comment()?;
            return comment
                .get_description()
                .map(|desc| preprocess_description(&desc.get_description_text()))
                .filter(|text| !text.is_empty());
        }
    }
    None
}

fn value_owner_description(
    model: &SemanticModel<'_>,
    file_id: FileId,
    value_syntax: LuaSyntaxId,
) -> Option<String> {
    let tree = model.syntax_tree_of(file_id)?;
    let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
    for ancestor in node.ancestors().skip(1) {
        if LuaTableField::cast(ancestor.clone()).is_some() {
            return None;
        }
        if let Some(stat) = LuaStat::cast(ancestor) {
            let comment = stat.get_left_comment()?;
            return comment
                .get_description()
                .map(|desc| preprocess_description(&desc.get_description_text()))
                .filter(|text| !text.is_empty());
        }
    }
    None
}

/// Docs attached to a statement owner (declaration / type definition).
fn doc_comment_description(
    model: &SemanticModel<'_>,
    file_id: FileId,
    owner_syntax: LuaSyntaxId,
) -> Option<String> {
    let tree = model.syntax_tree_of(file_id)?;
    let node = owner_syntax.to_node_from_root(&tree.get_red_root())?;
    let stat = LuaStat::cast(node)?;
    let comment = stat.get_left_comment()?;
    comment
        .get_description()
        .map(|desc| preprocess_description(&desc.get_description_text()))
        .filter(|text| !text.is_empty())
}

/// Mirrors the old analyzer `preprocess_description`: drops a leading `#`/`@`
/// and a single space of doc-comment indentation.
fn preprocess_description(description: &str) -> String {
    let description = description.trim_start_matches(['#', '@']);
    let mut result = String::new();
    let mut start_with_one_space = None;
    for mut line in description.lines() {
        let indent_count = line.chars().take_while(|c| c.is_whitespace()).count();
        if indent_count == line.len() {
            result.push('\n');
            continue;
        }
        if start_with_one_space.is_none() {
            start_with_one_space = Some(indent_count == 1);
        }
        if let Some(true) = start_with_one_space
            && let Some(c) = line.chars().next()
            && c.is_whitespace()
        {
            line = &line[c.len_utf8()..];
        }
        result.push_str(line);
        result.push('\n');
    }
    result.trim_end().to_string()
}

fn project_signature(model: &SemanticModel<'_>, signature: &Signature) -> DocSignature {
    let Some(docs) = signature.docs.as_deref() else {
        return DocSignature {
            overloads: Vec::new(),
            nodiscard: None,
            params: Vec::new(),
            returns: Vec::new(),
        };
    };

    let file_id = signature.file_id;
    let generics = docs.generic_params.clone();
    let params = signature
        .param_names
        .iter()
        .map(|name| {
            let documented = docs
                .param_types
                .iter()
                .find(|(param_name, _)| param_name == name);
            match documented {
                Some((_, syntax)) => DocSignatureParam {
                    name: name.to_string(),
                    ty: Some(model.doc_type_lua_in(file_id, *syntax, &generics)),
                    description: doc_tag_description(model, file_id, *syntax),
                },
                None => DocSignatureParam {
                    name: name.to_string(),
                    ty: None,
                    description: None,
                },
            }
        })
        .collect();
    let returns = docs
        .returns
        .iter()
        .map(|syntax| DocSignatureReturn {
            name: docs
                .named_returns
                .iter()
                .find(|(_, named_syntax)| named_syntax == syntax)
                .map(|(name, _)| name.to_string()),
            ty: model.doc_type_lua_in(file_id, *syntax, &generics),
            description: doc_tag_description(model, file_id, *syntax),
        })
        .collect();

    DocSignature {
        overloads: docs
            .overloads
            .iter()
            .map(|syntax| model.doc_type_lua_in(file_id, *syntax, &generics))
            .collect(),
        nodiscard: docs.nodiscard.as_ref().map(|msg| msg.to_string()),
        params,
        returns,
    }
}

/// Reads the description attached to the doc tag that owns a `---@param` /
/// `---@return` type node (`---@param a number @the value`).
fn doc_tag_description(
    model: &SemanticModel<'_>,
    file_id: FileId,
    syntax: LuaSyntaxId,
) -> Option<String> {
    let tree = model.syntax_tree_of(file_id)?;
    let node = syntax.to_node_from_root(&tree.get_red_root())?;
    let parent = node.parent()?;
    let description = if let Some(param) = LuaDocTagParam::cast(parent.clone()) {
        param.get_description()
    } else if let Some(ret) = LuaDocTagReturn::cast(parent) {
        ret.get_description()
    } else {
        None
    };
    description
        .map(|desc| {
            let text = desc.get_description_text();
            let text = text.trim();
            text.strip_prefix('@').unwrap_or(text).trim().to_string()
        })
        .filter(|text| !text.is_empty())
}

/// Determines whether runtime members of `decl` belong on the page of the
/// annotated class. A `---@type` annotation on the declaration overrides the
/// `---@class` instance, so members of an unrelated table (for example
/// `---@class Iterator` above `---@type table<string, any> local methods = {}`)
/// must not leak onto the class page.
fn decl_belongs_to_type(model: &SemanticModel<'_>, decl: &SemanticId, defs: &[TypeDef]) -> bool {
    let file_id = member_file_id(decl);
    let model = model.model_for(file_id);
    let Some(facts) = model.file_facts() else {
        return false;
    };
    let Some(decl) = facts.decl_by_id(decl) else {
        return false;
    };
    if decl.doc_type_syntax.is_none() {
        return true;
    }
    model
        .type_of_decl(&decl.id)
        .is_some_and(|ty| type_matches_decl(&ty, defs))
}

fn type_matches_decl(ty: &LuaType, defs: &[TypeDef]) -> bool {
    match ty {
        LuaType::Ref(id) | LuaType::Def(id) => defs.iter().any(|def| def.type_decl_id() == *id),
        LuaType::Instance(instance) => type_matches_decl(instance.get_base(), defs),
        _ => false,
    }
}

fn dedupe_members(members: Vec<DocMember>) -> Vec<DocMember> {
    let mut result: Vec<DocMember> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for member in members {
        if let Some(&idx) = index.get(&member.name) {
            let existing = &mut result[idx];
            if existing.signature.is_none() && member.signature.is_some() {
                existing.signature = member.signature;
            }
            if matches!(existing.ty, LuaType::Function) && !matches!(member.ty, LuaType::Function) {
                existing.ty = member.ty;
            }
            if existing.property.description.is_none() {
                existing.property.description = member.property.description;
            }
            continue;
        }
        index.insert(member.name.clone(), result.len());
        result.push(member);
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

fn expand_returns(ret: &LuaType) -> Vec<LuaType> {
    match ret {
        LuaType::Variadic(variadic) => match variadic.as_ref() {
            VariadicType::Multi(types) => types.clone(),
            VariadicType::Base(base) | VariadicType::Repeat(base) => vec![base.clone()],
        },
        _ => vec![ret.clone()],
    }
}

// --- Pure Semantic type text rendering ---

struct TypeTextRenderer<'a> {
    model: &'a DocModel,
    level: RenderLevel,
    depth: u8,
    visited: HashSet<DocTypeKey>,
}

impl TypeTextRenderer<'_> {
    fn render(&mut self, ty: &LuaType) -> String {
        if self.depth >= 12 {
            return "...".to_string();
        }
        self.depth += 1;
        let out = self.render_inner(ty);
        self.depth -= 1;
        out
    }

    fn render_inner(&mut self, ty: &LuaType) -> String {
        match ty {
            LuaType::Unknown => "unknown".to_string(),
            LuaType::Any => "any".to_string(),
            LuaType::Nil => "nil".to_string(),
            LuaType::Table => "table".to_string(),
            LuaType::Userdata => "userdata".to_string(),
            LuaType::Function => "function".to_string(),
            LuaType::Thread => "thread".to_string(),
            LuaType::Boolean => "boolean".to_string(),
            LuaType::String => "string".to_string(),
            LuaType::Integer => "integer".to_string(),
            LuaType::Number => "number".to_string(),
            LuaType::Io => "io".to_string(),
            LuaType::SelfInfer => "self".to_string(),
            LuaType::Global => "global".to_string(),
            LuaType::Never => "never".to_string(),
            LuaType::BooleanConst(value) | LuaType::DocBooleanConst(value) => value.to_string(),
            LuaType::IntegerConst(value) | LuaType::DocIntegerConst(value) => value.to_string(),
            LuaType::FloatConst(value) => format_float(*value),
            LuaType::StringConst(value) | LuaType::DocStringConst(value) => {
                format!("\"{}\"", value)
            }
            LuaType::Language(value) => value.to_string(),
            LuaType::Namespace(value) => format!("{{ {value} }}"),
            LuaType::Ref(id) | LuaType::Def(id) => self.render_named(id),
            LuaType::Array(array) => format!("{}[]", self.render(array.get_base())),
            LuaType::Tuple(tuple) => self.render_tuple(tuple),
            LuaType::Union(union) => union
                .into_vec()
                .iter()
                .map(|ty| self.render(ty))
                .collect::<Vec<_>>()
                .join(" | "),
            LuaType::Intersection(intersection) => intersection
                .get_types()
                .iter()
                .map(|ty| self.render(ty))
                .collect::<Vec<_>>()
                .join(" & "),
            LuaType::MultiLineUnion(union) => union
                .get_unions()
                .iter()
                .map(|(ty, description)| match description {
                    Some(description) => format!("{} # {description}", self.render(ty)),
                    None => self.render(ty),
                })
                .collect::<Vec<_>>()
                .join(" | "),
            LuaType::DocFunction(func) => self.render_function(func),
            LuaType::Object(object) => self.render_object(object),
            LuaType::Generic(generic) => self.render_generic(generic),
            LuaType::TableGeneric(params) => {
                let parts = params
                    .iter()
                    .map(|ty| self.render(ty))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("table<{parts}>")
            }
            LuaType::TplRef(tpl) => tpl.get_name().to_string(),
            LuaType::StrTplRef(tpl) => {
                if tpl.get_prefix().is_empty() {
                    tpl.get_name().to_string()
                } else {
                    format!("{}`{}`", tpl.get_prefix(), tpl.get_name())
                }
            }
            LuaType::Variadic(variadic) => match variadic.as_ref() {
                VariadicType::Multi(types) => {
                    let parts = types
                        .iter()
                        .map(|ty| self.render(ty))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({parts})")
                }
                VariadicType::Base(base) | VariadicType::Repeat(base) => {
                    format!("{}...", self.render(base))
                }
            },
            LuaType::Signature(_) => "fun(...)".to_string(),
            LuaType::Instance(instance) => self.render(instance.get_base()),
            LuaType::Call(call) => self.render_call(call),
            LuaType::TypeGuard(inner) => format!("TypeGuard<{}>", self.render(inner)),
            LuaType::ModuleRef(file_id) => self
                .model
                .module_name(*file_id)
                .map(|name| format!("module '{name}'"))
                .unwrap_or_else(|| "module 'unknown'".to_string()),
            LuaType::Conditional(conditional) => self.render_conditional(conditional),
            LuaType::Mapped(mapped) => self.render_mapped(mapped),
            LuaType::TableConst(table) => {
                let owner = SemanticId::member(table.file_id, table.value);
                let members = self
                    .model
                    .members_by_owner
                    .get(&owner)
                    .map(|members| members.as_slice())
                    .unwrap_or(&[]);
                let parts = members
                    .iter()
                    .map(|member| format!("{}: {}", member.name, self.render(&member.ty)))
                    .collect::<Vec<_>>();
                format!("{{ {} }}", parts.join(", "))
            }
        }
    }

    fn render_named(&mut self, id: &LuaTypeDeclId) -> String {
        let name = self.model.type_name(id);
        let key = DocTypeKey::from_lua_id(id);
        let expand = matches!(
            self.level,
            RenderLevel::Documentation | RenderLevel::Detailed
        ) && self.visited.insert(key.clone());
        if !expand {
            return name;
        }

        let Some(doc_type) = self.model.type_by_key(&key) else {
            self.visited.remove(&key);
            return name;
        };
        if doc_type.members.is_empty() {
            self.visited.remove(&key);
            return name;
        }

        let parts = doc_type
            .members
            .iter()
            .map(|member| {
                let key = if member.is_method {
                    format!("fun {}", member.name)
                } else {
                    member.name.clone()
                };
                format!("{key}: {}", self.render(&member.ty))
            })
            .collect::<Vec<_>>();
        self.visited.remove(&key);
        format!("{name}{{ {} }}", parts.join(", "))
    }

    fn render_tuple(&mut self, tuple: &LuaTupleType) -> String {
        let parts = tuple
            .get_types()
            .iter()
            .map(|ty| self.render(ty))
            .collect::<Vec<_>>()
            .join(", ");
        format!("({parts})")
    }

    fn render_function(&mut self, func: &LuaFunctionType) -> String {
        let params = func
            .get_params()
            .iter()
            .map(|(name, ty)| match ty {
                Some(ty) => format!("{name}: {}", self.render(ty)),
                None => name.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        let ret = self.render(func.get_ret());
        if ret == "unknown" || ret.is_empty() {
            format!("fun({params})")
        } else {
            format!("fun({params}) -> {ret}")
        }
    }

    fn render_object(&mut self, object: &LuaObjectType) -> String {
        let mut parts = object
            .get_fields()
            .iter()
            .map(|(key, ty)| {
                let key = match key {
                    LuaMemberKey::None => "_".to_string(),
                    LuaMemberKey::Integer(index) => format!("[{index}]"),
                    LuaMemberKey::Name(name) => name.to_string(),
                };
                format!("{}: {}", key, self.render(ty))
            })
            .collect::<Vec<_>>();
        parts.extend(
            object
                .get_index_access()
                .iter()
                .map(|(key, ty)| format!("[{}]: {}", self.render(key), self.render(ty))),
        );
        format!("{{ {} }}", parts.join(", "))
    }

    fn render_generic(&mut self, generic: &LuaGenericType) -> String {
        let base = self.render_named(&generic.get_base_type_id());
        let params = generic
            .get_params()
            .iter()
            .map(|ty| self.render(ty))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{base}<{params}>")
    }

    fn render_call(&mut self, call: &emmylua_code_analysis::LuaAliasCallType) -> String {
        let operands = call
            .get_operands()
            .iter()
            .map(|ty| self.render(ty))
            .collect::<Vec<_>>();
        match call.get_call_kind() {
            LuaAliasCallKind::KeyOf => format!("keyof({})", operands.join(", ")),
            LuaAliasCallKind::Index => format!(
                "({})[{}]",
                operands.first().map(String::as_str).unwrap_or("?"),
                operands.get(1).map(String::as_str).unwrap_or("")
            ),
            LuaAliasCallKind::Extends => format!(
                "{} extends {}",
                operands.first().map(String::as_str).unwrap_or("?"),
                operands.get(1).map(String::as_str).unwrap_or("")
            ),
            LuaAliasCallKind::Add => operands.join(" + "),
            LuaAliasCallKind::Sub => operands.join(" - "),
            LuaAliasCallKind::Select => format!("select({})", operands.join(", ")),
            LuaAliasCallKind::Unpack => format!("unpack({})", operands.join(", ")),
            LuaAliasCallKind::RawGet => format!("rawget({})", operands.join(", ")),
            LuaAliasCallKind::Merge => format!("merge({})", operands.join(", ")),
        }
    }

    fn render_conditional(&mut self, conditional: &LuaConditionalType) -> String {
        format!(
            "{} extends {} and {} or {}",
            self.render(conditional.get_checked_type()),
            self.render(conditional.get_extends_type()),
            self.render(conditional.get_true_type()),
            self.render(conditional.get_false_type())
        )
    }

    fn render_mapped(&mut self, mapped: &LuaMappedType) -> String {
        let readonly = if mapped.is_readonly { "readonly " } else { "" };
        let constraint = mapped
            .param
            .1
            .constraint
            .as_ref()
            .map(|ty| self.render(ty))
            .unwrap_or_else(|| "unknown".to_string());
        let optional = if mapped.is_optional { "?" } else { "" };
        format!(
            "{{ {readonly}[{} in {constraint}]{optional}: {}; }}",
            mapped.param.1.name,
            self.render(&mapped.value)
        )
    }
}

fn format_float(value: f64) -> String {
    let text = value.to_string();
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
    }
}
