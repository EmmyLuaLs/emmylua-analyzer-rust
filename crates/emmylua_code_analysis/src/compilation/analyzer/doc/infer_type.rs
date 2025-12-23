use std::sync::Arc;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaDocAttributeType, LuaDocBinaryType, LuaDocConditionalType,
    LuaDocDescriptionOwner, LuaDocFuncType, LuaDocGenericDecl, LuaDocGenericDeclList,
    LuaDocGenericType, LuaDocIndexAccessType, LuaDocInferType, LuaDocMappedType,
    LuaDocMultiLineUnionType, LuaDocObjectFieldKey, LuaDocObjectType, LuaDocStrTplType, LuaDocType,
    LuaDocUnaryType, LuaDocVariadicType, LuaLiteralToken, LuaSyntaxKind, LuaTypeBinaryOperator,
    LuaTypeUnaryOperator, LuaVarExpr, NumberResult,
};
use internment::ArcIntern;
use rowan::TextRange;
use smol_str::SmolStr;

use crate::{
    AsyncState, DiagnosticCode, GenericParam, GenericTpl, InFiled, LuaAliasCallKind, LuaArrayLen,
    LuaArrayType, LuaAttributeType, LuaMultiLineUnion, LuaTupleStatus, LuaTypeDeclId, TypeOps,
    VariadicType,
    db_index::{
        AnalyzeError, LuaAliasCallType, LuaConditionalType, LuaFunctionType, LuaGenericType,
        LuaIndexAccessKey, LuaIntersectionType, LuaMappedType, LuaObjectType, LuaStringTplType,
        LuaTupleType, LuaType,
    },
};

use super::{DocAnalyzer, preprocess_description};

pub fn infer_type(analyzer: &mut DocAnalyzer, node: LuaDocType) -> LuaType {
    match &node {
        LuaDocType::Name(name_type) => {
            if let Some(name) = name_type.get_name_text() {
                return infer_buildin_or_ref_type(analyzer, &name, name_type.get_range(), &node);
            }
        }
        LuaDocType::Nullable(nullable_type) => {
            if let Some(inner_type) = nullable_type.get_type() {
                let t = infer_type(analyzer, inner_type);
                if t.is_unknown() {
                    return LuaType::Unknown;
                }

                if !t.is_nullable() {
                    return TypeOps::Union.apply(analyzer.db, &t, &LuaType::Nil);
                }

                return t;
            }
        }
        LuaDocType::Array(array_type) => {
            if let Some(inner_type) = array_type.get_type() {
                let t = infer_type(analyzer, inner_type);
                if t.is_unknown() {
                    return LuaType::Unknown;
                }
                return LuaType::Array(LuaArrayType::new(t, LuaArrayLen::None).into());
            }
        }
        LuaDocType::Literal(literal) => {
            if let Some(literal_token) = literal.get_literal() {
                match literal_token {
                    LuaLiteralToken::String(str_token) => {
                        return LuaType::DocStringConst(SmolStr::new(str_token.get_value()).into());
                    }
                    LuaLiteralToken::Number(number_token) => {
                        if let NumberResult::Int(i) = number_token.get_number_value() {
                            return LuaType::DocIntegerConst(i);
                        } else {
                            return LuaType::Number;
                        }
                    }
                    LuaLiteralToken::Bool(bool_token) => {
                        return LuaType::DocBooleanConst(bool_token.is_true());
                    }
                    LuaLiteralToken::Nil(_) => return LuaType::Nil,
                    // todo
                    LuaLiteralToken::Dots(_) => return LuaType::Any,
                    LuaLiteralToken::Question(_) => return LuaType::Nil,
                }
            }
        }
        LuaDocType::Tuple(tuple_type) => {
            let mut types = Vec::new();
            for type_node in tuple_type.get_types() {
                let t = infer_type(analyzer, type_node);
                if t.is_unknown() {
                    return LuaType::Unknown;
                }
                types.push(t);
            }
            return LuaType::Tuple(LuaTupleType::new(types, LuaTupleStatus::DocResolve).into());
        }
        LuaDocType::Generic(generic_type) => {
            return infer_generic_type(analyzer, generic_type);
        }
        LuaDocType::Binary(binary_type) => {
            return infer_binary_type(analyzer, binary_type);
        }
        LuaDocType::Unary(unary_type) => {
            return infer_unary_type(analyzer, unary_type);
        }
        LuaDocType::Func(func) => {
            return infer_func_type(analyzer, func);
        }
        LuaDocType::Object(object_type) => {
            return infer_object_type(analyzer, object_type);
        }
        LuaDocType::StrTpl(str_tpl) => {
            return infer_str_tpl(analyzer, str_tpl, &node);
        }
        LuaDocType::Variadic(variadic_type) => {
            return infer_variadic_type(analyzer, variadic_type).unwrap_or(LuaType::Unknown);
        }
        LuaDocType::MultiLineUnion(multi_union) => {
            return infer_multi_line_union_type(analyzer, multi_union);
        }
        LuaDocType::Attribute(attribute_type) => {
            return infer_attribute_type(analyzer, attribute_type);
        }
        LuaDocType::Conditional(cond_type) => {
            return infer_conditional_type(analyzer, cond_type);
        }
        LuaDocType::Infer(infer_type) => {
            if let Some(name) = infer_type.get_generic_decl_name_text() {
                return LuaType::ConditionalInfer(ArcIntern::new(SmolStr::new(&name)));
            }
        }
        LuaDocType::Mapped(mapped_type) => {
            return infer_mapped_type(analyzer, mapped_type).unwrap_or(LuaType::Unknown);
        }
        LuaDocType::IndexAccess(index_access) => {
            return infer_index_access_type(analyzer, index_access);
        }
    }
    LuaType::Unknown
}

