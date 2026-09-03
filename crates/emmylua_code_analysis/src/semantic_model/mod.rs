pub(crate) mod cache;
pub mod flow;
pub mod infer;
pub mod member;
pub mod render;
pub mod type_check;
pub mod type_eval;

#[cfg(test)]
mod legacy_visibility_tests;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaChunk, LuaClosureExpr, LuaCommentOwner, LuaDocNameType,
    LuaDocObjectFieldKey, LuaDocTag, LuaDocTagAlias, LuaDocTagClass, LuaDocType, LuaExpr,
    LuaIndexExpr, LuaLiteralExpr, LuaSyntaxId, LuaSyntaxKind, LuaSyntaxNode, LuaSyntaxToken,
    LuaSyntaxTree, LuaTableField,
};
use emmylua_parser::{
    LuaCallExpr, LuaDocConditionalType, LuaForRangeStat, LuaLiteralToken, LuaParseError,
    LuaTableExpr, LuaTypeBinaryOperator, LuaTypeUnaryOperator, LuaVersionNumber, NumberResult,
    VisibilityKind,
};
use rowan::TextSize;
use smol_str::SmolStr;

use crate::LuaType;
use crate::LuaTypeNode;
use crate::member_key::LuaMemberKey;
use crate::salsa_builder::SalsaDatabase;
use crate::salsa_builder::SalsaQueries;
use crate::salsa_builder::def::{
    ConstructorAttribute, Decl, DeclKind, Member, MemberRef, ModuleExport, NameUse, Scope,
    SemanticId, Signature, TypeDef, TypeDefKind, TypeScope, TypeVisibility,
};
use crate::salsa_builder::flow::FlowTree;
use crate::signature::LuaSignatureId;
use crate::{
    AsyncState, FileExports, FileFacts, FileId, GenericParam, GenericTpl, GenericTplId,
    LuaAliasCallKind, LuaAliasCallType, LuaFunctionType, LuaIntersectionType, LuaObjectType,
    LuaTupleStatus, LuaTupleType, LuaTypeDeclId, LuaUnionType, SalsaGenericParam, VariadicType,
};

/// Semantic model: a per-file access handle, only through the salsa analysis layer.
pub struct SemanticModel<'db> {
    db: &'db SalsaDatabase,
    file_id: FileId,
    /// Closure return-inference in-progress stack (replaces thread_local; scoped to each SemanticModel instance).
    closure_return_infer_stack: RefCell<Vec<LuaSyntaxId>>,
    /// Expression inference reentry guard.
    expr_infer_guard: RefCell<Vec<LuaSyntaxId>>,
    /// Declaration / member inference reentry guard.
    decl_member_guard: RefCell<Vec<SemanticId>>,
    /// Short-lived local query cache. This is the intended cache layer for
    /// high-frequency semantic queries; it is discarded with the model.
    cache: RefCell<cache::SemanticLocalCache>,
}

/// Member-reference resolution result: index expression -> actual member declaration.
#[derive(Debug, Clone)]
pub struct ResolvedMember {
    /// Member declaration id (`None` when no declaration resolved).
    pub member_id: Option<SemanticId>,
    /// File containing the member declaration (always present when `member_id` has a value).
    pub file_id: Option<FileId>,
    /// Owner before resolution (`SemanticId`).
    pub owner: SemanticId,
    /// Member name.
    pub name: SmolStr,
    /// Member declaration type (`type_of_member` projection).
    pub member_type: Option<LuaType>,
    /// Member visibility (@field/@private tags).
    pub visibility: Option<VisibilityKind>,
    /// Whether this is a method definition (runtime closure signature `is_method`).
    pub is_method: bool,
}

impl ResolvedMember {
    /// Lazy member type (avoids re-entrant recursion between resolve_member and type_of_member).
    pub fn member_type(&self, model: &SemanticModel<'_>) -> Option<LuaType> {
        let member_id = self.member_id.as_ref()?;
        model.type_of_member(member_id)
    }
}

/// Semantic info at a syntax location (node / token): type + semantic declaration identity.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticInfo {
    /// Type at this location (`Unknown` = no type / inference failed).
    pub typ: LuaType,
    /// Semantic declaration identity (Decl / Member / TypeDef).
    pub decl: Option<SemanticId>,
}

/// Shared call-site analysis computed once per Lua call expression during a file check.
#[derive(Clone)]
pub(crate) struct CallSiteAnalysis {
    /// Callable signatures extracted from the callee (including overloads and member fallbacks).
    pub(crate) candidates: Vec<LuaFunctionType>,
    /// Flow-sensitive types of the actual call arguments.
    pub(crate) arg_types: Vec<LuaType>,
    /// Whether the call uses `:` syntax.
    pub(crate) colon_call: bool,
    /// Type of the implicit receiver for colon calls (`Unknown` otherwise).
    pub(crate) receiver_ty: LuaType,
    /// Explicit generic arguments written in call syntax (`f<T>(...)`).
    pub(crate) explicit_generics: Vec<LuaSyntaxId>,
}

impl<'db> SemanticModel<'db> {
    pub fn new(db: &'db SalsaDatabase, file_id: FileId) -> Option<Self> {
        Some(Self {
            db,
            file_id,
            closure_return_infer_stack: RefCell::new(Vec::new()),
            expr_infer_guard: RefCell::new(Vec::new()),
            decl_member_guard: RefCell::new(Vec::new()),
            cache: RefCell::new(cache::SemanticLocalCache::default()),
        })
    }

    pub(crate) fn begin_expr_infer(&self, expr_syntax: LuaSyntaxId) {
        let mut guard = self.expr_infer_guard.borrow_mut();
        if !guard.contains(&expr_syntax) {
            guard.push(expr_syntax);
        }
    }

    pub(crate) fn end_expr_infer(&self, expr_syntax: LuaSyntaxId) {
        let mut guard = self.expr_infer_guard.borrow_mut();
        if let Some(pos) = guard.iter().rposition(|id| *id == expr_syntax) {
            guard.remove(pos);
        }
    }

    pub(crate) fn is_expr_infer_active(&self, expr_syntax: LuaSyntaxId) -> bool {
        self.expr_infer_guard.borrow().contains(&expr_syntax)
    }
    pub(crate) fn begin_closure_return_infer(&self, closure_syntax: LuaSyntaxId) {
        let mut stack = self.closure_return_infer_stack.borrow_mut();
        if !stack.contains(&closure_syntax) {
            stack.push(closure_syntax);
        }
    }

    pub(crate) fn end_closure_return_infer(&self, closure_syntax: LuaSyntaxId) {
        let mut stack = self.closure_return_infer_stack.borrow_mut();
        if let Some(pos) = stack.iter().rposition(|id| *id == closure_syntax) {
            stack.remove(pos);
        }
    }

    pub(crate) fn is_in_closure_return_infer(&self, closure_syntax: LuaSyntaxId) -> bool {
        self.closure_return_infer_stack
            .borrow()
            .contains(&closure_syntax)
    }

