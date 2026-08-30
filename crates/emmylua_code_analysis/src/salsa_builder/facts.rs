//! # FileFacts: the minimal per-file fact arena
//!
//! Only stores raw facts that require whole-file inspection: declarations + lexical scopes + type definitions.
//! Everything else (types, resolution, narrowing) is node-keyed salsa queries.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaAstToken, LuaBlock, LuaChunk, LuaClosureExpr, LuaComment,
    LuaDocFieldKey, LuaDocGenericDeclList, LuaDocTag, LuaDocTagAttributeUse, LuaDocTagDiagnostic,
    LuaDocTagField, LuaDocType, LuaDocTypeFlag, LuaExpr, LuaFuncStat, LuaIfClauseStat,
    LuaIndexExpr, LuaIndexKey, LuaLiteralExpr, LuaLiteralToken, LuaLocalStat, LuaNameToken,
    LuaStat, LuaSyntaxId, LuaSyntaxKind, LuaTableExpr, LuaTableField, LuaVarExpr,
    LuaVersionCondition, NumberResult, UnaryOperator,
};
use rowan::{TextRange, TextSize, WalkEvent};
use smol_str::SmolStr;

use crate::DiagnosticCode;
use crate::FileId;

use super::index::{Bucket, build_buckets, find_bucket};

// ──────────────────────────────────────────────
// Type definitions (from def/, with global SemanticId identity)
// ──────────────────────────────────────────────

use super::def::{
    ConstructorAttribute, ConstructorReturnMode, Decl, DeclKind, LuaMemberKey, Member,
    ModuleExport, ModuleVisibility, NameUse, OperatorDef, SalsaGenericParam, Scope, ScopeChild,
    ScopeKind, SemanticId, Signature, SignatureDoc, TypeDef, TypeDefFlags, TypeDefKind, TypeScope,
    TypeVisibility,
};
use crate::WorkspaceId;
use emmylua_parser::VisibilityKind;

/// `---@diagnostic disable[-next-line|-line]` annotation: affected range + disabled code (None = all).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticDisable {
    pub range: TextRange,
    pub code: Option<DiagnosticCode>,
}

/// Doc annotation usage error (consumed by analyze_error checker; currently only `@field` without a `@class` context).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationError {
    pub range: TextRange,
    pub message: SmolStr,
}

impl DiagnosticDisable {
    /// Same semantics as the old `DiagnosticAction::is_match(disable=true)`: ranges intersect and codes match.
    pub fn matches(&self, range: &TextRange, code: &DiagnosticCode) -> bool {
        if self.range.intersect(*range).is_none() {
            return false;
        }
        match &self.code {
            Some(disable_code) => disable_code == code,
            None => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts {
    pub file_id: FileId,
    pub decls: Vec<Decl>,
    pub scopes: Vec<Scope>,
    pub type_defs: Vec<TypeDef>,
    pub members: Vec<Member>,
    pub name_uses: Vec<NameUse>,
    /// All index-expression use sites in the file (`a.b` / `arr[1]` / `t[k]`).
    /// For on-demand cross-file reference index resolution, avoiding cross-file resolve during FileFacts.
    pub member_uses: Vec<LuaSyntaxId>,
    pub signatures: Vec<Signature>,
    pub module_export: ModuleExport,
    /// Module export visibility (`---@meta no-require` → Hide; export target `---@internal/@public` tags).
    pub module_visibility: ModuleVisibility,
    /// Whether the file contains `---@meta`.
    pub is_meta: bool,
    /// Module-level `---@version` conditions (require visibility filtered by runtime version).
    pub version_conds: Vec<LuaVersionCondition>,
    /// `---@operator add(Vector): Vector` operator overloads.
    pub operators: Vec<OperatorDef>,
    /// `---@namespace foo`: namespace prefix for file types.
    pub namespace: Option<SmolStr>,
    /// `---@using bar`: qualified namespace list used during type resolution.
    pub usings: Vec<SmolStr>,
    /// `---@diagnostic disable-next-line/disable-line/disable` annotations (range-matched).
    pub diagnostic_disables: Vec<DiagnosticDisable>,
    /// File-level `---@diagnostic disable: code`.
    pub file_diagnostic_disabled: HashSet<DiagnosticCode>,
    /// File-level `---@diagnostic enable: code` (force-enable, overriding global config disables).
    pub file_diagnostic_enabled: HashSet<DiagnosticCode>,
    /// Doc annotation usage errors (`@field` must be under a `@class`, etc.).
    pub annotation_errors: Vec<AnnotationError>,

    // Buckets / indexes
    decl_by_name: Vec<Bucket<SmolStr>>,
    decl_by_id: HashMap<SemanticId, usize>,
    member_by_id: HashMap<SemanticId, usize>,
    type_def_by_id: HashMap<SemanticId, usize>,
}

impl FileFacts {
    pub fn decl(&self, index: usize) -> Option<&Decl> {
        self.decls.get(index)
    }

    pub fn decl_by_id(&self, id: &SemanticId) -> Option<&Decl> {
        let index = self.decl_by_id.get(id)?;
        self.decls.get(*index)
    }

    pub fn member(&self, index: usize) -> Option<&Member> {
        self.members.get(index)
    }

    pub fn member_by_id(&self, id: &SemanticId) -> Option<&Member> {
        let index = self.member_by_id.get(id)?;
        self.members.get(*index)
    }

    /// Members whose owner is `SemanticId` (in this file).
    pub fn members_of_owner(&self, owner: &SemanticId) -> impl Iterator<Item = &Member> {
        self.members
            .iter()
            .filter(move |member| &member.owner == owner)
    }

    /// Type operator overloads (by owner + operator name).
    pub fn operator_of(&self, owner: &SemanticId, name: &str) -> Option<&OperatorDef> {
        self.operators
            .iter()
            .find(|op| &op.owner == owner && op.name == name)
    }

    /// `@field` members of the named type (`TypeDef` id).
    pub fn field_members_of_type(&self, type_def_id: &SemanticId, name: &str) -> Option<&Member> {
        self.members
            .iter()
            .find(|member| member.owner == *type_def_id && member.key.name() == Some(name))
    }

    pub fn type_def_by_id(&self, id: &SemanticId) -> Option<&TypeDef> {
        let index = self.type_def_by_id.get(id)?;
        self.type_defs.get(*index)
    }

    pub fn type_def_by_full_name(&self, full_name: &str) -> Option<&TypeDef> {
        self.type_defs.iter().find(|def| def.full_name == full_name)
    }

    pub fn type_def_by_name(&self, name: &str) -> Option<&TypeDef> {
        self.type_defs.iter().find(|def| def.name == name)
    }

    /// Finds the first declaration by name (any kind, in this file).
    pub fn decl_named(&self, name: &str) -> Option<&Decl> {
        self.decls.iter().find(|decl| decl.name == name)
    }

    pub fn signature_by_closure(&self, closure_syntax: LuaSyntaxId) -> Option<&Signature> {
        self.signatures
            .iter()
            .find(|sig| sig.closure_syntax == closure_syntax)
    }

    /// Finds a declaration by name visible at `offset` (scope-aware: visible within its enclosing Block/Chunk/FuncStat/Closure).
    pub fn find_visible_decl_before_offset(&self, name: &str, offset: TextSize) -> Option<&Decl> {
        let indices = find_bucket(&self.decl_by_name, &SmolStr::new(name))?;
        indices
            .iter()
            .filter_map(|&i| self.decls.get(i as usize))
            .filter(|decl| {
                visible_from(&self.scopes, decl) <= offset
                    && offset <= visibility_end(&self.scopes, decl.scope_id)
            })
            .max_by_key(|decl| visible_from(&self.scopes, decl))
    }

    /// All declarations lexically visible at `offset` (used by completion env provider).
    pub fn visible_decls_at_offset(&self, offset: TextSize) -> Vec<&Decl> {
        self.decls
            .iter()
            .filter(|decl| {
                visible_from(&self.scopes, decl) <= offset
                    && offset <= visibility_end(&self.scopes, decl.scope_id)
            })
            .collect()
    }

    /// Whether this code in this range is disabled by a `---@diagnostic disable*` annotation (mirrors old `is_file_diagnostic_code_disabled`).
    pub fn is_range_diagnostic_disabled(&self, code: &DiagnosticCode, range: &TextRange) -> bool {
        self.diagnostic_disables
            .iter()
            .any(|disable| disable.matches(range, code))
    }
}

/// Start of declaration visibility:
/// `local x = ...` is visible only **after the statement ends** (not inside its own initializer; in `local x = x`, the right-hand x is global),
/// other declarations (including `local function`, registered in the outer scope) are visible from their name position.
fn visible_from(scopes: &[Scope], decl: &Decl) -> TextSize {
    let scope = &scopes[decl.scope_id as usize];
    if matches!(scope.kind, ScopeKind::LocalStat) {
        scope.end
    } else {
        decl.name_offset()
    }
}

/// Declaration visibility ends at the end of the nearest "visibility boundary" scope (Block/Chunk/FuncStat/Closure/Repeat).
/// In Lua, local declarations are visible throughout the enclosing block, not just their declaration statement.
fn visibility_end(scopes: &[Scope], scope_id: u32) -> TextSize {
    let mut current = scope_id;
    loop {
        let scope = &scopes[current as usize];
        if matches!(
            scope.kind,
            ScopeKind::Block
                | ScopeKind::Chunk
                | ScopeKind::FuncStat
                | ScopeKind::Closure
                | ScopeKind::Repeat
        ) {
            return scope.end;
        }
        match scope.parent {
            Some(parent) => current = parent,
            None => return scope.end,
        }
    }
}

/// Performs the same scope-aware lookup on already-collected decls/scopes (used during construction).
fn find_visible_decl<'a>(
    decls: &'a [Decl],
    scopes: &[Scope],
    name: &str,
    offset: TextSize,
) -> Option<&'a Decl> {
    decls
        .iter()
        .filter(|decl| {
            decl.name == name
                && visible_from(scopes, decl) <= offset
                && offset <= visibility_end(scopes, decl.scope_id)
        })
        .max_by_key(|decl| visible_from(scopes, decl))
}

// ──────────────────────────────────────────────
// Extraction walker (implemented from scratch: declarations + scopes + type definitions)
// ──────────────────────────────────────────────

pub struct FactsBuilder {
    file_id: FileId,
    workspace_id: WorkspaceId,
    decls: Vec<Decl>,
    scopes: Vec<Scope>,
    scope_stack: Vec<u32>,
    type_defs: Vec<TypeDef>,
    members: Vec<Member>,
    name_uses: Vec<NameUse>,
    member_uses: Vec<LuaSyntaxId>,
    signatures: Vec<Signature>,
    module_export: ModuleExport,
    operators: Vec<OperatorDef>,
    namespace: Option<SmolStr>,
    usings: Vec<SmolStr>,
    /// Current statement being processed (for doc comment ownership).
    current_owner_syntax: Option<LuaSyntaxId>,
    /// Whether the comment block right after the statement contains `---@deprecated` (consumed by runtime members `T.x = v`; reset after statement processing).
    current_comment_deprecated: bool,
    /// `---@type` annotations: owner statement position → type nodes.
    doc_type_map: HashMap<LuaSyntaxId, Vec<LuaSyntaxId>>,
    /// `---@module "name"` annotations: owner statement position → module name.
    doc_module_map: HashMap<LuaSyntaxId, SmolStr>,
    /// Owner statements of `---@deprecated` annotations (attributed to declarations).
    doc_deprecated_owners: HashSet<LuaSyntaxId>,
    /// Owner statements of `---@readonly` annotations (attributed to declarations / members).
    doc_readonly_owners: HashSet<LuaSyntaxId>,
    /// Function doc annotations: owner statement → signature doc details.
    signature_doc_map: HashMap<LuaSyntaxId, SignatureDoc>,
    /// Owner statements with any doc comment block (including empty `---` comments).
    doc_comment_owners: HashSet<LuaSyntaxId>,
    /// Most recent bare `@class/@alias/@enum` name (for `@field` ownership).
    current_class: Option<SemanticId>,
    /// `---@[constructor("init")]`: constructor attributes waiting for the following `---@param`.
    /// The attribute line and `@param` line may be in adjacent comment blocks (mirrors old `find_attach_attribute`).
    pending_constructor: Option<(LuaSyntaxId, ConstructorAttribute)>,
    /// `---@[lsp_optimization("delayed_definition")]`: declaration types delayed until a later assignment.
    delayed_definition_owners: HashSet<LuaSyntaxId>,
    /// `---@diagnostic` annotation facts.
    diagnostic_disables: Vec<DiagnosticDisable>,
    file_diagnostic_disabled: HashSet<DiagnosticCode>,
    file_diagnostic_enabled: HashSet<DiagnosticCode>,
    /// File contains `---@meta` (Meta flag for type definitions).
    is_meta: bool,
    /// `---@meta` name (`no-require` / `_` → module Hide).
    meta_name: Option<SmolStr>,
    /// Visibility tags such as `---@internal/@public`: owner statement → visibility.
    visibility_labels: HashMap<LuaSyntaxId, VisibilityKind>,
    /// Module export visibility (post-processing: first top-level return).
    module_visibility: ModuleVisibility,
    /// Module-level `---@version` conditions.
    module_version_conds: Vec<LuaVersionCondition>,
    /// Doc annotation usage errors (`@field` must be under a `@class`, etc.).
    annotation_errors: Vec<AnnotationError>,
}

/// Module return flow (collects only the first reachable `return expr`, for module export).
#[derive(Default, Clone)]
struct ModuleReturnFlow {
    first_expr: Option<LuaExpr>,
    can_fall_through: bool,
    can_break: bool,
}

impl ModuleReturnFlow {
    fn fallthrough() -> Self {
        Self {
            can_fall_through: true,
            ..Default::default()
        }
    }

