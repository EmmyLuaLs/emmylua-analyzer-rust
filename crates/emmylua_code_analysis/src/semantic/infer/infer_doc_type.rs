use std::sync::Arc;

use emmylua_parser::{
    LuaAstNode, LuaDocAttributeType, LuaDocBinaryType, LuaDocDescriptionOwner, LuaDocFuncType,
    LuaDocGenericType, LuaDocMultiLineUnionType, LuaDocObjectFieldKey, LuaDocObjectType,
    LuaDocStrTplType, LuaDocType, LuaDocUnaryType, LuaDocVariadicType, LuaLiteralToken,
    LuaSyntaxKind, LuaTypeBinaryOperator, LuaTypeUnaryOperator, NumberResult,
};
use rowan::TextRange;
use smol_str::SmolStr;

use crate::{
    AsyncState, DbIndex, FileId, InFiled, LuaAliasCallKind, LuaAliasCallType, LuaArrayLen,
    LuaArrayType, LuaAttributeType, LuaFunctionType, LuaGenericType, LuaIndexAccessKey,
    LuaIntersectionType, LuaMultiLineUnion, LuaObjectType, LuaStringTplType, LuaTupleStatus,
    LuaTupleType, LuaType, LuaTypeDeclId, TypeOps, VariadicType,
};

#[derive(Clone, Copy)]
pub struct DocTypeInferContext<'a> {
    pub db: &'a DbIndex,
    pub file_id: FileId,
}

impl<'a> DocTypeInferContext<'a> {
    pub fn new(db: &'a DbIndex, file_id: FileId) -> Self {
        Self { db, file_id }
    }
}

pub fn infer_doc_type(ctx: DocTypeInferContext<'_>, node: &LuaDocType) -> LuaType {
    match node {
        LuaDocType::Name(name_type) => {
            if let Some(name) = name_type.get_name_text() {
                return infer_buildin_or_ref_type(ctx, &name, name_type.get_range(), node);
            }
        }
        LuaDocType::Nullable(nullable_type) => {
            if let Some(inner_type) = nullable_type.get_type() {
                let t = infer_doc_type(ctx, &inner_type);
                if t.is_unknown() {
                    return LuaType::Unknown;
                }

                if !t.is_nullable() {
                    return TypeOps::Union.apply(ctx.db, &t, &LuaType::Nil);
                }

                return t;
            }
        }
        LuaDocType::Array(array_type) => {
            if let Some(inner_type) = array_type.get_type() {
                let t = infer_doc_type(ctx, &inner_type);
                if t.is_unknown() {
                    return LuaType::Unknown;
                }
                return LuaType::Array(Arc::new(LuaArrayType::new(t, LuaArrayLen::None)));
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
                    LuaLiteralToken::Dots(_) => return LuaType::Any,
                    LuaLiteralToken::Question(_) => return LuaType::Nil,
                }
            }
        }
        LuaDocType::Tuple(tuple_type) => {
            let mut types = Vec::new();
            for type_node in tuple_type.get_types() {
                let t = infer_doc_type(ctx, &type_node);
                if t.is_unknown() {
                    return LuaType::Unknown;
                }
                types.push(t);
            }
            return LuaType::Tuple(LuaTupleType::new(types, LuaTupleStatus::DocResolve).into());
        }
        LuaDocType::Generic(generic_type) => {
            return infer_generic_type(ctx, generic_type);
        }
        LuaDocType::Binary(binary_type) => {
            return infer_binary_type(ctx, binary_type);
        }
        LuaDocType::Unary(unary_type) => {
            return infer_unary_type(ctx, unary_type);
        }
        LuaDocType::Func(func) => {
            return infer_func_type(ctx, func);
        }
        LuaDocType::Object(object_type) => {
            return infer_object_type(ctx, object_type);
        }
        LuaDocType::StrTpl(str_tpl) => {
            return infer_str_tpl(ctx, str_tpl, node);
        }
        LuaDocType::Variadic(variadic_type) => {
            return infer_variadic_type(ctx, variadic_type).unwrap_or(LuaType::Unknown);
        }
        LuaDocType::MultiLineUnion(multi_union) => {
            return infer_multi_line_union_type(ctx, multi_union);
        }
        LuaDocType::Attribute(attribute_type) => {
            return infer_attribute_type(ctx, attribute_type);
        }
        _ => {}
    }
    LuaType::Unknown
}

