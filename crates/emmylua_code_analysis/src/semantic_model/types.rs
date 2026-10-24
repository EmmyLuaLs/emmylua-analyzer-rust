use super::prelude::*;
use crate::LuaTypeIdentifier;
use crate::semantic_db::TypeDefList;
use crate::semantic_model::type_eval::substitute_named_refs;

impl<'db> SemanticModel<'db> {
    pub(crate) fn type_evaluation_errors(&self) -> Vec<Option<LuaTypeDeclId>> {
        self.cache
            .borrow()
            .evaluation_errors
            .iter()
            .cloned()
            .collect()
    }

    // -- Types --

    /// 按当前文件的命名空间和可见性规则解析类型名称.
    pub fn resolve_type_def(&self, name: &str) -> Option<TypeDef> {
        self.view.resolve_type_def(name)
    }

    /// Resolves a named type in a specified file scope (used for cross-file constraint/default projection).
    pub fn resolve_type_def_in(&self, file_id: FileId, name: &str) -> Option<TypeDef> {
        self.analysis().resolve_type_def(file_id, name)
    }

    /// Type name string -> `LuaType` (semantic facade uniformly handles built-in and named types).
    /// Named types that are aliases expand to the alias target type.
    pub fn type_from_name(&self, name: &str) -> LuaType {
        let ty = self.analysis().resolve_named(self.view.file_id(), name);
        if let LuaType::Ref(id) | LuaType::Def(id) = &ty
            && let Some(def) = self.type_def_of(id)
            && def.kind == TypeDefKind::Alias
            && let Some(target) = self.alias_target(&def)
        {
            return target;
        }
        ty
    }

    /// Type declaration identity of `LuaType::Ref/Def` -> `TypeDef`.
    pub fn type_def_of(&self, id: &LuaTypeDeclId) -> Option<TypeDef> {
        self.type_defs_of(id).iter().next().cloned()
    }

    /// Type declaration identity of `LuaType::Ref/Def` -> `TypeDefList`.
    pub fn type_defs_of(&self, id: &LuaTypeDeclId) -> TypeDefList {
        let (scope, name) = match id.get_id() {
            LuaTypeIdentifier::Global(name) => (TypeScope::Global, name.as_str()),
            LuaTypeIdentifier::Internal(workspace_id, name) => {
                (TypeScope::Internal(*workspace_id), name.as_str())
            }
            LuaTypeIdentifier::File(file_id, name) => (TypeScope::File(*file_id), name.as_str()),
        };
        self.analysis().type_defs_in_scope(scope, name)
    }

    /// Convert the `TypeDef` to a `LuaType::Ref`.
    pub fn type_def_ref(&self, def: &TypeDef) -> LuaType {
        LuaType::Ref(def.type_decl_id())
    }

    /// All definition locations of a named type (used by duplicate-type checks).
    pub fn type_def_locations(&self, name: &str) -> Vec<TypeDef> {
        self.analysis()
            .type_def_locations(self.view.file_id(), name)
    }