fn infer_buildin_or_ref_type(
    analyzer: &mut DocAnalyzer,
    name: &str,
    range: TextRange,
    node: &LuaDocType,
) -> LuaType {
    let position = range.start();
    match name {
        "unknown" => LuaType::Unknown,
        "never" => LuaType::Never,
        "nil" | "void" => LuaType::Nil,
        "any" => LuaType::Any,
        "userdata" => LuaType::Userdata,
        "thread" => LuaType::Thread,
        "boolean" | "bool" => LuaType::Boolean,
        "string" => LuaType::String,
        "integer" | "int" => LuaType::Integer,
        "number" => LuaType::Number,
        "io" => LuaType::Io,
        "self" => LuaType::SelfInfer,
        "global" => LuaType::Global,
        "function" => LuaType::Function,
        "table" => {
            if let Some(inst) = infer_special_table_type(analyzer, node) {
                return inst;
            }

            LuaType::Table
        }
        _ => {
            if let Some((tpl_id, constraint)) = analyzer.generic_index.find_generic(position, name)
            {
                return LuaType::TplRef(Arc::new(GenericTpl::new(
                    tpl_id,
                    SmolStr::new(name).into(),
                    constraint,
                )));
            }

            let mut founded = false;
            let type_id = if let Some(name_type_decl) = analyzer
                .db
                .get_type_index_mut()
                .find_type_decl(analyzer.file_id, name)
            {
                founded = true;
                name_type_decl.get_id()
            } else {
                LuaTypeDeclId::new(name)
            };

            if !founded {
                analyzer.db.get_diagnostic_index_mut().add_diagnostic(
                    analyzer.file_id,
                    AnalyzeError::new(
                        DiagnosticCode::TypeNotFound,
                        &t!("Type '%{name}' not found", name = name),
                        range,
                    ),
                );
            }

            analyzer.db.get_reference_index_mut().add_type_reference(
                analyzer.file_id,
                type_id.clone(),
                range,
            );

            LuaType::Ref(type_id)
        }
    }
}