    fn merge_choice(&mut self, other: Self) {
        if self.first_expr.is_none() {
            self.first_expr = other.first_expr;
        }
        self.can_fall_through |= other.can_fall_through;
        self.can_break |= other.can_break;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModuleConditionState {
    Truthy,
    Falsy,
    Dynamic,
}

fn module_condition_state(expr: &LuaExpr) -> ModuleConditionState {
    match module_static_truthiness(expr) {
        Some(true) => ModuleConditionState::Truthy,
        Some(false) => ModuleConditionState::Falsy,
        None => ModuleConditionState::Dynamic,
    }
}

fn module_static_truthiness(expr: &LuaExpr) -> Option<bool> {
    match expr {
        LuaExpr::LiteralExpr(literal_expr) => match literal_expr.get_literal()? {
            LuaLiteralToken::Bool(bool_token) => Some(bool_token.is_true()),
            LuaLiteralToken::Nil(_) => Some(false),
            LuaLiteralToken::String(_) | LuaLiteralToken::Number(_) => Some(true),
            LuaLiteralToken::Dots(_) | LuaLiteralToken::Question(_) => None,
        },
        LuaExpr::ParenExpr(paren_expr) => module_static_truthiness(&paren_expr.get_expr()?),
        LuaExpr::UnaryExpr(unary_expr)
            if unary_expr
                .get_op_token()
                .is_some_and(|op| op.get_op() == UnaryOperator::OpNot) =>
        {
            module_static_truthiness(&unary_expr.get_expr()?).map(|truthy| !truthy)
        }
        // Table literals / closures are always truthy in Lua.
        LuaExpr::TableExpr(_) | LuaExpr::ClosureExpr(_) => Some(true),
        _ => None,
    }
}

fn module_block_flow(block: LuaBlock) -> ModuleReturnFlow {
    let mut flow = ModuleReturnFlow::default();
    let mut can_fall_through = true;
    for stat in block.get_stats() {
        if !can_fall_through {
            break;
        }
        let stat_flow = module_stat_flow(stat);
        flow.merge_choice(stat_flow.clone());
        can_fall_through = stat_flow.can_fall_through;
    }
    flow.can_fall_through = can_fall_through;
    flow
}

fn module_optional_block_flow(block: Option<LuaBlock>) -> ModuleReturnFlow {
    match block {
        Some(block) => module_block_flow(block),
        None => ModuleReturnFlow::fallthrough(),
    }
}

fn module_stat_flow(stat: LuaStat) -> ModuleReturnFlow {
    match stat {
        LuaStat::DoStat(do_stat) => module_optional_block_flow(do_stat.get_block()),
        LuaStat::WhileStat(while_stat) => {
            let Some(condition) = while_stat.get_condition_expr() else {
                return ModuleReturnFlow::fallthrough();
            };
            match module_condition_state(&condition) {
                ModuleConditionState::Falsy => ModuleReturnFlow::fallthrough(),
                ModuleConditionState::Truthy => {
                    let body = module_optional_block_flow(while_stat.get_block());
                    ModuleReturnFlow {
                        first_expr: body.first_expr,
                        can_fall_through: body.can_break,
                        can_break: false,
                    }
                }
                ModuleConditionState::Dynamic => {
                    let body = module_optional_block_flow(while_stat.get_block());
                    ModuleReturnFlow {
                        first_expr: body.first_expr,
                        can_fall_through: true,
                        can_break: false,
                    }
                }
            }
        }
        LuaStat::RepeatStat(repeat_stat) => {
            let body = module_optional_block_flow(repeat_stat.get_block());
            let mut flow = ModuleReturnFlow {
                first_expr: body.first_expr,
                can_fall_through: body.can_break,
                can_break: false,
            };
            if body.can_fall_through {
                match repeat_stat.get_condition_expr() {
                    Some(condition) => match module_condition_state(&condition) {
                        ModuleConditionState::Truthy => flow.can_fall_through = true,
                        ModuleConditionState::Falsy => {
                            flow.can_fall_through = body.can_break;
                        }
                        ModuleConditionState::Dynamic => flow.can_fall_through = true,
                    },
                    None => flow.can_fall_through = true,
                }
            }
            flow
        }
        LuaStat::IfStat(if_stat) => {
            let mut flow = ModuleReturnFlow::default();
            let mut can_reach_next_clause = true;
            if let Some(condition) = if_stat.get_condition_expr() {
                match module_condition_state(&condition) {
                    ModuleConditionState::Truthy => {
                        return module_optional_block_flow(if_stat.get_block());
                    }
                    ModuleConditionState::Falsy => {}
                    ModuleConditionState::Dynamic => {
                        flow.merge_choice(module_optional_block_flow(if_stat.get_block()));
                    }
                }
            } else {
                return ModuleReturnFlow::fallthrough();
            }
            for clause in if_stat.get_all_clause() {
                if !can_reach_next_clause {
                    break;
                }
                match clause {
                    LuaIfClauseStat::ElseIf(clause) => {
                        if let Some(condition) = clause.get_condition_expr() {
                            match module_condition_state(&condition) {
                                ModuleConditionState::Truthy => {
                                    flow.merge_choice(module_optional_block_flow(
                                        clause.get_block(),
                                    ));
                                    can_reach_next_clause = false;
                                }
                                ModuleConditionState::Falsy => {}
                                ModuleConditionState::Dynamic => {
                                    flow.merge_choice(module_optional_block_flow(
                                        clause.get_block(),
                                    ));
                                }
                            }
                        } else {
                            can_reach_next_clause = false;
                        }
                    }
                    LuaIfClauseStat::Else(clause) => {
                        flow.merge_choice(module_optional_block_flow(clause.get_block()));
                        can_reach_next_clause = false;
                    }
                }
            }
            if can_reach_next_clause {
                flow.can_fall_through = true;
            }
            flow
        }
        LuaStat::ForStat(for_stat) => {
            let body = module_optional_block_flow(for_stat.get_block());
            ModuleReturnFlow {
                first_expr: body.first_expr,
                can_fall_through: true,
                can_break: false,
            }
        }
        LuaStat::ForRangeStat(for_range_stat) => {
            let body = module_optional_block_flow(for_range_stat.get_block());
            ModuleReturnFlow {
                first_expr: body.first_expr,
                can_fall_through: true,
                can_break: false,
            }
        }
        LuaStat::BreakStat(_) => ModuleReturnFlow {
            can_break: true,
            ..Default::default()
        },
        LuaStat::ReturnStat(return_stat) => {
            let mut flow = ModuleReturnFlow::default();
            if let Some(expr) = return_stat.get_expr_list().next() {
                flow.first_expr = Some(expr);
            }
            flow
        }
        _ => ModuleReturnFlow::fallthrough(),
    }
}

impl FactsBuilder {
    pub fn new(file_id: FileId, workspace_id: WorkspaceId) -> Self {
        Self {
            file_id,
            workspace_id,
            decls: Vec::new(),
            scopes: Vec::new(),
            scope_stack: Vec::new(),
            type_defs: Vec::new(),
            members: Vec::new(),
            name_uses: Vec::new(),
            member_uses: Vec::new(),
            signatures: Vec::new(),
            module_export: ModuleExport::None,
            operators: Vec::new(),
            namespace: None,
            usings: Vec::new(),
            current_owner_syntax: None,
            current_comment_deprecated: false,
            doc_type_map: HashMap::new(),
            doc_module_map: HashMap::new(),
            doc_deprecated_owners: HashSet::new(),
            doc_readonly_owners: HashSet::new(),
            signature_doc_map: HashMap::new(),
            doc_comment_owners: HashSet::new(),
            current_class: None,
            pending_constructor: None,
            delayed_definition_owners: HashSet::new(),
            diagnostic_disables: Vec::new(),
            file_diagnostic_disabled: HashSet::new(),
            file_diagnostic_enabled: HashSet::new(),
            is_meta: false,
            meta_name: None,
            visibility_labels: HashMap::new(),
            module_visibility: ModuleVisibility::Public,
            module_version_conds: Vec::new(),
            annotation_errors: Vec::new(),
        }
    }

    pub fn build(mut self, chunk: &LuaChunk, text: &str) -> FileFacts {
        // One full tree walk: scopes/declarations/members + comments (namespace/using/type defs/---@type).
        // Namespace dependencies and ---@type ownership are resolved in post-processing (over collected Vecs, without re-walking the tree).
        for event in chunk.walk_descendants::<LuaAst>() {
            match event {
                WalkEvent::Enter(node) => self.enter(&node, text),
                WalkEvent::Leave(node) => self.leave(&node),
            }
        }

        // Post-processing 0: inline `---@type` ownership for table fields (comments enter doc_type_map after the field node is visited).
        self.assign_member_doc_types();

        // Post-processing 1: type-def full_name depends on namespace (which may appear after the class definition).
        self.finalize_type_defs();

        // Post-processing 2: attach ---@type to declarations by owner.
        self.assign_doc_types();

        // Post-processing 2.5: apply delayed-definition attributes to declarations.
        for decl in &mut self.decls {
            if decl
                .owner_syntax
                .is_some_and(|owner| self.delayed_definition_owners.contains(&owner))
            {
                decl.delayed_definition = true;
            }
        }

        // Post-processing 3: module export from top-level return (needs decls fully collected).
        self.collect_module_export(chunk);

        let mut decl_entries = self
            .decls
            .iter()
            .enumerate()
            .map(|(i, decl)| (decl.name.clone(), i as u32))
            .collect::<Vec<_>>();
        decl_entries.sort_by(|a, b| (a.0.as_str(), a.1).cmp(&(b.0.as_str(), b.1)));
        let decl_by_name = build_buckets(decl_entries);

        let decl_by_id = self
            .decls
            .iter()
            .enumerate()
            .map(|(i, decl)| (decl.id.clone(), i))
            .collect();
        let member_by_id = self
            .members
            .iter()
            .enumerate()
            .map(|(i, member)| (member.id.clone(), i))
            .collect();
        let type_def_by_id = self
            .type_defs
            .iter()
            .enumerate()
            .map(|(i, def)| (def.id.clone(), i))
            .collect();

        FileFacts {
            file_id: self.file_id,
            decls: self.decls,
            scopes: self.scopes,
            type_defs: self.type_defs,
            members: self.members,
            name_uses: self.name_uses,
            member_uses: self.member_uses,
            signatures: self.signatures,
            module_export: self.module_export,
            module_visibility: self.module_visibility,
            is_meta: self.is_meta,
            version_conds: self.module_version_conds,
            operators: self.operators,
            namespace: self.namespace,
            usings: self.usings,
            diagnostic_disables: self.diagnostic_disables,
            file_diagnostic_disabled: self.file_diagnostic_disabled,
            file_diagnostic_enabled: self.file_diagnostic_enabled,
            annotation_errors: self.annotation_errors,
            decl_by_name,
            decl_by_id,
            member_by_id,
            type_def_by_id,
        }
    }

    fn collect_doc_comment(&mut self, comment: &LuaComment, text: &str) {
        let owner_syntax = comment.get_owner().map(|owner| owner.get_syntax_id());
        if let Some(owner_syntax) = owner_syntax {
            self.doc_comment_owners.insert(owner_syntax);
        }
        let tags: Vec<LuaDocTag> = comment.get_doc_tags().collect();
        // `---@deprecated` ownership (mirrors old `analyze_deprecated` tag-position semantics):
        // 1) if the next tag is `@field` → only mark that field;
        // 2) otherwise → mark the type (`@class`/`@alias`/`@enum`) + statement owner (decl/member/signature).
        // Consecutive doc lines in one comment block are merged into one LuaComment by the parser; tag order is line order.
        let mut field_deprecated: HashSet<usize> = HashSet::new();
        let mut owner_type_deprecated = false;
        for (idx, tag) in tags.iter().enumerate() {
            if matches!(tag, LuaDocTag::Deprecated(_)) {
                if matches!(tags.get(idx + 1), Some(LuaDocTag::Field(_))) {
                    field_deprecated.insert(idx + 1);
                } else {
                    owner_type_deprecated = true;
                }
            }
        }
        self.current_comment_deprecated = owner_type_deprecated;
        for (idx, tag) in tags.iter().enumerate() {
            match tag {
                LuaDocTag::Namespace(ns_tag) => {
                    // An incomplete `---@namespace <??>` has no name token, so don't clear an already parsed namespace.
                    if let Some(name) = ns_tag.get_name_token() {
                        self.namespace = Some(name.get_name_text().into());
                    }
                }
                LuaDocTag::Using(using_tag) => {
                    if let Some(name) = using_tag.get_name_token() {
                        self.usings.push(name.get_name_text().into());
                    }
                }
                LuaDocTag::Class(class_tag) => {
                    let supers = class_tag
                        .get_supers()
                        .map(|list| {
                            list.get_types()
                                .flat_map(|doc_type| doc_type_names(&doc_type))
                                .collect()
                        })
                        .unwrap_or_default();
                    self.push_type_def(
                        class_tag.get_name_token(),
                        TypeDefKind::Class,
                        class_tag.get_type_flag(),
                        supers,
                        collect_generics(class_tag.get_generic_decl()),
                        owner_type_deprecated,
                        None,
                        owner_syntax,
                    );
                }
                LuaDocTag::Alias(alias_tag) => {
                    self.push_type_def(
                        alias_tag.get_name_token(),
                        TypeDefKind::Alias,
                        alias_tag.get_type_flag(),
                        Vec::new(),
                        collect_generics(alias_tag.get_generic_decl_list()),
                        owner_type_deprecated,
                        alias_tag.get_type().map(|t| t.get_syntax_id()),
                        owner_syntax,
                    );
                }
                LuaDocTag::Enum(enum_tag) => {
                    self.push_type_def(
                        enum_tag.get_name_token(),
                        TypeDefKind::Enum,
                        enum_tag.get_type_flag(),
                        Vec::new(),
                        Vec::new(),
                        owner_type_deprecated,
                        None,
                        owner_syntax,
                    );
                }
                LuaDocTag::Field(field_tag) => {
                    if self.current_class.is_none() {
                        self.annotation_errors.push(AnnotationError {
                            range: field_tag.get_range(),
                            message: SmolStr::new("`@field` must be used under a `@class`"),
                        });
                    }
                    self.collect_field_member(&field_tag, field_deprecated.contains(&idx));
                }
                LuaDocTag::Operator(operator_tag) => {
                    if let (Some(owner), Some(name_token), Some(returns)) = (
                        self.current_class.clone(),
                        operator_tag.get_name_token(),
                        operator_tag.get_return_type(),
                    ) {
                        let params = operator_tag
                            .get_param_list()
                            .map(|list| {
                                list.get_types()
                                    .map(|doc_type| doc_type.get_syntax_id())
                                    .collect()
                            })
                            .unwrap_or_default();
                        self.operators.push(OperatorDef {
                            owner,
                            name: name_token.get_name_text().into(),
                            params,
                            returns: returns.get_syntax_id(),
                        });
                    }
                }
                LuaDocTag::Readonly(readonly) => {
                    let _ = readonly;
                    if let Some(owner_syntax) = owner_syntax {
                        self.doc_readonly_owners.insert(owner_syntax);
                    }
                }
                LuaDocTag::Nodiscard(nodiscard) => {
                    if let Some(owner_syntax) = owner_syntax {
                        use emmylua_parser::LuaDocDescriptionOwner;
                        let message = nodiscard
                            .get_description()
                            .map(|desc| SmolStr::new(desc.get_description_text()))
                            .unwrap_or_default();
                        // A `---@nodiscard` without a description must still be recorded (Some("") means NoDiscard).
                        self.signature_doc_map
                            .entry(owner_syntax)
                            .or_default()
                            .nodiscard = Some(message);
                    }
                }
                LuaDocTag::Type(type_tag) => {
                    if let Some(owner_syntax) = owner_syntax {
                        let types: Vec<LuaSyntaxId> = type_tag
                            .get_type_list()
                            .map(|ty| ty.get_syntax_id())
                            .collect();
                        if !types.is_empty() {
                            self.doc_type_map.insert(owner_syntax, types);
                        }
                    }
                }
                LuaDocTag::Module(module_tag) => {
                    if let (Some(owner_syntax), Some(string_token)) =
                        (owner_syntax, module_tag.get_string_token())
                    {
                        self.doc_module_map
                            .insert(owner_syntax, SmolStr::new(string_token.get_value()));
                    }
                }
                LuaDocTag::Param(param_tag) => {
                    let Some(doc_type) = param_tag.get_type() else {
                        continue;
                    };
                    let Some(name) = param_tag
                        .get_name_token()
                        .map(|token| SmolStr::new(token.get_name_text()))
                        .or_else(|| param_tag.is_vararg().then(|| SmolStr::new("...")))
                    else {
                        continue;
                    };
                    let constructor = owner_syntax.and_then(|owner| {
                        self.pending_constructor
                            .take()
                            .filter(|(pending_owner, _)| *pending_owner == owner)
                            .map(|(_, attribute)| attribute)
                    });
                    if let Some(owner_syntax) = owner_syntax {
                        let entry = self.signature_doc_map.entry(owner_syntax).or_default();
                        if param_tag.is_nullable() {
                            entry.nullable_params.push(name.clone());
                        }
                        if let Some(attribute) = constructor {
                            entry.constructor_params.push((name.clone(), attribute));
                        }
                        entry.param_types.push((name, doc_type.get_syntax_id()));
                    }
                }
                LuaDocTag::AttributeUse(attribute_use_tag) => {
                    if let Some(owner_syntax) = owner_syntax {
                        if let Some(attribute) = collect_constructor_attribute(attribute_use_tag) {
                            self.pending_constructor = Some((owner_syntax, attribute));
                        }
                        if is_lsp_delayed_definition(attribute_use_tag) {
                            self.delayed_definition_owners.insert(owner_syntax);
                        }
                    }
                }
                LuaDocTag::Return(return_tag) => {
                    if let Some(owner_syntax) = owner_syntax {
                        let entry = self.signature_doc_map.entry(owner_syntax).or_default();
                        let infos = return_tag.get_info_list();
                        // Named/anonymous `---@return`s all go into `returns` in source order
                        // as the "main return rows"; `return_overloads` keeps only real `---@return_overload`.
                        // Names are stored separately in `named_returns` so the presentation layer can restore `-> name: type`.
                        for (doc_type, name) in infos {
                            if let Some(name_token) = name {
                                entry.named_returns.push((
                                    name_token.get_name_text().into(),
                                    doc_type.get_syntax_id(),
                                ));
                            }
                            entry.returns.push(doc_type.get_syntax_id());
                        }
                    }
                }
                LuaDocTag::ReturnOverload(overload_tag) => {
                    if let Some(owner_syntax) = owner_syntax {
                        let entry = self.signature_doc_map.entry(owner_syntax).or_default();
                        let types: Vec<_> = overload_tag.get_types().collect();
                        entry.return_overload_rows.push(types.len());
                        for doc_type in types {
                            entry
                                .return_overloads
                                .push((None, doc_type.get_syntax_id()));
                        }
                    }
                }
                LuaDocTag::ReturnCast(return_cast_tag) => {
                    if let Some(owner_syntax) = owner_syntax {
                        let Some(name_token) = return_cast_tag.get_name_token() else {
                            continue;
                        };
                        let op_types: Vec<_> = return_cast_tag.get_op_types().collect();
                        let Some(cast_op) = op_types.first() else {
                            continue;
                        };
                        let Some(cast_type) = cast_op.get_type() else {
                            continue;
                        };
                        let fallback = op_types.get(1).and_then(|op| op.get_type());
                        let entry = self.signature_doc_map.entry(owner_syntax).or_default();
                        entry.return_cast = Some(crate::salsa_builder::def::SignatureReturnCast {
                            name: name_token.get_name_text().into(),
                            cast: cast_type.get_syntax_id(),
                            fallback: fallback.map(|ty| ty.get_syntax_id()),
                        });
                    }
                }
                LuaDocTag::Overload(overload_tag) => {
                    if let Some(doc_type) = overload_tag.get_type() {
                        // Comment block on a function statement → signature overload;
                        // `---@overload` immediately after `---@class X` → type call overload.
                        let owner_is_function = owner_syntax.is_some_and(|owner| {
                            matches!(
                                owner.get_kind(),
                                LuaSyntaxKind::FuncStat | LuaSyntaxKind::LocalFuncStat
                            )
                        });
                        if !owner_is_function
                            && let Some(type_def_id) = self.current_class.clone()
                            && let Some(def) =
                                self.type_defs.iter_mut().find(|def| def.id == type_def_id)
                        {
                            def.call_overloads.push(doc_type.get_syntax_id());
                        } else if let Some(owner_syntax) = owner_syntax {
                            self.signature_doc_map
                                .entry(owner_syntax)
                                .or_default()
                                .overloads
                                .push(doc_type.get_syntax_id());
                        }
                    }
                }
                LuaDocTag::Generic(generic_tag) => {
                    if let Some(owner_syntax) = owner_syntax {
                        let entry = self.signature_doc_map.entry(owner_syntax).or_default();
                        entry
                            .generic_params
                            .extend(collect_generics(generic_tag.get_generic_decl_list()));
                    }
                }
                LuaDocTag::Deprecated(_) => {
                    // No following @field: applies to the type (class/alias/enum tag) + statement owner.
                    if let Some(owner_syntax) = owner_syntax
                        && owner_type_deprecated
                    {
                        self.signature_doc_map
                            .entry(owner_syntax)
                            .or_default()
                            .deprecated = true;
                        self.doc_deprecated_owners.insert(owner_syntax);
                    }
                }
                LuaDocTag::Diagnostic(diag_tag) => {
                    self.collect_diagnostic_tag(comment, diag_tag, text);
                }
                LuaDocTag::Async(_) => {
                    if let Some(owner_syntax) = owner_syntax {
                        self.signature_doc_map
                            .entry(owner_syntax)
                            .or_default()
                            .is_async = true;
                    }
                }
                LuaDocTag::Meta(meta_tag) => {
                    // `---@meta`: Meta flag for type definitions in this file (mirrors old analyzer.is_meta).
                    self.is_meta = true;
                    self.meta_name = meta_tag
                        .get_name_token()
                        .map(|token| token.get_name_text().into());
                }
                LuaDocTag::Visibility(visibility_tag) => {
                    // `---@internal/@public/...`: attributed to the owner statement (declaration / return statement).
                    if let Some(owner_syntax) = owner_syntax
                        && let Some(token) = visibility_tag.get_visibility_token()
                        && let Some(kind) = token.get_visibility()
                    {
                        self.visibility_labels.insert(owner_syntax, kind);
                    }
                }
                LuaDocTag::Version(version_tag) => {
                    // `---@version 5.1, > 5.2`: both module-level version conditions;
                    // when there is an owner, also attach to the signature so members can be filtered by version.
                    self.module_version_conds.extend(
                        version_tag
                            .get_version_list()
                            .filter_map(|version| version.get_version_condition()),
                    );
                    if let Some(owner_syntax) = owner_syntax {
                        let entry = self.signature_doc_map.entry(owner_syntax).or_default();
                        entry.versions.extend(
                            version_tag
                                .get_version_list()
                                .filter_map(|version| version.get_version_condition()),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    /// Extracts `---@diagnostic disable[-next-line|-line] / enable` (mirrors old `analyze_diagnostic`).
    fn collect_diagnostic_tag(
        &mut self,
        comment: &LuaComment,
        diag_tag: &LuaDocTagDiagnostic,
        text: &str,
    ) {
        let Some(action_token) = diag_tag.get_action_token() else {
            return;
        };
        let action = action_token.get_text();
        let codes: Vec<DiagnosticCode> = diag_tag
            .get_code_list()
            .map(|list| {
                list.get_codes()
                    .filter_map(|code| DiagnosticCode::from_str(code.get_name_text()).ok())
                    .collect()
            })
            .unwrap_or_default();
        match action {
            "disable" => {
                // File-level (top-level block of chunk) → file-level disable; otherwise disable for the affecting block range.
                let owner_block = comment.ancestors::<LuaBlock>().next();
                let is_file_disable = owner_block
                    .as_ref()
                    .is_some_and(|block| block.get_parent::<LuaChunk>().is_some());
                if is_file_disable {
                    for code in codes {
                        self.file_diagnostic_disabled.insert(code);
                    }
                } else if let Some(block) = owner_block {
                    push_diagnostic_disables(
                        &mut self.diagnostic_disables,
                        block.get_range(),
                        codes,
                    );
                }
            }
            "disable-next-line" => {
                let comment_range = comment.get_range();
                let comment_end_line = line_index(text, comment_range.end());
                // Ignore if there is no next line (mirrors old logic: skip when get_line_range fails).
                let Some(next_line_range) = line_range(text, comment_end_line + 1) else {
                    return;
                };
                let valid_range = TextRange::new(comment_range.start(), next_line_range.end());
                push_diagnostic_disables(&mut self.diagnostic_disables, valid_range, codes);
            }
            "disable-line" => {
                let comment_range = comment.get_range();
                let comment_end_line = line_index(text, comment_range.end());
                let Some(line_range) = line_range(text, comment_end_line) else {
                    return;
                };
                push_diagnostic_disables(&mut self.diagnostic_disables, line_range, codes);
            }
            "enable" => {
                for code in codes {
                    self.file_diagnostic_enabled.insert(code);
                }
            }
            _ => {}
        }
    }

    /// Collects a type definition (full_name starts as the bare name; namespace is added in post-processing); also records `current_class`.
    #[allow(clippy::too_many_arguments)]
    fn push_type_def(
        &mut self,
        name_token: Option<LuaNameToken>,
        kind: TypeDefKind,
        flag: Option<LuaDocTypeFlag>,
        super_names: Vec<SmolStr>,
        generic_params: Vec<SalsaGenericParam>,
        deprecated: bool,
        alias_type: Option<LuaSyntaxId>,
        owner_syntax: Option<LuaSyntaxId>,
    ) {
        let Some(name_token) = name_token else {
            return;
        };
        let name: SmolStr = name_token.get_name_text().into();
        let mut def = TypeDef::new(
            self.file_id,
            self.workspace_id,
            name.clone(),
            name,
            type_visibility(flag.clone()),
            kind,
            name_token.get_range(),
            super_names,
        );
        def.generic_params = generic_params;
        def.alias_type = alias_type;
        def.deprecated = deprecated;
        def.owner_syntax = owner_syntax;
        def.flags = type_def_flags(flag, self.is_meta);
        self.current_class = Some(def.id.clone());
        self.type_defs.push(def);
    }

    /// `---@field bar string` → named-type member (owner = current class `TypeDef` id).
    fn collect_field_member(&mut self, field_tag: &LuaDocTagField, deprecated: bool) {
        let Some(type_def_id) = self.current_class.clone() else {
            return;
        };
        let Some(key) = field_tag.get_field_key() else {
            return;
        };
        // Integer keys in `@field [1]` are encoded as `LuaMemberKey::Integer` (matching index expression keys).
        let member_key = match &key {
            LuaDocFieldKey::Integer(token) => match token.get_number_value() {
                NumberResult::Int(i) => LuaMemberKey::Integer(i),
                _ => LuaMemberKey::Name(doc_field_key_name(&key)),
            },
            _ => LuaMemberKey::Name(doc_field_key_name(&key)),
        };
        let mut member = Member::new(
            self.file_id,
            field_tag
                .get_field_key_range()
                .unwrap_or_else(|| field_tag.get_range()),
            member_key,
            type_def_id,
        );
        member.is_index_signature = matches!(key, LuaDocFieldKey::Type(_));
        member.is_nullable = field_tag.is_nullable();
        member.value_syntax = field_tag
            .get_type()
            .map(|doc_type| doc_type.get_syntax_id());
        member.deprecated = deprecated;
        // Access visibility annotation: `---@field private pin string` (prefix) or `---@field x number @private` (trailing).
        member.visibility = field_tag
            .syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| {
                let kind: emmylua_parser::LuaTokenKind = token.kind().into();
                matches!(
                    kind,
                    emmylua_parser::LuaTokenKind::TkDocVisibility
                        | emmylua_parser::LuaTokenKind::TkTagVisibility
                )
            })
            .and_then(emmylua_parser::LuaDocVisibilityToken::cast)
            .and_then(|token| token.get_visibility())
            .unwrap_or(VisibilityKind::Public);
        self.mark_member(&mut member);
        self.members.push(member);
    }

    fn enter(&mut self, node: &LuaAst, text: &str) {
        match node {
            LuaAst::LuaChunk(chunk) => self.push_scope(chunk.get_range(), ScopeKind::Chunk),
            LuaAst::LuaBlock(block) => {
                // The repeat body and until condition share a scope (Lua semantics: body locals are visible in the condition).
                let is_repeat_body = block
                    .syntax()
                    .parent()
                    .is_some_and(|parent| parent.kind() == LuaSyntaxKind::RepeatStat.into());
                if !is_repeat_body {
                    self.push_scope(block.get_range(), ScopeKind::Block);
                }
            }
            LuaAst::LuaComment(comment) => self.collect_doc_comment(comment, text),
            LuaAst::LuaLocalStat(stat) => {
                self.push_scope(stat.get_range(), ScopeKind::LocalStat);
                self.current_owner_syntax = Some(stat.get_syntax_id());
                self.collect_local_stat(stat);
                self.current_comment_deprecated = false;
            }
            LuaAst::LuaAssignStat(stat) => {
                self.push_scope(stat.get_range(), ScopeKind::AssignStat);
                self.current_owner_syntax = Some(stat.get_syntax_id());
                self.collect_assign_stat(stat);
                self.current_comment_deprecated = false;
            }
            LuaAst::LuaForStat(stat) => {
                self.push_scope(stat.get_range(), ScopeKind::ForStat);
                if let Some(var) = stat.get_var_name() {
                    self.add_decl(
                        var.get_name_text().into(),
                        DeclKind::Local {
                            is_const: true,
                            is_iter: true,
                        },
                        var.get_range(),
                    );
                }
            }
            LuaAst::LuaForRangeStat(stat) => {
                self.push_scope(stat.get_range(), ScopeKind::ForRangeStat);
                self.current_owner_syntax = Some(stat.get_syntax_id());
                for var in stat.get_var_name_list() {
                    self.add_decl(
                        var.get_name_text().into(),
                        DeclKind::Local {
                            is_const: true,
                            is_iter: true,
                        },
                        var.get_range(),
                    );
                }
            }
            LuaAst::LuaFuncStat(stat) => {
                self.push_scope(stat.get_range(), ScopeKind::FuncStat);
                self.current_owner_syntax = Some(stat.get_syntax_id());
                self.collect_func_stat(stat);
                self.current_comment_deprecated = false;
            }
            LuaAst::LuaLocalFuncStat(stat) => {
                // `local function f`: the name is registered in the **outer scope** (visible after the function ends),
                // while parameters/body live in inner FuncStat + Closure scopes.
                // owner_syntax must be set before add_decl_with_value (for doc comment ownership).
                self.current_owner_syntax = Some(stat.get_syntax_id());
                if let (Some(local_name), Some(closure)) =
                    (stat.get_local_name(), stat.get_closure())
                {
                    if let Some(name) = local_name.get_name_token() {
                        self.add_decl_with_value(
                            name.get_name_text().into(),
                            DeclKind::Local {
                                is_const: false,
                                is_iter: false,
                            },
                            name.get_range(),
                            Some(closure.get_syntax_id()),
                        );
                    }
                }
                self.push_scope(stat.get_range(), ScopeKind::FuncStat);
            }
            LuaAst::LuaRepeatStat(stat) => {
                self.push_scope(stat.get_range(), ScopeKind::Repeat);
            }
            LuaAst::LuaClosureExpr(closure) => {
                self.push_scope(closure.get_range(), ScopeKind::Closure);
                // Parameters and locals in the function body belong to the closure scope and must not inherit the outer `---@type`.
                // Signature docs still belong to the outer statement (LocalStat/FuncStat), so save them now and use them in
                // collect_signature; after collect_params, the body uses the closure owner.
                let statement_owner = self.current_owner_syntax;
                self.current_owner_syntax = Some(closure.get_syntax_id());
                self.collect_params(closure);
                self.collect_signature(closure, statement_owner);
            }
            LuaAst::LuaNameExpr(name_expr) => {
                if let Some(name) = name_expr.get_name_text() {
                    self.name_uses.push(NameUse {
                        syntax: name_expr.get_syntax_id(),
                        name: name.into(),
                    });
                }
            }
            LuaAst::LuaIndexExpr(index_expr) => {
                self.member_uses.push(index_expr.get_syntax_id());
            }
            LuaAst::LuaTableExpr(table) => {
                // Anonymous table literal: fields are collected into a synthetic owner (`return { x = 1 }` etc.).
                let owner = SemanticId::member(self.file_id, table.get_range());
                self.collect_table_fields(&table, owner);
            }
            _ => {}
        }
    }

    fn leave(&mut self, node: &LuaAst) {
        if is_scope_owner(node) {
            // The repeat body's Block was not pushed, so it must not be popped.
            if matches!(node.syntax().kind().into(), LuaSyntaxKind::Block)
                && node
                    .syntax()
                    .parent()
                    .is_some_and(|parent| parent.kind() == LuaSyntaxKind::RepeatStat.into())
            {
                return;
            }
            self.scope_stack.pop();
        }
    }

    /// Slot for a multi-return assignment: in `local a, b = f()`, b points to the 2nd return value of f();
    /// in `local a, b, c = f(), g()`, c points to the 2nd return value of g().
    fn assignment_value_slot(
        value_exprs: &[LuaExpr],
        index: usize,
    ) -> (Option<LuaSyntaxId>, Option<usize>) {
        if let Some(expr) = value_exprs.get(index) {
            return (Some(expr.get_syntax_id()), Some(0));
        }
        if let Some(expr) = value_exprs.last() {
            let last_index = value_exprs.len() - 1;
            return (Some(expr.get_syntax_id()), Some(index - last_index));
        }
        (None, None)
    }

    fn collect_local_stat(&mut self, stat: &LuaLocalStat) {
        let name_list = stat.get_local_name_list().collect::<Vec<_>>();
        let value_exprs = stat.get_value_exprs().collect::<Vec<_>>();

        for (index, local_name) in name_list.iter().enumerate() {
            let Some(name_token) = local_name.get_name_token() else {
                continue;
            };
            let is_const = local_name.get_attrib().is_some_and(|a| a.is_const());
            // Only record when the name directly corresponds to a value expression (in `local a, b = f()`, b points to the 2nd slot of the whole f()).
            let (value_expr_syntax, multi_return_index) =
                Self::assignment_value_slot(&value_exprs, index);
            let decl_id = self.add_decl_with_value(
                name_token.get_name_text().into(),
                DeclKind::Local {
                    is_const,
                    is_iter: false,
                },
                name_token.get_range(),
                value_expr_syntax,
            );
            if let Some(decl) = self.decls.last_mut() {
                decl.doc_type_index = Some(index);
                if let Some(multi_return_index) = multi_return_index {
                    decl.multi_return_index = Some(multi_return_index);
                }
            }
            // `local T = { ... }`: table fields are T's members (owner = T's declaration id).
            if let Some(LuaExpr::TableExpr(table)) = value_exprs.get(index) {
                self.collect_table_fields(table, decl_id);
            }
        }
    }

    fn collect_assign_stat(&mut self, stat: &LuaAssignStat) {
        let (vars, value_exprs) = stat.get_var_and_expr_list();
        for (idx, var) in vars.iter().enumerate() {
            let value_expr_syntax = value_exprs.get(idx).map(|expr| expr.get_syntax_id());
            match var {
                LuaVarExpr::NameExpr(name_expr) => {
                    let Some(name_token) = name_expr.get_name_token() else {
                        continue;
                    };
                    let name = name_token.get_name_text();
                    if name != "_"
                        && self
                            .find_visible_decl_before_offset(name, name_expr.get_position())
                            .is_none()
                    {
                        let (value_expr_syntax, multi_return_index) =
                            Self::assignment_value_slot(&value_exprs, idx);
                        let _decl_id = self.add_decl_with_value(
                            name.into(),
                            DeclKind::Global,
                            name_token.get_range(),
                            value_expr_syntax,
                        );
                        if let Some(multi_return_index) = multi_return_index
                            && let Some(decl) = self.decls.last_mut()
                        {
                            decl.multi_return_index = Some(multi_return_index);
                        }
                    }
                }
                LuaVarExpr::IndexExpr(index_expr) => {
                    self.collect_member_from_index_expr(index_expr, value_expr_syntax, false);
                }
            }
        }
    }

    fn collect_func_stat(&mut self, stat: &LuaFuncStat) {
        let Some(func_name) = stat.get_func_name() else {
            return;
        };
        let Some(closure) = stat.get_closure() else {
            return;
        };

        match func_name {
            LuaVarExpr::NameExpr(name_expr) => {
                if let Some(name_token) = name_expr.get_name_token() {
                    let name = name_token.get_name_text();
                    if self
                        .find_visible_decl_before_offset(name, name_expr.get_position())
                        .is_none()
                    {
                        self.add_decl_with_value(
                            name.into(),
                            DeclKind::Global,
                            name_token.get_range(),
                            Some(closure.get_syntax_id()),
                        );
                    }
                }
            }
            LuaVarExpr::IndexExpr(index_expr) => {
                let is_method = index_expr
                    .get_index_token()
                    .is_some_and(|token| token.is_colon());
                self.collect_member_from_index_expr(
                    &index_expr,
                    Some(closure.get_syntax_id()),
                    is_method,
                );
            }
        }
        // Parameters and signatures are collected on ClosureExpr enter (to avoid duplicates).
    }

    /// `a.b.c = v` → Member{ owner: SemanticId, name }. The owner is resolved from the prefix:
    /// local in this file → `Decl`; global name → `Name(path)` (linked in phase 2).
    fn collect_member_from_index_expr(
        &mut self,
        index_expr: &LuaIndexExpr,
        value_syntax: Option<LuaSyntaxId>,
        is_method: bool,
    ) {
        let Some(member_key) = index_expr
            .get_index_key()
            .and_then(member_key_from_index_key)
        else {
            return;
        };
        let Some(prefix) = index_expr.get_prefix_expr() else {
            return;
        };
        let mut segments = Vec::new();
        let Some(owner) = self.resolve_member_owner(prefix, &mut segments) else {
            return;
        };
        let mut member = Member::new(
            self.file_id,
            member_key_range(index_expr),
            member_key,
            owner,
        );
        member.value_syntax = value_syntax;
        member.is_method = is_method;
        member.deprecated = self.current_comment_deprecated;
        // A `---@type` annotation on the assignment statement is also attached to the runtime member, as a fallback annotation for flow member assignment.
        if member.doc_type_syntax.is_none()
            && let Some(owner_syntax) = self.current_owner_syntax
            && let Some(type_syntaxes) = self.doc_type_map.get(&owner_syntax)
        {
            member.doc_type_syntax = type_syntaxes.first().copied();
        }
        self.mark_member(&mut member);
        self.members.push(member);
    }

    /// Resolves a member owner's prefix into `SemanticId`.
    /// Local root → `Decl(local)`; global root → `Name(full path "a.b")` (linked in phase 2).
    fn resolve_member_owner(
        &self,
        expr: LuaExpr,
        segments: &mut Vec<SmolStr>,
    ) -> Option<SemanticId> {
        match expr {
            // `(t).x = v` is equivalent to `t.x = v`; the member belongs to the same underlying declaration.
            LuaExpr::ParenExpr(paren) => self.resolve_member_owner(paren.get_expr()?, segments),
            LuaExpr::IndexExpr(parent) => {
                let segment = parent
                    .get_index_key()
                    .map(|k| k.get_path_part())
                    .and_then(|p| (!p.is_empty()).then(|| SmolStr::new(p)))?;
                let owner = self.resolve_member_owner(parent.get_prefix_expr()?, segments)?;
                segments.push(segment);
                // Global root: rebuild the full path (root name + all segments, e.g. "M.N").
                if let SemanticId::Name(root) = &owner {
                    let mut path = root.as_str().to_string();
                    for s in segments.iter() {
                        path.push('.');
                        path.push_str(s);
                    }
                    return Some(SemanticId::name(SmolStr::new(path)));
                }
                Some(owner)
            }
            LuaExpr::NameExpr(name_expr) => {
                let name = name_expr.get_name_text()?;
                if name == "_ENV" || name == "_G" {
                    return Some(SemanticId::name(SmolStr::new(&name)));
                }
                let offset = name_expr.get_position();
                if let Some(decl) = self.find_visible_decl_before_offset(&name, offset)
                    && !matches!(decl.kind, DeclKind::Global)
                {
                    return Some(decl.id.clone());
                }
                Some(SemanticId::name(SmolStr::new(name)))
            }
            _ => None,
        }
    }

    /// Direct fields of `{ foo = 1, ... }` → owner = the local declaring the table (`Decl`).
    fn collect_table_fields(&mut self, table: &LuaTableExpr, owner: SemanticId) {
        for (field, key) in table.get_fields_with_keys() {
            let path_part = SmolStr::new(key.get_path_part());
            let member_key =
                member_key_from_index_key(key).unwrap_or(LuaMemberKey::Name(path_part));
            let mut member = Member::new(
                self.file_id,
                field_key_range(&field),
                member_key,
                owner.clone(),
            );
            member.value_syntax = field.get_value_expr().map(|expr| expr.get_syntax_id());
            self.mark_member(&mut member);
            self.members.push(member);
        }
    }

    /// Member ownership flags: `---@readonly` / statement-level visibility tags like `---@private`.
    fn mark_member(&self, member: &mut Member) {
        if let Some(owner) = self.current_owner_syntax {
            member.readonly = self.doc_readonly_owners.contains(&owner);
            // `---@private function M.init()` / `---@private M.log = 1`:
            // Statement-level visibility tags apply to runtime members (@field's own tags take precedence).
            if member.visibility == VisibilityKind::Public
                && let Some(visibility) = self.visibility_labels.get(&owner)
            {
                member.visibility = *visibility;
            }
        }
    }

    fn collect_params(&mut self, closure: &LuaClosureExpr) {
        let Some(params) = closure.get_params_list() else {
            return;
        };
        for param in params.get_params() {
            let name = if let Some(token) = param.get_name_token() {
                token.get_name_text().into()
            } else if param.is_dots() {
                SmolStr::new("...")
            } else {
                continue;
            };
            self.add_decl(name, DeclKind::Param, param.get_range());
        }
    }

    /// Extracts the `ClosureExpr` signature (name/method/params + doc annotations).
    fn collect_signature(&mut self, closure: &LuaClosureExpr, doc_owner: Option<LuaSyntaxId>) {
        // At this point current_owner_syntax is the closure itself, so params/self declarations belong to the closure;
        // signature docs are still looked up from the outer statement.
        let mut name = None;
        let mut is_method = false;
        if let Some(func_stat) = closure.get_parent::<LuaFuncStat>() {
            if let Some(func_name) = func_stat.get_func_name() {
                match func_name {
                    LuaVarExpr::NameExpr(name_expr) => {
                        name = name_expr.get_name_text().map(Into::into);
                    }
                    LuaVarExpr::IndexExpr(index_expr) => {
                        is_method = index_expr
                            .get_index_token()
                            .is_some_and(|token| token.is_colon());
                        name = index_expr
                            .get_index_key()
                            .map(|k| k.get_path_part())
                            .and_then(|p| (!p.is_empty()).then(|| SmolStr::new(p)));
                    }
                }
            }
        }

        let mut is_variadic = false;
        let param_names: Vec<SmolStr> = closure
            .get_params_list()
            .map(|params| {
                params
                    .get_params()
                    .map(|param| {
                        if param.is_dots() {
                            is_variadic = true;
                        }
                        param
                            .get_name_token()
                            .map(|token| token.get_name_text().into())
                            .or_else(|| param.is_dots().then(|| SmolStr::new("...")))
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Register the implicit `self` in methods as a closure-scope parameter so it can participate in flow narrowing.
        // Use a zero-width range at the closure start to avoid shadowing other declarations inside the closure.
        if is_method {
            self.add_decl(
                SmolStr::new("self"),
                DeclKind::Param,
                TextRange::empty(closure.get_range().start()),
            );
        }

        let mut docs = doc_owner.and_then(|owner| self.signature_doc_map.remove(&owner));
        if docs.is_none() && doc_owner.is_some_and(|owner| self.doc_comment_owners.contains(&owner))
        {
            docs = Some(SignatureDoc::default());
        }

        self.signatures.push(Signature {
            id: SemanticId::signature(self.file_id, closure.get_syntax_id()),
            file_id: self.file_id,
            closure_syntax: closure.get_syntax_id(),
            name,
            is_method,
            owner_syntax: doc_owner,
            param_names,
            is_variadic,
            docs: docs.filter(|doc| !doc.is_empty()).map(Box::new),
        });
    }

    fn add_decl(&mut self, name: SmolStr, kind: DeclKind, name_range: TextRange) -> SemanticId {
        self.add_decl_with_value(name, kind, name_range, None)
    }

    fn add_decl_with_value(
        &mut self,
        name: SmolStr,
        kind: DeclKind,
        name_range: TextRange,
        value_expr_syntax: Option<LuaSyntaxId>,
    ) -> SemanticId {
        let scope_id = self.scope_stack.last().copied().unwrap_or(0);
        let mut decl = Decl::new(self.file_id, name, kind, name_range);
        decl.scope_id = scope_id;
        decl.value_expr_syntax = value_expr_syntax;
        decl.owner_syntax = self.current_owner_syntax;
        let decl_id = decl.id.clone();
        self.scopes[scope_id as usize]
            .children
            .push(ScopeChild::Decl(decl_id.clone()));
        self.decls.push(decl);
        decl_id
    }

    /// A type definition's full_name depends on namespace (which may appear after the class definition) — completed in post-processing.
    /// Also rebuilds identity so id.full_name stays consistent with def.full_name.
    fn finalize_type_defs(&mut self) {
        let Some(ns) = &self.namespace else {
            return;
        };
        for def in &mut self.type_defs {
            let old_id = def.id.clone();
            def.full_name = SmolStr::new(format!("{}.{}", ns, def.name));
            let scope = match def.visibility {
                TypeVisibility::Public => TypeScope::Global,
                TypeVisibility::Internal => TypeScope::Internal(self.workspace_id),
                TypeVisibility::Private => TypeScope::File(def.file_id),
            };
            def.id = SemanticId::type_def(scope, def.full_name.clone());
            // Namespace qualification rebuilds TypeDef identity; member/operator owners are updated accordingly.
            for member in &mut self.members {
                if member.owner == old_id {
                    member.owner = def.id.clone();
                }
            }
            for operator in &mut self.operators {
                if operator.owner == old_id {
                    operator.owner = def.id.clone();
                }
            }
        }
    }

    /// Inline `---@type` / `---@module` on table fields: attach to members by field syntax range.
    fn assign_member_doc_types(&mut self) {
        for member in &mut self.members {
            if member.doc_type_syntax.is_some() {
                continue;
            }
            let Some(key_range) = member.id.member_key_range() else {
                continue;
            };
            if let Some((_, type_syntaxes)) = self.doc_type_map.iter().find(|(owner, _)| {
                owner.get_kind() == LuaSyntaxKind::TableFieldAssign
                    && owner.get_range().contains_range(key_range)
            }) {
                member.doc_type_syntax = type_syntaxes.first().copied();
            }
            if member.module_path.is_none()
                && let Some((_, module_path)) = self.doc_module_map.iter().find(|(owner, _)| {
                    owner.get_kind() == LuaSyntaxKind::TableFieldAssign
                        && owner.get_range().contains_range(key_range)
                })
            {
                member.module_path = Some(module_path.clone());
            }
        }
    }

    /// Attaches `---@type` / `---@deprecated` annotations to declarations by owner position (iterates collected decls, without re-walking the tree).
    fn assign_doc_types(&mut self) {
        for decl in &mut self.decls {
            if let Some(owner) = decl.owner_syntax {
                if let Some(type_syntaxes) = self.doc_type_map.get(&owner) {
                    decl.doc_type_syntax = decl
                        .doc_type_index
                        .and_then(|index| type_syntaxes.get(index).copied())
                        .or_else(|| type_syntaxes.first().copied());
                }
                decl.module_path = self.doc_module_map.get(&owner).cloned();
                decl.deprecated = self.doc_deprecated_owners.contains(&owner);
                decl.readonly = self.doc_readonly_owners.contains(&owner);
            }
        }
        // Member readonly ownership: the statement containing the member key (current_owner_syntax is available at collection time;
        // assigning by member id's key_range here is not feasible, so it is set at the collection point).
    }

    /// Top-level `return X` → module export target (the last top-level return wins).
    fn collect_module_export(&mut self, chunk: &LuaChunk) {
        let Some(block) = chunk.get_block() else {
            return;
        };
        // Module visibility still uses the first top-level return as the baseline (mirrors old analyze_chunk_return).
        for stat in block.get_stats() {
            if let LuaStat::ReturnStat(ret) = &stat
                && let Some(expr) = ret.get_expr_list().next()
            {
                self.module_visibility = self.return_visibility(&stat, &expr);
                break;
            }
        }

        // Module export takes the first "reachable" return expression (including truthy while/if control flow).
        let flow = module_block_flow(block);
        let Some(expr) = flow.first_expr else {
            self.module_export = ModuleExport::None;
            return;
        };
        self.module_export = match &expr {
            LuaExpr::NameExpr(name_expr) => {
                let name = name_expr.get_name_text().unwrap_or_default();
                let offset = name_expr.get_position();
                if let Some(decl) = self.find_visible_decl_before_offset(&name, offset) {
                    ModuleExport::Decl {
                        decl: decl.id.clone(),
                        name: name.into(),
                    }
                } else {
                    ModuleExport::Global { name: name.into() }
                }
            }
            _ => ModuleExport::Expr {
                value_syntax: expr.get_syntax_id(),
            },
        };
    }

    /// Visibility of the first top-level return (mirrors old `analyze_chunk_return` semantics):
    /// `---@meta no-require`/`_` → Hide (highest priority);
    /// NameExpr → use the visibility tag on the **declaration** (tags on the return statement are ignored);
    /// other expressions (anonymous tables, etc.) → use the visibility tag on the return statement; default Public.
    fn return_visibility(&self, stat: &LuaStat, expr: &LuaExpr) -> ModuleVisibility {
        if matches!(self.meta_name.as_deref(), Some("no-require") | Some("_")) {
            return ModuleVisibility::Hide;
        }
        let label = match expr {
            LuaExpr::NameExpr(name_expr) => {
                let name = name_expr.get_name_text().unwrap_or_default();
                let offset = name_expr.get_position();
                self.find_visible_decl_before_offset(&name, offset)
                    .and_then(|decl| decl.owner_syntax)
                    .and_then(|owner| self.visibility_labels.get(&owner))
                    .copied()
            }
            _ => self.visibility_labels.get(&stat.get_syntax_id()).copied(),
        };
        match label {
            Some(VisibilityKind::Internal) => ModuleVisibility::Internal,
            Some(VisibilityKind::Public) => ModuleVisibility::Public,
            _ => ModuleVisibility::Public,
        }
    }

    fn push_scope(&mut self, range: TextRange, kind: ScopeKind) {
        let id = self.scopes.len() as u32;
        let parent = self.scope_stack.last().copied();
        self.scopes.push(Scope {
            id,
            parent,
            kind,
            start: range.start(),
            end: range.end(),
            children: Vec::new(),
        });
        if let Some(parent_id) = parent {
            self.scopes[parent_id as usize]
                .children
                .push(ScopeChild::Scope(id));
        }
        self.scope_stack.push(id);
    }

    fn find_visible_decl_before_offset(&self, name: &str, offset: TextSize) -> Option<&Decl> {
        find_visible_decl(&self.decls, &self.scopes, name, offset)
    }
}

fn is_scope_owner(node: &LuaAst) -> bool {
    matches!(
        node.syntax().kind().into(),
        LuaSyntaxKind::Chunk
            | LuaSyntaxKind::Block
            | LuaSyntaxKind::LocalStat
            | LuaSyntaxKind::AssignStat
            | LuaSyntaxKind::ForStat
            | LuaSyntaxKind::ForRangeStat
            | LuaSyntaxKind::FuncStat
            | LuaSyntaxKind::LocalFuncStat
            | LuaSyntaxKind::ClosureExpr
            | LuaSyntaxKind::RepeatStat
    )
}

/// `LuaIndexKey` → file-independent member key.
fn member_key_from_index_key(key: LuaIndexKey) -> Option<LuaMemberKey> {
    match key {
        LuaIndexKey::Name(_) | LuaIndexKey::String(_) => {
            Some(LuaMemberKey::Name(SmolStr::new(key.get_path_part())))
        }
        LuaIndexKey::Integer(num) => match num.get_number_value() {
            NumberResult::Int(i) => Some(LuaMemberKey::Integer(i)),
            _ => None,
        },
        LuaIndexKey::Idx(idx) => Some(LuaMemberKey::Integer(idx as i64)),
        LuaIndexKey::Expr(_) => None,
    }
}

/// Member-key token range (for goto-def), falling back to the whole index expression range.
fn member_key_range(index_expr: &LuaIndexExpr) -> TextRange {
    index_expr
        .get_index_key()
        .and_then(|key| key.get_range())
        .unwrap_or_else(|| index_expr.get_range())
}

/// Table-field key token range, falling back to the whole field range.
fn field_key_range(field: &LuaTableField) -> TextRange {
    field
        .get_field_key()
        .and_then(|key| key.get_range())
        .unwrap_or_else(|| field.get_range())
}

/// `LuaDocGenericDeclList` → generic parameters (doc node references).
fn collect_generics(list: Option<LuaDocGenericDeclList>) -> Vec<SalsaGenericParam> {
    let Some(list) = list else {
        return Vec::new();
    };
    list.get_generic_decl()
        .map(|decl| {
            let name = decl
                .get_name_token()
                .map(|token| token.get_name_text().into())
                .unwrap_or_default();
            SalsaGenericParam::new(
                name,
                decl.get_constraint_type().map(|t| t.get_syntax_id()),
                decl.get_default_type().map(|t| t.get_syntax_id()),
                decl.has_const_modifier(),
                decl.is_variadic(),
            )
        })
        .collect()
}

/// `---@field` key → name text.
fn doc_field_key_name(key: &LuaDocFieldKey) -> SmolStr {
    match key {
        LuaDocFieldKey::Name(token) => token.get_name_text().into(),
        LuaDocFieldKey::String(token) => token.get_value().into(),
        LuaDocFieldKey::Integer(token) => SmolStr::new(format!("{}", token.get_number_value())),
        LuaDocFieldKey::Type(doc_type) => doc_type_name(doc_type).unwrap_or_default(),
    }
}

/// Returns the name when the doc type is `Name` (or the base name of a generic instantiation `Foo<T>` → `Foo`).
fn doc_type_name(doc_type: &LuaDocType) -> Option<SmolStr> {
    match doc_type {
        LuaDocType::Name(name_type) => name_type.get_name_text().map(Into::into),
        LuaDocType::Generic(generic) => generic.get_name_type()?.get_name_text().map(Into::into),
        _ => None,
    }
}

/// Expands a parent type list: `A & B` into [A, B].
fn doc_type_names(doc_type: &LuaDocType) -> Vec<SmolStr> {
    match doc_type {
        LuaDocType::Binary(binary) => {
            let mut names = Vec::new();
            if let Some((left, right)) = binary.get_types() {
                names.extend(doc_type_names(&left));
                names.extend(doc_type_names(&right));
            }
            names
        }
        other => doc_type_name(other).into_iter().collect(),
    }
}

/// `---@[constructor("init", "Base", false, "doc")]` → constructor attributes.
/// Only the built-in `constructor` is recognized; positional args match std lib `Attribute.constructor`'s
/// `---@operator call(name, root_class, strip_self, return_mode)`.
/// Whether `---@[lsp_optimization("delayed_definition")]` is the delayed-definition attribute.
fn is_lsp_delayed_definition(tag: &LuaDocTagAttributeUse) -> bool {
    for attribute_use in tag.get_attribute_uses() {
        let Some(name_type) = attribute_use.get_type() else {
            continue;
        };
        let Some(name) = name_type.get_name_text() else {
            continue;
        };
        if name != "lsp_optimization" {
            continue;
        }
        let Some(arg_list) = attribute_use.get_arg_list() else {
            continue;
        };
        for arg in arg_list.get_args() {
            if let Some(LuaLiteralToken::String(token)) = arg.get_literal()
                && token.get_value() == "delayed_definition"
            {
                return true;
            }
        }
    }
    false
}

fn collect_constructor_attribute(tag: &LuaDocTagAttributeUse) -> Option<ConstructorAttribute> {
    for attribute_use in tag.get_attribute_uses() {
        let name = attribute_use.get_type()?.get_name_text()?;
        if name != "constructor" {
            continue;
        }
        let mut args = attribute_use.get_arg_list()?.get_args();
        let method_name = literal_string(args.next()?)?;
        let root_class = args.next().and_then(literal_string);
        let strip_self = args.next().and_then(literal_bool).unwrap_or(true);
        let return_mode = args
            .next()
            .and_then(literal_string)
            .as_deref()
            .and_then(ConstructorReturnMode::from_name)
            .unwrap_or_default();
        return Some(ConstructorAttribute {
            name: method_name,
            root_class,
            strip_self,
            return_mode,
        });
    }
    None
}

fn literal_string(expr: LuaLiteralExpr) -> Option<SmolStr> {
    match expr.get_literal()? {
        LuaLiteralToken::String(token) => Some(SmolStr::new(token.get_value())),
        _ => None,
    }
}

fn literal_bool(expr: LuaLiteralExpr) -> Option<bool> {
    match expr.get_literal()? {
        LuaLiteralToken::Bool(token) => Some(token.is_true()),
        _ => None,
    }
}

/// `@public`→Public, `@internal`→Internal, `@private`/`@file`→Private, default Public.
fn type_visibility(flag: Option<LuaDocTypeFlag>) -> TypeVisibility {
    let Some(flag) = flag else {
        return TypeVisibility::Public;
    };
    let mut visibility = TypeVisibility::Public;
    for token in flag.get_attrib_tokens() {
        match token.get_name_text() {
            "public" => visibility = TypeVisibility::Public,
            "internal" => visibility = TypeVisibility::Internal,
            "private" | "file" => visibility = TypeVisibility::Private,
            _ => {}
        }
    }
    visibility
}

/// Type definition flags (mirrors old `get_type_flag_value`: partial / constructor + file-level meta).
fn type_def_flags(flag: Option<LuaDocTypeFlag>, is_meta: bool) -> TypeDefFlags {
    let mut flags = TypeDefFlags {
        meta: is_meta,
        ..Default::default()
    };
    let Some(flag) = flag else {
        return flags;
    };
    for token in flag.get_attrib_tokens() {
        match token.get_name_text() {
            "partial" => flags.partial = true,
            "constructor" => flags.constructor = true,
            _ => {}
        }
    }
    flags
}

// ──────────────────────────────────────────────
// `---@diagnostic` line-number helpers (disable ranges need line numbers, computed directly from text)
// ──────────────────────────────────────────────

/// Creates disable entries from a code list (empty list = DisableAll with code None).
fn push_diagnostic_disables(
    disables: &mut Vec<DiagnosticDisable>,
    range: TextRange,
    codes: Vec<DiagnosticCode>,
) {
    if codes.is_empty() {
        disables.push(DiagnosticDisable { range, code: None });
    } else {
        for code in codes {
            disables.push(DiagnosticDisable {
                range,
                code: Some(code),
            });
        }
    }
}

/// Zero-based line number of offset (counted by '\n').
fn line_index(text: &str, offset: TextSize) -> usize {
    text[..usize::from(offset)]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
}

/// Range of line `line` (excluding the newline); returns `None` if the line does not exist.
fn line_range(text: &str, line: usize) -> Option<TextRange> {
    let bytes = text.as_bytes();
    let mut start = 0usize;
    let mut current = 0usize;
    while current < line {
        let pos = bytes[start..].iter().position(|b| *b == b'\n')?;
        start += pos + 1;
        current += 1;
    }
    let end = bytes[start..]
        .iter()
        .position(|b| *b == b'\n')
        .map(|pos| start + pos)
        .unwrap_or(bytes.len());
    Some(TextRange::new(
        TextSize::from(start as u32),
        TextSize::from(end as u32),
    ))
}
