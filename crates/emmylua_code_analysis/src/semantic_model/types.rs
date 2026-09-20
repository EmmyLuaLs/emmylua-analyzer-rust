use super::prelude::*;
use crate::semantic_db::TypeDefList;

impl<'db> SemanticModel<'db> {
    // -- Types --

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
        let key = (cache_file, decl.clone());
        match self.cache.borrow().decl_type.get(&key) {
            Some(cache::CacheEntry::Ready(cached)) => return cached.clone(),
            Some(cache::CacheEntry::InProgress) => return None,
            None => {}
        }
        self.cache
            .borrow_mut()
            .decl_type
            .insert(key.clone(), cache::CacheEntry::InProgress);
        let result = self.type_of_decl_impl(decl);
        self.cache
            .borrow_mut()
            .decl_type
            .insert(key, cache::CacheEntry::Ready(result.clone()));
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
                return Some(LuaType::DocFunction(Arc::new(fun)));
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
                    self.type_of_expr(value_syntax)
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
        // Cross-file declarations: query by the file carried in the declaration (the Decl key includes file_id).
        // Lazy type execution: `decl_type_lua` is keyed by the declaring file, entering only defining files, not consumer models.
        let decl_file = match decl {
            SemanticId::Decl(key) => key.file_id,
            _ => self.view.file_id(),
        };
        if decl_file != self.view.file_id()
            && let foreign_model = SemanticModel::new(self.db(), decl_file)
            && let Some(foreign_ty) = foreign_model.type_of_decl(decl)
        {
            return Some(foreign_ty);
        }
        self.analysis().decl_type_lua(decl_file, decl.clone())
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
            && let Some(def) = member::type_def_of(self, id)
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
    /// Infers `ipairs` slots directly from integer-index members of named types (does not depend on global pairs/ipairs signatures).
    pub(crate) fn infer_iter_from_type_def(
        &self,
        id: &LuaTypeDeclId,
        index: usize,
    ) -> Option<LuaType> {
        let def = member::type_def_of(self, id)?;
        let mut values: Vec<LuaType> = Vec::new();
        for member_ref in self.members_of_owner(&def.id).iter() {
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
            _ => self.view.file_id(),
        };
        let key = (cache_file, member.clone());
        match self.cache.borrow().member_type.get(&key) {
            Some(cache::CacheEntry::Ready(cached)) => return cached.clone(),
            Some(cache::CacheEntry::InProgress) => return None,
            None => {}
        }
        self.cache
            .borrow_mut()
            .member_type
            .insert(key.clone(), cache::CacheEntry::InProgress);
        let result = self.type_of_member_impl(member);
        self.cache
            .borrow_mut()
            .member_type
            .insert(key, cache::CacheEntry::Ready(result.clone()));
        result
    }
    pub(crate) fn type_of_member_impl(&self, member: &SemanticId) -> Option<LuaType> {
        // Members are keyed by declaring file: take the file from the Member key.
        let member_file = match member {
            SemanticId::Member(key) => key.file_id,
            _ => self.view.file_id(),
        };
        // Cross-file members are uniformly delegated to the member file's own model, keeping VM replay and cycle guards on the same model.
        if member_file != self.view.file_id() {
            let foreign_model = self.model_for(member_file);
            return foreign_model.type_of_member(member);
        }
        // Literal integers in `---@enum` table fields stay constant (`severity.ERROR` -> IntegerConst(1)).
        if let Some(enum_const) = self.enum_member_const(member_file, member) {
            return Some(enum_const);
        }
        // Runtime member assignments prefer VM projection: the TypeShell path does not perform higher-order generic call inference,
        // and would keep `E.foo_wrapped = wrap(function(a) ... end)` as `fun(...: T...)`.
        // Cycles/reentry are handled by `CacheEntry::InProgress` in the per-model cache.
        if let Some(facts) = self.file_facts_of(member_file)
            && let Some(member_def) = facts.member_by_id(member)
            && !matches!(member_def.owner, SemanticId::TypeDef(_))
            && let Some(value_syntax) = member_def.value_syntax
        {
            // Table-literal fields keep the expression type from construction (`[key] = 1`, named fields
            // `foo = 123` keep `IntegerConst`; flow/type matching widens when `number` is needed).
            // Reentry is handled by `CacheEntry::InProgress` in the per-model cache.
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

        let shell = self.analysis().member_type(member_file, member.clone())?;
        let generic_names = self.member_generic_names(member_file, member);
        let ty = self
            .analysis()
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
        let ty = flow::type_of_decl_at(self, decl, offset);
        if let Some(start) = start {
            self.cache
                .borrow_mut()
                .flow_decl
                .insert((self.view.file_id(), decl.clone(), start), ty.clone());
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
        let cache_file = match member {
            SemanticId::Member(key) => key.file_id,
            _ => self.view.file_id(),
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
        let file_id = self.view.file_id();
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
            .analysis()
            .doc_type_lua(def.file_id, op.returns, &def.generic_params);
        (!matches!(returns, LuaType::Unknown)).then_some(returns)
    }
    /// All type definitions for a scope + full name (cross-file; used by checkers / inheritance chains).
    pub fn type_defs_in_scope(&self, scope: TypeScope, full_name: &str) -> TypeDefList {
        self.analysis().type_defs_in_scope(scope, full_name)
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
