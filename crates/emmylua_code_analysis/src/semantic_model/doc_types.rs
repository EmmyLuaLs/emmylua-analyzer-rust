use super::prelude::*;
use crate::semantic_model::infer::vm::{InferVm, bind_signature_generics};
use crate::{LuaArrayType, LuaConditionalType, LuaMappedType};

/// 注解投影的定义文件、泛型声明及其已解析元数据.
pub(crate) struct DocTypeScope<'a> {
    pub(crate) file_id: FileId,
    pub(crate) params: &'a [DocGenericParam],
    pub(crate) templates: Vec<GenericTpl>,
}

impl<'db> SemanticModel<'db> {
    /// Alias target type (after projection; generic parameter references keep `TplRef`, and instantiation is substituted by the caller).
    pub fn alias_target(&self, def: &TypeDef) -> Option<LuaType> {
        {
            let cache = self.cache.borrow();
            if let Some(entry) = cache.alias_targets.get(&def.id) {
                return entry.get(&cache).cloned().flatten();
            }
        }
        let _query = cache::QueryCacheScope::new(&self.cache);
        // Projection can re-enter semantic queries. Publish the guard before
        // starting it, and release the cache borrow before calling the query.
        self.cache
            .borrow_mut()
            .alias_targets
            .insert(def.id.clone(), cache::CacheEntry::InProgress);
        let revision = self.cache.borrow().revision();
        let result = self.alias_target_uncached(def);
        let mut cache = self.cache.borrow_mut();
        let entry = cache::CacheEntry::finish(result.clone(), cache.is_complete(revision));
        cache.alias_targets.insert(def.id.clone(), entry);
        result
    }
    pub(crate) fn alias_target_uncached(&self, def: &TypeDef) -> Option<LuaType> {
        let syntax = def.alias_type?;
        let mut ty = self.project_doc_type(def.file_id, syntax, &def.generic_params);
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
                let call = LuaType::Call(Rc::new(LuaAliasCallType::new(
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
                ty = LuaType::Mapped(Rc::new(LuaMappedType::new(
                    param,
                    mapped.value.clone(),
                    mapped.is_readonly,
                    mapped.is_optional,
                )));
            }
        }
        // `unknown` is a valid alias target. Do not discard it just because Unknown is also
        // used as the failure sentinel for unresolved type names.
        if matches!(ty, LuaType::Unknown)
            && let Some(tree) = self.syntax_tree_of(def.file_id)
            && let Some(node) = syntax.to_node_from_root(&tree.get_red_root())
            && let Some(LuaDocType::Name(name)) = LuaDocType::cast(node)
            && name.get_name_text().as_deref() == Some("unknown")
        {
            return Some(LuaType::Unknown);
        }
        (!matches!(ty, LuaType::Unknown)).then_some(ty)
    }
    /// Attempts to concretely evaluate `T[K]` (K is a literal / union / keyof / alias).
    /// Unified delegation to the semantic-layer TypeEvaluator.
    pub(crate) fn try_eval_index_access(&self, base: &LuaType, key: &LuaType) -> Option<LuaType> {
        type_eval::eval_index_access(self, base, key)
    }
    /// Doc type node (by syntax location, in this file) -> projected `LuaType` (consumed by checkers such as cast).
    pub fn doc_type_lua(&self, type_syntax: LuaSyntaxId) -> LuaType {
        self.doc_type_lua_in(self.view.file_id(), type_syntax, &[])
    }
    /// Doc type projection for a specified file + generic context (unified entry point).
    pub fn doc_type_lua_in(
        &self,
        file_id: FileId,
        type_syntax: LuaSyntaxId,
        generics: &[DocGenericParam],
    ) -> LuaType {
        self.analysis().doc_type_lua(file_id, type_syntax, generics)
    }
    /// Builds `GenericTpl` with full metadata (constraint/default/is_const) from `DocGenericParam`.
    /// All signature projection paths go through here so constraints/defaults are not lost at different call sites.
    pub fn generic_tpls_with_metadata(
        &self,
        file_id: FileId,
        params: &[DocGenericParam],
    ) -> Vec<GenericTpl> {
        if params.is_empty() {
            return Vec::new();
        }

        params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let constraint = param.constraint.map(|syntax| {
                    let ty = self.analysis().doc_type_lua(file_id, syntax, params);
                    if matches!(ty, LuaType::Unknown | LuaType::Table) {
                        let rich = self.project_doc_type(file_id, syntax, &[]);
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
                    .map(|syntax| self.project_doc_type(file_id, syntax, &[]));
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

    /// Prepares the generic metadata shared by this projection scope.
    pub(crate) fn doc_type_scope<'a>(
        &self,
        file_id: FileId,
        params: &'a [DocGenericParam],
    ) -> DocTypeScope<'a> {
        DocTypeScope {
            file_id,
            params,
            templates: self.generic_tpls_with_metadata(file_id, params),
        }
    }

    /// Keep declared return values in annotation order.
    pub(crate) fn project_return_types_in_scope(
        &self,
        returns: &[LuaSyntaxId],
        scope: &DocTypeScope<'_>,
    ) -> LuaType {
        match returns {
            [] => LuaType::Unknown,
            [single] => self.project_doc_type_in_scope(*single, scope),
            _ => LuaType::Variadic(Rc::new(VariadicType::Multi(
                returns
                    .iter()
                    .map(|syntax| self.project_doc_type_in_scope(*syntax, scope))
                    .collect(),
            ))),
        }
    }

    /// Projects a complete doc type with the given file and generic scope.
    pub fn project_doc_type(
        &self,
        file_id: FileId,
        type_syntax: LuaSyntaxId,
        generics: &[DocGenericParam],
    ) -> LuaType {
        let scope = self.doc_type_scope(file_id, generics);
        self.project_doc_type_in_scope(type_syntax, &scope)
    }

    /// Projects a doc type inside an already prepared generic scope.
    pub(crate) fn project_doc_type_in_scope(
        &self,
        type_syntax: LuaSyntaxId,
        scope: &DocTypeScope,
    ) -> LuaType {
        let file_id = scope.file_id;
        let generics = scope.params;
        let Some(tree) = self.syntax_tree_of(file_id) else {
            return self.analysis().doc_type_lua(file_id, type_syntax, generics);
        };
        let Some(node) = type_syntax.to_node_from_root(&tree.get_red_root()) else {
            return self.analysis().doc_type_lua(file_id, type_syntax, generics);
        };

        let Some(doc_ty) = LuaDocType::cast(node) else {
            return self.analysis().doc_type_lua(file_id, type_syntax, generics);
        };
        self.project_doc_type_node(&doc_ty, scope)
    }

    /// Fully projects a doc type node within its owning generic scope.
    pub(crate) fn project_doc_type_node(
        &self,
        doc_ty: &LuaDocType,
        scope: &DocTypeScope,
    ) -> LuaType {
        let ty = self.project_doc_type_impl(doc_ty, scope);
        bind_signature_generics(&ty, &scope.templates)
    }

    /// 投影组合类型时保留声明所属的泛型作用域.
    fn project_doc_type_impl(&self, doc_ty: &LuaDocType, scope: &DocTypeScope) -> LuaType {
        let file_id = scope.file_id;
        let generics = scope.params;
        match doc_ty {
            LuaDocType::Array(array) => array
                .get_type()
                .map(|base| {
                    LuaType::Array(Rc::new(LuaArrayType::from_base_type(
                        self.project_doc_type_impl(&base, scope),
                    )))
                })
                .unwrap_or_else(|| self.analysis().doc_type_node_lua(file_id, doc_ty, generics)),
            LuaDocType::Tuple(tuple) => {
                let types = tuple
                    .get_types()
                    .map(|item| self.project_doc_type_impl(&item, scope))
                    .collect();
                LuaType::Tuple(Rc::new(LuaTupleType::new(
                    types,
                    LuaTupleStatus::DocResolve,
                )))
            }
            LuaDocType::Func(func) => LuaType::DocFunction(Rc::new(
                InferVm::new(self, &[]).doc_func_from_ast(func, scope, false),
            )),
            LuaDocType::Conditional(conditional) => {
                let mut state = ConditionalInferState::default();
                self.doc_type_lua_rich_conditional_in_state(conditional, scope, &mut state)
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
                            key_ty = self.project_doc_type_impl(&constraint, scope);
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
                                .map(|t| self.project_doc_type_impl(&t, scope))
                                .unwrap_or(LuaType::Unknown);
                            let key = types
                                .next()
                                .map(|t| self.project_doc_type_impl(&t, scope))
                                .unwrap_or(LuaType::Unknown);
                            LuaType::Call(Rc::new(LuaAliasCallType::new(
                                LuaAliasCallKind::Index,
                                vec![base, key],
                            )))
                        }
                        _ => self.project_doc_type_impl(&ty, scope),
                    })
                    .unwrap_or(LuaType::Unknown);
                if param_name.is_empty() {
                    return LuaType::Unknown;
                }
                let param = (
                    GenericTplId::Mapped(mapped.get_range().start().into()),
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
                LuaType::Mapped(Rc::new(LuaMappedType::new(
                    param,
                    value_ty,
                    mapped.is_readonly(),
                    mapped.is_optional(),
                )))
            }
            LuaDocType::Object(object) => {
                let mut fields = HashMap::new();
                let mut index_access = Vec::new();
                for field in object.get_fields() {
                    let Some(key) = field.get_field_key() else {
                        continue;
                    };
                    let Some(value_ty) = field.get_type() else {
                        continue;
                    };
                    let mut value_ty = self.project_doc_type_impl(&value_ty, scope);
                    if field.is_nullable() && !value_ty.is_nullable() {
                        value_ty = LuaType::from_vec(vec![value_ty, LuaType::Nil]);
                    }
                    match key {
                        LuaDocObjectFieldKey::Name(name) => {
                            fields.insert(
                                LuaMemberKey::Name(SmolStr::new(name.get_name_text())),
                                value_ty,
                            );
                        }
                        LuaDocObjectFieldKey::String(str) => {
                            fields.insert(
                                LuaMemberKey::Name(SmolStr::new(str.get_value())),
                                value_ty,
                            );
                        }
                        LuaDocObjectFieldKey::Integer(num) => {
                            if let NumberResult::Int(i) = num.get_number_value() {
                                fields.insert(LuaMemberKey::Integer(i), value_ty);
                            }
                        }
                        LuaDocObjectFieldKey::Type(key_ty) => {
                            index_access
                                .push((self.project_doc_type_impl(&key_ty, scope), value_ty));
                        }
                    };
                }
                LuaType::Object(Rc::new(LuaObjectType::new_with_fields(
                    fields,
                    index_access,
                )))
            }
            LuaDocType::IndexAccess(index) => {
                let mut types = index.syntax().children().filter_map(LuaDocType::cast);
                let base = types
                    .next()
                    .map(|ty| self.project_doc_type_impl(&ty, scope))
                    .unwrap_or(LuaType::Unknown);
                let key = types
                    .next()
                    .map(|ty| self.project_doc_type_impl(&ty, scope))
                    .unwrap_or(LuaType::Unknown);
                if let Some(evaluated) = self.try_eval_index_access(&base, &key) {
                    return evaluated;
                }
                LuaType::Call(Rc::new(LuaAliasCallType::new(
                    LuaAliasCallKind::Index,
                    vec![base, key],
                )))
            }
            LuaDocType::Binary(binary) => {
                let Some((left, right)) = binary.get_types() else {
                    return self.analysis().doc_type_node_lua(file_id, doc_ty, generics);
                };
                let left_ty = self.project_doc_type_impl(&left, scope);
                let right_ty = self.project_doc_type_impl(&right, scope);
                match binary.get_op_token().map(|op| op.get_op()) {
                    Some(LuaTypeBinaryOperator::Intersection) => {
                        LuaType::Intersection(Rc::new(LuaIntersectionType::new(vec![
                            left_ty, right_ty,
                        ])))
                    }
                    Some(LuaTypeBinaryOperator::Union) => {
                        let mut types = Vec::new();
                        for ty in [left_ty, right_ty] {
                            match ty {
                                LuaType::Union(union) => types.extend(union.iter().cloned()),
                                other => types.push(other),
                            }
                        }
                        LuaType::union(types)
                    }
                    _ => self.analysis().doc_type_node_lua(file_id, doc_ty, generics),
                }
            }
            LuaDocType::Nullable(nullable) => nullable
                .get_type()
                .map(|inner| {
                    let ty = self.project_doc_type_impl(&inner, scope);
                    LuaType::optional(ty)
                })
                .unwrap_or(LuaType::Unknown),
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
                _ => self.analysis().doc_type_node_lua(file_id, doc_ty, generics),
            },
            LuaDocType::Unary(unary) => {
                if !unary
                    .get_op_token()
                    .is_some_and(|op| op.get_op() == LuaTypeUnaryOperator::Keyof)
                {
                    return self.analysis().doc_type_node_lua(file_id, doc_ty, generics);
                }
                let Some(target) = unary.get_type() else {
                    return self.analysis().doc_type_node_lua(file_id, doc_ty, generics);
                };
                let LuaDocType::Name(name_ty) = &target else {
                    return self.analysis().doc_type_node_lua(file_id, doc_ty, generics);
                };
                let Some(name) = name_ty.get_name_text() else {
                    return self.analysis().doc_type_node_lua(file_id, doc_ty, generics);
                };
                let Some(def) = self.resolve_type_def_in(file_id, &name) else {
                    return self.analysis().doc_type_node_lua(file_id, doc_ty, generics);
                };
                let members: Vec<LuaType> = self
                    .member_infos(&self.type_def_ref(&def))
                    .into_iter()
                    .filter_map(|member| member.key.to_index_type())
                    .collect();
                if members.is_empty() {
                    self.analysis().doc_type_node_lua(file_id, doc_ty, generics)
                } else {
                    LuaType::union(members)
                }
            }
            LuaDocType::MultiLineUnion(multi) => {
                let mut types = Vec::new();
                for field in multi.get_fields() {
                    let Some(ty) = field.get_type() else {
                        continue;
                    };
                    let ty = self.project_doc_type_impl(&ty, scope);
                    if matches!(ty, LuaType::Unknown) {
                        continue;
                    }
                    match ty {
                        LuaType::Union(union) => types.extend(union.iter().cloned()),
                        other => types.push(other),
                    }
                }
                if types.is_empty() {
                    self.analysis().doc_type_node_lua(file_id, doc_ty, generics)
                } else {
                    LuaType::union(types)
                }
            }
            LuaDocType::Variadic(variadic) => variadic
                .get_type()
                .map(|inner| {
                    let base = self.project_doc_type_impl(&inner, scope);
                    // Rich projection without generic context projects the T in `T...` as `Ref("T")`,
                    // leaving it to the shell layer to handle the original context so the variadic base type keeps TplRef.
                    if matches!(base, LuaType::Unknown | LuaType::Ref(_) | LuaType::Def(_)) {
                        self.analysis().doc_type_node_lua(file_id, doc_ty, generics)
                    } else {
                        LuaType::Variadic(VariadicType::Base(base).into())
                    }
                })
                .unwrap_or_else(|| self.analysis().doc_type_node_lua(file_id, doc_ty, generics)),
            LuaDocType::Generic(generic) => {
                let Some(name) = generic.get_name_type().and_then(|n| n.get_name_text()) else {
                    return self.analysis().doc_type_node_lua(file_id, doc_ty, generics);
                };
                let params: Vec<LuaType> = generic
                    .get_generic_types()
                    .map(|list| {
                        list.get_types()
                            .map(|arg| self.project_doc_type_impl(&arg, scope))
                            .collect()
                    })
                    .unwrap_or_default();
                self.analysis().project_generic_type(file_id, &name, params)
            }
            _ => self.analysis().doc_type_node_lua(file_id, doc_ty, generics),
        }
    }
    /// Common implementation of conditional type projection. `state` can come from an outer conditional, giving nested `infer`
    /// globally unique IDs, and inner conditionals can see `infer` references bound by outer conditionals.
    fn doc_type_lua_rich_conditional_in_state(
        &self,
        conditional: &LuaDocConditionalType,
        scope: &DocTypeScope,
        state: &mut ConditionalInferState,
    ) -> LuaType {
        let Some((condition, when_true, when_false)) = conditional.get_types() else {
            return LuaType::Unknown;
        };
        state.enter_scope();
        let condition_ty = self.doc_type_lua_rich_scoped(&condition, scope, state);
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
        let true_type = self.doc_type_lua_rich_scoped(&when_true, scope, state);
        let infer_params = state.leave_scope();
        let false_type = self.doc_type_lua_rich_scoped(&when_false, scope, state);

        LuaType::Conditional(Rc::new(LuaConditionalType::new(
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
        doc_ty: &LuaDocType,
        scope: &DocTypeScope,
        state: &mut ConditionalInferState,
    ) -> LuaType {
        let file_id = scope.file_id;

        match doc_ty {
            LuaDocType::Infer(infer) => {
                let Some(name) = infer.get_generic_decl_name_text() else {
                    return LuaType::Unknown;
                };
                match state.declare(&name) {
                    Some(tpl) => LuaType::TplRef(Rc::new(tpl)),
                    None => LuaType::Unknown,
                }
            }
            LuaDocType::Name(name) => {
                if let Some(name) = name.get_name_text() {
                    if let Some(tpl) = state.find_ref(&name) {
                        return LuaType::TplRef(Rc::new(tpl));
                    }
                }
                self.project_doc_type_impl(doc_ty, scope)
            }
            LuaDocType::Binary(binary) => {
                let Some((left, right)) = binary.get_types() else {
                    return self
                        .analysis()
                        .doc_type_node_lua(file_id, doc_ty, scope.params);
                };
                let left_ty = self.doc_type_lua_rich_scoped(&left, scope, state);
                let right_ty = self.doc_type_lua_rich_scoped(&right, scope, state);
                match binary.get_op_token().map(|op| op.get_op()) {
                    Some(LuaTypeBinaryOperator::Union) => {
                        let mut types = Vec::new();
                        for ty in [left_ty, right_ty] {
                            match ty {
                                LuaType::Union(union) => types.extend(union.iter().cloned()),
                                other => types.push(other),
                            }
                        }
                        LuaType::union(types)
                    }
                    Some(LuaTypeBinaryOperator::Intersection) => {
                        LuaType::Intersection(Rc::new(LuaIntersectionType::new(vec![
                            left_ty, right_ty,
                        ])))
                    }
                    Some(LuaTypeBinaryOperator::Extends) => LuaType::Call(Rc::new(
                        LuaAliasCallType::new(LuaAliasCallKind::Extends, vec![left_ty, right_ty]),
                    )),
                    _ => self
                        .analysis()
                        .doc_type_node_lua(file_id, doc_ty, scope.params),
                }
            }
            LuaDocType::Object(object) => {
                let mut fields = HashMap::new();
                let mut index_access = Vec::new();
                for field in object.get_fields() {
                    let Some(key) = field.get_field_key() else {
                        continue;
                    };
                    let Some(value_ty) = field.get_type() else {
                        continue;
                    };
                    let mut value_ty = self.doc_type_lua_rich_scoped(&value_ty, scope, state);
                    if field.is_nullable() && !value_ty.is_nullable() {
                        value_ty = LuaType::from_vec(vec![value_ty, LuaType::Nil]);
                    }
                    match key {
                        LuaDocObjectFieldKey::Name(name) => {
                            fields.insert(
                                LuaMemberKey::Name(SmolStr::new(name.get_name_text())),
                                value_ty,
                            );
                        }
                        LuaDocObjectFieldKey::String(str) => {
                            fields.insert(
                                LuaMemberKey::Name(SmolStr::new(str.get_value())),
                                value_ty,
                            );
                        }
                        LuaDocObjectFieldKey::Integer(num) => {
                            if let NumberResult::Int(i) = num.get_number_value() {
                                fields.insert(LuaMemberKey::Integer(i), value_ty);
                            }
                        }
                        LuaDocObjectFieldKey::Type(key_ty) => {
                            index_access.push((
                                self.doc_type_lua_rich_scoped(&key_ty, scope, state),
                                value_ty,
                            ));
                        }
                    };
                }
                LuaType::Object(Rc::new(LuaObjectType::new_with_fields(
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
                        .map(|t| self.doc_type_lua_rich_scoped(&t, scope, state))
                        .unwrap_or(LuaType::Unknown);
                    if param.is_nullable() {
                        ty = LuaType::make_nullable(ty);
                    }
                    params.push((name, Some(ty)));
                }
                let ret = match func.get_return_type_list() {
                    Some(list) => {
                        let mut types = Vec::new();
                        for ret in list.get_return_type_list() {
                            if let (_, Some(ret_type)) = ret.get_name_and_type() {
                                types.push(self.doc_type_lua_rich_scoped(&ret_type, scope, state));
                            }
                        }
                        if types.is_empty() {
                            LuaType::Unknown
                        } else if types.len() == 1 {
                            types.pop().unwrap_or(LuaType::Unknown)
                        } else {
                            LuaType::Variadic(Rc::new(VariadicType::Multi(types)))
                        }
                    }
                    None => LuaType::Unknown,
                };
                LuaType::DocFunction(Rc::new(LuaFunctionType::new(
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
                    let base = self.doc_type_lua_rich_scoped(&inner, scope, state);
                    if matches!(base, LuaType::Unknown | LuaType::Ref(_) | LuaType::Def(_)) {
                        self.analysis()
                            .doc_type_node_lua(file_id, doc_ty, scope.params)
                    } else {
                        LuaType::Variadic(VariadicType::Base(base).into())
                    }
                })
                .unwrap_or_else(|| {
                    self.analysis()
                        .doc_type_node_lua(file_id, doc_ty, scope.params)
                }),
            LuaDocType::Generic(generic) => {
                let Some(name) = generic.get_name_type().and_then(|n| n.get_name_text()) else {
                    return self
                        .analysis()
                        .doc_type_node_lua(file_id, doc_ty, scope.params);
                };
                let params: Vec<LuaType> = generic
                    .get_generic_types()
                    .map(|list| {
                        list.get_types()
                            .map(|arg| self.doc_type_lua_rich_scoped(&arg, scope, state))
                            .collect()
                    })
                    .unwrap_or_default();
                self.analysis().project_generic_type(file_id, &name, params)
            }
            LuaDocType::Array(array) => array
                .get_type()
                .map(|base| {
                    LuaType::Array(Rc::new(LuaArrayType::from_base_type(
                        self.doc_type_lua_rich_scoped(&base, scope, state),
                    )))
                })
                .unwrap_or_else(|| {
                    self.analysis()
                        .doc_type_node_lua(file_id, doc_ty, scope.params)
                }),
            LuaDocType::Tuple(tuple) => {
                let types: Vec<LuaType> = tuple
                    .get_types()
                    .map(|item| self.doc_type_lua_rich_scoped(&item, scope, state))
                    .collect();
                LuaType::Tuple(Rc::new(LuaTupleType::new(
                    types,
                    LuaTupleStatus::DocResolve,
                )))
            }
            LuaDocType::Conditional(conditional) => {
                self.doc_type_lua_rich_conditional_in_state(conditional, scope, state)
            }
            _ => self.project_doc_type_impl(doc_ty, scope),
        }
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