fn infer_special_table_type(
    analyzer: &mut DocAnalyzer,
    table_type: &LuaDocType,
) -> Option<LuaType> {
    let parent = table_type.syntax().parent()?;
    if matches!(
        parent.kind().into(),
        LuaSyntaxKind::DocTagAs | LuaSyntaxKind::DocTagType
    ) {
        return Some(LuaType::TableConst(InFiled::new(
            analyzer.file_id,
            table_type.get_range(),
        )));
    }

    None
}

fn infer_generic_type(analyzer: &mut DocAnalyzer, generic_type: &LuaDocGenericType) -> LuaType {
    if let Some(name_type) = generic_type.get_name_type()
        && let Some(name) = name_type.get_name_text()
    {
        if let Some(typ) = infer_special_generic_type(analyzer, &name, generic_type) {
            return typ;
        }

        let id = if let Some(name_type_decl) = analyzer
            .db
            .get_type_index_mut()
            .find_type_decl(analyzer.file_id, &name)
        {
            name_type_decl.get_id()
        } else {
            analyzer.db.get_diagnostic_index_mut().add_diagnostic(
                analyzer.file_id,
                AnalyzeError::new(
                    DiagnosticCode::TypeNotFound,
                    &t!("Type '%{name}' not found", name = name),
                    generic_type.get_range(),
                ),
            );
            return LuaType::Unknown;
        };

        let mut generic_params = Vec::new();
        if let Some(generic_decl_list) = generic_type.get_generic_types() {
            for param in generic_decl_list.get_types() {
                let param_type = infer_type(analyzer, param);
                if param_type.is_unknown() {
                    return LuaType::Unknown;
                }
                generic_params.push(param_type);
            }
        }
        if let Some(name_type) = generic_type.get_name_type() {
            analyzer.db.get_reference_index_mut().add_type_reference(
                analyzer.file_id,
                id.clone(),
                name_type.get_range(),
            );
        }

        return LuaType::Generic(LuaGenericType::new(id, generic_params).into());
    }

    LuaType::Unknown
}

fn infer_special_generic_type(
    analyzer: &mut DocAnalyzer,
    name: &str,
    generic_type: &LuaDocGenericType,
) -> Option<LuaType> {
    match name {
        "table" => {
            let mut types = Vec::new();
            if let Some(generic_decl_list) = generic_type.get_generic_types() {
                for param in generic_decl_list.get_types() {
                    let param_type = infer_type(analyzer, param);
                    types.push(param_type);
                }
            }
            return Some(LuaType::TableGeneric(types.into()));
        }
        "namespace" => {
            let first_doc_param_type = generic_type.get_generic_types()?.get_types().next()?;
            let first_param = infer_type(analyzer, first_doc_param_type);
            if let LuaType::DocStringConst(ns_str) = first_param {
                return Some(LuaType::Namespace(ns_str));
            }
        }
        "std.Select" => {
            let mut params = Vec::new();
            for param in generic_type.get_generic_types()?.get_types() {
                let param_type = infer_type(analyzer, param);
                params.push(param_type);
            }
            return Some(LuaType::Call(
                LuaAliasCallType::new(LuaAliasCallKind::Select, params).into(),
            ));
        }
        "std.Unpack" => {
            let mut params = Vec::new();
            for param in generic_type.get_generic_types()?.get_types() {
                let param_type = infer_type(analyzer, param);
                params.push(param_type);
            }
            return Some(LuaType::Call(
                LuaAliasCallType::new(LuaAliasCallKind::Unpack, params).into(),
            ));
        }
        "std.RawGet" => {
            let mut params = Vec::new();
            for param in generic_type.get_generic_types()?.get_types() {
                let param_type = infer_type(analyzer, param);
                params.push(param_type);
            }
            return Some(LuaType::Call(
                LuaAliasCallType::new(LuaAliasCallKind::RawGet, params).into(),
            ));
        }
        "TypeGuard" => {
            let first_doc_param_type = generic_type.get_generic_types()?.get_types().next()?;
            let first_param = infer_type(analyzer, first_doc_param_type);

            return Some(LuaType::TypeGuard(first_param.into()));
        }
        "std.ConstTpl" => {
            let first_doc_param_type = generic_type.get_generic_types()?.get_types().next()?;
            let first_param = infer_type(analyzer, first_doc_param_type);
            if let LuaType::TplRef(tpl) = first_param {
                return Some(LuaType::ConstTplRef(tpl));
            }
        }
        "Language" => {
            let first_doc_param_type = generic_type.get_generic_types()?.get_types().next()?;
            let first_param = infer_type(analyzer, first_doc_param_type);
            if let LuaType::DocStringConst(lang_str) = first_param {
                return Some(LuaType::Language(lang_str));
            }
        }
        _ => {}
    }

    None
}