    /// Whether the doc syntax node is a bare name type (`Base`, not `Base<T>`).
    pub(crate) fn doc_type_is_bare_name(&self, syntax: LuaSyntaxId) -> bool {
        let Some(tree) = self.syntax_tree() else {
            return false;
        };
        let Some(node) = syntax.to_node_from_root(&tree.get_red_root()) else {
            return false;
        };
        matches!(LuaDocType::cast(node), Some(LuaDocType::Name(_)))
    }
    /// Whether a bare name reference has at least one generic parameter without a default (missing required arg -> any).
    pub(crate) fn type_has_missing_required_generic_args(&self, ty: &LuaType) -> bool {
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
    pub(crate) fn alias_cycle_any(&self, ty: &LuaType) -> Option<LuaType> {
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
    /// (table identity / constants / setmetatable / require special cases); falls back to semantic shell projection when VM fails.
    pub fn type_of_decl(&self, decl: &SemanticId) -> Option<LuaType> {
        let cache_file = match decl {
            SemanticId::Decl(key) => key.file_id,
            SemanticId::Member(key) => key.file_id,
            _ => self.view.file_id(),
        };
        if cache_file != self.file_id() {
            return self.model_for(cache_file).type_of_decl(decl);
        }
        let key = (cache_file, decl.clone());
        {
            let cache = self.cache.borrow();
            if let Some(entry) = cache.decl_type.get(&key) {
                return entry.get_inference_seed(&cache).cloned().flatten();
            }
        }
        let _query = cache::QueryCacheScope::new(&self.cache);
        self.cache
            .borrow_mut()
            .decl_type
            .insert(key.clone(), cache::CacheEntry::InProgress);
        let revision = self.cache.borrow().revision();
        let result = self.type_of_decl_impl(decl);
        let mut cache = self.cache.borrow_mut();
        let entry = cache::CacheEntry::finish(result.clone(), cache.is_complete(revision));
        cache.decl_type.insert(key, entry);
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
                if let Some(ty) = self
                    .analysis()
                    .decl_type_lua(self.view.file_id(), decl.id.clone())
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
                && let Some(facts) = self.file_facts()
                && let Some((signature, param_index)) =
                    facts.signature_and_param_index_of_decl(decl)
            {
                let closure_syntax = signature.closure_syntax;
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
                return Some(flow::apply_expr_casts(
                    self,
                    value_syntax,
                    LuaType::DocFunction(Rc::new(fun)),
                    flow::ExprCastMode::AsOnly,
                ));
            }
            // Initializer expression: VM inference; re-entrant declaration/expression queries
            // return `None` through `CacheEntry::InProgress`.
            // Index/member-access RHS needs flow-sensitive types to preserve narrowing for table literals/array lengths inside branches.
            if let Some(value_syntax) = decl.value_expr_syntax {
                let use_flow = self
                    .syntax_tree()
                    .and_then(|tree| value_syntax.to_node_from_root(&tree.get_red_root()))
                    .and_then(LuaIndexExpr::cast)
                    .is_some();
                let ty = if use_flow {
                    self.type_of_expr_at(value_syntax, value_syntax.get_range().start())
                } else {
                    flow::type_of_expr_with_cast(self, value_syntax, flow::ExprCastMode::AsOnly)
                };
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
        self.analysis().decl_type_lua(self.file_id(), decl.clone())
    }
    /// Parameter declarations may lose generic constraints in `type_of_decl`'s fallback path; re-attach them here.
    pub(crate) fn attach_param_decl_constraint(&self, decl: &SemanticId, ty: LuaType) -> LuaType {
        let Some(facts) = self.file_facts() else {
            return ty;
        };
        let Some(decl_info) = facts.decl_by_id(decl) else {
            return ty;
        };
        if !matches!(decl_info.kind, DeclKind::Param) {
            return ty;
        }
        let Some((signature, _)) = facts.signature_and_param_index_of_decl(decl_info) else {
            return ty;
        };
        self.attach_generic_constraints(ty, signature.closure_syntax)
    }
    /// Fills generic constraints from signature docs back into `TplRef` (so constraints survive parameter/return type projection).
    pub(crate) fn attach_generic_constraints(
        &self,
        ty: LuaType,
        closure_syntax: LuaSyntaxId,
    ) -> LuaType {
        let Some(signature) = self
            .file_facts()
            .and_then(|facts| facts.signature_by_closure(closure_syntax))
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
                    LuaType::TplRef(Rc::new(GenericTpl::new(
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
                    LuaType::TplRef(Rc::new(GenericTpl::new(
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
    pub(crate) fn infer_delayed_definition_type(&self, decl: &Decl) -> Option<LuaType> {
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
    pub(crate) fn infer_for_range_var(&self, decl: &Decl) -> Option<LuaType> {
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
            && let Some(def) = self.type_def_of(id)
        {
            if let Some(target) = self.alias_target(&def) {
                iter_ty = target;
            } else if let Some(syntax) = def.call_overloads.first() {
                let overload =
                    self.analysis()
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
            iter_ty = LuaType::DocFunction(Rc::new(doc));
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
    pub(crate) fn infer_standard_iter_slot(
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
            _ => self.view.file_id(),
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
            let Some(def) = self.type_def_of(id) else {
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
                for member_ref in self.members_of_owner(&owner).iter() {
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
                    LuaType::Union(Rc::new(LuaUnionType::from_vec(keys)))
                };
                let value_ty = if values.is_empty() {
                    LuaType::Unknown
                } else if values.len() == 1 {
                    values.pop()?
                } else {
                    LuaType::Union(Rc::new(LuaUnionType::from_vec(values)))
                };
                (key_ty, value_ty)
            }
            LuaType::Ref(id) | LuaType::Def(id) => {
                let def = self.type_def_of(id)?;
                let mut values: Vec<LuaType> = Vec::new();
                for member_ref in self.members_of_owner(&def.id).iter() {
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
                    LuaType::Union(Rc::new(LuaUnionType::from_vec(values)))
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
                slots.push(self.analysis().doc_type_lua(
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
    /// Infers `ipairs` slots from integer-index members, including inherited generic members.
    pub(crate) fn infer_iter_from_type_def(
        &self,
        id: &LuaTypeDeclId,
        index: usize,
    ) -> Option<LuaType> {
        let mut members = member::member_infos_all(self, &LuaType::Ref(id.clone()));
        // Filter before selecting overrides: an ordinary `integer` field and
        // an `[integer]` index signature have the same text key.
        let mut context = type_check::TypeCheckContext::new(self, false);
        let mut incomplete = false;
        members.retain(|info| {
            if matches!(info.key, LuaMemberKey::Integer(_)) {
                return true;
            }
            let Some(key) = &info.index_key_type else {
                return false;
            };
            match type_check::integer_index_applies(&mut context, key) {
                Ok(applies) => applies,
                Err(_) => {
                    incomplete = true;
                    false
                }
            }
        });
        if incomplete {
            self.cache.borrow_mut().mark_incomplete();
            return None;
        }
        let mut values: Vec<LuaType> = Vec::new();
        for info in member::dedup_by_key(members) {
            let value = info.typ;
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
            LuaType::Union(Rc::new(LuaUnionType::from_vec(values)))
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
            _ => self.view.file_id(),
        };
        if cache_file != self.file_id() {
            return self.model_for(cache_file).type_of_member(member);
        }
        let key = (cache_file, member.clone());
        {
            let cache = self.cache.borrow();
            if let Some(entry) = cache.member_type.get(&key) {
                return entry.get_inference_seed(&cache).cloned().flatten();
            }
        }
        let _query = cache::QueryCacheScope::new(&self.cache);
        self.cache
            .borrow_mut()
            .member_type
            .insert(key.clone(), cache::CacheEntry::InProgress);
        let revision = self.cache.borrow().revision();
        let result = self.type_of_member_impl(member);
        let mut cache = self.cache.borrow_mut();
        let entry = cache::CacheEntry::finish(result.clone(), cache.is_complete(revision));
        cache.member_type.insert(key, entry);
        result
    }
    pub(crate) fn type_of_member_impl(&self, member: &SemanticId) -> Option<LuaType> {
        let member_file = self.file_id();
        // Literal integers in `---@enum` table fields stay constant (`severity.ERROR` -> IntegerConst(1)).
        if let Some(enum_const) = self.enum_member_const(member_file, member) {
            return Some(enum_const);
        }
        // Runtime member assignments prefer VM projection: the TypeShell path does not perform higher-order generic call inference,
        // and would keep `E.foo_wrapped = wrap(function(a) ... end)` as `fun(...: T...)`.
        // Cycles/reentry are handled by `CacheEntry::InProgress` in the shared cache.
        // A declared `---@type` on the member wins over its value (`---@type table<string, string?>`
        // on `G.env = setmetatable({}, { __index = ... })`); the shell below lowers that doc type.
        if let Some(facts) = self.file_facts_of(member_file)
            && let Some(member_def) = facts.member_by_id(member)
            && !matches!(member_def.owner, SemanticId::TypeDef(_))
            && member_def.doc_type_syntax.is_none()
            && let Some(value_syntax) = member_def.value_syntax
        {
            // Table-literal fields keep the expression type from construction (`[key] = 1`, named fields
            // `foo = 123` keep `IntegerConst`; flow/type matching widens when `number` is needed).
            // Reentry is handled by `CacheEntry::InProgress` in the shared cache.
            let vm_ty = (|| {
                let tree = self.syntax_tree_of(member_file)?;
                let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
                let expr = LuaExpr::cast(node)?;
                let eval = if self.is_initializer_table_field(member, &member_def) {
                    flow::type_of_expr_with_cast(
                        self,
                        expr.get_syntax_id(),
                        flow::ExprCastMode::AsOnly,
                    )
                } else if matches!(expr, LuaExpr::CallExpr(_)) {
                    flow::type_of_expr_with_cast(self, value_syntax, flow::ExprCastMode::AsOnly)
                } else {
                    return None;
                };
                (!matches!(eval, LuaType::Unknown)).then_some(eval)
            })();
            if let Some(ty) = vm_ty {
                return Some(ty);
            }
        }

        let shell = self.analysis().member_type(member_file, member.clone())?;
        let generic_names = self.member_generic_names(member_file, member);
        let ty = self
            .analysis()
            .type_shell_lua_in(member_file, &shell, &generic_names);
        let ty = if let Some(facts) = self.file_facts_of(member_file)
            && let Some(member_def) = facts.member_by_id(member)
            && member_def.doc_type_syntax.is_none()
            && let Some(value_syntax) = member_def.value_syntax
        {
            flow::apply_expr_casts(self, value_syntax, ty, flow::ExprCastMode::AsOnly)
        } else {
            ty
        };
        Some(ty)
    }
    /// Whether this is a table-literal field in a declaration initializer.
    /// Such members keep the literal shape from table construction; explicit `t.x = v` is not an initializer field.
    /// Evaluate a member's value expression with the same-member reentry guard.
    /// Member resolution can recursively need this expression's type; without sharing the
    /// SemanticModel cycle guard, `member_info -> type_of_expr -> new InferVm -> member_info`
    /// can restart and eventually overflow the native stack.
    pub(crate) fn is_initializer_table_field(
        &self,
        _member: &SemanticId,
        member_def: &Member,
    ) -> bool {
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
    pub(crate) fn enum_member_const(
        &self,
        member_file: FileId,
        member: &SemanticId,
    ) -> Option<LuaType> {
        let facts = self.file_facts_of(member_file)?;
        let member_def = facts.member_by_id(member)?;
        let SemanticId::Member(table_key) = &member_def.owner else {
            return None;
        };
        let table_range = table_key.key_range;
        let decl = facts.decl_by_value_range(table_range)?;
        facts
            .type_def_by_owner_syntax(decl.owner_syntax?)
            .filter(|def| matches!(def.kind, TypeDefKind::Enum))?;
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
    pub(crate) fn member_generic_names(
        &self,
        member_file: FileId,
        member: &SemanticId,
    ) -> Vec<SmolStr> {
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
        let key = (self.file_id(), prefix_type.clone());
        if let Some(cached) = self.cache.borrow().member_infos.get(&key) {
            return cached.clone();
        }
        let revision = self.cache.borrow().revision();
        let value = member::member_infos(self, prefix_type);
        self.cache.publish_if_complete(revision, |cache| {
            cache.member_infos.insert(key, value.clone());
        });
        value
    }
    /// The specified-key member of a prefix type (first match).
    pub fn member_info(
        &self,
        prefix_type: &LuaType,
        key: &LuaMemberKey,
    ) -> Option<member::MemberInfo> {
        let cache_key = (self.file_id(), prefix_type.clone(), key.clone());
        if let Some(value) = self.cache.borrow().member_info.get(&cache_key) {
            return value.clone();
        }
        let revision = self.cache.borrow().revision();
        let value = member::member_info(self, prefix_type, key);
        self.cache.publish_if_complete(revision, |cache| {
            cache.member_info.insert(cache_key, value.clone());
        });
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
        substitute_named_refs(&substituted, &map)
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
            let candidate_set = match &resolved.member_id {
                Some(member_id) => {
                    infer::callable::CallableCandidateSet::from_prefix_type_for_member(
                        self, &prefix_ty, &key, member_id,
                    )
                }
                None => {
                    infer::callable::CallableCandidateSet::from_prefix_type(self, &prefix_ty, &key)
                }
            };
            if candidate_set.candidates().len() > 1 {
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
                let selected = candidate_set
                    .select(self, &args, call.is_colon_call(), Some(&prefix_ty))
                    .or_else(|| {
                        candidate_set.select_partial(
                            self,
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
                        &LuaType::DocFunction(Rc::new(fun)),
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
                    Rc::new(self.type_of_signature_in_file(member_file, value_syntax)?)
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
    pub(crate) fn hover_widen_const(ty: &LuaType) -> LuaType {
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
        self.analysis().is_global_deprecated(name)
    }
    pub(crate) fn is_deprecated_member_name(&self, name: &str) -> bool {
        self.analysis().is_deprecated_member_name(name)
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
                .get(&(self.view.file_id(), decl.clone(), start))
                .cloned();
            (Some(start), cached)
        } else {
            (None, None)
        };
        if let Some(cached) = cached {
            return self.sanitize_global_generic_decl(decl, cached);
        }
        let revision = self.cache.borrow().revision();
        let ty = flow::type_of_decl_at(self, decl, offset);
        if let Some(start) = start {
            self.cache.publish_if_complete(revision, |cache| {
                cache
                    .flow_decl
                    .insert((self.view.file_id(), decl.clone(), start), ty.clone());
            });
        }
        self.sanitize_global_generic_decl(decl, ty)
    }
    /// Global variables must not leak uninstantiated generic parameters from function bodies into the global scope
    /// (the `a` in `function f(x) a = x end` is unknown externally).
    pub(crate) fn sanitize_global_generic_decl(&self, decl: &SemanticId, ty: LuaType) -> LuaType {
        let SemanticId::Decl(key) = decl else {
            return ty;
        };
        let Some(decl) = self
            .analysis()
            .file_facts(key.file_id)
            .and_then(|facts| facts.decl_by_id(decl))
        else {
            return ty;
        };
        if !matches!(decl.kind, DeclKind::Global) {
            return ty;
        }
        let generic_names: HashSet<SmolStr> = self
            .analysis()
            .signatures(key.file_id)
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
        let key = (self.file_id(), member.clone(), offset);
        if let Some(cached) = self.cache.borrow().member_type_at.get(&key) {
            return cached.clone();
        }
        let revision = self.cache.borrow().revision();
        let ty = flow::type_of_member_at(self, member, offset);
        self.cache.publish_if_complete(revision, |cache| {
            cache.member_type_at.insert(key, ty.clone());
        });
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
    /// Assignment contract: widens inferred literals, preserves parameter and
    /// `---@type` contracts, and applies casts without conditional narrowing.
    pub fn type_of_decl_assign_target_at(&self, decl: &SemanticId, offset: TextSize) -> LuaType {
        self.type_of_decl_assign_target_at_impl(decl, offset)
    }
    pub(crate) fn type_of_decl_assign_target_at_impl(
        &self,
        decl: &SemanticId,
        offset: TextSize,
    ) -> LuaType {
        let target = flow::type_of_decl_assign_target_at(self, decl, offset);
        let inferred = self
            .file_facts()
            .and_then(|facts| facts.decl_by_id(decl))
            .is_some_and(|decl| {
                decl.doc_type_syntax.is_none() && !matches!(decl.kind, DeclKind::Param)
            });
        if inferred {
            widen_assignment_type(target)
        } else {
            target
        }
    }
    /// Member assignment contract, keeping explicit field annotations exact.
    pub(crate) fn member_assignment_type(&self, member: &SemanticId, target: LuaType) -> LuaType {
        let inferred = match member {
            SemanticId::Member(key) => self.file_facts_of(key.file_id),
            _ => None,
        }
        .and_then(|facts| facts.member_by_id(member))
        .is_some_and(|member| {
            member.doc_type_syntax.is_none() && !matches!(member.owner, SemanticId::TypeDef(_))
        });
        if inferred {
            widen_assignment_type(target)
        } else {
            target
        }
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
        // Flow reads must not turn a long binary/member chain into native
        // recursion. Evaluate each prefix/operand before its parent, as the VM does.
        enum Task {
            Enter(LuaExpr, TextSize),
            Finish(LuaExpr, TextSize, u64),
        }
        let file_id = self.view.file_id();
        let root_key = (file_id, expr_syntax, offset);
        {
            let cache = self.cache.borrow();
            if let Some(entry) = cache.expr_type_at.get(&root_key) {
                return entry.get(&cache).cloned().unwrap_or(LuaType::Unknown);
            }
        }
        let Some(expr) = self
            .syntax_tree()
            .and_then(|tree| expr_syntax.to_node_from_root(&tree.get_red_root()))
            .and_then(LuaExpr::cast)
        else {
            return self.type_of_expr(expr_syntax);
        };
        let _query = cache::QueryCacheScope::new(&self.cache);
        let mut pending = vec![Task::Enter(expr, offset)];
        while let Some(task) = pending.pop() {
            match task {
                Task::Enter(expr, at) => {
                    let key = (file_id, expr.get_syntax_id(), at);
                    if self.cache.borrow().expr_type_at.contains_key(&key) {
                        continue;
                    }
                    self.cache
                        .borrow_mut()
                        .expr_type_at
                        .insert(key, cache::CacheEntry::InProgress);
                    pending.push(Task::Finish(
                        expr.clone(),
                        at,
                        self.cache.borrow().revision(),
                    ));
                    match expr {
                        LuaExpr::BinaryExpr(binary) => {
                            if let Some((left, right)) = binary.get_exprs() {
                                let right_pos = right.get_position();
                                let left_pos = left.get_position();
                                pending.push(Task::Enter(right, right_pos));
                                pending.push(Task::Enter(left, left_pos));
                            }
                        }
                        LuaExpr::IndexExpr(index) => {
                            if let Some(prefix) = index.get_prefix_expr() {
                                pending.push(Task::Enter(prefix, at));
                            }
                        }
                        _ => {}
                    }
                }
                Task::Finish(expr, at, revision) => {
                    let syntax = expr.get_syntax_id();
                    let is_index = matches!(expr, LuaExpr::IndexExpr(_));
                    let ty = flow::type_of_expr_at_node(self, expr, at);
                    let ty = if is_index {
                        let ty = type_eval::expand_alias_generic(self, &ty);
                        type_eval::eval_conditionals(self, &ty)
                    } else {
                        ty
                    };
                    let mut cache = self.cache.borrow_mut();
                    let entry = cache::CacheEntry::finish(ty, cache.is_complete(revision));
                    cache.expr_type_at.insert((file_id, syntax, at), entry);
                }
            }
        }
        let cache = self.cache.borrow();
        cache
            .expr_type_at
            .get(&root_key)
            .and_then(|entry| entry.get(&cache))
            .cloned()
            .unwrap_or(LuaType::Unknown)
    }
    /// Operator-overload return type: when operand is a named type with `---@operator`, look up the return type by operator name.
    pub fn operator_type(&self, op_name: &str, operand: &LuaType) -> Option<LuaType> {
        let mut current = operand.clone();
        let mut visited = HashSet::new();
        loop {
            let decl_id = match &current {
                LuaType::Ref(id) | LuaType::Def(id) => id,
                _ => return None,
            };
            if !visited.insert(decl_id.clone()) {
                return None;
            }
            let def = self.type_def_of(decl_id)?;
            if def.kind == TypeDefKind::Alias {
                current = self.alias_target(&def)?;
                continue;
            }
            let facts = self.file_facts_of(def.file_id)?;
            let op = facts.operator_of(&def.id, op_name)?;
            let returns =
                self.analysis()
                    .doc_type_lua(def.file_id, op.returns, &def.generic_params);
            return (!matches!(returns, LuaType::Unknown)).then_some(returns);
        }
    }

    /// All type definitions for a scope + full name (cross-file; used by checkers / inheritance chains).
    pub fn type_defs_in_scope(&self, scope: TypeScope, full_name: &str) -> TypeDefList {
        self.analysis().type_defs_in_scope(scope, full_name)
    }

    // -- base type --

    /// 类型的直接父类型模版
    pub fn base_types(&self, id: &LuaTypeDeclId) -> Vec<LuaType> {
        if let Some(cached) = self
            .cache
            .borrow()
            .type_defs
            .get(id)
            .and_then(|cache| cache.base_types.as_ref())
        {
            return cached.clone();
        }
        let mut types = Vec::new();
        for def in self.type_defs_of(id).iter() {
            if def.kind != TypeDefKind::Class {
                continue;
            }
            let Some(tree) = self.syntax_tree_of(def.file_id) else {
                continue;
            };
            let root = tree.get_red_root();
            // 具名形参在各自声明中转成位置模板, 避免合并后混用泛型名称.
            let generic_bindings = def
                .generic_params
                .iter()
                .enumerate()
                .map(|(index, param)| {
                    (
                        param.name.to_string(),
                        LuaType::TplRef(Rc::new(GenericTpl::new(
                            GenericTplId::Type(index as u32),
                            param.name.clone(),
                            None,
                            None,
                            false,
                            None,
                        ))),
                    )
                })
                .collect::<HashMap<_, _>>();
            types.extend(def.base_types.iter().filter_map(|syntax| {
                let doc_type = LuaDocType::cast(syntax.to_node_from_root(&root)?)?;
                let mut ty = project_base_type(self, def, doc_type);
                if !generic_bindings.is_empty() {
                    ty = substitute_named_refs(&ty, &generic_bindings);
                }
                (!matches!(ty, LuaType::Unknown)).then_some(ty)
            }));
        }
        self.cache
            .borrow_mut()
            .type_defs
            .entry(id.clone())
            .or_default()
            .base_types = Some(types.clone());
        types
    }

    /// 查询类型的直接父类型中的具名声明, 按声明身份去重.
    pub fn base_type_defs(&self, id: &LuaTypeDeclId) -> Vec<TypeDef> {
        let mut pending = self.base_types(id);
        pending.reverse();
        let mut defs = Vec::new();
        let mut seen = HashSet::new();
        while let Some(ty) = pending.pop() {
            let id = match ty {
                LuaType::Ref(id) | LuaType::Def(id) => id,
                LuaType::Generic(generic) => generic.get_base_type_id(),
                LuaType::Intersection(intersection) => {
                    pending.extend(intersection.get_types().iter().rev().cloned());
                    continue;
                }
                LuaType::Union(union) => {
                    pending.extend(union.into_vec().into_iter().rev());
                    continue;
                }
                _ => continue,
            };
            if let Some(base_def) = self.type_def_of(&id)
                && seen.insert(base_def.id.clone())
            {
                defs.push(base_def);
            }
        }
        defs
    }
}

fn project_base_type(model: &SemanticModel<'_>, def: &TypeDef, doc_type: LuaDocType) -> LuaType {
    if let LuaDocType::Binary(binary) = &doc_type {
        let Some((left, right)) = binary.get_types() else {
            return LuaType::Unknown;
        };
        match binary.get_op_token().map(|op| op.get_op()) {
            Some(LuaTypeBinaryOperator::Intersection) => {
                return LuaType::Intersection(Rc::new(LuaIntersectionType::new(vec![
                    project_base_type(model, def, left),
                    project_base_type(model, def, right),
                ])));
            }
            Some(LuaTypeBinaryOperator::Union) => {
                return LuaType::Union(Rc::new(LuaUnionType::from_vec(vec![
                    project_base_type(model, def, left),
                    project_base_type(model, def, right),
                ])));
            }
            _ => {}
        }
    }
    model.doc_type_lua_in(def.file_id, doc_type.get_syntax_id(), &def.generic_params)
}

fn widen_assignment_type(ty: LuaType) -> LuaType {
    match ty {
        LuaType::Union(union) => {
            LuaType::from_vec(union.iter().cloned().map(widen_assignment_type).collect())
        }
        LuaType::FloatConst(_) => LuaType::Number,
        _ => infer::vm::widen_variadic_const(&ty),
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
                _ => LuaType::Union(Rc::new(LuaUnionType::from_vec(types))),
            }
        }
        LuaType::Nil => LuaType::Unknown,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use crate::{LuaGenericType, LuaMemberKey, LuaType, LuaTypeDeclId, VirtualWorkspace};

    fn named(name: &str) -> LuaType {
        LuaType::Ref(LuaTypeDeclId::global(name))
    }

    fn generic(name: &str, params: Vec<LuaType>) -> LuaType {
        LuaType::Generic(Rc::new(LuaGenericType::new(
            LuaTypeDeclId::global(name),
            params,
        )))
    }

    #[test]
    fn base_types_use_the_declaring_file_scope() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file("library.lua", "---@namespace Library\n---@class Parent<T>");
        let declaration_file = ws.def_file(
            "declaration.lua",
            r#"
            ---@namespace Library
            ---@class (file) Private<T>
            ---@class Child: Parent<string>, Library.Private<number>
            "#,
        );
        let private_file = ws.def_file(
            "private.lua",
            r#"
            ---@class (file) Private<T>
            ---@class PrivateChild: Private<number>
            "#,
        );
        let consumer_file = ws.def_file(
            "consumer.lua",
            r#"
            ---@namespace Consumer
            ---@class Parent<T>
            ---@class Private<T>
            "#,
        );
        let model = ws.analysis.semantic_model(consumer_file);
        let child = model
            .resolve_type_def_in(declaration_file, "Child")
            .unwrap();
        let private_id = LuaTypeDeclId::file(declaration_file, "Library.Private");
        assert_eq!(
            model.base_types(&child.type_decl_id()),
            vec![
                generic("Library.Parent", vec![LuaType::String]),
                LuaType::Generic(Rc::new(LuaGenericType::new(
                    private_id.clone(),
                    vec![LuaType::Number]
                ))),
            ],
        );
        let bases = model.base_type_defs(&child.type_decl_id());
        assert_eq!(bases.len(), 2);
        assert_eq!(model.type_def_ref(&bases[0]), named("Library.Parent"));
        assert_eq!(model.type_def_ref(&bases[1]), LuaType::Ref(private_id));
        let private_child = model
            .resolve_type_def_in(private_file, "PrivateChild")
            .unwrap();
        assert_eq!(
            model.base_types(&private_child.type_decl_id()),
            vec![LuaType::Generic(Rc::new(LuaGenericType::new(
                LuaTypeDeclId::file(private_file, "Private"),
                vec![LuaType::Number],
            )))],
        );
    }

    #[test]
    fn base_types_feed_existing_member_substitution() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@class Parent<T>
            ---@field value T
            ---@class Fixed<T>: Parent<string>
            ---@class Swapped<A, B>: Parent<B>
            "#,
        );
        let model = ws.analysis.semantic_model(file_id);
        let key = LuaMemberKey::Name("value".into());
        for ty in [
            generic("Fixed", vec![LuaType::Number]),
            generic("Swapped", vec![LuaType::Number, LuaType::String]),
        ] {
            assert_eq!(model.member_info(&ty, &key).unwrap().typ, LuaType::String);
            let infos = model.member_infos(&ty);
            assert_eq!(
                infos.iter().find(|info| info.key == key).unwrap().typ,
                LuaType::String
            );
        }
        let fixed = model.resolve_type_def("Fixed").unwrap();
        assert_eq!(
            model.analysis().member_keys_of_type(file_id, fixed.id),
            vec!["value"]
        );
    }
}
