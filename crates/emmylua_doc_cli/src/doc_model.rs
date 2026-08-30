//! Pure Salsa doc model: projects the data needed for documentation generation
//! (modules / types / members / globals / function signatures / type rendering)
//! directly from `SalsaDatabase` / `SemanticModel` / `FileFacts`.
//!
//! It bypasses the old `DbIndex` index and property/signature structures; `LuaType`
//! is just the value type projected by the Salsa facade, and rendering is done here.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use emmylua_code_analysis::{
    Decl, DeclKind, EmmyLuaAnalysis, FileId, LuaAliasCallKind, LuaConditionalType, LuaFunctionType,
    LuaGenericType, LuaMappedType, LuaMemberKey, LuaObjectType, LuaTupleType, LuaType,
    LuaTypeDeclId, Member, ModuleExport, RenderLevel, SalsaSemanticModel, SalsaSignature,
    SemanticId, TypeDef, TypeDefKind, TypeVisibility, VariadicType,
};
use emmylua_parser::{LuaSyntaxId, VisibilityKind};
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

/// Property annotations (Salsa facts only carry visibility and deprecated; description text is not in facts yet).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocProperty {
    pub visibility: Option<VisibilityKind>,
    pub deprecated: bool,
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

/// Additional function info projected from Salsa `Signature`/`SignatureDoc`.
#[derive(Debug, Clone)]
pub struct DocSignature {
    pub overloads: Vec<LuaType>,
    pub nodiscard: Option<String>,
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
    pub ty: LuaType,
    pub property: DocProperty,
    pub is_method: bool,
    pub loc: Option<DocLoc>,
    pub signature: Option<DocSignature>,
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
    pub property: DocProperty,
}

#[derive(Debug, Clone)]
pub struct DocGlobal {
    pub name: String,
    pub ty: LuaType,
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
        let salsa = &analysis.salsa;

        let mut file_ids = salsa.file_ids();
        file_ids.sort();
        let mut main_file_ids: HashSet<FileId> =
            salsa.main_workspace_file_ids().into_iter().collect();
        // When main_root is not set, Salsa falls back to all files; old doc behavior also used all main workspace files.
        if salsa.main_root().is_none() {
            main_file_ids.extend(file_ids.iter().copied());
        }

        let mut file_paths = HashMap::new();
        let mut module_name_by_file = HashMap::new();

        // First pass: collect facts.
        let mut type_builders: HashMap<DocTypeKey, Vec<TypeDef>> = HashMap::new();
        let mut all_type_defs: Vec<TypeDef> = Vec::new();
        #[allow(clippy::type_complexity)]
        let mut member_sources: HashMap<SemanticId, Vec<(FileId, Member)>> = HashMap::new();
        let mut signature_sources: HashMap<LuaSyntaxId, (FileId, SalsaSignature)> = HashMap::new();
        let mut global_sources: Vec<(FileId, Decl)> = Vec::new();
        let mut module_sources: Vec<ModuleSource> = Vec::new();
        let mut properties: HashMap<DocOwner, DocProperty> = HashMap::new();

        for file_id in file_ids {
            let Some(path) = salsa.file_path(file_id) else {
                continue;
            };
            file_paths.insert(file_id, path.clone());
            if main_file_ids.contains(&file_id)
                && let Some(name) = salsa.module_name_of(file_id)
            {
                module_name_by_file.insert(file_id, name);
            }

            let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
                continue;
            };
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
                    },
                );
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
            if let Some(model) = SalsaSemanticModel::new(salsa, *file_id) {
                signatures.insert(*syntax, project_signature(&model, signature));
            }
        }

        // Project members (cross-file owner -> member).
        let mut member_by_id: HashMap<SemanticId, DocMember> = HashMap::new();
        let mut members_by_owner: HashMap<SemanticId, Vec<DocMember>> = HashMap::new();
        for (owner, sources) in &member_sources {
            for (file_id, member) in sources {
                if member_by_id.contains_key(&member.id) {
                    continue;
                }
                let Some(model) = SalsaSemanticModel::new(salsa, *file_id) else {
                    continue;
                };
                let signature = member
                    .value_syntax
                    .as_ref()
                    .and_then(|syntax| signatures.get(syntax));
                let doc = project_member(&model, member, &file_paths, signature);
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
            let Some(model) = SalsaSemanticModel::new(salsa, first.file_id) else {
                continue;
            };
            let kind = match first.kind {
                TypeDefKind::Class => DocTypeKind::Class,
                TypeDefKind::Enum => DocTypeKind::Enum,
                TypeDefKind::Alias => DocTypeKind::Alias,
            };
            let mut members: Vec<DocMember> = Vec::new();
            let mut seen_members = HashSet::new();
            for def in &defs {
                if let Some(list) = members_by_owner.get(&def.id) {
                    for member in list {
                        if seen_members.insert(member.name.clone()) {
                            members.push(member.clone());
                        }
                    }
                }
            }
            members.sort_by(|a, b| a.name.cmp(&b.name));

            let mut bases = Vec::new();
            for def in &defs {
                for super_name in &def.super_names {
                    if let Some(super_def) = all_type_defs
                        .iter()
                        .find(|candidate| candidate.full_name == *super_name)
                        .or_else(|| {
                            all_type_defs
                                .iter()
                                .find(|candidate| candidate.name == *super_name)
                        })
                    {
                        bases.push(LuaType::Ref(type_def_key(super_def).to_lua_id()));
                    }
                }
            }
            bases.dedup();

            let locations = defs
                .iter()
                .filter_map(|def| make_loc(salsa, &file_paths, def.file_id, def.name_range))
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
            let Some(model) = SalsaSemanticModel::new(salsa, *file_id) else {
                continue;
            };
            let ty = model.type_of_decl(&decl.id).unwrap_or(LuaType::Unknown);
            globals.push(DocGlobal {
                name: decl.name.to_string(),
                property: properties
                    .get(&DocOwner::Decl(decl.id.clone()))
                    .cloned()
                    .unwrap_or_default(),
                loc: make_loc(salsa, &file_paths, *file_id, decl.name_range),
                signature: decl
                    .value_expr_syntax
                    .and_then(|syntax| signatures.get(&syntax).cloned()),
                ty,
            });
        }
        globals.sort_by(|a, b| a.name.cmp(&b.name));

        // Finalize modules.
        let mut modules = Vec::new();
        for source in module_sources {
            let Some(model) = SalsaSemanticModel::new(salsa, source.file_id) else {
                continue;
            };
            let export = match &source.module_export {
                ModuleExport::Decl { decl, .. } => {
                    let ty = model.type_of_decl(decl).unwrap_or(LuaType::Unknown);
                    Some((slot0(ty), Some(DocOwner::Decl(decl.clone()))))
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
                    Some((slot0(ty), decl.map(DocOwner::Decl)))
                }
                ModuleExport::Expr { value_syntax } => {
                    let ty = slot0(model.type_of_expr(*value_syntax));
                    Some((
                        ty,
                        Some(DocOwner::Expr(source.file_id, value_syntax.get_range())),
                    ))
                }
                ModuleExport::None => None,
            };
            let Some((export_type, export_owner)) = export else {
                continue;
            };

            let property = export_owner
                .as_ref()
                .and_then(|owner| properties.get(owner).cloned())
                .unwrap_or_default();
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
                    if seen.insert(member.name.clone()) {
                        members.push(member.clone());
                    }
                }
            }
        }
        members.sort_by(|a, b| a.name.cmp(&b.name));
        members
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
    salsa: &emmylua_code_analysis::SalsaDatabase,
    file_paths: &HashMap<FileId, PathBuf>,
    file_id: FileId,
    range: TextRange,
) -> Option<DocLoc> {
    let file = file_paths.get(&file_id)?.clone();
    let line = match (salsa.line_index(file_id), salsa.get_file_text(file_id)) {
        (Some(index), Some(text)) => index
            .get_line_col(range.start(), text)
            .map(|(line, _)| line + 1),
        _ => None,
    };
    Some(DocLoc {
        file,
        line: line.unwrap_or_default(),
    })
}