fn infer_binary_type(analyzer: &mut DocAnalyzer, binary_type: &LuaDocBinaryType) -> LuaType {
    if let Some((left, right)) = binary_type.get_types() {
        let left_type = infer_type(analyzer, left);
        let right_type = infer_type(analyzer, right);
        if left_type.is_unknown() {
            return right_type;
        }
        if right_type.is_unknown() {
            return left_type;
        }

        if let Some(op) = binary_type.get_op_token() {
            match op.get_op() {
                LuaTypeBinaryOperator::Union => match (left_type, right_type) {
                    (LuaType::Union(left_type_union), LuaType::Union(right_type_union)) => {
                        let mut left_type_set = left_type_union.into_vec();
                        let right_types = right_type_union.into_vec();
                        left_type_set.extend(right_types);
                        return LuaType::from_vec(left_type_set);
                    }
                    (LuaType::Union(left_type_union), right) => {
                        let mut left_types = (*left_type_union).into_vec();
                        left_types.push(right);
                        return LuaType::from_vec(left_types);
                    }
                    (left, LuaType::Union(right_type_union)) => {
                        let mut right_types = (*right_type_union).into_vec();
                        right_types.push(left);
                        return LuaType::from_vec(right_types);
                    }
                    (left, right) => {
                        return LuaType::from_vec(vec![left, right]);
                    }
                },
                LuaTypeBinaryOperator::Intersection => match (left_type, right_type) {
                    (
                        LuaType::Intersection(left_type_union),
                        LuaType::Intersection(right_type_union),
                    ) => {
                        let mut left_types = left_type_union.into_types();
                        let right_types = right_type_union.into_types();
                        left_types.extend(right_types);
                        return LuaType::Intersection(LuaIntersectionType::new(left_types).into());
                    }
                    (LuaType::Intersection(left_type_union), right) => {
                        let mut left_types = left_type_union.into_types();
                        left_types.push(right);
                        return LuaType::Intersection(LuaIntersectionType::new(left_types).into());
                    }
                    (left, LuaType::Intersection(right_type_union)) => {
                        let mut right_types = right_type_union.into_types();
                        right_types.push(left);
                        return LuaType::Intersection(LuaIntersectionType::new(right_types).into());
                    }
                    (left, right) => {
                        return LuaType::Intersection(
                            LuaIntersectionType::new(vec![left, right]).into(),
                        );
                    }
                },
                LuaTypeBinaryOperator::Extends => {
                    return LuaType::Call(
                        LuaAliasCallType::new(
                            LuaAliasCallKind::Extends,
                            vec![left_type, right_type],
                        )
                        .into(),
                    );
                }
                LuaTypeBinaryOperator::Add => {
                    return LuaType::Call(
                        LuaAliasCallType::new(LuaAliasCallKind::Add, vec![left_type, right_type])
                            .into(),
                    );
                }
                LuaTypeBinaryOperator::Sub => {
                    return LuaType::Call(
                        LuaAliasCallType::new(LuaAliasCallKind::Sub, vec![left_type, right_type])
                            .into(),
                    );
                }
                _ => {}
            }
        }
    }

    LuaType::Unknown
}