    /// Underlying salsa database (escape hatch for cross-file model construction / queries).
    pub fn db(&self) -> &'db SalsaDatabase {
        self.db
    }

    /// Currently configured runtime version (used for `---@version` visibility checks).
    pub fn lua_version(&self) -> Option<LuaVersionNumber> {
        self.db.lua_version()
    }

    /// A model for any file on the same database (replaces scattered `SemanticModel::new(model.db(), ...)` calls).
    pub fn model_for(&self, file_id: FileId) -> Option<Self> {
        SemanticModel::new(self.db, file_id)
    }

    pub fn file_id(&self) -> FileId {
        self.file_id
    }

    pub fn file_path(&self) -> Option<std::path::PathBuf> {
        self.db.file_path(self.file_id)
    }

    fn q(&self) -> SalsaQueries<'db> {
        SalsaQueries::new(self.db)
    }

    // -- File / syntax --

    pub fn syntax_tree(&self) -> Option<&'db LuaSyntaxTree> {
        self.q().syntax_tree(self.file_id)
    }

    /// Syntax tree of any file (used to locate cross-file doc type nodes).
    pub fn syntax_tree_of(&self, file_id: FileId) -> Option<&'db LuaSyntaxTree> {
        self.q().syntax_tree(file_id)
    }

    pub fn chunk(&self) -> Option<LuaChunk> {
        self.q().chunk(self.file_id)
    }

    pub fn parse_errors(&self) -> Option<Vec<LuaParseError>> {
        self.q().parse_errors(self.file_id)
    }

    /// Whether the doc tag is in `emmyrc.doc.known_tags` (used by unknown_doc_tag checks).
    pub fn is_known_doc_tag(&self, name: &str) -> bool {
        self.q().is_known_doc_tag(self.file_id, name)
    }

    // -- Facts --

    /// Per-file facts arena (decls/scopes/members/... + `---@diagnostic` annotations).
    pub fn file_facts(&self) -> Option<&'db FileFacts> {
        self.q().file_facts(self.file_id)
    }

    /// Cross-file facts (member flags from members_of_owner results, etc.).
    pub fn file_facts_of(&self, file_id: FileId) -> Option<&'db FileFacts> {
        self.q().file_facts(file_id)
    }

    /// Exported facts for a file (types/globals/runtime_values/members/module identity layer).
    /// Cross-file consumption entry: computed only on the defining file's facts, never entering the defining file's function bodies.
    pub fn file_exports(&self, file_id: FileId) -> Option<&'db FileExports> {
        self.q().file_exports(file_id)
    }

    /// Exported facts for the current file.
    pub fn file_exports_current(&self) -> Option<&'db FileExports> {
        self.file_exports(self.file_id)
    }

    pub fn decls(&self) -> Option<&'db [Decl]> {
        self.q().decls(self.file_id)
    }

    pub fn scopes(&self) -> Option<&'db [Scope]> {
        self.q().scopes(self.file_id)
    }

    pub fn members(&self) -> Option<&'db [Member]> {
        self.q().members(self.file_id)
    }

    pub fn signatures(&self) -> Option<&'db [Signature]> {
        self.q().signatures(self.file_id)
    }

    pub fn name_uses(&self) -> Option<&'db [NameUse]> {
        self.q().name_uses(self.file_id)
    }

    pub fn decl_by_offset(&self, offset: TextSize) -> Option<SemanticId> {
        self.q().decl_by_offset(self.file_id, offset)
    }

    // -- Names / references --

    pub fn resolve_name(&self, offset: TextSize) -> Option<SemanticId> {
        self.q().resolve_name(self.file_id, offset)
    }

    /// Resolve a name use to a **local** declaration only.
    ///
    /// Unlike `resolve_name`, this never falls back to the workspace global index.
    /// It is useful for checkers that only need same-file declarations and then
    /// handle cross-file cases with a cheaper precomputed structure.
    pub(crate) fn resolve_local_name(&self, offset: TextSize) -> Option<SemanticId> {
        let facts = self.file_facts()?;
        let name_use = facts.name_use_at_offset(offset)?;
        facts
            .find_visible_decl_before_offset(&name_use.name, offset)
            .map(|decl| decl.id.clone())
    }

    /// Workspace global declaration (cross-file). The `Decl` key carries the declaring file.
    pub fn global_decl(&self, name: &str) -> Option<SemanticId> {
        self.q().global_decl(name)
    }

    pub fn decl_references(&self, decl: &SemanticId) -> Vec<LuaSyntaxId> {
        self.q().decl_references(self.file_id, decl.clone())
    }

    /// Syntax location -> semantic declaration (mirrors old `find_decl`; M0 supports Decl / Member / TypeDef):
    /// Definition name -> declaration name hit -> index member key -> doc name type -> name use point.
    pub fn find_decl(
        &self,
        node_or_token: rowan::NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>,
    ) -> Option<SemanticId> {
        let token = match node_or_token {
            rowan::NodeOrToken::Node(node) => node.first_token()?,
            rowan::NodeOrToken::Token(token) => token,
        };
        let offset = token.text_range().start();

        // 1. Declaration name (definition site; the `x` in `local x = 1`).
        if let Some(facts) = self.file_facts()
            && let Some(decl) = facts.decl_at_offset(offset)
        {
            return Some(decl.id.clone());
        }

        // 1.5 Member key (definition site: the `x` in `{ x = 1 }` / `@field x` / `@field [1]` / `T.x = v`) -> Member.
        if let Some(facts) = self.file_facts()
            && let Some(member) = facts.member_at_offset(offset)
        {
            return Some(member.id.clone());
        }

        let parent = token.parent()?;
        let kind: LuaSyntaxKind = parent.kind().into();
        // 2. Index-expression member key (the `x` in `T.x`, including definition sites) -> Member.
        if kind == LuaSyntaxKind::IndexExpr
            && let Some(index_expr) = LuaIndexExpr::cast(parent.clone())
        {
            if let Some(resolved) = self.resolve_member(&index_expr) {
                return resolved.member_id;
            }
        }
        // 3. Doc name type (the `Old` in `---@type Old`) -> TypeDef.
        if kind == LuaSyntaxKind::TypeName
            && let Some(name_type) = LuaDocNameType::cast(parent.clone())
            && let Some(name) = name_type.get_name_text()
            && let Some(def) = self.resolve_type_def(&name)
        {
            return Some(def.id);
        }
        // 3b. Name token of `---@class Test.Abc` (parent node DocTagClass; names may be dotted full names).
        if kind == LuaSyntaxKind::DocTagClass
            && let Some(tag) = LuaDocTagClass::cast(parent.clone())
            && let Some(name_token) = tag.get_name_token()
            && let Some(def) = self.resolve_type_def(name_token.get_name_text())
        {
            return Some(def.id);
        }
        // 3c. Name token of `---@alias schema.DiagnosticCode` -> TypeDef.
        if kind == LuaSyntaxKind::DocTagAlias
            && let Some(tag) = LuaDocTagAlias::cast(parent.clone())
            && let Some(name_token) = tag.get_name_token()
            && let Some(def) = self.resolve_type_def(name_token.get_name_text())
        {
            return Some(def.id);
        }
        // 4. Name use point (right side of `x = 1`, etc.) -> Decl.
        if kind == LuaSyntaxKind::NameExpr {
            return self.resolve_name(offset);
        }
        None
    }

    /// Syntax location (node / token) -> semantic info (type + declaration identity).
    /// Shared query for LSP features such as hover / semantic_token and checkers.
    pub fn semantic_info(
        &self,
        node_or_token: rowan::NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>,
    ) -> Option<SemanticInfo> {
        let token = match node_or_token {
            rowan::NodeOrToken::Node(node) => node.first_token()?,
            rowan::NodeOrToken::Token(token) => token,
        };
        let offset = token.text_range().start();

        // 1. Declaration name (definition site: `local x` / parameter / function name / for variable) -> Decl.
        if let Some(facts) = self.file_facts()
            && let Some(decl) = facts.decl_at_offset(offset)
        {
            return Some(SemanticInfo {
                typ: self.type_of_decl(&decl.id).unwrap_or(LuaType::Unknown),
                decl: Some(decl.id.clone()),
            });
        }

        // 2. Member key (the `x` in table field `{ x = 1 }` / `@field x` / `T.x = v` at definition sites) -> Member.
        if let Some(facts) = self.file_facts()
            && let Some(member) = facts.member_at_offset(offset)
        {
            return Some(SemanticInfo {
                typ: self.type_of_member(&member.id).unwrap_or(LuaType::Unknown),
                decl: Some(member.id.clone()),
            });
        }

        let parent = token.parent()?;
        let kind: LuaSyntaxKind = parent.kind().into();

        // 3. Doc name type (the Old/Foo in `---@type Old` / `---@class Foo`) -> TypeDef.
        if kind == LuaSyntaxKind::TypeName
            && let Some(name_type) = LuaDocNameType::cast(parent.clone())
            && let Some(name) = name_type.get_name_text()
            && let Some(def) = self.resolve_type_def(&name)
        {
            return Some(SemanticInfo {
                typ: type_def_ref(&def),
                decl: Some(def.id),
            });
        }

        // 3b. Name token of `---@class Test.Abc` (parent node DocTagClass) -> TypeDef.
        if kind == LuaSyntaxKind::DocTagClass
            && let Some(tag) = LuaDocTagClass::cast(parent.clone())
            && let Some(name_token) = tag.get_name_token()
            && let Some(def) = self.resolve_type_def(name_token.get_name_text())
        {
            return Some(SemanticInfo {
                typ: type_def_ref(&def),
                decl: Some(def.id),
            });
        }
        // 3c. Name token of `---@alias schema.DiagnosticCode` -> TypeDef.
        if kind == LuaSyntaxKind::DocTagAlias
            && let Some(tag) = LuaDocTagAlias::cast(parent.clone())
            && let Some(name_token) = tag.get_name_token()
            && let Some(def) = self.resolve_type_def(name_token.get_name_text())
        {
            return Some(SemanticInfo {
                typ: type_def_ref(&def),
                decl: Some(def.id),
            });
        }

        // 4. Index-expression member key (the `x` in `T.x` at use sites) -> Member (falls back to the expression type if not found).
        if kind == LuaSyntaxKind::IndexExpr
            && let Some(index_expr) = LuaIndexExpr::cast(parent.clone())
            && let Some(resolved) = self.resolve_member(&index_expr)
            && let Some(member_id) = resolved.member_id
        {
            return Some(SemanticInfo {
                typ: self.type_of_member(&member_id).unwrap_or(LuaType::Unknown),
                decl: Some(member_id),
            });
        }

        // 5. Expression -> type (name expressions carry declaration identity).
        if let Some(expr) = LuaExpr::cast(parent) {
            let decl = if let LuaExpr::NameExpr(name_expr) = &expr {
                self.resolve_name(name_expr.get_position())
            } else {
                None
            };
            let typ = if matches!(&expr, LuaExpr::NameExpr(_)) {
                self.assignment_target_value_type(&expr)
                    .unwrap_or_else(|| self.type_of_expr(expr.get_syntax_id()))
            } else {
                self.type_of_expr(expr.get_syntax_id())
            };
            return Some(SemanticInfo { typ, decl });
        }
        None
    }

    /// Hover type for assignment target `x = value`: when the right-hand type is known and non-nil,
    /// show the actual type after assignment (`x = create()` no longer shows the old `T?`).
    fn assignment_target_value_type(&self, expr: &LuaExpr) -> Option<LuaType> {
        let LuaExpr::NameExpr(_) = expr else {
            return None;
        };
        let assign = expr.syntax().parent().and_then(LuaAssignStat::cast)?;
        let (vars, values) = assign.get_var_and_expr_list();
        let idx = vars
            .iter()
            .position(|var| var.to_expr().get_syntax_id() == expr.get_syntax_id())?;
        let value = values.get(idx)?;
        // Pure name references `x = y` keep declaration/literal display (hover still shows `integer` rather than widened `number`);
        // Assignments that clearly produce a new value (calls/constructors) use the RHS type (`x = create()` removes the old `T?`).
        if matches!(value, LuaExpr::NameExpr(_)) {
            return None;
        }
        let ty = self.type_of_expr(value.get_syntax_id());
        (!matches!(ty, LuaType::Unknown | LuaType::Nil)).then_some(ty)
    }

    /// Whether a syntax node references the given declaration (convenience wrapper, equivalent to `is_reference_to(NodeOrToken::Node)`).
    pub fn is_reference_to_syntax(&self, node: &LuaSyntaxNode, decl: &SemanticId) -> bool {
        self.is_reference_to(rowan::NodeOrToken::Node(node.clone()), decl)
    }

    /// Whether a syntax node can access the given declaration (convenience wrapper).
    pub fn is_visible_syntax(&self, node: &LuaSyntaxNode, decl: &SemanticId) -> bool {
        self.is_visible(rowan::NodeOrToken::Node(node.clone()), decl)
    }

    /// Whether a syntax location (node / token) references the given declaration (references / rename / highlight scenarios).
    pub fn is_reference_to(
        &self,
        node_or_token: rowan::NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>,
        decl: &SemanticId,
    ) -> bool {
        let Some(info) = self.semantic_info(node_or_token) else {
            return false;
        };
        info.decl.as_ref() == Some(decl)
    }

    /// Whether a syntax location (node / token) can access the given declaration (visibility check).
    /// M0: Public/Internal -> visible; Package/Private/Protected -> visible within the same file (intra-class access refinement left for later).
    pub fn is_visible(
        &self,
        node_or_token: rowan::NodeOrToken<LuaSyntaxNode, LuaSyntaxToken>,
        decl: &SemanticId,
    ) -> bool {
        let _ = node_or_token;
        // Declaring file (visibility is determined by the declaring file).
        let decl_file = match decl {
            SemanticId::Decl(key) => key.file_id,
            SemanticId::Member(key) => key.file_id,
            SemanticId::TypeDef(key) => match key.scope {
                TypeScope::File(file_id) => file_id,
                _ => return true, // Global types are always visible
            },
            _ => return true,
        };
        // Member visibility annotation.
        if let SemanticId::Member(_) = decl
            && let Some(facts) = self.file_facts_of(decl_file)
            && let Some(member) = facts.member_by_id(decl)
        {
            use VisibilityKind;
            return match member.visibility {
                VisibilityKind::Public | VisibilityKind::Internal => true,
                VisibilityKind::Package | VisibilityKind::Private | VisibilityKind::Protected => {
                    decl_file == self.file_id
                }
            };
        }
        // Type definitions: File scope (@private) is visible only in the same file.
        if let SemanticId::TypeDef(_) = decl {
            return decl_file == self.file_id;
        }
        true
    }

    /// Assembles `ResolvedMember` (fills type/visibility/method flag).
    fn resolved_member(
        &self,
        member_id: Option<SemanticId>,
        file_id: Option<FileId>,
        owner: SemanticId,
        name: SmolStr,
    ) -> ResolvedMember {
        let mut member_type = None;
        let mut visibility = None;
        let mut is_method = false;
        if let (Some(member_id), Some(file_id)) = (&member_id, file_id)
            && let Some(facts) = self.file_facts_of(file_id)
            && let Some(member) = facts.member_by_id(member_id)
        {
            // Type is computed lazily (avoids recursion when resolve_member is reverse-called by type_of_member);
            // Consumers can query type_of_member later when they need the full type.
            member_type = None;
            visibility = Some(member.visibility);
            if let Some(value_syntax) = member.value_syntax
                && let Some(signature) = facts.signature_by_closure(value_syntax)
            {
                is_method = signature.is_method;
            }
        }
        ResolvedMember {
            member_id,
            file_id,
            owner,
            name,
            member_type,
            visibility,
            is_method,
        }
    }

    /// Gets the prefix type during member resolution: NameExpr goes directly through non-flow `type_of_decl`,
    /// avoiding recursive explosion from the VM's `type_of_expr` -> `type_of_decl_at` during flow backtracking.
    fn prefix_type_for_member_resolution(&self, expr: &LuaExpr) -> LuaType {
        if let LuaExpr::NameExpr(name_expr) = expr {
            if let Some(decl) = self.resolve_name(name_expr.get_position()) {
                let mut ty = self.type_of_decl(&decl).unwrap_or(LuaType::Unknown);
                // `type_of_decl` may return Unknown for parameters with no call-site inference;
                // member resolution needs the annotated parameter type (the `p._cfg` for `---@param p T2`).
                if matches!(ty, LuaType::Unknown)
                    && let Some(param_ty) = self.param_type_for_decl(&decl)
                {
                    ty = param_ty;
                }
                return self.attach_param_decl_constraint(&decl, ty);
            }
        }
        self.type_of_expr(expr.get_syntax_id())
    }

    /// Takes the `---@param` annotation type directly from the parameter's owning closure (skips `type_of_decl`).
    fn param_type_for_decl(&self, decl: &SemanticId) -> Option<LuaType> {
        let SemanticId::Decl(decl_key) = decl else {
            return None;
        };
        let facts = self.file_facts_of(decl_key.file_id)?;
        let decl_info = facts.decl_by_id(decl)?;
        if !matches!(decl_info.kind, DeclKind::Param) {
            return None;
        }
        let closure_syntax = decl_info.owner_syntax?;
        let signature = facts
            .signatures
            .iter()
            .find(|sig| sig.closure_syntax == closure_syntax)?;
        let param_index = signature
            .param_names
            .iter()
            .position(|name| name == &decl_info.name)?;
        self.param_type(closure_syntax, param_index)
    }

    /// Whether the union member is missing from some components: when `A|C` has `handle` only on A,
    /// `target.handle` should report a missing member in parameter checks rather than silently taking A's field type.
    pub fn member_missing_in_union(&self, index_expr: &LuaIndexExpr) -> bool {
        let Some(index_key) = index_expr.get_index_key() else {
            return false;
        };
        let Some(prefix) = index_expr.get_prefix_expr() else {
            return false;
        };
        let prefix_ty = self.prefix_type_for_member_resolution(&prefix);
        let LuaType::Union(union) = &prefix_ty else {
            return false;
        };
        let key = LuaMemberKey::Name(SmolStr::new(index_key.get_path_part()));
        union
            .into_vec()
            .iter()
            .any(|component| self.member_type(component, &key).is_none())
    }

    /// Resolves member references in index expressions (single member resolution entry point):
    /// same-file member -> same-file class `@field` -> cross-file runtime member (resolve owner -> merge members).
    /// Cached `callable_functions` result for a callee type.
    pub(crate) fn callable_functions_cached(&self, ty: &LuaType) -> Vec<LuaFunctionType> {
        if let Some(cached) = self.cache.borrow().callable_functions.get(ty) {
            return cached.clone();
        }
        let value = crate::check::checker::param_count::callable_functions(self, ty);
        self.cache
            .borrow_mut()
            .callable_functions
            .insert(ty.clone(), value.clone());
        value
    }

    pub(crate) fn callable_candidates_cached(&self, callee: &LuaExpr) -> Vec<LuaFunctionType> {
        let syntax = callee.get_syntax_id();
        let file_id = self.file_id;
        if let Some(cached) = self
            .cache
            .borrow()
            .callable_candidates
            .get(&(file_id, syntax))
        {
            return cached.clone();
        }
        let value =
            crate::check::checker::param_type_check::callable_candidates_uncached(self, callee);
        self.cache
            .borrow_mut()
            .callable_candidates
            .insert((file_id, syntax), value.clone());
        value
    }

    pub(crate) fn call_site_analysis(&self, call_expr: &LuaCallExpr) -> CallSiteAnalysis {
        let syntax = call_expr.get_syntax_id();
        let file_id = self.file_id;
        if let Some(cached) = self.cache.borrow().call_site.get(&(file_id, syntax)) {
            return cached.clone();
        }
        let analysis = self.call_site_analysis_uncached(call_expr);
        self.cache
            .borrow_mut()
            .call_site
            .insert((file_id, syntax), analysis.clone());
        analysis
    }

    pub(crate) fn call_site_analysis_uncached(&self, call_expr: &LuaCallExpr) -> CallSiteAnalysis {
        let Some(callee) = call_expr.get_prefix_expr() else {
            return CallSiteAnalysis {
                candidates: Vec::new(),
                arg_types: Vec::new(),
                colon_call: call_expr.is_colon_call(),
                receiver_ty: LuaType::Unknown,
                explicit_generics: Vec::new(),
            };
        };
        let candidates = self.callable_candidates_cached(&callee);
        if candidates.is_empty() {
            return CallSiteAnalysis {
                candidates,
                arg_types: Vec::new(),
                colon_call: call_expr.is_colon_call(),
                receiver_ty: LuaType::Unknown,
                explicit_generics: Vec::new(),
            };
        }
        let args = call_expr
            .get_args_list()
            .map(|list| list.get_args().collect::<Vec<_>>())
            .unwrap_or_default();
        let arg_types: Vec<LuaType> = args
            .iter()
            .map(|arg| self.type_of_expr(arg.get_syntax_id()))
            .collect();
        let colon_call = call_expr.is_colon_call();
        let receiver_ty = if colon_call {
            LuaIndexExpr::cast(callee.syntax().clone())
                .and_then(|index| index.get_prefix_expr())
                .map(|prefix| self.type_of_expr(prefix.get_syntax_id()))
                .unwrap_or(LuaType::Unknown)
        } else {
            LuaType::Unknown
        };
        let explicit_generics: Vec<LuaSyntaxId> = call_expr
            .get_call_generic_type_list()
            .map(|list| list.get_types().map(|ty| ty.get_syntax_id()).collect())
            .unwrap_or_default();
        // Resolved signatures are intentionally lazy: they are only needed by the
        // generic-constraint checker. Computing them eagerly for every call is one of
        // the largest costs in `ParamTypeChecker`.
        CallSiteAnalysis {
            candidates,
            arg_types,
            colon_call,
            receiver_ty,
            explicit_generics,
        }
    }

    /// Lazily compute and cache resolved call signatures (generic bindings per candidate).
    pub(crate) fn call_site_signatures(
        &self,
        call_expr: &LuaCallExpr,
    ) -> Vec<(LuaFunctionType, infer::unify::TplBindings)> {
        let syntax = call_expr.get_syntax_id();
        let file_id = self.file_id;
        if let Some(cached) = self
            .cache
            .borrow()
            .call_site_signatures
            .get(&(file_id, syntax))
        {
            return cached.clone();
        }
        let analysis = self.call_site_analysis(call_expr);
        let signatures =
            crate::check::checker::generic_constraint_mismatch::resolved_call_signatures(
                self,
                call_expr,
                &analysis.candidates,
                &analysis.arg_types,
                analysis.colon_call,
                &analysis.receiver_ty,
            );
        self.cache
            .borrow_mut()
            .call_site_signatures
            .insert((file_id, syntax), signatures.clone());
        signatures
    }

    pub fn resolve_member(&self, index_expr: &LuaIndexExpr) -> Option<ResolvedMember> {
        let syntax = index_expr.get_syntax_id();
        let file_id = self.file_id;
        if let Some(cached) = self.cache.borrow().resolve_member.get(&(file_id, syntax)) {
            return cached.clone();
        }
        let result = self.resolve_member_impl(index_expr);
        self.cache
            .borrow_mut()
            .resolve_member
            .insert((file_id, syntax), result.clone());
        result
    }

    pub(crate) fn resolve_member_impl(&self, index_expr: &LuaIndexExpr) -> Option<ResolvedMember> {
        let (owner, name) = self
            .q()
            .member_ref_of_index(self.file_id, index_expr.get_syntax_id())?;

        // 1. Same-file members (owner key). Members with explicit `---@type` take priority over purely inferred runtime members;
        // if only runtime members exist and the prefix type is a named type with a same-named `@field`, skip this step --
        // under `---@param data CreateData`, `data.owner = ""` should resolve to the class field rather than this assignment.
        {
            let facts = self.file_facts()?;
            let same_name = facts
                .members_of_owner_named(&owner, name.as_str())
                .collect::<Vec<_>>();
            let prefer_typed = same_name
                .iter()
                .all(|member| member.doc_type_syntax.is_none())
                && (self.prefix_has_named_member(index_expr, &name)
                    || self.require_module_has_member(index_expr, &name));
            if !prefer_typed {
                let member_id = same_name
                    .iter()
                    .find(|member| member.doc_type_syntax.is_some())
                    .or_else(|| same_name.first())
                    .map(|member| member.id.clone());
                if let Some(member_id) = member_id {
                    return Some(self.resolved_member(
                        Some(member_id),
                        Some(self.file_id),
                        owner,
                        name,
                    ));
                }
            }
        }

        // 2. Same-file class definition: `@field` for `C.field`.
        if let Some(facts) = self.file_facts()
            && let Some(LuaExpr::NameExpr(prefix)) = index_expr.get_prefix_expr()
            && let Some(prefix_text) = prefix.get_name_text()
            && let Some(def) = facts.type_def_by_name(&prefix_text)
            && let Some(member_id) = facts
                .field_members_of_type(&def.id, &name)
                .map(|member| member.id.clone())
        {
            return Some(self.resolved_member(Some(member_id), Some(self.file_id), owner, name));
        }

        // Compute the prefix type once for the remaining slow-path stages. `resolve_member`
        // must not repeatedly infer the same prefix expression (3 / 3.5 / inherited fallback).
        let prefix_ty = index_expr
            .get_prefix_expr()
            .map(|prefix| self.prefix_type_for_member_resolution(&prefix));

        // 3. Prefix type is a named type -> its `@field` members (cross-file, e.g. `c.secret` where c: C).
        //    Also look up the runtime value declaration for that type (`---@class Game` + `local Game = {}`),
        //    otherwise dot access like `game.add` cannot resolve `function Game:add()`.
        if let Some(prefix_ty) = &prefix_ty {
            let type_id = match prefix_ty {
                LuaType::Ref(id) | LuaType::Def(id) => Some(id),
                LuaType::Generic(generic) => Some(generic.get_base_type_id_ref()),
                LuaType::TplRef(tpl) => match tpl.get_constraint() {
                    Some(LuaType::Ref(id)) | Some(LuaType::Def(id)) => Some(id),
                    Some(LuaType::Generic(generic)) => Some(generic.get_base_type_id_ref()),
                    _ => None,
                },
                _ => None,
            };
            if let Some(id) = type_id
                && let Some(def) = member::type_def_of(self, id)
            {
                // Class types also consider runtime implementations (`---@class Game` + `local Game = {}` +
                // `function Game:add()`); enums/aliases only look at the `@field` surface to avoid
                // treating fields in enum implementations as valid members.
                let owners = if def.kind == TypeDefKind::Class {
                    self.q().resolve_owner_set(def.id.clone())
                } else {
                    vec![def.id.clone()]
                };
                for resolved in owners {
                    for member_ref in self.members_of_owner(&resolved) {
                        if member_ref.name == name {
                            return Some(self.resolved_member(
                                Some(member_ref.id),
                                Some(member_ref.file_id),
                                owner,
                                name,
                            ));
                        }
                    }
                    // Runtime members from `self.x = ...` in method bodies: attach members on the implicit self parameter
                    // to the class/runtime owner of that method (fields assigned in `function T:init`,
                    // then `p.x` from a `T` instance should resolve).
                    for method_ref in self.members_of_owner(&resolved) {
                        let Some(method_facts) = self.file_facts_of(method_ref.file_id) else {
                            continue;
                        };
                        let Some(method_member) = method_facts.member_by_id(&method_ref.id) else {
                            continue;
                        };
                        let Some(closure_syntax) = method_member.value_syntax else {
                            continue;
                        };
                        let Some(self_decl) = method_facts
                            .decls
                            .iter()
                            .find(|d| d.name == "self" && d.owner_syntax == Some(closure_syntax))
                        else {
                            continue;
                        };
                        for self_member_ref in method_facts.members_of_owner(&self_decl.id) {
                            if self_member_ref.key.name() == Some(name.as_str()) {
                                let file_id = match &self_member_ref.id {
                                    SemanticId::Member(key) => Some(key.file_id),
                                    _ => None,
                                };
                                return Some(self.resolved_member(
                                    Some(self_member_ref.id.clone()),
                                    file_id,
                                    owner,
                                    name,
                                ));
                            }
                        }
                    }
                }
            }
        }

        // 3.5 Prefix type is an anonymous table (TableConst) -> its table-field members.
        //     Handles the `y` in `T.x.y` after `local T = { x = { y = 1 } }`.
        if let Some(LuaType::TableConst(in_field)) = prefix_ty.as_ref() {
            let table_owner = SemanticId::member(in_field.file_id, in_field.value);
            let mut table_owners = vec![table_owner];
            // TableConst for named local tables (`local checker = { ... }`) must also resolve
            // members like `function checker:is_player()`, not only the synthetic owner of anonymous tables.
            if let Some(facts) = self.file_facts_of(in_field.file_id) {
                for decl in &facts.decls {
                    if decl
                        .value_expr_syntax
                        .is_some_and(|syntax| syntax.get_range() == in_field.value)
                    {
                        table_owners.push(decl.id.clone());
                        break;
                    }
                }
            }
            for table_owner in table_owners {
                for member in self.members_of_owner(&table_owner) {
                    if member.name == name {
                        return Some(self.resolved_member(
                            Some(member.id),
                            Some(member.file_id),
                            owner,
                            name,
                        ));
                    }
                }
                // Runtime members defined by `self.x = ...` in that table's method bodies are also attached to the table.
                for method_ref in self.members_of_owner(&table_owner) {
                    let Some(method_facts) = self.file_facts_of(method_ref.file_id) else {
                        continue;
                    };
                    let Some(method_member) = method_facts.member_by_id(&method_ref.id) else {
                        continue;
                    };
                    if !method_member.is_method {
                        continue;
                    }
                    let Some(closure_syntax) = method_member.value_syntax else {
                        continue;
                    };
                    let Some(self_decl) = method_facts
                        .decls
                        .iter()
                        .find(|d| d.name == "self" && d.owner_syntax == Some(closure_syntax))
                    else {
                        continue;
                    };
                    for self_member_ref in method_facts.members_of_owner(&self_decl.id) {
                        if self_member_ref.key.name() == Some(name.as_str()) {
                            let file_id = match &self_member_ref.id {
                                SemanticId::Member(key) => Some(key.file_id),
                                _ => None,
                            };
                            return Some(self.resolved_member(
                                Some(self_member_ref.id.clone()),
                                file_id,
                                owner,
                                name,
                            ));
                        }
                    }
                }
            }
        }

        // 4. Cross-file runtime members + require module export members.
        //    Expand multiple identities locally (avoids dependency cycles between tracked resolve_owner_set and type_of_member):
        //    Old-path owners take priority; non-public members or members with doc declarations win by score.
        let old_owner = self.resolve_owner(&owner);
        let mut cross_owners = vec![owner.clone()];
        for resolved in old_owner.iter() {
            if !cross_owners.contains(resolved) {
                cross_owners.push(resolved.clone());
            }
        }
        match &owner {
            SemanticId::Name(name) => {
                if let Some(type_def) = self
                    .type_defs_in_scope(TypeScope::Global, name.as_str())
                    .into_iter()
                    .next()
                {
                    if !cross_owners.contains(&type_def.id) {
                        cross_owners.push(type_def.id);
                    }
                }
                if let Some(decl) = self.global_decl(name.as_str())
                    && !cross_owners.contains(&decl)
                {
                    cross_owners.push(decl);
                }
            }
            SemanticId::TypeDef(_type_def) => {
                if let Some(decl) = self.resolve_owner(&owner)
                    && !cross_owners.contains(&decl)
                {
                    cross_owners.push(decl);
                }
            }
            _ => {}
        }
        if let Some(module_owner) = self.require_module_owner(index_expr)
            && !cross_owners.contains(&module_owner)
        {
            cross_owners.push(module_owner);
        }
        let mut best: Option<(i64, MemberRef)> = None;
        for resolved in &cross_owners {
            let is_old = old_owner.as_ref() == Some(resolved);
            for member in self.members_of_owner(resolved) {
                if member.name != name {
                    continue;
                }
                let mut score = if is_old { 10_000 } else { 0 };
                if let Some(facts) = self.file_facts_of(member.file_id)
                    && let Some(member_facts) = facts.member_by_id(&member.id)
                {
                    if member_facts.visibility != VisibilityKind::Public {
                        score -= 2_000;
                    }
                    if member_facts.doc_type_syntax.is_some() {
                        score -= 1;
                    }
                }
                if best
                    .as_ref()
                    .is_none_or(|(best_score, _)| score < *best_score)
                {
                    best = Some((score, member));
                }
            }
        }
        if let Some((_, member)) = best {
            return Some(self.resolved_member(Some(member.id), Some(member.file_id), owner, name));
        }

        // Inherited/cross-class members: `member_info` projects along parent types, enabled only for class types;
        // same-name member resolution on unions is left to flow narrowing to avoid breaking dynamic field tests on `Foo|Bar`.
        if let Some(prefix_ty) = &prefix_ty {
            let is_class = match prefix_ty {
                LuaType::Ref(id) | LuaType::Def(id) => {
                    member::type_def_of(self, id).is_some_and(|def| def.kind == TypeDefKind::Class)
                }
                LuaType::Generic(generic) => {
                    member::type_def_of(self, generic.get_base_type_id_ref())
                        .is_some_and(|def| def.kind == TypeDefKind::Class)
                }
                _ => false,
            };
            if is_class {
                let key = LuaMemberKey::Name(name.clone());
                if let Some(info) = self.member_info(prefix_ty, &key)
                    && let Some(id) = info.id
                    && let Some(file_id) = info.file_id
                {
                    return Some(self.resolved_member(Some(id), Some(file_id), owner, name));
                }
            }
        }

        Some(self.resolved_member(None, None, owner, name))
    }

    /// Whether the prefix type is a named type with a same-named member (avoids runtime inferred members shadowing class `@field`).
    fn prefix_has_named_member(&self, index_expr: &LuaIndexExpr, name: &SmolStr) -> bool {
        let Some(prefix) = index_expr.get_prefix_expr() else {
            return false;
        };
        let prefix_ty = self.prefix_type_for_member_resolution(&prefix);
        let type_id = match &prefix_ty {
            LuaType::Ref(id) | LuaType::Def(id) => Some(id),
            LuaType::Generic(generic) => Some(generic.get_base_type_id_ref()),
            _ => None,
        };
        let Some(def) = type_id.and_then(|id| member::type_def_of(self, id)) else {
            return false;
        };
        self.members_of_owner(&def.id)
            .iter()
            .any(|member| member.name == *name)
    }

    /// Whether the require module export declares this member (same-name assignments do not shadow exported members).
    fn require_module_has_member(&self, index_expr: &LuaIndexExpr, name: &SmolStr) -> bool {
        let Some(owner) = self.require_module_owner(index_expr) else {
            return false;
        };
        self.members_of_owner(&owner)
            .iter()
            .any(|member| member.name == *name)
    }

    /// `local M = require("mod")` -> module export declaration owner (member bridging).
    fn require_module_owner(&self, index_expr: &LuaIndexExpr) -> Option<SemanticId> {
        let prefix = index_expr.get_prefix_expr()?;
        let LuaExpr::NameExpr(name_expr) = prefix else {
            return None;
        };
        let decl = self.resolve_name(name_expr.get_position())?;
        let facts = self.file_facts()?;
        let decl = facts.decl_by_id(&decl)?;
        let call_syntax = decl.value_expr_syntax?;
        let tree = self.syntax_tree()?;
        let node = call_syntax.to_node_from_root(&tree.get_red_root())?;
        let call = LuaCallExpr::cast(node)?;
        let arg = call.get_args_list()?.get_args().next()?;
        let module_name = match self.type_of_expr(arg.get_syntax_id()) {
            LuaType::StringConst(s) | LuaType::DocStringConst(s) => s.as_ref().to_string(),
            _ => return None,
        };
        let module_file = self.module_file_of(&module_name)?;
        let module_facts = self.file_facts_of(module_file)?;
        match &module_facts.module_export {
            ModuleExport::Decl { decl, .. } => Some(decl.clone()),
            ModuleExport::Expr { value_syntax } => {
                Some(SemanticId::member(module_file, value_syntax.get_range()))
            }
            _ => None,
        }
    }

    // -- Types --

    /// Whether the doc syntax node is a bare name type (`Base`, not `Base<T>`).
    fn doc_type_is_bare_name(&self, syntax: LuaSyntaxId) -> bool {
        let Some(tree) = self.syntax_tree() else {
            return false;
        };
        let Some(node) = syntax.to_node_from_root(&tree.get_red_root()) else {
            return false;
        };
        matches!(LuaDocType::cast(node), Some(LuaDocType::Name(_)))
    }

    /// Whether a bare name reference has at least one generic parameter without a default (missing required arg -> any).
    fn type_has_missing_required_generic_args(&self, ty: &LuaType) -> bool {
        let id = match ty {
            LuaType::Ref(id) | LuaType::Def(id) => id,
            _ => return false,
        };
        let Some(def) = self.type_def_of(id) else {
            return false;
        };
        !def.generic_params.is_empty()
            && def
                .generic_params
                .iter()
                .any(|param| param.default.is_none())
    }

    /// Pure alias cycle detection: `A -> B -> A` collapses to `any` (returns only cycle results; normal aliases return None).
    fn alias_cycle_any(&self, ty: &LuaType) -> Option<LuaType> {
        let mut current = ty.clone();
        let mut visited = Vec::new();
        loop {
            let id = match &current {
                LuaType::Ref(id) | LuaType::Def(id) => id.clone(),
                _ => return None,
            };
            let def = self.type_def_of(&id)?;
            if def.kind != TypeDefKind::Alias {
                return None;
            }
            if visited.contains(&id) {
                return Some(LuaType::Any);
            }
            visited.push(id.clone());
            current = self.alias_target(&def)?;
        }
    }

    /// Type of a declaration: `---@type` annotation takes priority; closure -> `DocFunction` signature; otherwise VM infers the initializer
    /// (table identity / constants / setmetatable / require special cases); falls back to salsa shell projection when VM fails.
    pub fn type_of_decl(&self, decl: &SemanticId) -> Option<LuaType> {
        let cache_file = match decl {
            SemanticId::Decl(key) => key.file_id,
            SemanticId::Member(key) => key.file_id,
            _ => self.file_id,
        };
        if let Some(cached) = self
            .cache
            .borrow()
            .decl_type
            .get(&(cache_file, decl.clone()))
        {
            return cached.clone();
        }
        {
            let mut guard = self.decl_member_guard.borrow_mut();
            if guard.contains(decl) {
                return None;
            }
            guard.push(decl.clone());
        }
        let result = self.type_of_decl_impl(decl);
        self.decl_member_guard.borrow_mut().pop();
        self.cache
            .borrow_mut()
            .decl_type
            .insert((cache_file, decl.clone()), result.clone());
        result
    }

    pub(crate) fn type_of_decl_impl(&self, decl: &SemanticId) -> Option<LuaType> {
        if let Some(facts) = self.file_facts()
            && let Some(decl) = facts.decl_by_id(decl)
        {
            // Implicit method `self`: type is the method owner's instance type. Once registered as a Param,
            // `type_of_decl_at` flow queries then apply narrowing like `self == ...` on top of this.
            if decl.name == "self"
                && matches!(decl.kind, DeclKind::Param)
                && let Some(closure_syntax) = self.method_closure_for_self_decl(decl)
                && let Some(ty) = self.method_owner_type(closure_syntax)
            {
                return Some(ty);
            }
            // `for k, v in pairs(x)`: iteration variables are inferred from the iterator's return slots.
            if matches!(decl.kind, DeclKind::Local { is_iter: true, .. })
                && let Some(ty) = self.infer_for_range_var(decl)
                && !matches!(ty, LuaType::Unknown)
            {
                return Some(ty);
            }
            // `---@type` annotations (including fun<T> structures) take priority over closure signatures;
            // when shell degrades to bare Table / Unknown, rich projection fills in object/keyof/intersection structures.
            if let Some(doc_syntax) = decl.doc_type_syntax {
                if let Some(ty) = self.q().decl_type_lua(self.file_id, decl.id.clone())
                    && !matches!(ty, LuaType::Unknown)
                {
                    // When required generic arguments are missing (`---@type Base`, and at least one parameter has no default),
                    // TypeScript/LuaLS semantics are `any` plus MissingTypeArgument; constraints/Unknown cannot be used as defaults.
                    if matches!(ty, LuaType::Ref(_) | LuaType::Def(_))
                        && self.doc_type_is_bare_name(doc_syntax)
                        && self.type_has_missing_required_generic_args(&ty)
                    {
                        return Some(LuaType::Any);
                    }
                    // Pure alias cycles collapse to any at the declaration type entry; ordinary structural aliases keep the original nominal Ref form.
                    if let Some(any) = self.alias_cycle_any(&ty) {
                        return Some(any);
                    }
                    let expanded = type_eval::expand_alias_generic(self, &ty);
                    let ty = if matches!(expanded, LuaType::Unknown | LuaType::Any) {
                        return Some(expanded);
                    } else {
                        ty
                    };
                    // Rich projection takes priority: structural annotations like Object / Intersection / keyof must not be
                    // overridden by shell's broad Table / Nil fallback.
                    let rich = self.doc_type_lua_rich(doc_syntax);
                    if !matches!(rich, LuaType::Unknown) && rich != ty {
                        let rich_is_index_not_precise = matches!(
                            &rich,
                            LuaType::Call(call)
                                if call.get_call_kind() == LuaAliasCallKind::Index
                        );
                        if !rich_is_index_not_precise
                            || matches!(ty, LuaType::Table | LuaType::Unknown)
                        {
                            return Some(rich);
                        }
                    }
                    if matches!(ty, LuaType::Table) {
                        if !matches!(rich, LuaType::Unknown) {
                            return Some(rich);
                        }
                    }
                    // When literal unions / constant annotations are downgraded to broad primitives by shell, use rich projection to preserve them.
                    if matches!(
                        ty,
                        LuaType::String | LuaType::Number | LuaType::Integer | LuaType::Boolean
                    ) {
                        let rich = self.doc_type_lua_rich(doc_syntax);
                        if matches!(
                            rich,
                            LuaType::Union(_)
                                | LuaType::StringConst(_)
                                | LuaType::DocStringConst(_)
                                | LuaType::IntegerConst(_)
                                | LuaType::DocIntegerConst(_)
                                | LuaType::BooleanConst(_)
                                | LuaType::DocBooleanConst(_)
                        ) {
                            return Some(rich);
                        }
                    }
                    return Some(ty);
                }
                let rich = self.doc_type_lua_rich(doc_syntax);
                if !matches!(rich, LuaType::Unknown) {
                    return Some(rich);
                }
            }
            // `---@module "name"`: project the declaration directly as a module reference.
            if let Some(module_path) = &decl.module_path
                && let Some(module_file) = self.module_file_of(module_path)
            {
                return Some(LuaType::ModuleRef(module_file));
            }
            // `---@[lsp_optimization("delayed_definition")]`: delay the type until later assignments are resolved.
            if decl.delayed_definition
                && let Some(ty) = self.infer_delayed_definition_type(decl)
            {
                return Some(ty);
            }
            // `---@class Foo` / `---@enum Foo` + `x = {}`: associate the runtime variable following the comment
            // with the type definition even if the variable name differs from the class name. Semantically that runtime table is the class table.
            if let Some(owner_syntax) = decl.owner_syntax
                && let Some(facts) = self.file_facts()
                && let Some(def) = facts
                    .type_def_by_owner_syntax(owner_syntax)
                    .filter(|def| matches!(def.kind, TypeDefKind::Class | TypeDefKind::Enum))
            {
                return Some(self.type_def_ref(def));
            }
            // Parameter declarations: without `---@param`, try inferring closure parameter types from call sites
            // (in `f(function(msg) end)`, msg is determined by `fun(msg: string)`).
            if matches!(decl.kind, DeclKind::Param)
                && let Some(signatures) = self.signatures()
                && let Some((closure_syntax, param_index)) = signatures.iter().find_map(|sig| {
                    // Same-named parameters can appear in multiple functions, so prefer the parameter's own owner closure
                    // to avoid mistaking the p in `a:init(p)` for the p in `fun(p)`.
                    if decl
                        .owner_syntax
                        .is_some_and(|owner| owner != sig.closure_syntax)
                    {
                        return None;
                    }
                    sig.param_names
                        .iter()
                        .position(|name| name == &decl.name)
                        .map(|idx| (sig.closure_syntax, idx))
                })
            {
                let ty = infer::closure_param_lua(self, closure_syntax, param_index);
                if !matches!(ty, LuaType::Unknown) {
                    return Some(self.attach_generic_constraints(ty, closure_syntax));
                }
                // `function a.aaa(x)`: when no call site can be inferred, fill in parameter types from the owner type's field signature.
                if let Some(mut param_ty) =
                    self.expected_member_param_for_closure(closure_syntax, param_index)
                {
                    // `self` in field types must be concretized to the owner instance during function-body variable inference,
                    // but stays SelfInfer in signature types so call checks remain consistent with the original field signature.
                    if let Some(owner_ty) = self.method_owner_type(closure_syntax) {
                        param_ty = infer::vm::replace_self_type(&param_ty, &owner_ty);
                    }
                    return Some(self.attach_generic_constraints(param_ty, closure_syntax));
                }
            }

            // Value is a closure -> function signature structure.
            if let Some(value_syntax) = decl.value_expr_syntax
                && let Some(tree) = self.syntax_tree()
                && let Some(node) = value_syntax.to_node_from_root(&tree.get_red_root())
                && let Some(closure) = LuaClosureExpr::cast(node)
                && let Some(fun) = self.type_of_signature(closure.get_syntax_id())
            {
                return Some(LuaType::DocFunction(Arc::new(fun)));
            }
            // Initializer expression: VM inference (cycles are guarded by thread-local; inside a cycle fall back to shell).
            // Index/member-access RHS needs flow-sensitive types to preserve narrowing for table literals/array lengths inside branches.
            if let Some(value_syntax) = decl.value_expr_syntax
                && let Some(ty) = self.infer_decl_guarded(decl.id.clone(), || {
                    let use_flow = self
                        .syntax_tree()
                        .and_then(|tree| value_syntax.to_node_from_root(&tree.get_red_root()))
                        .and_then(LuaIndexExpr::cast)
                        .is_some();
                    if use_flow {
                        self.type_of_expr_at(value_syntax, value_syntax.get_range().start())
                    } else {
                        self.type_of_expr(value_syntax)
                    }
                })
            {
                if !matches!(ty, LuaType::Unknown) {
                    // Multi-return assignment slot: the `b` in `local a, b = f()` takes f()'s 2nd return.
                    // When a single-return function has no second slot, extra assignment targets stay Any (undeclared missing values),
                    // rather than repeating the whole single return type for later variables.
                    if let Some(return_index) = decl.multi_return_index {
                        if let Some(slot) = ty.get_result_slot_type(return_index) {
                            return Some(slot);
                        }
                        if !ty.contain_multi_return() {
                            return Some(LuaType::Any);
                        }
                    }
                    return Some(ty);
                }
            }
        }
        // Cross-file declarations: query by the file carried in the declaration (the Decl key includes file_id).
        // Lazy type execution: `decl_type_lua` is keyed by the declaring file, entering only defining files, not consumer models.
        let decl_file = match decl {
            SemanticId::Decl(key) => key.file_id,
            _ => self.file_id,
        };
        if decl_file != self.file_id
            && let Some(foreign_model) = SemanticModel::new(self.db, decl_file)
            && let Some(foreign_ty) = foreign_model.type_of_decl(decl)
        {
            return Some(foreign_ty);
        }
        self.q().decl_type_lua(decl_file, decl.clone())
    }

    /// Parameter declarations may lose generic constraints in `type_of_decl`'s fallback path; re-attach them here.
    fn attach_param_decl_constraint(&self, decl: &SemanticId, ty: LuaType) -> LuaType {
        let Some(facts) = self.file_facts() else {
            return ty;
        };
        let Some(decl_info) = facts.decl_by_id(decl) else {
            return ty;
        };
        if !matches!(decl_info.kind, DeclKind::Param) {
            return ty;
        }
        let Some(signatures) = self.signatures() else {
            return ty;
        };
        let Some((closure_syntax, _)) = signatures.iter().find_map(|sig| {
            sig.param_names
                .iter()
                .position(|name| name == &decl_info.name)
                .map(|idx| (sig.closure_syntax, idx))
        }) else {
            return ty;
        };
        self.attach_generic_constraints(ty, closure_syntax)
    }

    /// Fills generic constraints from signature docs back into `TplRef` (so constraints survive parameter/return type projection).
    fn attach_generic_constraints(&self, ty: LuaType, closure_syntax: LuaSyntaxId) -> LuaType {
        let Some(signatures) = self.signatures() else {
            return ty;
        };
        let Some(signature) = signatures
            .iter()
            .find(|sig| sig.closure_syntax == closure_syntax)
        else {
            return ty;
        };
        let Some(docs) = &signature.docs else {
            return ty;
        };
        let generic_params = &docs.generic_params;
        match ty {
            LuaType::TplRef(ref tpl) => {
                if let Some(param) = generic_params.iter().find(|g| g.name == tpl.get_name()) {
                    let constraint = param.constraint.map(|syntax| self.doc_type_lua(syntax));
                    let default = tpl.get_default_type().cloned();
                    LuaType::TplRef(Arc::new(GenericTpl::new(
                        tpl.get_tpl_id(),
                        SmolStr::from(tpl.get_name()),
                        constraint,
                        default,
                        tpl.is_const(),
                        None,
                    )))
                } else {
                    ty
                }
            }
            LuaType::Ref(ref id) if generic_params.iter().any(|g| g.name == id.get_name()) => {
                if let Some((index, param)) = generic_params
                    .iter()
                    .enumerate()
                    .find(|(_, g)| g.name == id.get_name())
                {
                    let constraint = param.constraint.map(|syntax| self.doc_type_lua(syntax));
                    LuaType::TplRef(Arc::new(GenericTpl::new(
                        GenericTplId::Type(index as u32),
                        SmolStr::from(id.get_name()),
                        constraint,
                        None,
                        false,
                        None,
                    )))
                } else {
                    ty
                }
            }
            _ => ty,
        }
    }

    /// `---@[lsp_optimization("delayed_definition")]`: no longer treat "uninitialized" as nil,
    /// instead take the union of types from all assignments after that declaration.
    fn infer_delayed_definition_type(&self, decl: &Decl) -> Option<LuaType> {
        let tree = self.syntax_tree()?;
        let chunk = tree.get_chunk_node();
        let mut types = Vec::new();
        for assign in chunk.descendants::<LuaAssignStat>() {
            let (vars, values) = assign.get_var_and_expr_list();
            for (var, value) in vars.into_iter().zip(values) {
                let emmylua_parser::LuaVarExpr::NameExpr(name_expr) = var else {
                    continue;
                };
                if name_expr.get_name_text().as_deref() != Some(decl.name.as_str()) {
                    continue;
                }
                let offset = name_expr.get_position();
                if offset <= decl.name_offset() {
                    continue;
                }
                if self.resolve_name(offset) != Some(decl.id.clone()) {
                    continue;
                }
                let ty = infer::vm::widen_const(&self.type_of_expr(value.get_syntax_id()));
                if !matches!(ty, LuaType::Unknown | LuaType::Any) && !types.contains(&ty) {
                    types.push(ty);
                }
            }
        }
        match types.len() {
            0 => None,
            1 => types.pop(),
            _ => Some(LuaType::from_vec(types)),
        }
    }

    /// Iteration variable types for `for k, v in pairs(x)`:
    /// taken from the return function slots of the iteration expression's callee (pairs/ipairs/next or custom `__pairs` members).
    fn infer_for_range_var(&self, decl: &Decl) -> Option<LuaType> {
        let owner = decl.owner_syntax?;
        let tree = self.syntax_tree()?;
        let node = owner.to_node_from_root(&tree.get_red_root())?;
        let stat = LuaForRangeStat::cast(node)?;
        let vars = stat.get_var_name_list().collect::<Vec<_>>();
        let index = vars
            .iter()
            .position(|var| var.get_name_text() == decl.name.as_str())?;
        let iter_expr = stat.get_expr_list().next()?;

        // Standard `pairs(x)` / `ipairs(x)` / `next(x)`.
        if let LuaExpr::CallExpr(call) = &iter_expr
            && let LuaExpr::NameExpr(name_expr) = call.get_prefix_expr()?
            && let Some(name) = name_expr.get_name_text()
            && matches!(name.as_str(), "pairs" | "ipairs" | "next")
        {
            let arg = call.get_args_list()?.get_args().next()?;
            let arg_ty = self.type_of_expr(arg.get_syntax_id());
            let member = if name.as_str() == "ipairs" {
                "__ipairs"
            } else {
                "__pairs"
            };
            // 1. Custom `__pairs` / `__ipairs`.
            if let Some(gen_ty) =
                self.member_type(&arg_ty, &LuaMemberKey::Name(SmolStr::new(member)))
                && let LuaType::DocFunction(generator) = gen_ty
            {
                if let Some(slot) = generator.get_ret().get_result_slot_type(index) {
                    return Some(slot);
                }
            }
            // 2. Standard global pairs/ipairs: bind K/V from the parameter structure, then take the return function slot.
            return self.infer_standard_iter_slot(call.get_prefix_expr()?.clone(), &arg_ty, index);
        }

        // Generic iterator functions: `for k in test()` / `for k in iter_fn`.
        // The iteration expression itself evaluates to an iterator function; take its return slots. The first slot (loop key) stops the loop when nil, so remove nil.
        let mut iter_ty = if let LuaExpr::CallExpr(call) = &iter_expr {
            infer::infer_call_with_bindings(self, call.get_syntax_id())
                .map(|(ty, _)| ty)
                .unwrap_or_else(|| self.type_of_expr(iter_expr.get_syntax_id()))
        } else {
            self.type_of_expr(iter_expr.get_syntax_id())
        };
        // Expand function aliases (`---@alias foo fun(...)` -> DocFunction).
        if let LuaType::Ref(id) | LuaType::Def(id) = &iter_ty
            && let Some(def) = member::type_def_of(self, id)
        {
            if let Some(target) = self.alias_target(&def) {
                iter_ty = target;
            } else if let Some(syntax) = def.call_overloads.first() {
                let overload = self
                    .q()
                    .doc_type_lua(def.file_id, *syntax, &def.generic_params);
                if !matches!(overload, LuaType::Unknown) {
                    iter_ty = overload;
                }
            }
        }
        // When a call returns multiple values (`spairs(t)` returns iterator function + table), take the DocFunction as the iterator function.
        if let LuaType::Variadic(variadic) = &iter_ty
            && let VariadicType::Multi(types) = variadic.as_ref()
            && let Some(doc) = types.iter().find_map(|ty| match ty {
                LuaType::DocFunction(fun) => Some(fun.as_ref().clone()),
                _ => None,
            })
        {
            iter_ty = LuaType::DocFunction(Arc::new(doc));
        }
        let LuaType::DocFunction(iter_fun) = iter_ty else {
            return None;
        };
        // If the iterator function comes from a call (`spairs(t)`), infer its internal generics from arguments (`table<K,V>` <- `table<string,integer>`).
        let ret = if let LuaExpr::CallExpr(call) = &iter_expr {
            let mut bindings = infer::unify::TplBindings::new();
            let arg_types: Vec<LuaType> = call
                .get_args_list()
                .map(|list| {
                    list.get_args()
                        .map(|arg| self.type_of_expr(arg.get_syntax_id()))
                        .collect()
                })
                .unwrap_or_default();
            for (param, arg_ty) in iter_fun.get_params().iter().zip(arg_types.iter()) {
                if let Some(param_ty) = &param.1 {
                    let _ = infer::unify::unify_bindings(param_ty, arg_ty, &mut bindings);
                }
            }
            infer::unify::substitute(iter_fun.get_ret(), &bindings)
        } else {
            iter_fun.get_ret().clone()
        };
        let slot = ret.get_result_slot_type(index)?;
        if index == 0 {
            Some(remove_nil_from_type(slot))
        } else {
            Some(slot)
        }
    }

    /// Standard `pairs(x)` / `ipairs(x)`: take the type from the inner slot of the global function signature's return function.
    fn infer_standard_iter_slot(
        &self,
        callee: LuaExpr,
        arg_ty: &LuaType,
        index: usize,
    ) -> Option<LuaType> {
        let LuaExpr::NameExpr(name_expr) = callee else {
            return None;
        };
        let name = name_expr.get_name_text()?;
        if !matches!(name.as_str(), "pairs" | "ipairs" | "next") {
            return None;
        }
        // Integer-index members of named types (`@field [integer] string` / `@field [1] string`)
        // do not need global pairs/ipairs signatures; infer directly from the type definition.
        if let LuaType::Ref(id) | LuaType::Def(id) = &arg_ty {
            if let Some(ty) = self.infer_iter_from_type_def(id, index) {
                return Some(ty);
            }
        }
        let callee_decl = self.resolve_name(name_expr.get_position())?;
        let decl_file = match &callee_decl {
            SemanticId::Decl(key) => key.file_id,
            _ => self.file_id,
        };
        let facts = self.file_facts_of(decl_file)?;
        let facts_decl = facts.decl_by_id(&callee_decl)?;
        let closure_syntax = facts_decl.value_expr_syntax?;
        let signature = facts.signature_by_closure(closure_syntax)?;
        let docs = signature.docs.as_ref()?;
        let generic_params = &docs.generic_params;
        let (key_id, value_id) = match generic_params.len() {
            n if n >= 2 => (GenericTplId::Type(0), GenericTplId::Type(1)),
            _ => return None,
        };
        // alias expansion (`MatchersObject` -> object).
        let mut iter_arg = arg_ty.clone();
        let mut visited = Vec::new();
        #[allow(clippy::while_let_loop)]
        loop {
            let (LuaType::Ref(id) | LuaType::Def(id)) = &iter_arg else {
                break;
            };
            if visited.contains(id) {
                break;
            }
            visited.push(id.clone());
            let Some(def) = member::type_def_of(self, id) else {
                break;
            };
            let mut target = self.alias_target(&def);
            if target
                .as_ref()
                .is_none_or(|ty| matches!(ty, LuaType::Table))
                && let Some(syntax) = def.alias_type
            {
                let rich = self.doc_type_lua_rich_in(def.file_id, syntax);
                if !matches!(rich, LuaType::Unknown) {
                    target = Some(rich);
                }
            }
            let Some(target) = target else {
                break;
            };
            iter_arg = target;
        }
        let (key_ty, value_ty) = match &iter_arg {
            LuaType::Any => (LuaType::Any, LuaType::Any),
            LuaType::TableConst(table) => {
                let owner = SemanticId::member(table.file_id, table.value);
                let mut keys: Vec<LuaType> = Vec::new();
                let mut values: Vec<LuaType> = Vec::new();
                for member_ref in self.members_of_owner(&owner) {
                    let Some(facts) = self.file_facts_of(member_ref.file_id) else {
                        continue;
                    };
                    let Some(member) = facts.member_by_id(&member_ref.id) else {
                        continue;
                    };
                    // Prefer the real type of a computed key from the syntax tree (`[severity.ERROR]` / `[key]`).
                    // Only try for Name keys: implicit integer keys' key_range hits the value expression and cannot be used as a key.
                    let computed_key = if matches!(member.key, LuaMemberKey::Name(_)) {
                        member.id.member_key_range().and_then(|key_range| {
                            let tree = self.syntax_tree_of(member_ref.file_id)?;
                            let chunk = tree.get_chunk_node();
                            chunk
                                .descendants::<LuaExpr>()
                                .find(|expr| expr.get_range() == key_range)
                                .map(|expr| self.type_of_expr(expr.get_syntax_id()))
                                .filter(|ty| !matches!(ty, LuaType::Unknown))
                        })
                    } else {
                        None
                    };
                    let key = match computed_key {
                        Some(ty) => ty,
                        None => match &member.key {
                            LuaMemberKey::Integer(i) => LuaType::IntegerConst(*i),
                            LuaMemberKey::Name(name) => {
                                LuaType::StringConst(SmolStr::new(name.as_str()).into())
                            }
                            _ => continue,
                        },
                    };
                    let value = if let Some(value_syntax) = member.value_syntax {
                        self.type_of_expr(value_syntax)
                    } else {
                        self.type_of_member(&member_ref.id)
                            .unwrap_or(LuaType::Unknown)
                    };
                    // Table-literal member values are displayed as doc constants (old chain semantics).
                    let value = match value {
                        LuaType::IntegerConst(i) => LuaType::DocIntegerConst(i),
                        LuaType::StringConst(s) => LuaType::DocStringConst(s.clone()),
                        other => other,
                    };
                    if !keys.contains(&key) {
                        keys.push(key);
                    }
                    if !values.contains(&value) {
                        values.push(value);
                    }
                }
                let key_ty = if keys.is_empty() {
                    LuaType::Unknown
                } else if keys.len() == 1 {
                    keys.pop()?
                } else {
                    LuaType::Union(Arc::new(LuaUnionType::from_vec(keys)))
                };
                let value_ty = if values.is_empty() {
                    LuaType::Unknown
                } else if values.len() == 1 {
                    values.pop()?
                } else {
                    LuaType::Union(Arc::new(LuaUnionType::from_vec(values)))
                };
                (key_ty, value_ty)
            }
            LuaType::Ref(id) | LuaType::Def(id) => {
                let def = member::type_def_of(self, id)?;
                let mut values: Vec<LuaType> = Vec::new();
                for member_ref in self.members_of_owner(&def.id) {
                    let Some(facts) = self.file_facts_of(member_ref.file_id) else {
                        continue;
                    };
                    let Some(member) = facts.member_by_id(&member_ref.id) else {
                        continue;
                    };
                    let is_integer_key = matches!(member.key, LuaMemberKey::Integer(_));
                    let is_index_sig = member.is_index_signature;
                    if !is_integer_key && !is_index_sig {
                        continue;
                    }
                    let value = self
                        .type_of_member(&member_ref.id)
                        .unwrap_or(LuaType::Unknown);
                    if !values.contains(&value) {
                        values.push(value);
                    }
                }
                if values.is_empty() {
                    return None;
                }
                let value_ty = if values.len() == 1 {
                    values.pop()?
                } else {
                    LuaType::Union(Arc::new(LuaUnionType::from_vec(values)))
                };
                (LuaType::Integer, value_ty)
            }
            LuaType::Object(object) => {
                let value = object
                    .get_fields()
                    .values()
                    .next()
                    .cloned()
                    .or_else(|| object.get_index_access().first().map(|(_, ty)| ty.clone()))
                    .unwrap_or(LuaType::Unknown);
                (LuaType::String, value)
            }
            LuaType::Array(array) => (LuaType::Integer, array.get_base().clone()),
            LuaType::Generic(generic) if generic.get_base_type_id().get_name() == "table" => {
                let params = generic.get_params();
                (
                    params.first().cloned().unwrap_or(LuaType::Unknown),
                    params.get(1).cloned().unwrap_or(LuaType::Unknown),
                )
            }
            _ => return None,
        };
        let mut bindings = infer::unify::TplBindings::new();
        bindings.insert(key_id, key_ty);
        bindings.insert(value_id, value_ty);
        // Resolves the inner return slots of `---@return fun(tbl:any):K, V`.
        let return_syntax = docs.returns.first()?;
        let tree = self.syntax_tree_of(decl_file)?;
        let node = return_syntax.to_node_from_root(&tree.get_red_root())?;
        let Some(LuaDocType::Func(func_doc)) = LuaDocType::cast(node) else {
            return None;
        };
        let return_list = func_doc.get_return_type_list()?;
        let mut slots = Vec::new();
        for ret in return_list.get_return_type_list() {
            if let (_, Some(ret_type)) = ret.get_name_and_type() {
                slots.push(self.q().doc_type_lua(
                    decl_file,
                    ret_type.get_syntax_id(),
                    &docs.generic_params,
                ));
            }
        }
        let slot = slots.get(index)?;
        let substituted = infer::unify::substitute(slot, &bindings);
        (!matches!(substituted, LuaType::Unknown)).then_some(substituted)
    }

    /// Infers `ipairs` slots directly from integer-index members of named types (does not depend on global pairs/ipairs signatures).
    fn infer_iter_from_type_def(&self, id: &LuaTypeDeclId, index: usize) -> Option<LuaType> {
        let def = member::type_def_of(self, id)?;
        let mut values: Vec<LuaType> = Vec::new();
        for member_ref in self.members_of_owner(&def.id) {
            let facts = self.file_facts_of(member_ref.file_id)?;
            let member = facts.member_by_id(&member_ref.id)?;
            let is_integer_key = matches!(member.key, LuaMemberKey::Integer(_));
            let is_index_sig = member.is_index_signature;
            if !is_integer_key && !is_index_sig {
                continue;
            }
            let value = self
                .type_of_member(&member_ref.id)
                .unwrap_or(LuaType::Unknown);
            if !values.contains(&value) {
                values.push(value);
            }
        }
        if values.is_empty() {
            return None;
        }
        let value_ty = if values.len() == 1 {
            values.pop()?
        } else {
            LuaType::Union(Arc::new(LuaUnionType::from_vec(values)))
        };
        Some(if index == 0 {
            LuaType::Integer
        } else {
            value_ty
        })
    }

    /// A member's declared type (keyed by declaring file, supports cross-file members; `@field` members carry the owner's generic context).
    pub fn type_of_member(&self, member: &SemanticId) -> Option<LuaType> {
        let cache_file = match member {
            SemanticId::Member(key) => key.file_id,
            _ => self.file_id,
        };
        if let Some(cached) = self
            .cache
            .borrow()
            .member_type
            .get(&(cache_file, member.clone()))
        {
            return cached.clone();
        }
        {
            let mut guard = self.decl_member_guard.borrow_mut();
            if guard.contains(member) {
                return None;
            }
            guard.push(member.clone());
        }
        let result = self.type_of_member_impl(member);
        self.decl_member_guard.borrow_mut().pop();
        self.cache
            .borrow_mut()
            .member_type
            .insert((cache_file, member.clone()), result.clone());
        result
    }

    pub(crate) fn type_of_member_impl(&self, member: &SemanticId) -> Option<LuaType> {
        // Members are keyed by declaring file: take the file from the Member key.
        let member_file = match member {
            SemanticId::Member(key) => key.file_id,
            _ => self.file_id,
        };
        // Cross-file members are uniformly delegated to the member file's own model, keeping VM replay and cycle guards on the same model.
        if member_file != self.file_id {
            if let Some(foreign) = self.model_for(member_file) {
                return foreign.type_of_member(member);
            }
        }
        // Literal integers in `---@enum` table fields stay constant (`severity.ERROR` -> IntegerConst(1)).
        if let Some(enum_const) = self.enum_member_const(member_file, member) {
            return Some(enum_const);
        }
        // Runtime member assignments prefer VM projection: the TypeShell path does not perform higher-order generic call inference,
        // and would keep `E.foo_wrapped = wrap(function(a) ... end)` as `fun(...: T...)`.
        // Cycles are handled by the Salsa tracked `semantic_member_type` / `semantic_expr_type` queries.
        if let Some(facts) = self.file_facts_of(member_file)
            && let Some(member_def) = facts.member_by_id(member)
            && !matches!(member_def.owner, SemanticId::TypeDef(_))
            && let Some(value_syntax) = member_def.value_syntax
        {
            // Table-literal fields keep the expression type from construction (`[key] = 1`, named fields
            // `foo = 123` keep `IntegerConst`; flow/type matching widens when `number` is needed).
            // Reentry is handled by the Salsa tracked `semantic_expr_type` / `semantic_member_type`.
            let vm_ty = (|| {
                let tree = self.syntax_tree_of(member_file)?;
                let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
                let expr = LuaExpr::cast(node)?;
                let eval = if self.is_initializer_table_field(member, &member_def) {
                    self.type_of_expr(expr.get_syntax_id())
                } else if matches!(expr, LuaExpr::CallExpr(_)) {
                    self.type_of_expr(value_syntax)
                } else {
                    return None;
                };
                (!matches!(eval, LuaType::Unknown)).then_some(eval)
            })();
            if let Some(ty) = vm_ty {
                return Some(ty);
            }
        }

        let shell = self.q().member_type(member_file, member.clone())?;
        let generic_names = self.member_generic_names(member_file, member);
        let ty = self
            .q()
            .type_shell_lua_in(member_file, &shell, &generic_names);
        let ty = type_eval::expand_alias_generic(self, &ty);
        let ty = type_eval::eval_conditionals(self, &ty);
        Some(ty)
    }

    /// Whether this is a table-literal field in a declaration initializer.
    /// Such members keep the literal shape from table construction; explicit `t.x = v` is not an initializer field.
    /// Evaluate a member's value expression with the same-member reentry guard.
    /// Member resolution can recursively need this expression's type; without sharing the
    /// SemanticModel cycle guard, `member_info -> type_of_expr -> new InferVm -> member_info`
    /// can restart and eventually overflow the native stack.
    fn is_initializer_table_field(&self, _member: &SemanticId, member_def: &Member) -> bool {
        let Some(value_syntax) = member_def.value_syntax else {
            return false;
        };
        let value_range = value_syntax.get_range();
        match &member_def.owner {
            SemanticId::Decl(decl_key) => {
                let Some(facts) = self.file_facts_of(decl_key.file_id) else {
                    return false;
                };
                let Some(decl) = facts.decl_by_id(&SemanticId::Decl(decl_key.clone())) else {
                    return false;
                };
                let Some(init_syntax) = decl.value_expr_syntax else {
                    return false;
                };
                let Some(tree) = self.syntax_tree_of(decl_key.file_id) else {
                    return false;
                };
                let Some(node) = init_syntax.to_node_from_root(&tree.get_red_root()) else {
                    return false;
                };
                LuaTableExpr::cast(node)
                    .is_some_and(|table| table.get_range().contains(value_range.start()))
            }
            SemanticId::Member(table_key) => {
                let Some(tree) = self.syntax_tree_of(table_key.file_id) else {
                    return false;
                };
                let root = tree.get_red_root();
                root.descendants()
                    .filter_map(LuaTableExpr::cast)
                    .any(|table| {
                        table.get_range() == table_key.key_range
                            && table.get_range().contains(value_range.start())
                    })
            }
            _ => false,
        }
    }

    /// If the member belongs to an `---@enum` table and its value is an integer literal, return the corresponding IntegerConst.
    fn enum_member_const(&self, member_file: FileId, member: &SemanticId) -> Option<LuaType> {
        let facts = self.file_facts_of(member_file)?;
        let member_def = facts.member_by_id(member)?;
        let SemanticId::Member(table_key) = &member_def.owner else {
            return None;
        };
        let table_range = table_key.key_range;
        let decl = facts.decls.iter().find(|decl| {
            decl.value_expr_syntax
                .map(|syntax| syntax.get_range())
                .is_some_and(|range| range == table_range)
        })?;
        let def = facts.type_defs.iter().find(|def| {
            def.owner_syntax.is_some()
                && def.owner_syntax == decl.owner_syntax
                && matches!(def.kind, TypeDefKind::Enum)
        })?;
        let _ = def;
        let value_syntax = member_def.value_syntax?;
        let tree = self.syntax_tree_of(member_file)?;
        let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
        let literal = LuaLiteralExpr::cast(node)?;
        match literal.get_literal()? {
            LuaLiteralToken::Number(number) => match number.get_number_value() {
                NumberResult::Int(i) => Some(LuaType::IntegerConst(i)),
                NumberResult::Uint(u) => Some(LuaType::IntegerConst(u as i64)),
                _ => None,
            },
            _ => None,
        }
    }

    /// Generic parameter names of the owner type for `@field` members; runtime members (owner = Decl) are associated
    /// with the type definition via the statement owned by the `---@class X<T>` comment to get X's generic parameter names.
    fn member_generic_names(&self, member_file: FileId, member: &SemanticId) -> Vec<SmolStr> {
        let Some(facts) = self.file_facts_of(member_file) else {
            return Vec::new();
        };
        let Some(member) = facts.member_by_id(member) else {
            return Vec::new();
        };
        let type_def_id = match &member.owner {
            SemanticId::TypeDef(type_def_id) => Some(SemanticId::TypeDef(type_def_id.clone())),
            SemanticId::Decl(_) => facts
                .decl_by_id(&member.owner)
                .and_then(|owner_decl| {
                    owner_decl
                        .owner_syntax
                        .and_then(|syntax| facts.type_def_by_owner_syntax(syntax))
                })
                .map(|def| def.id.clone()),
            _ => None,
        };
        let Some(type_def_id) = type_def_id else {
            return Vec::new();
        };
        let Some(def) = facts.type_def_by_id(&type_def_id) else {
            return Vec::new();
        };
        def.generic_params.iter().map(|g| g.name.clone()).collect()
    }

    /// All members of a prefix type (completion candidates; `@field` + inheritance + runtime values, with generic substitution).
    pub fn member_infos(&self, prefix_type: &LuaType) -> Vec<member::MemberInfo> {
        if let Some(cached) = self.cache.borrow().member_infos.get(prefix_type) {
            return cached.clone();
        }
        let value = member::member_infos(self, prefix_type);
        self.cache
            .borrow_mut()
            .member_infos
            .insert(prefix_type.clone(), value.clone());
        value
    }

    /// The specified-key member of a prefix type (first match).
    pub fn member_info(
        &self,
        prefix_type: &LuaType,
        key: &LuaMemberKey,
    ) -> Option<member::MemberInfo> {
        if let Some(value) = self
            .cache
            .borrow()
            .member_info
            .get(&(prefix_type.clone(), key.clone()))
        {
            return value.clone();
        }
        let value = member::member_info(self, prefix_type, key);
        self.cache
            .borrow_mut()
            .member_info
            .insert((prefix_type.clone(), key.clone()), value.clone());
        value
    }

    /// Replaces `TplRef` in a type with a set of generic arguments (for hover/display-layer call-site projection).
    pub fn substitute_generic_params(&self, ty: &LuaType, params: &[LuaType]) -> LuaType {
        let mut bindings = infer::unify::TplBindings::new();
        for (index, param) in params.iter().enumerate() {
            bindings.insert(GenericTplId::Type(index as u32), param.clone());
        }
        infer::unify::substitute(ty, &bindings)
    }

    /// Performs call-site projection by both class generic names (`Ref("T")`) and `TplRef`.
    pub fn substitute_generic_params_named(
        &self,
        ty: &LuaType,
        params: &[LuaType],
        names: &[SmolStr],
    ) -> LuaType {
        let mut bindings = infer::unify::TplBindings::new();
        for (index, param) in params.iter().enumerate() {
            bindings.insert(GenericTplId::Type(index as u32), param.clone());
        }
        let mut map = HashMap::new();
        for (name, param) in names.iter().zip(params.iter()) {
            map.insert(name.to_string(), param.clone());
        }
        let substituted = infer::unify::substitute(ty, &bindings);
        type_eval::substitute_named_refs(&substituted, &map)
    }

    /// String-argument version of `substitute_generic_params_named` (for callers that don't directly depend on smol_str).
    pub fn substitute_generic_params_named_str(
        &self,
        ty: &LuaType,
        params: &[LuaType],
        names: &[&str],
    ) -> LuaType {
        let names = names.iter().map(SmolStr::new).collect::<Vec<_>>();
        self.substitute_generic_params_named(ty, params, &names)
    }

    /// Callee function type at a call site (generic substitution inferred from arguments; member overloads are selected by arguments first).
    pub fn inferred_call_doc_function(&self, call_syntax: LuaSyntaxId) -> Option<LuaFunctionType> {
        let tree = self.syntax_tree()?;
        let node = call_syntax.to_node_from_root(&tree.get_red_root())?;
        let call = LuaCallExpr::cast(node)?;
        let callee = call.get_prefix_expr()?;

        // Member overloads: collect all `@field` candidates with `member_infos_with_key_all`, then select by arguments.
        if let LuaExpr::IndexExpr(index_expr) = &callee
            && let Some(prefix) = index_expr.get_prefix_expr()
            && let Some(resolved) = self.resolve_member(index_expr)
        {
            let prefix_ty = self.type_of_expr(prefix.get_syntax_id());
            let key = LuaMemberKey::Name(resolved.name.clone());
            let candidates = self
                .member_infos_with_key_all(&prefix_ty, &key)
                .into_iter()
                .filter_map(|info| match &info.typ {
                    LuaType::DocFunction(fun) => Some(fun.as_ref().clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if candidates.len() > 1 {
                let args = call
                    .get_args_list()
                    .map(|list| {
                        list.get_args()
                            .map(|arg| {
                                infer::overload::CallArg::new(
                                    self.type_of_expr(arg.get_syntax_id()),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let selected = infer::overload::select_callable(
                    self,
                    &candidates,
                    &args,
                    call.is_colon_call(),
                    Some(&prefix_ty),
                )
                .or_else(|| {
                    infer::overload::select_callable_partial(
                        self,
                        &candidates,
                        &args,
                        call.is_colon_call(),
                        Some(&prefix_ty),
                    )
                });
                if let Some((fun, bindings)) = selected {
                    let bindings = bindings
                        .into_iter()
                        .map(|(id, ty)| (id, Self::hover_widen_const(&ty)))
                        .collect::<HashMap<_, _>>();
                    let generic_params = fun.get_generic_params();
                    let names = generic_params
                        .iter()
                        .map(|param| SmolStr::new(param.get_name()))
                        .collect::<Vec<_>>();
                    let params = generic_params
                        .iter()
                        .enumerate()
                        .map(|(index, _)| {
                            bindings
                                .get(&GenericTplId::Type(index as u32))
                                .cloned()
                                .unwrap_or(LuaType::Unknown)
                        })
                        .collect::<Vec<_>>();
                    if let LuaType::DocFunction(substituted) = self.substitute_generic_params_named(
                        &LuaType::DocFunction(Arc::new(fun)),
                        &params,
                        &names,
                    ) {
                        return Some(substituted.as_ref().clone());
                    }
                }
            }
        }

        // When no member overload matches, fall back to the old single-callee inference (keeps the primary signature for no-match display).
        let (_, bindings) = infer::infer_call_with_bindings(self, call_syntax)?;
        let callee_ty = self.type_of_expr(callee.get_syntax_id());
        let fun = match callee_ty {
            LuaType::DocFunction(fun) => fun,
            _ => {
                if let LuaExpr::IndexExpr(index_expr) = &callee {
                    let resolved = self.resolve_member(index_expr)?;
                    let member_id = resolved.member_id?;
                    let member_file = match &member_id {
                        SemanticId::Member(key) => key.file_id,
                        _ => return None,
                    };
                    let member = self.file_facts_of(member_file)?.member_by_id(&member_id)?;
                    let value_syntax = member.value_syntax?;
                    Arc::new(self.type_of_signature_in_file(member_file, value_syntax)?)
                } else {
                    return None;
                }
            }
        };
        let bindings = bindings
            .into_iter()
            .map(|(id, ty)| (id, Self::hover_widen_const(&ty)))
            .collect::<HashMap<_, _>>();
        match infer::unify::substitute(&LuaType::DocFunction(fun), &bindings) {
            LuaType::DocFunction(f) => Some(f.as_ref().clone()),
            _ => None,
        }
    }

    /// Hover-display generic binding widening: literal arguments are shown as base types in call-site signatures
    /// (`false` -> `boolean`, `1` -> `integer`) to keep hover readable.
    fn hover_widen_const(ty: &LuaType) -> LuaType {
        match ty {
            LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_) => LuaType::Boolean,
            _ => infer::vm::widen_const(ty),
        }
    }

    /// Call-site generic bindings (for render/hover display layers to substitute overloads by `GenericTplId::Type(i)`).
    pub fn inferred_call_bindings(
        &self,
        call_syntax: LuaSyntaxId,
    ) -> Option<HashMap<GenericTplId, LuaType>> {
        let (_, bindings) = infer::infer_call_with_bindings(self, call_syntax)?;
        Some(
            bindings
                .into_iter()
                .map(|(id, ty)| (id, Self::hover_widen_const(&ty)))
                .collect(),
        )
    }

    /// The specified-key members of a prefix type (all matches, overload scenarios).
    pub fn member_infos_with_key(
        &self,
        prefix_type: &LuaType,
        key: &LuaMemberKey,
    ) -> Vec<member::MemberInfo> {
        member::member_infos_with_key(self, prefix_type, key)
    }

    /// The specified-key members of a prefix type (all matches, no dedup; keeps repeated `@field` lines as overloads).
    pub fn member_infos_with_key_all(
        &self,
        prefix_type: &LuaType,
        key: &LuaMemberKey,
    ) -> Vec<member::MemberInfo> {
        member::member_infos_with_key_all(self, prefix_type, key)
    }

    /// Member type for prefix type + key (old `infer_member_type`).
    pub fn member_type(&self, prefix_type: &LuaType, key: &LuaMemberKey) -> Option<LuaType> {
        member::member_type(self, prefix_type, key)
    }

    /// Whether a global name is deprecated in any workspace.
    pub(crate) fn is_global_deprecated(&self, name: &str) -> bool {
        self.q().is_global_deprecated(name)
    }

    pub(crate) fn is_deprecated_member_name(&self, name: &str) -> bool {
        self.q().is_deprecated_member_name(name)
    }

    /// Flow-sensitive type of a decl at offset (assignment-flow aware: last assignment's RHS type / declaration initial type,
    /// branching merges take unions).
    pub fn type_of_decl_at(&self, decl: &SemanticId, offset: TextSize) -> LuaType {
        self.type_of_decl_at_impl(decl, offset)
    }

    pub(crate) fn type_of_decl_at_impl(&self, decl: &SemanticId, offset: TextSize) -> LuaType {
        let (start, cached) = if let Some(tree) = self.flow_tree()
            && let Some(flow_id) = tree.get_flow_id_at(offset)
        {
            let start = flow::skip_own_decl_assign(decl, &tree, flow_id, offset);
            let cached = self
                .cache
                .borrow()
                .flow_decl
                .get(&(self.file_id, decl.clone(), start))
                .cloned();
            (Some(start), cached)
        } else {
            (None, None)
        };
        if let Some(cached) = cached {
            return self.sanitize_global_generic_decl(decl, cached);
        }
        let ty = flow::type_of_decl_at(self, decl, offset);
        if let Some(start) = start {
            self.cache
                .borrow_mut()
                .flow_decl
                .insert((self.file_id, decl.clone(), start), ty.clone());
        }
        self.sanitize_global_generic_decl(decl, ty)
    }

    /// Global variables must not leak uninstantiated generic parameters from function bodies into the global scope
    /// (the `a` in `function f(x) a = x end` is unknown externally).
    fn sanitize_global_generic_decl(&self, decl: &SemanticId, ty: LuaType) -> LuaType {
        let SemanticId::Decl(key) = decl else {
            return ty;
        };
        let Some(foreign_model) = SemanticModel::new(self.db, key.file_id) else {
            return ty;
        };
        let Some(decl) = foreign_model
            .file_facts_of(key.file_id)
            .and_then(|facts| facts.decl_by_id(decl))
        else {
            return ty;
        };
        if !matches!(decl.kind, DeclKind::Global) {
            return ty;
        }
        let generic_names: std::collections::HashSet<SmolStr> = foreign_model
            .signatures()
            .map(|sigs| {
                sigs.iter()
                    .filter_map(|sig| sig.docs.as_ref())
                    .flat_map(|docs| docs.generic_params.iter().map(|g| g.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        if matches!(&ty, LuaType::Ref(id) if generic_names.contains(id.get_name())) {
            LuaType::Unknown
        } else {
            ty
        }
    }

    /// Flow-sensitive type of a member at offset (member assignment flow awareness + `---@cast t.x +T` widening).
    pub fn type_of_member_at(&self, member: &SemanticId, offset: TextSize) -> LuaType {
        self.type_of_member_at_impl(member, offset)
    }

    pub(crate) fn type_of_member_at_impl(&self, member: &SemanticId, offset: TextSize) -> LuaType {
        let cache_file = match member {
            SemanticId::Member(key) => key.file_id,
            _ => self.file_id,
        };
        if let Some(cached) =
            self.cache
                .borrow()
                .member_type_at
                .get(&(cache_file, member.clone(), offset))
        {
            return cached.clone();
        }
        let ty = flow::type_of_member_at(self, member, offset);
        self.cache
            .borrow_mut()
            .member_type_at
            .insert((cache_file, member.clone(), offset), ty.clone());
        ty
    }

    /// Decl type before the flow node at offset (for assignment checks: this assignment does not participate in the target type).
    pub fn type_of_decl_before_at(&self, decl: &SemanticId, offset: TextSize) -> LuaType {
        self.type_of_decl_before_at_impl(decl, offset)
    }

    pub(crate) fn type_of_decl_before_at_impl(
        &self,
        decl: &SemanticId,
        offset: TextSize,
    ) -> LuaType {
        flow::type_of_decl_before_at(self, decl, offset)
    }

    /// Target type for assignment checks: applies `---@cast +T`, excludes this assignment, but does not apply conditional narrowing.
    pub fn type_of_decl_assign_target_at(&self, decl: &SemanticId, offset: TextSize) -> LuaType {
        self.type_of_decl_assign_target_at_impl(decl, offset)
    }

    pub(crate) fn type_of_decl_assign_target_at_impl(
        &self,
        decl: &SemanticId,
        offset: TextSize,
    ) -> LuaType {
        flow::type_of_decl_assign_target_at(self, decl, offset)
    }

    /// Member type before the flow node at offset (for assignment checks: this member assignment does not participate in the target type).
    pub fn type_of_member_before_at(&self, member: &SemanticId, offset: TextSize) -> LuaType {
        self.type_of_member_before_at_impl(member, offset)
    }

    pub(crate) fn type_of_member_before_at_impl(
        &self,
        member: &SemanticId,
        offset: TextSize,
    ) -> LuaType {
        flow::type_of_member_before_at(self, member, offset)
    }

    /// Flow-sensitive type of an expression at offset (NameExpr / IndexExpr use flow backtracking; others use ordinary inference).
    pub fn type_of_expr_at(&self, expr_syntax: LuaSyntaxId, offset: TextSize) -> LuaType {
        self.type_of_expr_at_impl(expr_syntax, offset)
    }

    pub(crate) fn type_of_expr_at_impl(
        &self,
        expr_syntax: LuaSyntaxId,
        offset: TextSize,
    ) -> LuaType {
        let file_id = self.file_id;
        if let Some(cached) = self
            .cache
            .borrow()
            .expr_type_at
            .get(&(file_id, expr_syntax, offset))
        {
            return cached.clone();
        }
        let ty = flow::type_of_expr_at(self, expr_syntax, offset);
        self.cache
            .borrow_mut()
            .expr_type_at
            .insert((file_id, expr_syntax, offset), ty.clone());
        ty
    }

    /// Operator-overload return type: when operand is a named type with `---@operator`, look up the return type by operator name.
    pub fn operator_type(&self, op_name: &str, operand: &LuaType) -> Option<LuaType> {
        let decl_id = match operand {
            LuaType::Ref(id) | LuaType::Def(id) => id,
            _ => return None,
        };
        let def = member::type_def_of(self, decl_id)?;
        // Alias forwarding: `AliasType` -> `Origin`, where the operator is defined on Origin.
        if def.kind == TypeDefKind::Alias
            && let Some(target) = self.alias_target(&def)
        {
            return self.operator_type(op_name, &target);
        }
        let facts = self.file_facts_of(def.file_id)?;
        let op = facts.operator_of(&def.id, op_name)?;
        let returns = self
            .q()
            .doc_type_lua(def.file_id, op.returns, &def.generic_params);
        (!matches!(returns, LuaType::Unknown)).then_some(returns)
    }

    /// All type definitions for a scope + full name (cross-file; used by checkers / inheritance chains).
    pub fn type_defs_in_scope(
        &self,
        scope: TypeScope,
        full_name: &str,
    ) -> crate::salsa_builder::TypeDefList {
        self.q().type_defs_in_scope(scope, full_name)
    }

    /// Alias target type (after projection; generic parameter references keep `TplRef`, and instantiation is substituted by the caller).
    pub fn alias_target(&self, def: &TypeDef) -> Option<LuaType> {
        if let Some(cached) = self.cache.borrow().alias_targets.get(&def.id) {
            return cached.clone();
        }
        let result = self.alias_target_uncached(def);
        self.cache
            .borrow_mut()
            .alias_targets
            .insert(def.id.clone(), result.clone());
        result
    }

    fn alias_target_uncached(&self, def: &TypeDef) -> Option<LuaType> {
        let syntax = def.alias_type?;
        let mut ty = self
            .q()
            .doc_type_lua(def.file_id, syntax, &def.generic_params);
        if matches!(ty, LuaType::Table | LuaType::Unknown) {
            let rich = self.doc_type_lua_rich_in(def.file_id, syntax);
            if !matches!(rich, LuaType::Unknown) {
                ty = rich;
            }
        }
        // When the shell layer cannot lower generic `keyof T`, rich projection drops the mapped constraint to None,
        // making it impossible to expand after instantiation. Here we restore the generic keyof constraint on alias targets from the AST;
        // normal doc/completion paths do not go through this restoration, so `keyof T` constraints still display as Unknown there.
        if let LuaType::Mapped(mapped) = &ty {
            if mapped.param.1.constraint.is_none()
                && let Some(tree) = self.syntax_tree_of(def.file_id)
                && let Some(node) = syntax.to_node_from_root(&tree.get_red_root())
                && let Some(doc_ty) = LuaDocType::cast(node)
                && let LuaDocType::Mapped(mapped_doc) = doc_ty
                && let Some(key) = mapped_doc.get_key()
                && let Some(decl) = key
                    .syntax()
                    .children()
                    .find_map(emmylua_parser::LuaDocGenericDecl::cast)
                && let Some(constr) = decl.get_constraint_type()
                && let LuaDocType::Unary(unary) = constr
                && unary
                    .get_op_token()
                    .is_some_and(|op| op.get_op() == LuaTypeUnaryOperator::Keyof)
                && let Some(target) = unary.get_type()
                && let LuaDocType::Name(name_ty) = target
                && let Some(name) = name_ty.get_name_text()
            {
                let key_ty = LuaType::Ref(LuaTypeDeclId::global(&name));
                let call = LuaType::Call(Arc::new(LuaAliasCallType::new(
                    LuaAliasCallKind::KeyOf,
                    vec![key_ty],
                )));
                let param = (
                    mapped.param.0,
                    GenericParam::new(
                        mapped.param.1.name.clone(),
                        Some(call),
                        mapped.param.1.default.clone(),
                        mapped.param.1.is_const,
                        mapped.param.1.attributes.clone(),
                    ),
                );
                ty = LuaType::Mapped(Arc::new(crate::LuaMappedType::new(
                    param,
                    mapped.value.clone(),
                    mapped.is_readonly,
                    mapped.is_optional,
                )));
            }
        }
        (!matches!(ty, LuaType::Unknown)).then_some(ty)
    }

    /// Attempts to concretely evaluate `T[K]` (K is a literal / union / keyof / alias).
    /// Unified delegation to the semantic-layer TypeEvaluator.
    fn try_eval_index_access(&self, base: &LuaType, key: &LuaType) -> Option<LuaType> {
        type_eval::eval_index_access(self, base, key)
    }

    /// Doc type node (by syntax location, in this file) -> projected `LuaType` (consumed by checkers such as cast).
    pub fn doc_type_lua(&self, type_syntax: LuaSyntaxId) -> LuaType {
        self.doc_type_lua_in(self.file_id, type_syntax, &[])
    }

    /// Doc type projection for a specified file + generic context (unified entry point).
    pub fn doc_type_lua_in(
        &self,
        file_id: FileId,
        type_syntax: LuaSyntaxId,
        generics: &[SalsaGenericParam],
    ) -> LuaType {
        self.q().doc_type_lua(file_id, type_syntax, generics)
    }

    /// Builds `GenericTpl` with full metadata (constraint/default/is_const) from `SalsaGenericParam`.
    /// All signature projection paths go through here so constraints/defaults are not lost at different call sites.
    pub fn generic_tpls_with_metadata(
        &self,
        file_id: FileId,
        params: &[SalsaGenericParam],
    ) -> Vec<GenericTpl> {
        params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let constraint = param.constraint.map(|syntax| {
                    let ty = self.q().doc_type_lua(file_id, syntax, params);
                    if matches!(ty, LuaType::Unknown | LuaType::Table) {
                        let rich = self.doc_type_lua_rich_in(file_id, syntax);
                        if !matches!(rich, LuaType::Unknown) {
                            rich
                        } else {
                            ty
                        }
                    } else {
                        ty
                    }
                });
                let default = param
                    .default
                    .map(|syntax| self.doc_type_lua_rich_in(file_id, syntax));
                GenericTpl::new(
                    GenericTplId::Type(index as u32),
                    param.name.clone(),
                    constraint,
                    default,
                    param.is_const,
                    None,
                )
            })
            .collect()
    }

    /// When projection fails, supplement object / intersection / union structures from the AST
    /// (the `TypeShell` layer does not yet support `{ y: integer } & { z: string }`).
    pub fn doc_type_lua_rich(&self, type_syntax: LuaSyntaxId) -> LuaType {
        self.doc_type_lua_rich_in(self.file_id, type_syntax)
    }

    /// Rich projection for any file (used by cross-file signature doc parameters).
    pub fn doc_type_lua_rich_in(&self, file_id: FileId, type_syntax: LuaSyntaxId) -> LuaType {
        let Some(tree) = self.syntax_tree_of(file_id) else {
            return self.q().doc_type_lua(file_id, type_syntax, &[]);
        };
        let Some(node) = type_syntax.to_node_from_root(&tree.get_red_root()) else {
            return self.q().doc_type_lua(file_id, type_syntax, &[]);
        };
        let Some(doc_ty) = LuaDocType::cast(node) else {
            return self.q().doc_type_lua(file_id, type_syntax, &[]);
        };
        match doc_ty {
            LuaDocType::Func(_) => {
                if let Some(fun) = infer::infer_doc_func(self, file_id, type_syntax) {
                    LuaType::DocFunction(Arc::new(fun))
                } else {
                    self.q().doc_type_lua(file_id, type_syntax, &[])
                }
            }
            LuaDocType::Conditional(conditional) => {
                self.doc_type_lua_rich_conditional(file_id, &conditional)
            }
            LuaDocType::Mapped(mapped) => {
                let mut param_name = String::new();
                let mut key_ty = LuaType::Unknown;
                if let Some(key) = mapped.get_key() {
                    if let Some(decl) = key
                        .syntax()
                        .children()
                        .find_map(emmylua_parser::LuaDocGenericDecl::cast)
                    {
                        if let Some(token) = decl.get_name_token() {
                            param_name = token.get_name_text().to_string();
                        }
                        if let Some(constraint) = decl.get_constraint_type() {
                            key_ty = self.doc_type_lua_rich_in(file_id, constraint.get_syntax_id());
                        }
                    }
                }
                let value_ty = mapped
                    .get_value_type()
                    .map(|ty| match ty {
                        LuaDocType::IndexAccess(index) => {
                            let mut types = index.syntax().children().filter_map(LuaDocType::cast);
                            let base = types
                                .next()
                                .map(|t| self.doc_type_lua_rich_in(file_id, t.get_syntax_id()))
                                .unwrap_or(LuaType::Unknown);
                            let key = types
                                .next()
                                .map(|t| self.doc_type_lua_rich_in(file_id, t.get_syntax_id()))
                                .unwrap_or(LuaType::Unknown);
                            LuaType::Call(Arc::new(LuaAliasCallType::new(
                                LuaAliasCallKind::Index,
                                vec![base, key],
                            )))
                        }
                        _ => self.doc_type_lua_rich_in(file_id, ty.get_syntax_id()),
                    })
                    .unwrap_or(LuaType::Unknown);
                if param_name.is_empty() {
                    return LuaType::Unknown;
                }
                let param = (
                    GenericTplId::Type(0),
                    GenericParam::new(
                        SmolStr::new(param_name),
                        if matches!(key_ty, LuaType::Unknown) {
                            None
                        } else {
                            Some(key_ty)
                        },
                        None,
                        false,
                        None,
                    ),
                );
                LuaType::Mapped(Arc::new(crate::LuaMappedType::new(
                    param,
                    value_ty,
                    mapped.is_readonly(),
                    mapped.is_optional(),
                )))
            }
            LuaDocType::Object(object) => {
                let mut fields = hashbrown::HashMap::new();
                let mut index_access = Vec::new();
                for field in object.get_fields() {
                    let Some(key) = field.get_field_key() else {
                        continue;
                    };
                    let Some(value_ty) = field.get_type() else {
                        continue;
                    };
                    match key {
                        LuaDocObjectFieldKey::Name(name) => {
                            fields.insert(
                                LuaMemberKey::Name(SmolStr::new(name.get_name_text())),
                                self.doc_type_lua_rich_in(file_id, value_ty.get_syntax_id()),
                            );
                        }
                        LuaDocObjectFieldKey::String(str) => {
                            fields.insert(
                                LuaMemberKey::Name(SmolStr::new(str.get_value())),
                                self.doc_type_lua_rich_in(file_id, value_ty.get_syntax_id()),
                            );
                        }
                        LuaDocObjectFieldKey::Integer(num) => {
                            if let NumberResult::Int(i) = num.get_number_value() {
                                fields.insert(
                                    LuaMemberKey::Integer(i),
                                    self.doc_type_lua_rich_in(file_id, value_ty.get_syntax_id()),
                                );
                            }
                        }
                        LuaDocObjectFieldKey::Type(key_ty) => {
                            index_access.push((
                                self.doc_type_lua_rich_in(file_id, key_ty.get_syntax_id()),
                                self.doc_type_lua_rich_in(file_id, value_ty.get_syntax_id()),
                            ));
                        }
                    };
                }
                LuaType::Object(Arc::new(LuaObjectType::new_with_fields(
                    fields,
                    index_access,
                )))
            }
            LuaDocType::IndexAccess(index) => {
                let mut types = index.syntax().children().filter_map(LuaDocType::cast);
                let base = types
                    .next()
                    .map(|ty| self.doc_type_lua_rich_in(file_id, ty.get_syntax_id()))
                    .unwrap_or(LuaType::Unknown);
                let key = types
                    .next()
                    .map(|ty| self.doc_type_lua_rich_in(file_id, ty.get_syntax_id()))
                    .unwrap_or(LuaType::Unknown);
                if let Some(evaluated) = self.try_eval_index_access(&base, &key) {
                    return evaluated;
                }
                LuaType::Call(Arc::new(LuaAliasCallType::new(
                    LuaAliasCallKind::Index,
                    vec![base, key],
                )))
            }
            LuaDocType::Binary(binary) => {
                let Some((left, right)) = binary.get_types() else {
                    return self.q().doc_type_lua(file_id, type_syntax, &[]);
                };
                let left_ty = self.doc_type_lua_rich_in(file_id, left.get_syntax_id());
                let right_ty = self.doc_type_lua_rich_in(file_id, right.get_syntax_id());
                match binary.get_op_token().map(|op| op.get_op()) {
                    Some(LuaTypeBinaryOperator::Intersection) => {
                        LuaType::Intersection(Arc::new(LuaIntersectionType::new(vec![
                            left_ty, right_ty,
                        ])))
                    }
                    Some(LuaTypeBinaryOperator::Union) => {
                        let mut types = Vec::new();
                        for ty in [left_ty, right_ty] {
                            match ty {
                                LuaType::Union(union) => types.extend(union.into_vec()),
                                other => types.push(other),
                            }
                        }
                        LuaType::Union(Arc::new(LuaUnionType::from_vec(types)))
                    }
                    _ => self.q().doc_type_lua(file_id, type_syntax, &[]),
                }
            }
            LuaDocType::Literal(literal) => match literal.get_literal() {
                Some(LuaLiteralToken::String(str_token)) => {
                    LuaType::StringConst(SmolStr::new(str_token.get_value()).into())
                }
                Some(LuaLiteralToken::Number(number_token)) => {
                    match number_token.get_number_value() {
                        NumberResult::Int(i) => LuaType::IntegerConst(i),
                        _ => LuaType::Number,
                    }
                }
                Some(LuaLiteralToken::Bool(bool_token)) => {
                    LuaType::BooleanConst(bool_token.is_true())
                }
                Some(LuaLiteralToken::Nil(_)) => LuaType::Nil,
                _ => self.q().doc_type_lua(file_id, type_syntax, &[]),
            },
            LuaDocType::Unary(unary) => {
                if !unary
                    .get_op_token()
                    .is_some_and(|op| op.get_op() == LuaTypeUnaryOperator::Keyof)
                {
                    return self.q().doc_type_lua(file_id, type_syntax, &[]);
                }
                let Some(target) = unary.get_type() else {
                    return self.q().doc_type_lua(file_id, type_syntax, &[]);
                };
                let LuaDocType::Name(name_ty) = &target else {
                    return self.q().doc_type_lua(file_id, type_syntax, &[]);
                };
                let Some(name) = name_ty.get_name_text() else {
                    return self.q().doc_type_lua(file_id, type_syntax, &[]);
                };
                let Some(def) = self.resolve_type_def_in(file_id, &name) else {
                    return self.q().doc_type_lua(file_id, type_syntax, &[]);
                };
                let members: Vec<LuaType> = self
                    .members_of_owner(&def.id)
                    .into_iter()
                    .map(|member| LuaType::StringConst(SmolStr::new(member.name.as_str()).into()))
                    .collect();
                if members.is_empty() {
                    self.q().doc_type_lua(file_id, type_syntax, &[])
                } else {
                    LuaType::Union(Arc::new(LuaUnionType::from_vec(members)))
                }
            }
            LuaDocType::MultiLineUnion(multi) => {
                let mut types = Vec::new();
                for field in multi.get_fields() {
                    let Some(ty) = field.get_type() else {
                        continue;
                    };
                    let ty = self.doc_type_lua_rich_in(file_id, ty.get_syntax_id());
                    if matches!(ty, LuaType::Unknown) {
                        continue;
                    }
                    match ty {
                        LuaType::Union(union) => types.extend(union.into_vec()),
                        other => types.push(other),
                    }
                }
                if types.is_empty() {
                    self.q().doc_type_lua(file_id, type_syntax, &[])
                } else {
                    LuaType::Union(Arc::new(LuaUnionType::from_vec(types)))
                }
            }
            LuaDocType::Variadic(variadic) => variadic
                .get_type()
                .map(|inner| {
                    let base = self.doc_type_lua_rich_in(file_id, inner.get_syntax_id());
                    // Rich projection without generic context projects the T in `T...` as `Ref("T")`,
                    // leaving it to the shell layer to handle the original context so the variadic base type keeps TplRef.
                    if matches!(base, LuaType::Unknown | LuaType::Ref(_) | LuaType::Def(_)) {
                        self.q().doc_type_lua(file_id, type_syntax, &[])
                    } else {
                        LuaType::Variadic(VariadicType::Base(base).into())
                    }
                })
                .unwrap_or_else(|| self.q().doc_type_lua(file_id, type_syntax, &[])),
            LuaDocType::Generic(generic) => {
                let Some(name) = generic.get_name_type().and_then(|n| n.get_name_text()) else {
                    return self.q().doc_type_lua(file_id, type_syntax, &[]);
                };
                let base_id = self.q().resolve_named_id(file_id, &name);
                let params: Vec<LuaType> = generic
                    .get_generic_types()
                    .map(|list| {
                        list.get_types()
                            .map(|arg| self.doc_type_lua_rich_in(file_id, arg.get_syntax_id()))
                            .collect()
                    })
                    .unwrap_or_default();
                LuaType::Generic(Arc::new(crate::LuaGenericType::new(base_id, params)))
            }
            _ => self.q().doc_type_lua(file_id, type_syntax, &[]),
        }
    }

    /// Projects `---@alias X<T> T extends Pattern and True or False` to `LuaType::Conditional`.
    /// `infer` names have an independent scope: declared in the condition phase, referenceable in the true branch, and returning to the outer scope in the false branch.
    fn doc_type_lua_rich_conditional(
        &self,
        file_id: FileId,
        conditional: &LuaDocConditionalType,
    ) -> LuaType {
        let mut state = ConditionalInferState::default();
        self.doc_type_lua_rich_conditional_in_state(file_id, conditional, &mut state)
    }

    /// Common implementation of conditional type projection. `state` can come from an outer conditional, giving nested `infer`
    /// globally unique IDs, and inner conditionals can see `infer` references bound by outer conditionals.
    fn doc_type_lua_rich_conditional_in_state(
        &self,
        file_id: FileId,
        conditional: &LuaDocConditionalType,
        state: &mut ConditionalInferState,
    ) -> LuaType {
        let Some((condition, when_true, when_false)) = conditional.get_types() else {
            return LuaType::Unknown;
        };
        state.enter_scope();
        let condition_ty = self.doc_type_lua_rich_scoped(file_id, condition.get_syntax_id(), state);
        let LuaType::Call(alias_call) = condition_ty else {
            state.leave_scope();
            return LuaType::Unknown;
        };
        if alias_call.get_call_kind() != LuaAliasCallKind::Extends
            || alias_call.get_operands().len() != 2
        {
            state.leave_scope();
            return LuaType::Unknown;
        }
        let operands = alias_call.get_operands();
        let checked_type = operands[0].clone();
        let extends_type = operands[1].clone();

        state.set_refs_visible(true);
        let true_type = self.doc_type_lua_rich_scoped(file_id, when_true.get_syntax_id(), state);
        let infer_params = state.leave_scope();
        let false_type = self.doc_type_lua_rich_scoped(file_id, when_false.get_syntax_id(), state);

        LuaType::Conditional(Arc::new(crate::LuaConditionalType::new(
            checked_type,
            extends_type,
            true_type,
            false_type,
            infer_params,
            conditional.has_new().unwrap_or(false),
        )))
    }

    /// Inner-scope rich projection for conditional types: handles only `infer`-related structures; other types use ordinary rich projection.
    fn doc_type_lua_rich_scoped(
        &self,
        file_id: FileId,
        type_syntax: LuaSyntaxId,
        state: &mut ConditionalInferState,
    ) -> LuaType {
        let Some(tree) = self.syntax_tree_of(file_id) else {
            return LuaType::Unknown;
        };
        let Some(node) = type_syntax.to_node_from_root(&tree.get_red_root()) else {
            return LuaType::Unknown;
        };
        let Some(doc_ty) = LuaDocType::cast(node) else {
            return LuaType::Unknown;
        };
        match doc_ty {
            LuaDocType::Infer(infer) => {
                let Some(name) = infer.get_generic_decl_name_text() else {
                    return LuaType::Unknown;
                };
                match state.declare(&name) {
                    Some(tpl) => LuaType::TplRef(Arc::new(tpl)),
                    None => LuaType::Unknown,
                }
            }
            LuaDocType::Name(name) => {
                if let Some(name) = name.get_name_text() {
                    if let Some(tpl) = state.find_ref(&name) {
                        return LuaType::TplRef(Arc::new(tpl));
                    }
                }
                self.doc_type_lua_rich_in(file_id, type_syntax)
            }
            LuaDocType::Binary(binary) => {
                let Some((left, right)) = binary.get_types() else {
                    return self.q().doc_type_lua(file_id, type_syntax, &[]);
                };
                let left_ty = self.doc_type_lua_rich_scoped(file_id, left.get_syntax_id(), state);
                let right_ty = self.doc_type_lua_rich_scoped(file_id, right.get_syntax_id(), state);
                match binary.get_op_token().map(|op| op.get_op()) {
                    Some(LuaTypeBinaryOperator::Union) => {
                        let mut types = Vec::new();
                        for ty in [left_ty, right_ty] {
                            match ty {
                                LuaType::Union(union) => types.extend(union.into_vec()),
                                other => types.push(other),
                            }
                        }
                        LuaType::Union(Arc::new(LuaUnionType::from_vec(types)))
                    }
                    Some(LuaTypeBinaryOperator::Intersection) => {
                        LuaType::Intersection(Arc::new(LuaIntersectionType::new(vec![
                            left_ty, right_ty,
                        ])))
                    }
                    Some(LuaTypeBinaryOperator::Extends) => LuaType::Call(Arc::new(
                        LuaAliasCallType::new(LuaAliasCallKind::Extends, vec![left_ty, right_ty]),
                    )),
                    _ => self.q().doc_type_lua(file_id, type_syntax, &[]),
                }
            }
            LuaDocType::Object(object) => {
                let mut fields = hashbrown::HashMap::new();
                let mut index_access = Vec::new();
                for field in object.get_fields() {
                    let Some(key) = field.get_field_key() else {
                        continue;
                    };
                    let Some(value_ty) = field.get_type() else {
                        continue;
                    };
                    match key {
                        LuaDocObjectFieldKey::Name(name) => {
                            fields.insert(
                                LuaMemberKey::Name(SmolStr::new(name.get_name_text())),
                                self.doc_type_lua_rich_scoped(
                                    file_id,
                                    value_ty.get_syntax_id(),
                                    state,
                                ),
                            );
                        }
                        LuaDocObjectFieldKey::String(str) => {
                            fields.insert(
                                LuaMemberKey::Name(SmolStr::new(str.get_value())),
                                self.doc_type_lua_rich_scoped(
                                    file_id,
                                    value_ty.get_syntax_id(),
                                    state,
                                ),
                            );
                        }
                        LuaDocObjectFieldKey::Integer(num) => {
                            if let NumberResult::Int(i) = num.get_number_value() {
                                fields.insert(
                                    LuaMemberKey::Integer(i),
                                    self.doc_type_lua_rich_scoped(
                                        file_id,
                                        value_ty.get_syntax_id(),
                                        state,
                                    ),
                                );
                            }
                        }
                        LuaDocObjectFieldKey::Type(key_ty) => {
                            index_access.push((
                                self.doc_type_lua_rich_scoped(
                                    file_id,
                                    key_ty.get_syntax_id(),
                                    state,
                                ),
                                self.doc_type_lua_rich_scoped(
                                    file_id,
                                    value_ty.get_syntax_id(),
                                    state,
                                ),
                            ));
                        }
                    };
                }
                LuaType::Object(Arc::new(LuaObjectType::new_with_fields(
                    fields,
                    index_access,
                )))
            }
            LuaDocType::Func(func) => {
                let mut params = Vec::new();
                let mut is_variadic = false;
                for param in func.get_params() {
                    if param.is_dots() {
                        is_variadic = true;
                    }
                    let name = param
                        .get_name_token()
                        .map(|token| token.get_name_text().to_string())
                        .unwrap_or_else(|| {
                            if param.is_dots() {
                                "...".to_string()
                            } else {
                                String::new()
                            }
                        });
                    let mut ty = param
                        .get_type()
                        .map(|t| self.doc_type_lua_rich_scoped(file_id, t.get_syntax_id(), state))
                        .unwrap_or(LuaType::Unknown);
                    if param.is_nullable() && !ty.is_nullable() {
                        ty = LuaType::Union(Arc::new(LuaUnionType::from_vec(vec![
                            ty,
                            LuaType::Nil,
                        ])));
                    }
                    params.push((name, Some(ty)));
                }
                let ret = match func.get_return_type_list() {
                    Some(list) => {
                        let mut types = Vec::new();
                        for ret in list.get_return_type_list() {
                            if let (_, Some(ret_type)) = ret.get_name_and_type() {
                                types.push(self.doc_type_lua_rich_scoped(
                                    file_id,
                                    ret_type.get_syntax_id(),
                                    state,
                                ));
                            }
                        }
                        if types.is_empty() {
                            LuaType::Unknown
                        } else if types.len() == 1 {
                            types.pop().unwrap_or(LuaType::Unknown)
                        } else {
                            LuaType::Variadic(Arc::new(VariadicType::Multi(types)))
                        }
                    }
                    None => LuaType::Unknown,
                };
                LuaType::DocFunction(Arc::new(LuaFunctionType::new(
                    AsyncState::None,
                    false,
                    is_variadic,
                    params,
                    ret,
                    None,
                )))
            }
            LuaDocType::Variadic(variadic) => variadic
                .get_type()
                .map(|inner| {
                    let base = self.doc_type_lua_rich_scoped(file_id, inner.get_syntax_id(), state);
                    if matches!(base, LuaType::Unknown | LuaType::Ref(_) | LuaType::Def(_)) {
                        self.q().doc_type_lua(file_id, type_syntax, &[])
                    } else {
                        LuaType::Variadic(VariadicType::Base(base).into())
                    }
                })
                .unwrap_or_else(|| self.q().doc_type_lua(file_id, type_syntax, &[])),
            LuaDocType::Generic(generic) => {
                let Some(name) = generic.get_name_type().and_then(|n| n.get_name_text()) else {
                    return self.q().doc_type_lua(file_id, type_syntax, &[]);
                };
                let base_id = self.q().resolve_named_id(file_id, &name);
                let params: Vec<LuaType> = generic
                    .get_generic_types()
                    .map(|list| {
                        list.get_types()
                            .map(|arg| {
                                self.doc_type_lua_rich_scoped(file_id, arg.get_syntax_id(), state)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                LuaType::Generic(Arc::new(crate::LuaGenericType::new(base_id, params)))
            }
            LuaDocType::Array(array) => array
                .get_type()
                .map(|base| {
                    LuaType::Array(Arc::new(crate::LuaArrayType::from_base_type(
                        self.doc_type_lua_rich_scoped(file_id, base.get_syntax_id(), state),
                    )))
                })
                .unwrap_or_else(|| self.q().doc_type_lua(file_id, type_syntax, &[])),
            LuaDocType::Tuple(tuple) => {
                let types: Vec<LuaType> = tuple
                    .get_types()
                    .map(|item| self.doc_type_lua_rich_scoped(file_id, item.get_syntax_id(), state))
                    .collect();
                LuaType::Tuple(Arc::new(LuaTupleType::new(
                    types,
                    LuaTupleStatus::DocResolve,
                )))
            }
            LuaDocType::Conditional(conditional) => {
                self.doc_type_lua_rich_conditional_in_state(file_id, &conditional, state)
            }
            _ => self.doc_type_lua_rich_in(file_id, type_syntax),
        }
    }

    /// Members whose owner is a `SemanticId` (cross-file).
    pub fn members_of_owner(&self, owner: &SemanticId) -> crate::salsa_builder::MemberList {
        self.q().members_of_owner(owner.clone())
    }

    /// Members of an owner with a specific name.
    pub fn members_of_owner_named(
        &self,
        owner: &SemanticId,
        name: &str,
    ) -> crate::salsa_builder::MemberList {
        self.q()
            .members_of_owner_named(owner.clone(), SmolStr::new(name))
    }

    /// Constructor attributes for a type definition (`---@[constructor("init")]` from the `meta("Class")` factory).
    pub fn constructor_attribute_of_type(
        &self,
        type_def: &SemanticId,
    ) -> Option<ConstructorAttribute> {
        self.q().constructor_attribute_of_type(type_def.clone())
    }

    /// A function's return type (doc annotation takes priority; otherwise scan function body returns).
    pub fn return_type(&self, closure_syntax: LuaSyntaxId) -> Option<LuaType> {
        let shell = self.q().signature_return(self.file_id, closure_syntax)?;
        let generic_names = self.signature_generic_names(closure_syntax);
        let mut ty = self
            .q()
            .type_shell_lua_in(self.file_id, &shell, &generic_names);
        // `self` in method annotations is the receiver instance; concretize it by the owner type before return checks.
        if let Some(owner_ty) = self.method_owner_type(closure_syntax) {
            ty = infer::vm::replace_self_type(&ty, &owner_ty);
        }
        ty = type_eval::expand_alias_generic(self, &ty);
        (!matches!(ty, LuaType::Unknown)).then_some(ty)
    }

    /// Type of the `param_index`-th function parameter (`---@param` annotation + member field signature fallback).
    pub fn param_type(&self, closure_syntax: LuaSyntaxId, param_index: usize) -> Option<LuaType> {
        let shell = self
            .q()
            .param_type(self.file_id, closure_syntax, param_index)?;
        let generic_names = self.signature_generic_names(closure_syntax);
        let ty = self
            .q()
            .type_shell_lua_in(self.file_id, &shell, &generic_names);
        if !matches!(ty, LuaType::Unknown) {
            return Some(ty);
        }
        // For `function a.aaa(x)` without `---@param`, fill in from the function signature of the same-named field on the owner type.
        self.expected_member_param_for_closure(closure_syntax, param_index)
    }

    /// Generic parameter names from signature docs (`---@generic T`), used as the shell projection context.
    fn signature_generic_names(&self, closure_syntax: LuaSyntaxId) -> Vec<SmolStr> {
        let Some(signatures) = self.signatures() else {
            return Vec::new();
        };
        let Some(signature) = signatures
            .iter()
            .find(|sig| sig.closure_syntax == closure_syntax)
        else {
            return Vec::new();
        };
        signature
            .docs
            .as_ref()
            .map(|docs| docs.generic_params.iter().map(|g| g.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Type of the module's exported value.
    pub fn type_of_module_export(&self) -> Option<LuaType> {
        let shell = self.q().module_export_type(self.file_id)?;
        Some(self.q().type_shell_lua(self.file_id, &shell))
    }

    /// require module name -> module export type (cross-file, projected as `LuaType`).
    pub fn require_module_type(&self, module_name: &str) -> LuaType {
        let Some(module_file) = self.q().module_file_of(module_name) else {
            return LuaType::Unknown;
        };
        let Some(shell) = self.q().module_export_type(module_file) else {
            return LuaType::Unknown;
        };
        self.q().type_shell_lua(module_file, &shell)
    }

    /// require module name -> module file id (consumed by require_module_visibility checks).
    pub fn module_file_of(&self, module_name: &str) -> Option<FileId> {
        self.q().module_file_of(module_name)
    }

    // -- Type / member association --

    pub fn resolve_type_def(&self, name: &str) -> Option<TypeDef> {
        self.q().resolve_type_def(self.file_id, name)
    }

    /// Resolves a named type in a specified file scope (used for cross-file constraint/default projection).
    pub fn resolve_type_def_in(&self, file_id: FileId, name: &str) -> Option<TypeDef> {
        self.q().resolve_type_def(file_id, name)
    }

    /// Type name string -> `LuaType` (salsa facade uniformly handles built-in and named types).
    /// Named types that are aliases expand to the alias target type.
    pub fn type_from_name(&self, name: &str) -> LuaType {
        let ty = self.q().resolve_named(self.file_id, name);
        if let LuaType::Ref(id) | LuaType::Def(id) = &ty
            && let Some(def) = self.resolve_type_def(id.get_name())
            && def.kind == TypeDefKind::Alias
            && let Some(target) = self.alias_target(&def)
        {
            return target;
        }
        ty
    }

    /// Type declaration identity of `LuaType::Ref/Def` -> `TypeDef`.
    pub fn type_def_of(&self, id: &LuaTypeDeclId) -> Option<TypeDef> {
        member::type_def_of(self, id)
    }

    /// Named type definition -> reference type (visibility determines global/file identity).
    pub fn type_def_ref(&self, def: &TypeDef) -> LuaType {
        type_def_ref(def)
    }

    /// All definition locations of a named type (used by duplicate-type checks).
    pub fn type_def_locations(&self, name: &str) -> Vec<TypeDef> {
        self.q().type_def_locations(self.file_id, name)
    }

    pub fn member_keys_of_owner(&self, owner: &SemanticId) -> Vec<SmolStr> {
        self.q().member_keys_of_owner(owner.clone())
    }

    pub fn resolve_owner(&self, owner: &SemanticId) -> Option<SemanticId> {
        self.q().resolve_owner(owner.clone())
    }

    pub fn module_export(&self) -> Option<&'db ModuleExport> {
        self.q().module_export(self.file_id)
    }

    // -- Control flow --

    pub fn flow_tree(&self) -> Option<Arc<FlowTree>> {
        self.q().flow_tree(self.file_id)
    }

    // -- Convenience predicates --

    /// Type of an expression (VM inference; `Unknown` = no type / inference failed).
    pub fn type_of_expr(&self, expr_syntax: LuaSyntaxId) -> LuaType {
        self.type_of_expr_impl(expr_syntax)
    }

    pub(crate) fn type_of_expr_impl(&self, expr_syntax: LuaSyntaxId) -> LuaType {
        let file_id = self.file_id;
        if let Some(cached) = self.cache.borrow().expr_type.get(&(file_id, expr_syntax)) {
            return cached.clone();
        }
        if self.is_expr_infer_active(expr_syntax) {
            return LuaType::Unknown;
        }
        self.begin_expr_infer(expr_syntax);
        let ty = infer::infer_expr(self, expr_syntax);
        self.end_expr_infer(expr_syntax);
        self.cache
            .borrow_mut()
            .expr_type
            .insert((file_id, expr_syntax), ty.clone());
        ty
    }

    /// Hover/display layer only: expands generic alias instances (`Pick<...>` -> `T[K]` structure).
    pub fn expand_alias_for_hover(&self, ty: &LuaType) -> LuaType {
        type_eval::expand_alias_generic(self, ty)
    }

    /// Hover/display layer only: evaluates conditional types (`A extends B ? X : Y`).
    pub fn eval_conditionals_for_hover(&self, ty: &LuaType) -> LuaType {
        type_eval::eval_conditionals(self, ty)
    }

    /// Type compatibility check (boolean version, mirrors the old `SemanticModel::type_check`).
    pub fn type_check(&self, source: &LuaType, target: &LuaType) -> bool {
        if let Some(cached) = self
            .cache
            .borrow()
            .type_check
            .get(&(source.clone(), target.clone()))
        {
            return *cached;
        }
        let result = type_check::is_compatible_uncached(self, source, target);
        self.cache
            .borrow_mut()
            .type_check
            .insert((source.clone(), target.clone()), result);
        result
    }

    /// Strict subtype check: union targets across all components, object field-level checks, generic alias expansion.
    /// Only for tests/callers needing precise subtype relations; does not affect the old `type_check` loose compatibility semantics.
    pub fn type_check_subtype(&self, source: &LuaType, target: &LuaType) -> bool {
        type_check::check_type_subtype(self, source, target).is_ok()
    }

    /// Multi-value expression list types (mirrors old `infer_expr_list_types`; trailing multi-returns expand according to var_count).
    pub fn infer_expr_list_types(
        &self,
        exprs: &[LuaExpr],
        var_count: Option<usize>,
    ) -> Vec<(LuaType, rowan::TextRange)> {
        infer::infer_expr_list_types(self, exprs, var_count)
    }

    /// Unified signature return type projection: TypeShell takes priority, with rich projection as fallback for Unknown.
    fn signature_return_type(
        &self,
        file_id: FileId,
        closure_syntax: LuaSyntaxId,
        signature: &Signature,
        generic_names: &[SmolStr],
    ) -> LuaType {
        let return_shells = self
            .q()
            .signature_returns(file_id, closure_syntax)
            .unwrap_or_default();
        let mut ret = if return_shells.len() > 1 {
            LuaType::Variadic(Arc::new(VariadicType::Multi(
                return_shells
                    .iter()
                    .map(|shell| self.q().type_shell_lua_in(file_id, shell, generic_names))
                    .collect(),
            )))
        } else if let Some(shell) = return_shells.first() {
            self.q().type_shell_lua_in(file_id, shell, generic_names)
        } else {
            LuaType::Unknown
        };
        // Conditional types cannot be expressed by TypeShell; whenever the return type contains `Conditional`, prefer
        // rich projection to avoid `Test<T extends ...>` being downgraded to `Test<unknown>` by shell.
        if let Some(docs) = signature.docs.as_ref()
            && docs.returns.len() == 1
        {
            let rich = self.doc_type_lua_rich_in(file_id, docs.returns[0]);
            if !matches!(rich, LuaType::Unknown)
                && rich.any_type(|ty| matches!(ty, LuaType::Conditional(_)))
            {
                ret = rich;
            }
        }
        // Rich projection fallback: structures like `T[K]` / mapped may be Unknown at the TypeShell layer.
        if matches!(ret, LuaType::Unknown)
            && let Some(docs) = signature.docs.as_ref()
            && docs.returns.len() == 1
        {
            let rich = self.doc_type_lua_rich_in(file_id, docs.returns[0]);
            if !matches!(rich, LuaType::Unknown) {
                ret = rich;
            }
        }
        // `self` in method annotations is the receiver instance; concretize the signature return type by the method owner.
        if signature.is_method
            && let Some(owner_ty) = self.method_owner_type(closure_syntax)
        {
            ret = infer::vm::replace_self_type(&ret, &owner_ty);
        }
        ret
    }

    /// Function signature (by closure syntax location) -> structured function type (`DocFunction`).
    pub fn type_of_signature(&self, closure_syntax: LuaSyntaxId) -> Option<LuaFunctionType> {
        let signatures = self.signatures()?;
        let signature = signatures
            .iter()
            .find(|sig| sig.closure_syntax == closure_syntax)?;
        // `---@generic T: Base` -> unified projection context + GenericTpl list (including constraints/defaults).
        let generic_params: Vec<GenericTpl> = signature
            .docs
            .as_ref()
            .map(|docs| self.generic_tpls_with_metadata(self.file_id, &docs.generic_params))
            .unwrap_or_default();
        let nullable_params: Vec<SmolStr> = signature
            .docs
            .as_ref()
            .map(|docs| docs.nullable_params.clone())
            .unwrap_or_default();
        let mut params = Vec::new();
        for (index, name) in signature.param_names.iter().enumerate() {
            let mut ty = self
                .param_type(closure_syntax, index)
                .unwrap_or(LuaType::Unknown);
            if matches!(ty, LuaType::Unknown | LuaType::Table)
                && let Some(docs) = signature.docs.as_ref()
                && let Some((_, syntax)) = docs
                    .param_types
                    .iter()
                    .find(|(param_name, _)| param_name == name)
            {
                let rich = self.doc_type_lua_rich(*syntax);
                if !matches!(rich, LuaType::Unknown) {
                    ty = rich;
                }
            }
            // `function a.aaa(x)`: if this closure implements a member and the owner type has
            // a same-named field with a function type, use the field signature to fill in missing `---@param` types.
            if matches!(ty, LuaType::Unknown)
                && let Some(expected_param_ty) =
                    self.expected_member_param_for_closure(closure_syntax, index)
            {
                ty = expected_param_ty;
            }
            ty = type_eval::expand_alias_generic(self, &ty);
            // Do not evaluate conditional types here: before function generics are substituted, `T extends ...` would incorrectly take the false branch.
            // Call sites/diagnostics should call `eval_conditionals` after bindings are substituted.
            if nullable_params.iter().any(|n| n == name) && !ty.is_nullable() {
                ty = LuaType::Union(Arc::new(LuaUnionType::from_vec(vec![ty, LuaType::Nil])));
            }
            params.push((name.to_string(), Some(ty)));
        }
        // `function M:event_on(...)` only has unpacked `...`, but `---@overload` / class fields give concrete slots.
        // Here overload function types project `...` into Variadic(Multi(...)) so `local a,b,c = ...` can take slots.
        if signature.is_variadic {
            let overload_funcs: Vec<LuaFunctionType> = if let Some(docs) = signature.docs.as_ref()
                && !docs.overloads.is_empty()
            {
                docs.overloads
                    .iter()
                    .filter_map(
                        |syntax| match self.doc_type_lua_rich_in(self.file_id, *syntax) {
                            LuaType::DocFunction(fun) => Some(fun.as_ref().clone()),
                            _ => None,
                        },
                    )
                    .collect()
            } else {
                self.expected_member_signatures_for_closure(closure_syntax)
                    .unwrap_or_default()
            };
            if !overload_funcs.is_empty() {
                let has_self = overload_funcs.iter().any(|fun| {
                    fun.get_params().first().is_some_and(|(name, ty)| {
                        name == "self" || matches!(ty, Some(LuaType::SelfInfer))
                    })
                });
                let param_start = usize::from(has_self);
                let max_len = overload_funcs
                    .iter()
                    .map(|fun| fun.get_params().len().saturating_sub(param_start))
                    .max()
                    .unwrap_or(0);
                let mut slots = Vec::new();
                for slot in 0..max_len {
                    let mut parts = Vec::new();
                    for fun in &overload_funcs {
                        if let Some((_, Some(ty))) = fun.get_params().get(param_start + slot) {
                            let ty = type_eval::expand_alias_generic(self, ty);
                            let ty = if ty.any_type(|t| matches!(t, LuaType::Any)) {
                                LuaType::Any
                            } else {
                                ty
                            };
                            if !parts.contains(&ty) {
                                parts.push(ty);
                            }
                        }
                    }
                    // Unions containing any, such as `any[] | any`, should collapse to any directly.
                    if parts.iter().any(|ty| matches!(ty, LuaType::Any)) {
                        parts = vec![LuaType::Any];
                    }
                    slots.push(LuaType::from_vec(parts));
                }
                if let Some((_, ty)) = params.iter_mut().find(|(name, _)| name == "...") {
                    *ty = Some(LuaType::Variadic(Arc::new(VariadicType::Multi(slots))));
                }
            }
        }
        let generic_names: Vec<SmolStr> = generic_params
            .iter()
            .map(|param| SmolStr::new(param.get_name()))
            .collect();
        let mut ret =
            self.signature_return_type(self.file_id, closure_syntax, signature, &generic_names);
        ret = type_eval::expand_alias_generic(self, &ret);
        // Without an explicit `---@return`, keep the return type declared by the member on the owner type.
        // This keeps member docs like class field `---@field f fun(): never` effective on the implementation function's return.
        if matches!(ret, LuaType::Unknown)
            && signature.docs.is_none()
            && let Some(expected_fun) = self.expected_member_signature_for_closure(closure_syntax)
        {
            ret = expected_fun.get_ret().clone();
        }
        if matches!(ret, LuaType::Unknown) && signature.docs.is_none() {
            let inferred = infer::closure_return_lua(self, closure_syntax);
            if !matches!(inferred, LuaType::Unknown | LuaType::Any) {
                ret = inferred;
            }
        }
        let is_variadic = signature.is_variadic;
        // `---@async` -> AsyncState::Async (consumed by await_in_sync checks).
        let async_state = signature
            .docs
            .as_ref()
            .map(|docs| {
                if docs.is_async {
                    AsyncState::Async
                } else {
                    AsyncState::None
                }
            })
            .unwrap_or(AsyncState::None);
        Some(LuaFunctionType::new(
            async_state,
            signature.is_method,
            is_variadic,
            params,
            ret,
            Some(generic_params),
        ))
    }

    /// Instance type of the owner that a method closure belongs to (the `self` type).
    /// Consistent with `method_self_return_shell`: if the method owner is a runtime declaration
    /// and that declaration has an attached `---@class/@enum` type definition (same owner_syntax), use the type definition.
    /// Finds the owning method closure from the implicit `self` parameter declaration.
    fn method_closure_for_self_decl(&self, decl: &Decl) -> Option<LuaSyntaxId> {
        let closure_syntax = decl.owner_syntax?;
        let facts = self.file_facts()?;
        let signature = facts.signature_by_closure(closure_syntax)?;
        if signature.is_method {
            Some(closure_syntax)
        } else {
            None
        }
    }

    fn method_owner_type(&self, closure_syntax: LuaSyntaxId) -> Option<LuaType> {
        let facts = self.file_facts()?;
        let member = facts.member_by_value_syntax(closure_syntax)?;
        let owner = member.owner.clone();
        let (owner_ty, owner_value) = match &owner {
            SemanticId::Decl(decl) => (
                self.type_of_decl(&SemanticId::Decl(decl.clone()))?,
                owner.clone(),
            ),
            SemanticId::TypeDef(def) => {
                let id = match &def.scope {
                    TypeScope::Global => LuaTypeDeclId::global(&def.full_name),
                    TypeScope::Internal(workspace_id) => {
                        LuaTypeDeclId::internal(*workspace_id, &def.full_name)
                    }
                    TypeScope::File(file_id) => LuaTypeDeclId::file(*file_id, &def.full_name),
                };
                let def = self.type_def_of(&id)?;
                (self.type_def_ref(&def), owner.clone())
            }
            SemanticId::Name(name) => {
                let resolved = self.resolve_owner(&SemanticId::Name(name.clone()))?;
                let ty = match &resolved {
                    SemanticId::Decl(decl) => self.type_of_decl(&SemanticId::Decl(decl.clone()))?,
                    SemanticId::TypeDef(def) => {
                        let id = match &def.scope {
                            TypeScope::Global => LuaTypeDeclId::global(&def.full_name),
                            TypeScope::Internal(workspace_id) => {
                                LuaTypeDeclId::internal(*workspace_id, &def.full_name)
                            }
                            TypeScope::File(file_id) => {
                                LuaTypeDeclId::file(*file_id, &def.full_name)
                            }
                        };
                        let def = self.type_def_of(&id)?;
                        self.type_def_ref(&def)
                    }
                    _ => return None,
                };
                (ty, resolved)
            }
            _ => return None,
        };
        let owner_ty = if let SemanticId::Decl(decl_id) = &owner_value {
            if let Some(facts) = self.file_facts_of(decl_id.file_id)
                && let Some(decl) = facts.decl_by_id(&SemanticId::Decl(decl_id.clone()))
                && let Some(def) = facts
                    .type_defs
                    .iter()
                    .find(|def| def.owner_syntax.is_some() && def.owner_syntax == decl.owner_syntax)
            {
                self.type_def_ref(def)
            } else {
                owner_ty
            }
        } else {
            owner_ty
        };
        Some(owner_ty)
    }

    /// If the closure is a member implementation like `function a.aaa(...)`, return the function signature of the same-named field on the owner type.
    /// Used to fill in parameter types from field declarations when `---@param` is absent.
    fn expected_member_param_for_closure(
        &self,
        closure_syntax: LuaSyntaxId,
        param_index: usize,
    ) -> Option<LuaType> {
        let is_method = self
            .file_facts()
            .and_then(|facts| facts.signature_by_closure(closure_syntax))
            .is_some_and(|sig| sig.is_method);
        let mut types = Vec::new();
        for expected_fun in self.expected_member_signatures_for_closure(closure_syntax)? {
            let has_self_in_expected = expected_fun
                .get_params()
                .first()
                .is_some_and(|(name, ty)| name == "self" || matches!(ty, Some(LuaType::SelfInfer)));
            // `:` methods don't write the implicit self in source, while `---@field` function types usually list self as the first parameter.
            // When inferring implementation function parameters, skip that self parameter.
            let expected_index = if is_method && has_self_in_expected {
                param_index + 1
            } else {
                param_index
            };
            if let Some(ty) = expected_fun
                .get_params()
                .get(expected_index)
                .and_then(|(_, ty)| ty.clone())
                && !types.contains(&ty)
            {
                types.push(ty);
            }
        }
        if types.is_empty() {
            None
        } else {
            Some(LuaType::from_vec(types))
        }
    }

    /// If the closure is a member implementation like `function a.aaa(...)`, return the function signature of the same-named field on the owner type.
    /// Used to fill in parameter types from field declarations when `---@param` is absent.
    pub(crate) fn expected_member_signature_for_closure(
        &self,
        closure_syntax: LuaSyntaxId,
    ) -> Option<LuaFunctionType> {
        self.expected_member_signatures_for_closure(closure_syntax)?
            .into_iter()
            .next()
    }

    /// Returns all usable function signatures for this member implementation:
    /// - `---@overload` as the overload set;
    /// - repeated `---@field` (non-overload) follows old semantics where the last one overrides previous ones;
    /// - a normal single field uses that field's signature directly.
    fn expected_member_signatures_for_closure(
        &self,
        closure_syntax: LuaSyntaxId,
    ) -> Option<Vec<LuaFunctionType>> {
        let facts = self.file_facts()?;
        let member = facts
            .members
            .iter()
            .find(|member| member.value_syntax == Some(closure_syntax))?;
        let member_file = self.file_id;
        // 0. Inline `---@type` on a table field (`{ ---@type test A = function(a, b) ... }`) directly gives the function type.
        if let Some(doc_syntax) = member.doc_type_syntax {
            let mut doc_ty = type_eval::expand_alias_generic(
                self,
                &self.doc_type_lua_rich_in(member_file, doc_syntax),
            );
            // When an inline field `---@type test` points to a plain alias, `expand_alias_generic` only expands
            // the `Alias<...>` form; bare `Ref(test)` needs to continue along the alias chain to the function type.
            let mut alias_visited = Vec::new();
            while let LuaType::Ref(id) | LuaType::Def(id) = &doc_ty {
                let alias_id = id.clone();
                if alias_visited.contains(&alias_id) {
                    break;
                }
                let Some(def) = self.type_def_of(&alias_id) else {
                    break;
                };
                if def.kind != TypeDefKind::Alias {
                    break;
                }
                let Some(target) = self.alias_target(&def) else {
                    break;
                };
                alias_visited.push(alias_id);
                doc_ty = type_eval::expand_alias_generic(self, &target);
            }
            let mut docs_out = Vec::new();
            match &doc_ty {
                LuaType::DocFunction(fun) => docs_out.push(fun.as_ref().clone()),
                LuaType::Union(union) => {
                    for ty in union.into_vec() {
                        if let LuaType::DocFunction(fun) = ty {
                            docs_out.push(fun.as_ref().clone());
                        }
                    }
                }
                _ => {}
            }
            if !docs_out.is_empty() {
                return Some(docs_out);
            }
        }
        // 1. `---@overload` on the function's own doc: each overload is an independent candidate.
        if let Some(signature) = facts.signature_by_closure(closure_syntax)
            && let Some(docs) = signature.docs.as_ref()
            && !docs.overloads.is_empty()
        {
            let mut out = Vec::new();
            for syntax in &docs.overloads {
                if let LuaType::DocFunction(fun) = self.doc_type_lua_rich_in(member_file, *syntax) {
                    out.push(fun.as_ref().clone());
                }
            }
            if !out.is_empty() {
                return Some(out);
            }
        }

        let owner = member.owner.clone();
        let key = member.key.clone();

        let owner_ty = match &owner {
            SemanticId::Decl(decl) => {
                let mut owner_ty = self.type_of_decl(&SemanticId::Decl(decl.clone()))?;
                // `---@class ClosureTest` + `local Test`: differently named locals should also be associated with the class definition,
                // otherwise `function Test:e` cannot fill parameter types from class fields.
                if !matches!(
                    owner_ty,
                    LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_)
                ) && let Some(facts) = self.file_facts_of(decl.file_id)
                    && let Some(decl_info) = facts.decl_by_id(&SemanticId::Decl(decl.clone()))
                    && let Some(def) = decl_info
                        .owner_syntax
                        .and_then(|syntax| facts.type_def_by_owner_syntax(syntax))
                {
                    owner_ty = self.type_def_ref(def);
                }
                owner_ty
            }
            SemanticId::TypeDef(def) => {
                let id = match &def.scope {
                    TypeScope::Global => LuaTypeDeclId::global(&def.full_name),
                    TypeScope::Internal(workspace_id) => {
                        LuaTypeDeclId::internal(*workspace_id, &def.full_name)
                    }
                    TypeScope::File(file_id) => LuaTypeDeclId::file(*file_id, &def.full_name),
                };
                let def = self.type_def_of(&id)?;
                self.type_def_ref(&def)
            }
            SemanticId::Name(name) => {
                let owner_id = SemanticId::Name(name.clone());
                match self.resolve_owner(&owner_id) {
                    Some(SemanticId::Decl(decl)) => self.type_of_decl(&SemanticId::Decl(decl))?,
                    Some(SemanticId::TypeDef(def)) => {
                        let id = match &def.scope {
                            TypeScope::Global => LuaTypeDeclId::global(&def.full_name),
                            TypeScope::Internal(workspace_id) => {
                                LuaTypeDeclId::internal(*workspace_id, &def.full_name)
                            }
                            TypeScope::File(file_id) => {
                                LuaTypeDeclId::file(*file_id, &def.full_name)
                            }
                        };
                        let def = self.type_def_of(&id)?;
                        self.type_def_ref(&def)
                    }
                    _ => return None,
                }
            }
            SemanticId::Member(table_key) => {
                // Table-literal field (`---@type D31; local f = { func = function(...) end }`):
                // the member owner is a synthetic table identity; go back to the declaration that initializes the table and use its declared type as the field type source.
                let decl = facts.decls.iter().find(|decl| {
                    decl.value_expr_syntax
                        .is_some_and(|syntax| syntax.get_range() == table_key.key_range)
                })?;
                self.type_of_decl(&decl.id)?
            }
            _ => return None,
        };

        let infos = member::member_infos_with_key_all(self, &owner_ty, &key);
        let mut out = Vec::new();
        for info in &infos {
            let typ = match &info.typ {
                LuaType::Ref(id) | LuaType::Def(id)
                    if self
                        .type_def_of(id)
                        .is_some_and(|def| def.kind == TypeDefKind::Alias) =>
                {
                    let def = self.type_def_of(id)?;
                    self.alias_target(&def)
                        .map(|target| type_eval::expand_alias_generic(self, &target))
                        .unwrap_or_else(|| info.typ.clone())
                }
                _ => type_eval::expand_alias_generic(self, &info.typ),
            };
            match &typ {
                LuaType::DocFunction(fun) => out.push(fun.as_ref().clone()),
                LuaType::Union(union) => {
                    for ty in union.into_vec() {
                        if let LuaType::DocFunction(fun) = ty {
                            out.push(fun.as_ref().clone());
                        }
                    }
                }
                _ => {}
            }
        }
        // Repeated `@field` non-overload: the last one overrides previous ones (expected by the third test case).
        if out.len() > 1 {
            out = vec![out.pop()?];
        }
        Some(out)
    }

    /// `---@param` / `---@return` annotations on table-literal fields override the closure's bare signature.
    /// salsa currently does not merge these field-level docs into `Signature.docs`, but `setmetatable`'s
    /// `__call` / `__index` metamethod fields depend on them to preserve the function signature.
    fn table_field_signature_override(
        &self,
        file_id: FileId,
        closure_syntax: LuaSyntaxId,
        mut params: Vec<(String, Option<LuaType>)>,
        mut ret: LuaType,
    ) -> Option<(Vec<(String, Option<LuaType>)>, LuaType)> {
        let facts = self.file_facts_of(file_id)?;
        facts
            .members
            .iter()
            .find(|member| member.value_syntax == Some(closure_syntax))?;
        let tree = self.syntax_tree_of(file_id)?;
        let node = closure_syntax.to_node_from_root(&tree.get_red_root())?;
        let field = node.ancestors().find_map(LuaTableField::cast)?;
        let mut has_param = false;
        let mut has_return = false;
        let mut returns = Vec::new();
        for comment in field.get_comments() {
            for tag in comment.get_doc_tags() {
                match tag {
                    LuaDocTag::Param(param) => {
                        if let Some(name_token) = param.get_name_token()
                            && let Some(doc_ty) = param.get_type()
                        {
                            let mut ty = self.doc_type_lua_rich_in(file_id, doc_ty.get_syntax_id());
                            if param.is_nullable() {
                                ty = LuaType::from_vec(vec![ty, LuaType::Nil]);
                            }
                            let name = name_token.get_name_text().to_string();
                            if let Some(slot) = params.iter_mut().find(|(n, _)| n == &name) {
                                slot.1 = Some(ty);
                                has_param = true;
                            }
                        }
                    }
                    LuaDocTag::Return(return_tag) => {
                        has_return = true;
                        for doc_ty in return_tag.get_types() {
                            let ty = self.doc_type_lua_rich_in(file_id, doc_ty.get_syntax_id());
                            if !matches!(ty, LuaType::Unknown) {
                                returns.push(ty);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if has_return && !returns.is_empty() {
            ret = if returns.len() == 1 {
                returns.pop()?
            } else {
                LuaType::Variadic(Arc::new(VariadicType::Multi(returns)))
            };
        }
        if has_param || has_return {
            Some((params, ret))
        } else {
            None
        }
    }

    /// Signature structure of a closure in any file (used for cross-file member/global function signature resolution).
    pub fn type_of_signature_in_file(
        &self,
        file_id: FileId,
        closure_syntax: LuaSyntaxId,
    ) -> Option<LuaFunctionType> {
        let facts = self.file_facts_of(file_id)?;
        let signature = facts.signature_by_closure(closure_syntax)?;
        let generic_params: Vec<GenericTpl> = signature
            .docs
            .as_ref()
            .map(|docs| self.generic_tpls_with_metadata(file_id, &docs.generic_params))
            .unwrap_or_default();
        let generic_names: Vec<SmolStr> = generic_params
            .iter()
            .map(|param| SmolStr::new(param.get_name()))
            .collect();
        let nullable_params: Vec<SmolStr> = signature
            .docs
            .as_ref()
            .map(|docs| docs.nullable_params.clone())
            .unwrap_or_default();
        let mut params = Vec::new();
        for (index, name) in signature.param_names.iter().enumerate() {
            let mut ty = self
                .q()
                .param_type(file_id, closure_syntax, index)
                .map(|shell| self.q().type_shell_lua_in(file_id, &shell, &generic_names))
                .filter(|ty| !matches!(ty, LuaType::Unknown))
                .or_else(|| {
                    signature.docs.as_ref().and_then(|docs| {
                        docs.param_types
                            .iter()
                            .find(|(param_name, _)| param_name == name)
                            .map(|(_, syntax)| self.doc_type_lua_rich_in(file_id, *syntax))
                    })
                })
                .unwrap_or(LuaType::Any);
            ty = type_eval::expand_alias_generic(self, &ty);
            // See `type_of_signature`: do not evaluate conditional types before function generics are substituted.
            if nullable_params.iter().any(|n| n == name) && !ty.is_nullable() {
                ty = LuaType::Union(Arc::new(LuaUnionType::from_vec(vec![ty, LuaType::Nil])));
            }
            params.push((name.to_string(), Some(ty)));
        }
        let mut ret =
            self.signature_return_type(file_id, closure_syntax, signature, &generic_names);
        ret = type_eval::expand_alias_generic(self, &ret);
        if matches!(ret, LuaType::Unknown) && signature.docs.is_none() && file_id == self.file_id {
            let inferred = infer::closure_return_lua(self, closure_syntax);
            if !matches!(inferred, LuaType::Unknown | LuaType::Any) {
                ret = inferred;
            }
        }
        if signature.docs.is_none()
            && let Some((new_params, new_ret)) = self.table_field_signature_override(
                file_id,
                closure_syntax,
                params.clone(),
                ret.clone(),
            )
        {
            params = new_params;
            ret = new_ret;
        }
        let is_variadic = signature.is_variadic;
        let async_state = signature
            .docs
            .as_ref()
            .map(|docs| {
                if docs.is_async {
                    AsyncState::Async
                } else {
                    AsyncState::None
                }
            })
            .unwrap_or(AsyncState::None);
        Some(LuaFunctionType::new(
            async_state,
            signature.is_method,
            is_variadic,
            params,
            ret,
            Some(generic_params),
        ))
    }

    /// Signature structure of a declaration (cross-file: the `Decl` key carries file_id).
    /// One unified entry: declaration identity -> locate `(file, closure)` -> `type_of_signature_in_file`.
    pub fn type_of_decl_signature(&self, decl: &SemanticId) -> Option<LuaFunctionType> {
        let SemanticId::Decl(decl_key) = decl else {
            return None;
        };
        let facts = self.file_facts_of(decl_key.file_id)?;
        let decl = facts.decl_by_id(decl)?;
        let closure_syntax = decl.value_expr_syntax?;
        self.type_of_signature_in_file(decl_key.file_id, closure_syntax)
    }

    /// Legacy `LuaSignatureId` (payload of the `LuaType::Signature` value variant) -> structured function type.
    /// The only bridge in the new layer: the main API is `type_of_signature(closure_syntax)`; this is only for consuming value variants.
    pub(crate) fn signature_lua_by_legacy_id(
        &self,
        signature_id: &LuaSignatureId,
    ) -> Option<LuaFunctionType> {
        let signatures = self.signatures()?;
        let signature = signatures.iter().find(|sig| {
            sig.file_id == signature_id.get_file_id()
                && sig.closure_syntax.get_range().start() == signature_id.get_position()
        })?;
        self.type_of_signature(signature.closure_syntax)
    }
}

/// Conditional-type `infer` scope: declare `infer P`; the true branch may reference `P`.
#[derive(Default)]
struct ConditionalInferState {
    scopes: Vec<HashMap<SmolStr, GenericTpl>>,
    refs_visible: Vec<bool>,
    params: Vec<Vec<GenericParam>>,
    next_id: u32,
}

impl ConditionalInferState {
    fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.refs_visible.push(false);
        self.params.push(Vec::new());
    }

    fn leave_scope(&mut self) -> Vec<GenericParam> {
        self.scopes.pop();
        self.refs_visible.pop();
        self.params.pop().unwrap_or_default()
    }

    fn set_refs_visible(&mut self, visible: bool) {
        if let Some(last) = self.refs_visible.last_mut() {
            *last = visible;
        }
    }

    fn declare(&mut self, name: &str) -> Option<GenericTpl> {
        let idx = self.scopes.len().checked_sub(1)?;
        let tpl_id = GenericTplId::ConditionalInfer(self.next_id);
        self.next_id += 1;
        let tpl = GenericTpl::new(tpl_id, SmolStr::new(name), None, None, false, None);
        if let Some(existing) = self.scopes[idx].get(name) {
            return Some(existing.clone());
        }
        self.scopes[idx].insert(SmolStr::new(name), tpl.clone());
        let param = GenericParam {
            name: tpl.get_param().name.clone(),
            constraint: None,
            default: None,
            attributes: None,
            is_const: false,
        };
        self.params[idx].push(param);
        Some(tpl)
    }

    fn find_ref(&self, name: &str) -> Option<GenericTpl> {
        self.scopes
            .iter()
            .zip(self.refs_visible.iter())
            .rev()
            .filter(|(_, visible)| **visible)
            .find_map(|(scope, _)| scope.get(name).cloned())
    }
}

/// Named type definition -> reference type (`LuaType::Ref`); visibility determines global/file identity.
fn type_def_ref(def: &TypeDef) -> LuaType {
    match def.visibility {
        TypeVisibility::Public => LuaType::Ref(LuaTypeDeclId::global(&def.full_name)),
        _ => LuaType::Ref(LuaTypeDeclId::file(def.file_id, &def.full_name)),
    }
}

impl<'db> SemanticModel<'db> {
    /// Declaration inference. Reentry/cycles are handled by the Salsa tracked
    /// `semantic_decl_type` / `semantic_expr_type` queries; no manual guard is needed here.
    fn infer_decl_guarded(
        &self,
        _decl: SemanticId,
        infer: impl FnOnce() -> LuaType,
    ) -> Option<LuaType> {
        Some(infer())
    }
}

/// Removes nil from a type (used for generic for loop keys: the loop stops when the first value is nil).
fn remove_nil_from_type(ty: LuaType) -> LuaType {
    match ty {
        LuaType::Union(union) => {
            let types: Vec<LuaType> = union
                .into_vec()
                .into_iter()
                .filter(|t| !t.is_nil())
                .collect();
            match types.len() {
                0 => LuaType::Unknown,
                1 => types.into_iter().next().expect("len checked"),
                _ => LuaType::Union(Arc::new(LuaUnionType::from_vec(types))),
            }
        }
        LuaType::Nil => LuaType::Unknown,
        other => other,
    }
}