fn infer_buildin_or_ref_type(
    ctx: DocTypeInferContext<'_>,
    name: &str,
    range: TextRange,
    _node: &LuaDocType,
) -> LuaType {
    let _position = range.start();
    match name {
        "unknown" => LuaType::Unknown,
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
            if let Some(inst) = infer_special_table_type(ctx, _node) {
                return inst;
            }
            LuaType::Table
        }
        _ => {
            let file_id = ctx.file_id;
            let type_id = if let Some(name_type_decl) =
                ctx.db.get_type_index().find_type_decl(file_id, name)
            {
                name_type_decl.get_id()
            } else {
                LuaTypeDeclId::new(name)
            };

            LuaType::Ref(type_id)
        }
    }
}

fn infer_special_table_type(
    ctx: DocTypeInferContext<'_>,
    table_type: &LuaDocType,
) -> Option<LuaType> {
    let parent = table_type.syntax().parent()?;
    if matches!(
        parent.kind().into(),
        LuaSyntaxKind::DocTagAs | LuaSyntaxKind::DocTagType
    ) {
        let file_id = ctx.file_id;
        return Some(LuaType::TableConst(InFiled::new(
            file_id,
            table_type.get_range(),
        )));
    }

    None
}

fn infer_generic_type(ctx: DocTypeInferContext<'_>, generic_type: &LuaDocGenericType) -> LuaType {
    if let Some(name_type) = generic_type.get_name_type()
        && let Some(name) = name_type.get_name_text()
    {
        if let Some(typ) = infer_special_generic_type(ctx, &name, generic_type) {
            return typ;
        }

        let file_id = ctx.file_id;
        let id =
            if let Some(name_type_decl) = ctx.db.get_type_index().find_type_decl(file_id, &name) {
                name_type_decl.get_id()
            } else {
                return LuaType::Unknown;
            };

        let mut generic_params = Vec::new();
        if let Some(generic_decl_list) = generic_type.get_generic_types() {
            for param in generic_decl_list.get_types() {
                let param_type = infer_doc_type(ctx, &param);
                if param_type.is_unknown() {
                    return LuaType::Unknown;
                }
                generic_params.push(param_type);
            }
        }

        return LuaType::Generic(LuaGenericType::new(id, generic_params).into());
    }

    LuaType::Unknown
}

fn infer_special_generic_type(
    ctx: DocTypeInferContext<'_>,
    name: &str,
    generic_type: &LuaDocGenericType,
) -> Option<LuaType> {
    match name {
        "table" => {
            let mut types = Vec::new();
            if let Some(generic_decl_list) = generic_type.get_generic_types() {
                for param in generic_decl_list.get_types() {
                    let param_type = infer_doc_type(ctx, &param);
                    types.push(param_type);
                }
            }
            return Some(LuaType::TableGeneric(types.into()));
        }
        "namespace" => {
            let first_doc_param_type = generic_type.get_generic_types()?.get_types().next()?;
            let first_param = infer_doc_type(ctx, &first_doc_param_type);
            if let LuaType::DocStringConst(ns_str) = first_param {
                return Some(LuaType::Namespace(ns_str));
            }
        }
        "std.Select" => {
            let mut params = Vec::new();
            for param in generic_type.get_generic_types()?.get_types() {
                let param_type = infer_doc_type(ctx, &param);
                params.push(param_type);
            }
            return Some(LuaType::Call(
                LuaAliasCallType::new(LuaAliasCallKind::Select, params).into(),
            ));
        }
        "std.Unpack" => {
            let mut params = Vec::new();
            for param in generic_type.get_generic_types()?.get_types() {
                let param_type = infer_doc_type(ctx, &param);
                params.push(param_type);
            }
            return Some(LuaType::Call(
                LuaAliasCallType::new(LuaAliasCallKind::Unpack, params).into(),
            ));
        }
        "std.RawGet" => {
            let mut params = Vec::new();
            for param in generic_type.get_generic_types()?.get_types() {
                let param_type = infer_doc_type(ctx, &param);
                params.push(param_type);
            }
            return Some(LuaType::Call(
                LuaAliasCallType::new(LuaAliasCallKind::RawGet, params).into(),
            ));
        }
        "TypeGuard" => {
            let first_doc_param_type = generic_type.get_generic_types()?.get_types().next()?;
            let first_param = infer_doc_type(ctx, &first_doc_param_type);

            return Some(LuaType::TypeGuard(first_param.into()));
        }
        _ => {}
    }

    None
}