fn infer_unary_type(analyzer: &mut DocAnalyzer, unary_type: &LuaDocUnaryType) -> LuaType {
    if let Some(base_type) = unary_type.get_type() {
        let base = infer_type(analyzer, base_type);
        if base.is_unknown() {
            return LuaType::Unknown;
        }

        if let Some(op) = unary_type.get_op_token() {
            match op.get_op() {
                LuaTypeUnaryOperator::Keyof => {
                    return LuaType::Call(
                        LuaAliasCallType::new(LuaAliasCallKind::KeyOf, vec![base]).into(),
                    );
                }
                LuaTypeUnaryOperator::Neg => {
                    if let LuaType::DocIntegerConst(i) = base {
                        return LuaType::DocIntegerConst(-i);
                    }
                }
                _ => {}
            }
        }
    }

    LuaType::Unknown
}

fn infer_func_type(analyzer: &mut DocAnalyzer, func: &LuaDocFuncType) -> LuaType {
    if let Some(generic_list) = func.get_generic_decl_list() {
        register_inline_func_generics(analyzer, func, generic_list);
    }

    let mut params_result = Vec::new();
    let mut is_variadic = false;
    for param in func.get_params() {
        let name = if let Some(param) = param.get_name_token() {
            param.get_name_text().to_string()
        } else if param.is_dots() {
            is_variadic = true;
            "...".to_string()
        } else {
            continue;
        };

        let nullable = param.is_nullable();

        let type_ref = if let Some(type_ref) = param.get_type() {
            let mut typ = infer_type(analyzer, type_ref);
            if nullable && !typ.is_nullable() {
                typ = TypeOps::Union.apply(analyzer.db, &typ, &LuaType::Nil);
            }
            Some(typ)
        } else {
            None
        };

        params_result.push((name, type_ref));
    }

    let mut return_types = Vec::new();
    if let Some(return_type_list) = func.get_return_type_list() {
        for return_type in return_type_list.get_return_type_list() {
            let (_, typ) = return_type.get_name_and_type();
            if let Some(typ) = typ {
                let t = infer_type(analyzer, typ);
                return_types.push(t);
            } else {
                return_types.push(LuaType::Unknown);
            }
        }
    }

    let async_state = if func.is_async() {
        AsyncState::Async
    } else if func.is_sync() {
        AsyncState::Sync
    } else {
        AsyncState::None
    };

    let mut is_colon = false;
    if let Some(parent) = func.get_parent::<LuaAst>() {
        // old emmylua feature will auto infer colon define
        if parent.syntax().kind() == LuaSyntaxKind::DocTagOverload.into() {
            is_colon = get_colon_define(analyzer).unwrap_or(false);
        }
    }

    // compact luals
    if is_colon
        && let Some(first_param) = params_result.first()
        && first_param.0 == "self"
    {
        is_colon = false
    }

    let return_type = if return_types.len() == 1 {
        return_types[0].clone()
    } else if return_types.len() > 1 {
        LuaType::Variadic(VariadicType::Multi(return_types).into())
    } else {
        LuaType::Nil
    };

    LuaType::DocFunction(
        LuaFunctionType::new(
            async_state,
            is_colon,
            is_variadic,
            params_result,
            return_type,
        )
        .into(),
    )
}

fn register_inline_func_generics(
    analyzer: &mut DocAnalyzer,
    func: &LuaDocFuncType,
    generic_list: LuaDocGenericDeclList,
) {
    let mut generics = Vec::new();
    for param in generic_list.get_generic_decl() {
        let Some(name_token) = param.get_name_token() else {
            continue;
        };

        let constraint = param.get_type().map(|ty| infer_type(analyzer, ty));
        generics.push(GenericParam::new(
            SmolStr::new(name_token.get_name_text()),
            constraint,
            None,
        ));
    }
    if generics.is_empty() {
        return;
    }

    let scope_id = analyzer
        .generic_index
        .add_generic_scope(vec![func.get_range()], true);
    analyzer
        .generic_index
        .append_generic_params(scope_id, generics);
}

fn get_colon_define(analyzer: &mut DocAnalyzer) -> Option<bool> {
    let owner = analyzer.comment.get_owner()?;
    if let LuaAst::LuaFuncStat(func_stat) = owner {
        let func_name = func_stat.get_func_name()?;
        if let LuaVarExpr::IndexExpr(index_expr) = func_name {
            return Some(index_expr.get_index_token()?.is_colon());
        }
    }

    None
}

