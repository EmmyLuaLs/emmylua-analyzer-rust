use emmylua_parser::{LuaAstNode, LuaChunk, LuaDocObjectFieldKey, LuaDocType, NumberResult};
use emmylua_parser::{
    LuaAstToken, LuaDocGenericDeclList, LuaDocObjectField, LuaDocTypeParam, LuaTypeBinaryOperator,
    LuaTypeUnaryOperator,
};
use smol_str::SmolStr;

use super::super::{
    SalsaDocGenericParamSummary, SalsaDocObjectFieldKeySummary, SalsaDocObjectFieldSummary,
    SalsaDocReturnTypeSummary, SalsaDocTypeBinaryOperatorSummary, SalsaDocTypeIndexSummary,
    SalsaDocTypeKindSummary, SalsaDocTypeNodeKey, SalsaDocTypeNodeSummary,
    SalsaDocTypeUnaryOperatorSummary, SalsaDocTypedParamSummary,
};

pub fn analyze_doc_type_summary(chunk: LuaChunk) -> SalsaDocTypeIndexSummary {
    SalsaDocTypeIndexSummary {
        types: chunk
            .descendants::<LuaDocType>()
            .map(build_doc_type_node)
            .collect(),
    }
}

fn build_doc_type_node(doc_type: LuaDocType) -> SalsaDocTypeNodeSummary {
    let syntax_offset = doc_type.get_position();
    let syntax_id = doc_type.get_syntax_id().into();
    let kind = match doc_type {
        LuaDocType::Name(name_type) => SalsaDocTypeKindSummary::Name {
            name: name_type.get_name_text().map(Into::into),
        },
        LuaDocType::Infer(infer_type) => SalsaDocTypeKindSummary::Infer {
            generic_name: infer_type.get_generic_decl_name_text().map(Into::into),
        },
        LuaDocType::Array(array_type) => SalsaDocTypeKindSummary::Array {
            item_type_offset: array_type.get_type().map(doc_type_node_key),
        },
        LuaDocType::Func(func_type) => SalsaDocTypeKindSummary::Function {
            is_async: func_type.is_async(),
            is_sync: func_type.is_sync(),
            generic_params: collect_generic_params(func_type.get_generic_decl_list()),
            params: func_type.get_params().map(build_typed_param).collect(),
            returns: func_type
                .get_return_type_list()
                .into_iter()
                .flat_map(|type_list| type_list.get_return_type_list())
                .map(|named_return| {
                    let (name, doc_type) = named_return.get_name_and_type();
                    SalsaDocReturnTypeSummary {
                        name: name.map(|name| name.get_name_text().into()),
                        type_offset: doc_type.map(doc_type_node_key),
                    }
                })
                .collect(),
        },
        LuaDocType::Object(object_type) => SalsaDocTypeKindSummary::Object {
            fields: object_type
                .get_fields()
                .filter_map(build_object_field)
                .collect(),
        },
        LuaDocType::Binary(binary_type) => {
            let (left_type_offset, right_type_offset) = binary_type
                .get_types()
                .map(|(left, right)| {
                    (
                        Some(doc_type_node_key(left)),
                        Some(doc_type_node_key(right)),
                    )
                })
                .unwrap_or((None, None));
            SalsaDocTypeKindSummary::Binary {
                op: binary_type
                    .get_op_token()
                    .map(|token| map_binary_operator(token.get_op()))
                    .unwrap_or(SalsaDocTypeBinaryOperatorSummary::None),
                left_type_offset,
                right_type_offset,
            }
        }
        LuaDocType::Unary(unary_type) => SalsaDocTypeKindSummary::Unary {
            op: unary_type
                .get_op_token()
                .map(|token| map_unary_operator(token.get_op()))
                .unwrap_or(SalsaDocTypeUnaryOperatorSummary::None),
            inner_type_offset: unary_type.get_type().map(doc_type_node_key),
        },
        LuaDocType::Conditional(conditional_type) => {
            let (condition_type_offset, true_type_offset, false_type_offset) = conditional_type
                .get_types()
                .map(|(condition, true_type, false_type)| {
                    (
                        Some(doc_type_node_key(condition)),
                        Some(doc_type_node_key(true_type)),
                        Some(doc_type_node_key(false_type)),
                    )
                })
                .unwrap_or((None, None, None));
            SalsaDocTypeKindSummary::Conditional {
                condition_type_offset,
                true_type_offset,
                false_type_offset,
                has_new: conditional_type.has_new(),
            }
        }
        LuaDocType::Tuple(tuple_type) => SalsaDocTypeKindSummary::Tuple {
            item_type_offsets: tuple_type.get_types().map(doc_type_node_key).collect(),
        },
        LuaDocType::Literal(literal_type) => SalsaDocTypeKindSummary::Literal {
            text: literal_type
                .get_literal()
                .map(|literal| SmolStr::new(literal.syntax().text()))
                .unwrap_or_default(),
        },
        LuaDocType::Variadic(variadic_type) => SalsaDocTypeKindSummary::Variadic {
            item_type_offset: variadic_type.get_type().map(doc_type_node_key),
        },
        LuaDocType::Nullable(nullable_type) => SalsaDocTypeKindSummary::Nullable {
            inner_type_offset: nullable_type.get_type().map(doc_type_node_key),
        },
        LuaDocType::Generic(generic_type) => SalsaDocTypeKindSummary::Generic {
            base_type_offset: generic_type
                .get_name_type()
                .map(|name_type| name_type.get_syntax_id().into()),
            arg_type_offsets: generic_type
                .get_generic_types()
                .into_iter()
                .flat_map(|type_list| type_list.get_types())
                .map(doc_type_node_key)
                .collect(),
        },
        LuaDocType::StrTpl(str_tpl_type) => {
            let (prefix, interpolated, suffix) = str_tpl_type.get_name();
            SalsaDocTypeKindSummary::StringTemplate {
                prefix: prefix.map(Into::into),
                interpolated: interpolated.map(Into::into),
                suffix: suffix.map(Into::into),
            }
        }
        LuaDocType::MultiLineUnion(union_type) => SalsaDocTypeKindSummary::MultiLineUnion {
            item_type_offsets: union_type
                .get_fields()
                .filter_map(|field| field.get_type())
                .map(doc_type_node_key)
                .collect(),
        },
        LuaDocType::Attribute(attribute_type) => SalsaDocTypeKindSummary::Attribute {
            params: attribute_type.get_params().map(build_typed_param).collect(),
        },
        LuaDocType::Mapped(mapped_type) => SalsaDocTypeKindSummary::Mapped {
            key_type_offsets: mapped_type
                .get_key()
                .into_iter()
                .flat_map(|key| key.descendants::<LuaDocType>().collect::<Vec<_>>())
                .map(doc_type_node_key)
                .collect(),
            value_type_offset: mapped_type.get_value_type().map(doc_type_node_key),
            is_readonly: mapped_type.is_readonly(),
            is_optional: mapped_type.is_optional(),
        },
        LuaDocType::IndexAccess(index_access_type) => {
            let mut type_children = index_access_type.children::<LuaDocType>();
            SalsaDocTypeKindSummary::IndexAccess {
                base_type_offset: type_children.next().map(doc_type_node_key),
                index_type_offset: type_children.next().map(doc_type_node_key),
            }
        }
    };

    SalsaDocTypeNodeSummary {
        syntax_offset,
        syntax_id,
        kind,
    }
}