fn project_member(
    model: &SalsaSemanticModel<'_>,
    member: &Member,
    file_paths: &HashMap<FileId, PathBuf>,
    signature: Option<&DocSignature>,
) -> DocMember {
    let file_id = member_file_id(&member.id);
    let ty = model.type_of_member(&member.id).unwrap_or(LuaType::Unknown);
    let key_range = member.id.member_key_range().unwrap_or_else(|| {
        member
            .value_syntax
            .map(|syntax| syntax.get_range())
            .unwrap_or_default()
    });
    DocMember {
        name: match &member.key {
            LuaMemberKey::Name(name) => name.to_string(),
            LuaMemberKey::Integer(index) => format!("[{index}]"),
            LuaMemberKey::None | LuaMemberKey::TypeKey(_) => String::new(),
        },
        ty,
        property: DocProperty {
            visibility: Some(member.visibility),
            deprecated: member.deprecated,
        },
        is_method: member.is_method,
        loc: make_loc(model.db(), file_paths, file_id, key_range),
        signature: signature.cloned(),
    }
}

fn member_file_id(id: &SemanticId) -> FileId {
    match id {
        SemanticId::Member(key) => key.file_id,
        SemanticId::Decl(key) => key.file_id,
        SemanticId::Signature(key) => key.file_id,
        _ => FileId::new(0),
    }
}

fn project_signature(model: &SalsaSemanticModel<'_>, signature: &SalsaSignature) -> DocSignature {
    let Some(docs) = signature.docs.as_deref() else {
        return DocSignature {
            overloads: Vec::new(),
            nodiscard: None,
        };
    };

    let file_id = signature.file_id;
    let generics = docs.generic_params.clone();
    DocSignature {
        overloads: docs
            .overloads
            .iter()
            .map(|syntax| model.doc_type_lua_in(file_id, *syntax, &generics))
            .collect(),
        nodiscard: docs.nodiscard.as_ref().map(|msg| msg.to_string()),
    }
}

fn expand_returns(ret: &LuaType) -> Vec<LuaType> {
    match ret {
        LuaType::Variadic(variadic) => match variadic.as_ref() {
            VariadicType::Multi(types) => types.clone(),
            VariadicType::Base(base) => vec![base.clone()],
        },
        _ => vec![ret.clone()],
    }
}

// --- Pure Salsa type text rendering ---

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
                VariadicType::Base(base) => format!("{}...", self.render(base)),
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
            .map(|(key, ty)| format!("{}: {}", self.render_member_key(key), self.render(ty)))
            .collect::<Vec<_>>();
        parts.extend(
            object
                .get_index_access()
                .iter()
                .map(|(key, ty)| format!("[{}]: {}", self.render(key), self.render(ty))),
        );
        format!("{{ {} }}", parts.join(", "))
    }

    fn render_member_key(&mut self, key: &LuaMemberKey) -> String {
        match key {
            LuaMemberKey::None => "_".to_string(),
            LuaMemberKey::Integer(index) => format!("[{index}]"),
            LuaMemberKey::Name(name) => name.to_string(),
            LuaMemberKey::TypeKey(ty) => format!("[{}]", self.render(ty)),
        }
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