fn infer_object_type(analyzer: &mut DocAnalyzer, object_type: &LuaDocObjectType) -> LuaType {
    let mut fields = Vec::new();
    for field in object_type.get_fields() {
        let key = if let Some(field_key) = field.get_field_key() {
            match field_key {
                LuaDocObjectFieldKey::Name(name) => {
                    LuaIndexAccessKey::String(name.get_name_text().to_string().into())
                }
                LuaDocObjectFieldKey::Integer(int) => {
                    if let NumberResult::Int(i) = int.get_number_value() {
                        LuaIndexAccessKey::Integer(i)
                    } else {
                        continue;
                    }
                }
                LuaDocObjectFieldKey::String(str) => {
                    LuaIndexAccessKey::String(str.get_value().to_string().into())
                }
                LuaDocObjectFieldKey::Type(t) => LuaIndexAccessKey::Type(infer_type(analyzer, t)),
            }
        } else {
            continue;
        };

        let mut type_ref = if let Some(type_ref) = field.get_type() {
            infer_type(analyzer, type_ref)
        } else {
            LuaType::Unknown
        };

        if field.is_nullable() {
            type_ref = TypeOps::Union.apply(analyzer.db, &type_ref, &LuaType::Nil);
        }

        fields.push((key, type_ref));
    }

    LuaType::Object(LuaObjectType::new(fields).into())
}

fn infer_str_tpl(
    analyzer: &mut DocAnalyzer,
    str_tpl: &LuaDocStrTplType,
    node: &LuaDocType,
) -> LuaType {
    let (prefix, tpl_name, suffix) = str_tpl.get_name();
    if let Some(tpl) = tpl_name {
        let typ = infer_buildin_or_ref_type(analyzer, &tpl, str_tpl.get_range(), node);
        if let LuaType::TplRef(tpl) = typ {
            let tpl_id = tpl.get_tpl_id();
            let prefix = prefix.unwrap_or_default();
            let suffix = suffix.unwrap_or_default();
            if tpl_id.is_func() {
                let str_tpl_type = LuaStringTplType::new(
                    &prefix,
                    tpl.get_name(),
                    tpl_id,
                    &suffix,
                    tpl.get_constraint().cloned(),
                );
                return LuaType::StrTplRef(str_tpl_type.into());
            }
        }
    }

    LuaType::Unknown
}

fn infer_variadic_type(
    analyzer: &mut DocAnalyzer,
    variadic_type: &LuaDocVariadicType,
) -> Option<LuaType> {
    let inner_type = variadic_type.get_type()?;
    let base = infer_type(analyzer, inner_type);
    let variadic = VariadicType::Base(base.clone());
    Some(LuaType::Variadic(variadic.into()))
}

fn infer_multi_line_union_type(
    analyzer: &mut DocAnalyzer,
    multi_union: &LuaDocMultiLineUnionType,
) -> LuaType {
    let mut union_members = Vec::new();
    for field in multi_union.get_fields() {
        let alias_member_type = if let Some(field_type) = field.get_type() {
            let type_ref = infer_type(analyzer, field_type);
            if type_ref.is_unknown() {
                continue;
            }
            type_ref
        } else {
            continue;
        };

        let description = if let Some(description) = field.get_description() {
            let description_text =
                preprocess_description(&description.get_description_text(), None);
            if !description_text.is_empty() {
                Some(description_text)
            } else {
                None
            }
        } else {
            None
        };

        union_members.push((alias_member_type, description));
    }

    LuaType::MultiLineUnion(LuaMultiLineUnion::new(union_members).into())
}

