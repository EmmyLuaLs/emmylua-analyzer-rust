use crate::MemberList;

use super::prelude::*;

impl<'db> SemanticModel<'db> {
    /// Members whose owner is a `SemanticId` (cross-file).
    pub fn members_of_owner(&self, owner: &SemanticId) -> MemberList {
        if let Some(cached) = self.cache.borrow().members_of_owner.get(owner) {
            return cached.clone();
        }
        let result = self.q().members_of_owner(owner.clone());
        self.cache
            .borrow_mut()
            .members_of_owner
            .insert(owner.clone(), result.clone());
        result
    }
    /// Members of an owner with a specific name.
    pub fn members_of_owner_named(&self, owner: &SemanticId, name: &str) -> MemberList {
        let key = (owner.clone(), SmolStr::new(name));
        if let Some(cached) = self.cache.borrow().members_of_owner_named.get(&key) {
            return cached.clone();
        }
        let result = self
            .q()
            .members_of_owner_named(owner.clone(), SmolStr::new(name));
        self.cache
            .borrow_mut()
            .members_of_owner_named
            .insert(key, result.clone());
        result
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
    pub(crate) fn signature_generic_names(&self, closure_syntax: LuaSyntaxId) -> Vec<SmolStr> {
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
    /// Type name string -> `LuaType` (semantic facade uniformly handles built-in and named types).
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
        match def.visibility {
            TypeVisibility::Public => LuaType::Ref(LuaTypeDeclId::global(&def.full_name)),
            _ => LuaType::Ref(LuaTypeDeclId::file(def.file_id, &def.full_name)),
        }
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
    pub(crate) fn resolve_owner_set(&self, owner: SemanticId) -> Vec<SemanticId> {
        if let Some(cached) = self.cache.borrow().resolve_owner_set.get(&owner) {
            return cached.clone();
        }
        let result = self.q().resolve_owner_set(owner.clone());
        self.cache
            .borrow_mut()
            .resolve_owner_set
            .insert(owner, result.clone());
        result
    }
    pub fn module_export(&self) -> Option<&'db ModuleExport> {
        self.q().module_export(self.file_id)
    }
    // -- Control flow --

    pub fn flow_tree(&self) -> Option<&'db FlowTree> {
        self.q().flow_tree(self.file_id)
    }
    // -- Convenience predicates --

    /// Type of an expression (VM inference; `Unknown` = no type / inference failed).
    pub fn type_of_expr(&self, expr_syntax: LuaSyntaxId) -> LuaType {
        self.type_of_expr_impl(expr_syntax)
    }
    pub(crate) fn type_of_expr_impl(&self, expr_syntax: LuaSyntaxId) -> LuaType {
        let key = (self.file_id, expr_syntax);
        match self.cache.borrow().expr_type.get(&key) {
            Some(cache::CacheEntry::Ready(cached)) => return cached.clone(),
            Some(cache::CacheEntry::InProgress) => return LuaType::Unknown,
            None => {}
        }
        self.cache
            .borrow_mut()
            .expr_type
            .insert(key, cache::CacheEntry::InProgress);
        let ty = infer::infer_expr(self, expr_syntax);
        self.cache
            .borrow_mut()
            .expr_type
            .insert(key, cache::CacheEntry::Ready(ty.clone()));
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
    pub(crate) fn signature_return_type(
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
        let signature = self.file_facts()?.signature_by_closure(closure_syntax)?;
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
    pub(crate) fn method_closure_for_self_decl(&self, decl: &Decl) -> Option<LuaSyntaxId> {
        let closure_syntax = decl.owner_syntax?;
        let facts = self.file_facts()?;
        let signature = facts.signature_by_closure(closure_syntax)?;
        if signature.is_method {
            Some(closure_syntax)
        } else {
            None
        }
    }
    pub(crate) fn method_owner_type(&self, closure_syntax: LuaSyntaxId) -> Option<LuaType> {
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
    pub(crate) fn expected_member_param_for_closure(
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
    pub(crate) fn expected_member_signatures_for_closure(
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
                let decl = facts.decl_by_value_range(table_key.key_range)?;
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
    /// semantic currently does not merge these field-level docs into `Signature.docs`, but `setmetatable`'s
    /// `__call` / `__index` metamethod fields depend on them to preserve the function signature.
    pub(crate) fn table_field_signature_override(
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
    /// Structured function type for a `LuaType::Signature` payload.
    ///
    /// This is only a bridge for consuming the legacy value variant; the main
    /// signature API is `type_of_signature(closure_syntax)`. Lookup is O(1)
    /// through `FileFacts::signature_by_position`.
    pub(crate) fn function_type_by_signature_id(
        &self,
        signature_id: &LuaSignatureId,
    ) -> Option<LuaFunctionType> {
        let file_id = signature_id.get_file_id();
        let facts = self.file_facts_of(file_id)?;
        let signature = facts.signature_by_position(signature_id.get_position())?;
        self.type_of_signature_in_file(file_id, signature.closure_syntax)
    }
}
