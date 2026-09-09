use super::prelude::*;

impl<'db> SemanticModel<'db> {
    /// Currently configured runtime version (used for `---@version` visibility checks).
    pub fn lua_version(&self) -> Option<LuaVersionNumber> {
        self.db.lua_version()
    }
    /// A model for any file on the same analysis database.
    pub fn model_for(&self, file_id: FileId) -> Self {
        SemanticModel::new(self.db, file_id)
    }
    pub fn file_id(&self) -> FileId {
        self.file_id
    }
    pub fn file_path(&self) -> Option<std::path::PathBuf> {
        self.db.file_path(self.file_id)
    }
    pub fn document(&self, file_id: FileId) -> Option<LuaDocument<'db>> {
        self.db.document(file_id)
    }
    pub fn document_current(&self) -> Option<LuaDocument<'db>> {
        self.document(self.file_id)
    }
    pub fn strict_array_index(&self) -> bool {
        self.db.strict_array_index()
    }
    pub fn file_ids(&self) -> Vec<FileId> {
        self.db.file_ids()
    }
    pub fn main_workspace_file_ids(&self) -> Vec<FileId> {
        self.db.main_workspace_file_ids()
    }
    pub fn file_path_of(&self, file_id: FileId) -> Option<std::path::PathBuf> {
        self.db.file_path(file_id)
    }
    pub fn file_uri_of(&self, file_id: FileId) -> Option<lsp_types::Uri> {
        self.db.file_uri(file_id)
    }
    pub fn module_name_of(&self, file_id: FileId) -> Option<String> {
        self.db.module_name_of(file_id)
    }
    pub fn workspace_id_of(&self, file_id: FileId) -> Option<WorkspaceId> {
        self.db.workspace_id_of(file_id)
    }
    pub fn is_std_file(&self, file_id: FileId) -> bool {
        self.db.is_std_file(file_id)
    }
    pub fn is_main_file(&self, file_id: FileId) -> bool {
        self.db.is_main_file(file_id)
    }
    pub(crate) fn q(&self) -> SemanticQueries<'db> {
        SemanticQueries::new(self.db)
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
    /// Innermost closure containing `offset`, found by walking token ancestors.
    pub(crate) fn enclosing_closure_at(&self, offset: TextSize) -> Option<LuaClosureExpr> {
        let tree = self.syntax_tree()?;
        let root = tree.get_red_root();
        let token = match root.token_at_offset(offset) {
            rowan::TokenAtOffset::Single(token) => token,
            rowan::TokenAtOffset::Between(_, right) => right,
            rowan::TokenAtOffset::None => return None,
        };
        token.parent_ancestors().find_map(LuaClosureExpr::cast)
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
        let key = (self.file_id, offset);
        if let Some(cached) = self.cache.borrow().resolve_name.get(&key) {
            return cached.clone();
        }
        let result = self.q().resolve_name(self.file_id, offset);
        self.cache
            .borrow_mut()
            .resolve_name
            .insert(key, result.clone());
        result
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
                typ: self.type_def_ref(&def),
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
                typ: self.type_def_ref(&def),
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
                typ: self.type_def_ref(&def),
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
    pub(crate) fn assignment_target_value_type(&self, expr: &LuaExpr) -> Option<LuaType> {
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
    pub(crate) fn resolved_member(
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
    pub(crate) fn prefix_type_for_member_resolution(&self, expr: &LuaExpr) -> LuaType {
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
    pub(crate) fn param_type_for_decl(&self, decl: &SemanticId) -> Option<LuaType> {
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
                    if let Some(member_ref) = self
                        .members_of_owner_named(&resolved, name.as_str())
                        .first()
                    {
                        return Some(self.resolved_member(
                            Some(member_ref.id.clone()),
                            Some(member_ref.file_id),
                            owner,
                            name,
                        ));
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
                        let self_members = method_facts
                            .members_of_owner_named(&self_decl.id, name.as_str())
                            .collect::<Vec<_>>();
                        if let Some(self_member_ref) = self_members.into_iter().next() {
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
                // Alias types have no `@field` surface of their own. Resolve through the alias
                // target so `---@type SomeAlias` values can still find class/table members.
                if def.kind == TypeDefKind::Alias
                    && let Some(info) =
                        self.member_info(prefix_ty, &LuaMemberKey::Name(name.clone()))
                    && let Some(member_id) = info.id
                {
                    let file_id = match &member_id {
                        SemanticId::Member(key) => Some(key.file_id),
                        _ => None,
                    };
                    return Some(self.resolved_member(Some(member_id), file_id, owner, name));
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
            if let Some(decl) = self
                .file_facts_of(in_field.file_id)
                .and_then(|facts| facts.decl_by_value_range(in_field.value))
            {
                table_owners.push(decl.id.clone());
            }
            for table_owner in table_owners {
                if let Some(member) = self
                    .members_of_owner_named(&table_owner, name.as_str())
                    .first()
                {
                    return Some(self.resolved_member(
                        Some(member.id.clone()),
                        Some(member.file_id),
                        owner,
                        name,
                    ));
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
                    let self_members = method_facts
                        .members_of_owner_named(&self_decl.id, name.as_str())
                        .collect::<Vec<_>>();
                    if let Some(self_member_ref) = self_members.into_iter().next() {
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
    pub(crate) fn prefix_has_named_member(
        &self,
        index_expr: &LuaIndexExpr,
        name: &SmolStr,
    ) -> bool {
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
    pub(crate) fn require_module_has_member(
        &self,
        index_expr: &LuaIndexExpr,
        name: &SmolStr,
    ) -> bool {
        let Some(owner) = self.require_module_owner(index_expr) else {
            return false;
        };
        self.members_of_owner(&owner)
            .iter()
            .any(|member| member.name == *name)
    }
    /// `local M = require("mod")` -> module export declaration owner (member bridging).
    pub(crate) fn require_module_owner(&self, index_expr: &LuaIndexExpr) -> Option<SemanticId> {
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
}