fn build_typed_param(param: LuaDocTypeParam) -> SalsaDocTypedParamSummary {
    let type_info = param.get_type();
    let is_nullable =
        param.is_nullable() || matches!(type_info.as_ref(), Some(LuaDocType::Nullable(_)));
    SalsaDocTypedParamSummary {
        name: param
            .get_name_token()
            .map(|name| name.get_name_text().into()),
        type_offset: type_info.map(doc_type_node_key),
        is_dots: param.is_dots(),
        is_nullable,
    }
}

fn build_object_field(field: LuaDocObjectField) -> Option<SalsaDocObjectFieldSummary> {
    let key = match field.get_field_key()? {
        LuaDocObjectFieldKey::Name(name) => {
            SalsaDocObjectFieldKeySummary::Name(name.get_name_text().into())
        }
        LuaDocObjectFieldKey::String(string) => {
            SalsaDocObjectFieldKeySummary::String(string.get_value().into())
        }
        LuaDocObjectFieldKey::Integer(number) => match number.get_number_value() {
            NumberResult::Int(value) => SalsaDocObjectFieldKeySummary::Integer(value),
            _ => return None,
        },
        LuaDocObjectFieldKey::Type(doc_type) => {
            SalsaDocObjectFieldKeySummary::Type(doc_type_node_key(doc_type))
        }
    };

    Some(SalsaDocObjectFieldSummary {
        syntax_offset: field.get_position(),
        key,
        value_type_offset: field.get_type().map(doc_type_node_key),
        is_nullable: field.is_nullable(),
    })
}

fn collect_generic_params(
    generic_decl_list: Option<LuaDocGenericDeclList>,
) -> Vec<SalsaDocGenericParamSummary> {
    generic_decl_list
        .into_iter()
        .flat_map(|generic_decl_list| generic_decl_list.get_generic_decl())
        .filter_map(|generic_decl| {
            Some(SalsaDocGenericParamSummary {
                name: generic_decl.get_name_token()?.get_name_text().into(),
                type_offset: generic_decl.get_constraint_type().map(doc_type_node_key),
            })
        })
        .collect()
}

fn doc_type_node_key(doc_type: LuaDocType) -> SalsaDocTypeNodeKey {
    doc_type.get_syntax_id().into()
}

fn map_binary_operator(op: LuaTypeBinaryOperator) -> SalsaDocTypeBinaryOperatorSummary {
    match op {
        LuaTypeBinaryOperator::None => SalsaDocTypeBinaryOperatorSummary::None,
        LuaTypeBinaryOperator::Union => SalsaDocTypeBinaryOperatorSummary::Union,
        LuaTypeBinaryOperator::Intersection => SalsaDocTypeBinaryOperatorSummary::Intersection,
        LuaTypeBinaryOperator::In => SalsaDocTypeBinaryOperatorSummary::In,
        LuaTypeBinaryOperator::Extends => SalsaDocTypeBinaryOperatorSummary::Extends,
        LuaTypeBinaryOperator::Add => SalsaDocTypeBinaryOperatorSummary::Add,
        LuaTypeBinaryOperator::Sub => SalsaDocTypeBinaryOperatorSummary::Sub,
    }
}

fn map_unary_operator(op: LuaTypeUnaryOperator) -> SalsaDocTypeUnaryOperatorSummary {
    match op {
        LuaTypeUnaryOperator::None => SalsaDocTypeUnaryOperatorSummary::None,
        LuaTypeUnaryOperator::Keyof => SalsaDocTypeUnaryOperatorSummary::Keyof,
        LuaTypeUnaryOperator::Neg => SalsaDocTypeUnaryOperatorSummary::Neg,
    }
}