fn infer_binary_type(ctx: DocTypeInferContext<'_>, binary_type: &LuaDocBinaryType) -> LuaType {
    if let Some((left, right)) = binary_type.get_types() {
        let left_type = infer_doc_type(ctx, &left);
        let right_type = infer_doc_type(ctx, &right);
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
                        let mut left_types = left_type_union.into_vec();
                        let right_types = right_type_union.into_vec();
                        left_types.extend(right_types);
                        return LuaType::from_vec(left_types);
                    }
                    (LuaType::Union(left_type_union), right) => {
                        let mut left_types = left_type_union.into_vec();
                        left_types.push(right);
                        return LuaType::from_vec(left_types);
                    }
                    (left, LuaType::Union(right_type_union)) => {
                        let mut right_types = right_type_union.into_vec();
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

fn infer_unary_type(ctx: DocTypeInferContext<'_>, unary_type: &LuaDocUnaryType) -> LuaType {
    if let Some(base_type) = unary_type.get_type() {
        let base = infer_doc_type(ctx, &base_type);
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

fn infer_func_type(ctx: DocTypeInferContext<'_>, func: &LuaDocFuncType) -> LuaType {
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
            let mut typ = infer_doc_type(ctx, &type_ref);
            if nullable && !typ.is_nullable() {
                typ = TypeOps::Union.apply(ctx.db, &typ, &LuaType::Nil);
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
                let t = infer_doc_type(ctx, &typ);
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

    // Note: In the diagnostic context, we can't easily determine colon define status
    // since we don't have the same context as the analyzer. This is a simplification.
    let is_colon = false;

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

fn infer_object_type(ctx: DocTypeInferContext<'_>, object_type: &LuaDocObjectType) -> LuaType {
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
                LuaDocObjectFieldKey::Type(t) => LuaIndexAccessKey::Type(infer_doc_type(ctx, &t)),
            }
        } else {
            continue;
        };

        let mut type_ref = if let Some(type_ref) = field.get_type() {
            infer_doc_type(ctx, &type_ref)
        } else {
            LuaType::Unknown
        };

        if field.is_nullable() {
            type_ref = TypeOps::Union.apply(ctx.db, &type_ref, &LuaType::Nil);
        }

        fields.push((key, type_ref));
    }

    LuaType::Object(LuaObjectType::new(fields).into())
}

fn infer_str_tpl(
    ctx: DocTypeInferContext<'_>,
    str_tpl: &LuaDocStrTplType,
    node: &LuaDocType,
) -> LuaType {
    let (prefix, tpl_name, suffix) = str_tpl.get_name();
    if let Some(tpl) = tpl_name {
        let typ = infer_buildin_or_ref_type(ctx, &tpl, str_tpl.get_range(), node);
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
    ctx: DocTypeInferContext<'_>,
    variadic_type: &LuaDocVariadicType,
) -> Option<LuaType> {
    let inner_type = variadic_type.get_type()?;
    let base = infer_doc_type(ctx, &inner_type);
    let variadic = VariadicType::Base(base.clone());
    Some(LuaType::Variadic(variadic.into()))
}

fn infer_multi_line_union_type(
    ctx: DocTypeInferContext<'_>,
    multi_union: &LuaDocMultiLineUnionType,
) -> LuaType {
    let mut union_members = Vec::new();
    for field in multi_union.get_fields() {
        let alias_member_type = if let Some(field_type) = field.get_type() {
            let type_ref = infer_doc_type(ctx, &field_type);
            if type_ref.is_unknown() {
                continue;
            }
            type_ref
        } else {
            continue;
        };

        let description = if let Some(description) = field.get_description() {
            let description_text = description.get_description_text();
            if !description_text.is_empty() {
                Some(description_text.to_string())
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
    ctx: DocTypeInferContext<'_>,
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
            let mut typ = infer_doc_type(ctx, &type_ref);
            if nullable && !typ.is_nullable() {
                typ = TypeOps::Union.apply(ctx.db, &typ, &LuaType::Nil);
            }
            Some(typ)
        } else {
            None
        };

        params_result.push((name, type_ref));
    }

    LuaType::DocAttribute(LuaAttributeType::new(params_result).into())
}