fn infer_attribute_type(
    analyzer: &mut DocAnalyzer,
    attribute_type: &LuaDocAttributeType,
) -> LuaType {
    let mut params_result = Vec::new();
    for param in attribute_type.get_params() {
        let name = if let Some(param) = param.get_name_token() {
            param.get_name_text().to_string()
        } else if param.is_dots() {
            "...".to_string()
        } else {
            continue;
        };

        let nullable = param.is_nullable();

        let type_ref = if let Some(type_ref) = param.get_type() {
            let mut typ = infer_type(analyzer, type_ref);
            if nullable && !typ.is_nullable() {
                typ = TypeOps::Union.apply(analyzer.db, &typ, &LuaType::Nil);
            }
            Some(typ)
        } else {
            None
        };

        params_result.push((name, type_ref));
    }

    LuaType::DocAttribute(LuaAttributeType::new(params_result).into())
}

fn infer_conditional_type(
    analyzer: &mut DocAnalyzer,
    cond_type: &LuaDocConditionalType,
) -> LuaType {
    if let Some((condition, when_true, when_false)) = cond_type.get_types() {
        // 收集条件中的所有 infer 声明
        let infer_params = collect_cond_infer_params(&condition);
        if !infer_params.is_empty() {
            // 条件表达式中 infer 声明的类型参数只允许在`true`分支中使用
            let true_range = when_true.get_range();
            let scope_id = analyzer
                .generic_index
                .add_generic_scope(vec![true_range], false);
            analyzer
                .generic_index
                .append_generic_params(scope_id, infer_params.clone());
        }

        // 处理条件和分支类型
        let condition_type = infer_type(analyzer, condition);
        let true_type = infer_type(analyzer, when_true);
        let false_type = infer_type(analyzer, when_false);

        return LuaConditionalType::new(
            condition_type,
            true_type,
            false_type,
            infer_params,
            cond_type.has_new().unwrap_or(false),
        )
        .into();
    }

    LuaType::Unknown
}

/// 收集条件类型中的条件表达式中所有 infer 声明
fn collect_cond_infer_params(doc_type: &LuaDocType) -> Vec<GenericParam> {
    let mut params = Vec::new();
    let doc_infer_types = doc_type.descendants::<LuaDocInferType>();
    for infer_type in doc_infer_types {
        if let Some(name) = infer_type.get_generic_decl_name_text() {
            params.push(GenericParam::new(SmolStr::new(&name), None, None));
        }
    }
    params
}

fn infer_mapped_type(
    analyzer: &mut DocAnalyzer,
    mapped_type: &LuaDocMappedType,
) -> Option<LuaType> {
    // [P in K]
    let mapped_key = mapped_type.get_key()?;
    let generic_decl = mapped_key.child::<LuaDocGenericDecl>()?;
    let name_token = generic_decl.get_name_token()?;
    let name = name_token.get_name_text();
    let constraint = generic_decl
        .get_type()
        .map(|constraint| infer_type(analyzer, constraint));
    let param = GenericParam::new(SmolStr::new(name), constraint, None);

    let scope_id = analyzer
        .generic_index
        .add_generic_scope(vec![mapped_type.get_range()], false);
    analyzer
        .generic_index
        .append_generic_param(scope_id, param.clone());
    let position = mapped_type.get_range().start();
    let (id, _) = analyzer.generic_index.find_generic(position, name)?;

    let doc_type = mapped_type.get_value_type()?;
    let value_type = infer_type(analyzer, doc_type);

    Some(LuaType::Mapped(
        LuaMappedType::new(
            (id, param),
            value_type,
            mapped_type.is_readonly(),
            mapped_type.is_optional(),
        )
        .into(),
    ))
}

fn infer_index_access_type(
    analyzer: &mut DocAnalyzer,
    index_access: &LuaDocIndexAccessType,
) -> LuaType {
    let mut types_iter = index_access.children::<LuaDocType>();
    let Some(source_doc) = types_iter.next() else {
        return LuaType::Unknown;
    };
    let Some(key_doc) = types_iter.next() else {
        return LuaType::Unknown;
    };

    let source_type = infer_type(analyzer, source_doc);
    let key_type = infer_type(analyzer, key_doc);

    LuaType::Call(
        LuaAliasCallType::new(LuaAliasCallKind::Index, vec![source_type, key_type]).into(),
    )
}
