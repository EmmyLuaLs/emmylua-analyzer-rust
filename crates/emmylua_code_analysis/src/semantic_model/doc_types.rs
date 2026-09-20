use super::prelude::*;
use crate::{LuaArrayType, LuaConditionalType, LuaGenericType, LuaMappedType};

impl<'db> SemanticModel<'db> {
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
    pub(crate) fn alias_target_uncached(&self, def: &TypeDef) -> Option<LuaType> {
        let syntax = def.alias_type?;
        let mut ty = self
            .analysis()
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
                ty = LuaType::Mapped(Arc::new(LuaMappedType::new(
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
        params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let constraint = param.constraint.map(|syntax| {
                    let ty = self.analysis().doc_type_lua(file_id, syntax, params);
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
        self.doc_type_lua_rich_in(self.view.file_id(), type_syntax)
    }
    /// Rich projection for any file (used by cross-file signature doc parameters).
    pub fn doc_type_lua_rich_in(&self, file_id: FileId, type_syntax: LuaSyntaxId) -> LuaType {
        let Some(tree) = self.syntax_tree_of(file_id) else {
            return self.analysis().doc_type_lua(file_id, type_syntax, &[]);
        };
        let Some(node) = type_syntax.to_node_from_root(&tree.get_red_root()) else {
            return self.analysis().doc_type_lua(file_id, type_syntax, &[]);
        };
        let Some(doc_ty) = LuaDocType::cast(node) else {
            return self.analysis().doc_type_lua(file_id, type_syntax, &[]);
        };
        match doc_ty {
            LuaDocType::Func(_) => {
                if let Some(fun) = infer::infer_doc_func(self, file_id, type_syntax) {
                    LuaType::DocFunction(Arc::new(fun))
                } else {
                    self.analysis().doc_type_lua(file_id, type_syntax, &[])
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
                LuaType::Mapped(Arc::new(LuaMappedType::new(
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
                    return self.analysis().doc_type_lua(file_id, type_syntax, &[]);
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
                    _ => self.analysis().doc_type_lua(file_id, type_syntax, &[]),
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
                _ => self.analysis().doc_type_lua(file_id, type_syntax, &[]),
            },
            LuaDocType::Unary(unary) => {
                if !unary
                    .get_op_token()
                    .is_some_and(|op| op.get_op() == LuaTypeUnaryOperator::Keyof)
                {
                    return self.analysis().doc_type_lua(file_id, type_syntax, &[]);
                }
                let Some(target) = unary.get_type() else {
                    return self.analysis().doc_type_lua(file_id, type_syntax, &[]);
                };
                let LuaDocType::Name(name_ty) = &target else {
                    return self.analysis().doc_type_lua(file_id, type_syntax, &[]);
                };
                let Some(name) = name_ty.get_name_text() else {
                    return self.analysis().doc_type_lua(file_id, type_syntax, &[]);
                };
                let Some(def) = self.resolve_type_def_in(file_id, &name) else {
                    return self.analysis().doc_type_lua(file_id, type_syntax, &[]);
                };
                let members: Vec<LuaType> = self
                    .members_of_owner(&def.id)
                    .into_iter()
                    .map(|member| LuaType::StringConst(SmolStr::new(member.name.as_str()).into()))
                    .collect();
                if members.is_empty() {
                    self.analysis().doc_type_lua(file_id, type_syntax, &[])
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
                    self.analysis().doc_type_lua(file_id, type_syntax, &[])
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
                        self.analysis().doc_type_lua(file_id, type_syntax, &[])
                    } else {
                        LuaType::Variadic(VariadicType::Base(base).into())
                    }
                })
                .unwrap_or_else(|| self.analysis().doc_type_lua(file_id, type_syntax, &[])),
            LuaDocType::Generic(generic) => {
                let Some(name) = generic.get_name_type().and_then(|n| n.get_name_text()) else {
                    return self.analysis().doc_type_lua(file_id, type_syntax, &[]);
                };
                let base_id = self.analysis().resolve_named_id(file_id, &name);
                let params: Vec<LuaType> = generic
                    .get_generic_types()
                    .map(|list| {
                        list.get_types()
                            .map(|arg| self.doc_type_lua_rich_in(file_id, arg.get_syntax_id()))
                            .collect()
                    })
                    .unwrap_or_default();
                LuaType::Generic(Arc::new(LuaGenericType::new(base_id, params)))
            }
            _ => self.analysis().doc_type_lua(file_id, type_syntax, &[]),
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

        LuaType::Conditional(Arc::new(LuaConditionalType::new(
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
                    return self.analysis().doc_type_lua(file_id, type_syntax, &[]);
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
                    _ => self.analysis().doc_type_lua(file_id, type_syntax, &[]),
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
                        self.analysis().doc_type_lua(file_id, type_syntax, &[])
                    } else {
                        LuaType::Variadic(VariadicType::Base(base).into())
                    }
                })
                .unwrap_or_else(|| self.analysis().doc_type_lua(file_id, type_syntax, &[])),
            LuaDocType::Generic(generic) => {
                let Some(name) = generic.get_name_type().and_then(|n| n.get_name_text()) else {
                    return self.analysis().doc_type_lua(file_id, type_syntax, &[]);
                };
                let base_id = self.analysis().resolve_named_id(file_id, &name);
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
                LuaType::Generic(Arc::new(LuaGenericType::new(base_id, params)))
            }
            LuaDocType::Array(array) => array
                .get_type()
                .map(|base| {
                    LuaType::Array(Arc::new(LuaArrayType::from_base_type(
                        self.doc_type_lua_rich_scoped(file_id, base.get_syntax_id(), state),
                    )))
                })
                .unwrap_or_else(|| self.analysis().doc_type_lua(file_id, type_syntax, &[])),
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
