use hashbrown::HashMap;
use rowan::TextSize;
use smol_str::SmolStr;

use crate::{
    SalsaDocGenericParamSummary, SalsaDocObjectFieldKeySummary, SalsaDocTypeBinaryOperatorSummary,
    SalsaDocTypeIndexSummary, SalsaDocTypeKindSummary, SalsaDocTypeNodeKey,
    SalsaDocTypeNodeSummary, SalsaDocTypeUnaryOperatorSummary, SalsaSyntaxIdSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocTypeRef {
    Node(SalsaDocTypeNodeKey),
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTypeLoweredGenericParam {
    pub name: SmolStr,
    pub bound: SalsaDocTypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTypeLoweredParam {
    pub name: Option<SmolStr>,
    pub doc_type: SalsaDocTypeRef,
    pub is_dots: bool,
    pub is_nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTypeLoweredReturn {
    pub name: Option<SmolStr>,
    pub doc_type: SalsaDocTypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocTypeLoweredObjectFieldKey {
    Name(SmolStr),
    String(SmolStr),
    Integer(i64),
    Type(SalsaDocTypeRef),
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTypeLoweredObjectField {
    pub syntax_offset: TextSize,
    pub key: SalsaDocTypeLoweredObjectFieldKey,
    pub value_type: SalsaDocTypeRef,
    pub is_nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaDocTypeLoweredKind {
    Unknown,
    Name {
        name: SmolStr,
    },
    Infer {
        generic_name: SmolStr,
    },
    Array {
        item_type: SalsaDocTypeRef,
    },
    Function {
        is_async: bool,
        is_sync: bool,
        generic_params: Vec<SalsaDocTypeLoweredGenericParam>,
        params: Vec<SalsaDocTypeLoweredParam>,
        returns: Vec<SalsaDocTypeLoweredReturn>,
    },
    Object {
        fields: Vec<SalsaDocTypeLoweredObjectField>,
    },
    Union {
        item_types: Vec<SalsaDocTypeRef>,
    },
    Intersection {
        item_types: Vec<SalsaDocTypeRef>,
    },
    Binary {
        op: SalsaDocTypeBinaryOperatorSummary,
        left_type: SalsaDocTypeRef,
        right_type: SalsaDocTypeRef,
    },
    Unary {
        op: SalsaDocTypeUnaryOperatorSummary,
        inner_type: SalsaDocTypeRef,
    },
    Conditional {
        condition_type: SalsaDocTypeRef,
        true_type: SalsaDocTypeRef,
        false_type: SalsaDocTypeRef,
        has_new: Option<bool>,
    },
    Tuple {
        item_types: Vec<SalsaDocTypeRef>,
    },
    Literal {
        text: SmolStr,
    },
    Variadic {
        item_type: SalsaDocTypeRef,
    },
    Nullable {
        inner_type: SalsaDocTypeRef,
    },
    Generic {
        base_type: SalsaDocTypeRef,
        arg_types: Vec<SalsaDocTypeRef>,
    },
    StringTemplate {
        prefix: Option<SmolStr>,
        interpolated: Option<SmolStr>,
        suffix: Option<SmolStr>,
    },
    MultiLineUnion {
        item_types: Vec<SalsaDocTypeRef>,
    },
    Attribute {
        params: Vec<SalsaDocTypeLoweredParam>,
    },
    Mapped {
        key_types: Vec<SalsaDocTypeRef>,
        value_type: SalsaDocTypeRef,
        is_readonly: bool,
        is_optional: bool,
    },
    IndexAccess {
        base_type: SalsaDocTypeRef,
        index_type: SalsaDocTypeRef,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTypeLoweredNode {
    pub syntax_offset: TextSize,
    pub syntax_id: SalsaSyntaxIdSummary,
    pub kind: SalsaDocTypeLoweredKind,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTypeLoweredIndex {
    pub types: Vec<SalsaDocTypeLoweredNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTypeResolvedSummary {
    pub type_key: SalsaDocTypeNodeKey,
    pub syntax_offset: TextSize,
    pub doc_type: SalsaDocTypeNodeSummary,
    pub lowered: SalsaDocTypeLoweredNode,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaDocTypeResolvedIndex {
    pub types: Vec<SalsaDocTypeResolvedSummary>,
}

pub fn build_lowered_doc_type_index(
    doc_types: &SalsaDocTypeIndexSummary,
) -> SalsaDocTypeLoweredIndex {
    let node_index = doc_types
        .types
        .iter()
        .map(|doc_type| (doc_type.node_key(), doc_type))
        .collect::<HashMap<_, _>>();

    SalsaDocTypeLoweredIndex {
        types: doc_types
            .types
            .iter()
            .map(|doc_type| lower_doc_type_node(doc_type, &node_index))
            .collect(),
    }
}

pub fn build_resolved_doc_type_index(
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
) -> SalsaDocTypeResolvedIndex {
    let lowered_by_key = lowered_types
        .types
        .iter()
        .map(|doc_type| (SalsaDocTypeNodeKey(doc_type.syntax_id), doc_type))
        .collect::<HashMap<_, _>>();

    SalsaDocTypeResolvedIndex {
        types: doc_types
            .types
            .iter()
            .filter_map(|doc_type| {
                let type_key = doc_type.node_key();
                let lowered = lowered_by_key.get(&type_key)?;
                Some(SalsaDocTypeResolvedSummary {
                    type_key,
                    syntax_offset: doc_type.syntax_offset,
                    doc_type: doc_type.clone(),
                    lowered: (*lowered).clone(),
                })
            })
            .collect(),
    }
}

pub fn find_lowered_doc_type_at(
    lowered_index: &SalsaDocTypeLoweredIndex,
    syntax_offset: TextSize,
) -> Option<SalsaDocTypeLoweredNode> {
    if let Some(exact) = lowered_index
        .types
        .iter()
        .find(|doc_type| doc_type.syntax_offset == syntax_offset)
    {
        return Some(exact.clone());
    }

    lowered_index
        .types
        .iter()
        .filter(|doc_type| doc_type.syntax_id.contains_offset(syntax_offset))
        .min_by_key(|doc_type| (doc_type.syntax_id.span_len(), doc_type.syntax_offset))
        .cloned()
}

pub fn find_lowered_doc_type_by_key(
    lowered_index: &SalsaDocTypeLoweredIndex,
    node_key: SalsaDocTypeNodeKey,
) -> Option<SalsaDocTypeLoweredNode> {
    lowered_index
        .types
        .iter()
        .find(|doc_type| doc_type.syntax_id == node_key.0)
        .cloned()
}

pub fn find_resolved_doc_type_at(
    resolved_index: &SalsaDocTypeResolvedIndex,
    syntax_offset: TextSize,
) -> Option<SalsaDocTypeResolvedSummary> {
    if let Some(exact) = resolved_index
        .types
        .iter()
        .find(|doc_type| doc_type.syntax_offset == syntax_offset)
    {
        return Some(exact.clone());
    }

    resolved_index
        .types
        .iter()
        .filter(|doc_type| doc_type.doc_type.syntax_id.contains_offset(syntax_offset))
        .min_by_key(|doc_type| {
            (
                doc_type.doc_type.syntax_id.span_len(),
                doc_type.doc_type.syntax_offset,
            )
        })
        .cloned()
}

pub fn find_resolved_doc_type_by_key(
    resolved_index: &SalsaDocTypeResolvedIndex,
    node_key: SalsaDocTypeNodeKey,
) -> Option<SalsaDocTypeResolvedSummary> {
    resolved_index
        .types
        .iter()
        .find(|doc_type| doc_type.type_key == node_key)
        .cloned()
}

pub fn find_resolved_doc_type_by_key_from_parts(
    doc_types: &SalsaDocTypeIndexSummary,
    lowered_types: &SalsaDocTypeLoweredIndex,
    node_key: SalsaDocTypeNodeKey,
) -> Option<SalsaDocTypeResolvedSummary> {
    let doc_type = doc_types
        .types
        .iter()
        .find(|doc_type| doc_type.node_key() == node_key)?
        .clone();
    let lowered = find_lowered_doc_type_by_key(lowered_types, node_key)?;
    Some(SalsaDocTypeResolvedSummary {
        type_key: node_key,
        syntax_offset: doc_type.syntax_offset,
        doc_type,
        lowered,
    })
}

fn lower_doc_type_node(
    doc_type: &SalsaDocTypeNodeSummary,
    node_index: &HashMap<SalsaDocTypeNodeKey, &SalsaDocTypeNodeSummary>,
) -> SalsaDocTypeLoweredNode {
    SalsaDocTypeLoweredNode {
        syntax_offset: doc_type.syntax_offset,
        syntax_id: doc_type.syntax_id,
        kind: lower_doc_type_kind(&doc_type.kind, node_index),
    }
}

fn lower_doc_type_kind(
    kind: &SalsaDocTypeKindSummary,
    node_index: &HashMap<SalsaDocTypeNodeKey, &SalsaDocTypeNodeSummary>,
) -> SalsaDocTypeLoweredKind {
    match kind {
        SalsaDocTypeKindSummary::Name { name } => name
            .clone()
            .map(|name| SalsaDocTypeLoweredKind::Name { name })
            .unwrap_or(SalsaDocTypeLoweredKind::Unknown),
        SalsaDocTypeKindSummary::Infer { generic_name } => generic_name
            .clone()
            .map(|generic_name| SalsaDocTypeLoweredKind::Infer { generic_name })
            .unwrap_or(SalsaDocTypeLoweredKind::Unknown),
        SalsaDocTypeKindSummary::Array { item_type_offset } => SalsaDocTypeLoweredKind::Array {
            item_type: lower_type_ref(*item_type_offset),
        },
        SalsaDocTypeKindSummary::Function {
            is_async,
            is_sync,
            generic_params,
            params,
            returns,
        } => SalsaDocTypeLoweredKind::Function {
            is_async: *is_async,
            is_sync: *is_sync,
            generic_params: generic_params.iter().map(lower_generic_param).collect(),
            params: params.iter().map(lower_param).collect(),
            returns: returns.iter().map(lower_return).collect(),
        },
        SalsaDocTypeKindSummary::Object { fields } => SalsaDocTypeLoweredKind::Object {
            fields: fields.iter().map(lower_object_field).collect(),
        },
        SalsaDocTypeKindSummary::Binary {
            op,
            left_type_offset,
            right_type_offset,
        } => match op {
            SalsaDocTypeBinaryOperatorSummary::Union => SalsaDocTypeLoweredKind::Union {
                item_types: collect_binary_chain(
                    *left_type_offset,
                    *right_type_offset,
                    SalsaDocTypeBinaryOperatorSummary::Union,
                    node_index,
                ),
            },
            SalsaDocTypeBinaryOperatorSummary::Intersection => {
                SalsaDocTypeLoweredKind::Intersection {
                    item_types: collect_binary_chain(
                        *left_type_offset,
                        *right_type_offset,
                        SalsaDocTypeBinaryOperatorSummary::Intersection,
                        node_index,
                    ),
                }
            }
            _ => SalsaDocTypeLoweredKind::Binary {
                op: op.clone(),
                left_type: lower_type_ref(*left_type_offset),
                right_type: lower_type_ref(*right_type_offset),
            },
        },
        SalsaDocTypeKindSummary::Unary {
            op,
            inner_type_offset,
        } => SalsaDocTypeLoweredKind::Unary {
            op: op.clone(),
            inner_type: lower_type_ref(*inner_type_offset),
        },
        SalsaDocTypeKindSummary::Conditional {
            condition_type_offset,
            true_type_offset,
            false_type_offset,
            has_new,
        } => SalsaDocTypeLoweredKind::Conditional {
            condition_type: lower_type_ref(*condition_type_offset),
            true_type: lower_type_ref(*true_type_offset),
            false_type: lower_type_ref(*false_type_offset),
            has_new: *has_new,
        },
        SalsaDocTypeKindSummary::Tuple { item_type_offsets } => SalsaDocTypeLoweredKind::Tuple {
            item_types: item_type_offsets
                .iter()
                .copied()
                .map(SalsaDocTypeRef::Node)
                .collect(),
        },
        SalsaDocTypeKindSummary::Literal { text } => {
            SalsaDocTypeLoweredKind::Literal { text: text.clone() }
        }
        SalsaDocTypeKindSummary::Variadic { item_type_offset } => {
            SalsaDocTypeLoweredKind::Variadic {
                item_type: lower_type_ref(*item_type_offset),
            }
        }
        SalsaDocTypeKindSummary::Nullable { inner_type_offset } => {
            SalsaDocTypeLoweredKind::Nullable {
                inner_type: lower_type_ref(*inner_type_offset),
            }
        }
        SalsaDocTypeKindSummary::Generic {
            base_type_offset,
            arg_type_offsets,
        } => SalsaDocTypeLoweredKind::Generic {
            base_type: lower_type_ref(*base_type_offset),
            arg_types: arg_type_offsets
                .iter()
                .copied()
                .map(SalsaDocTypeRef::Node)
                .collect(),
        },
        SalsaDocTypeKindSummary::StringTemplate {
            prefix,
            interpolated,
            suffix,
        } => SalsaDocTypeLoweredKind::StringTemplate {
            prefix: prefix.clone(),
            interpolated: interpolated.clone(),
            suffix: suffix.clone(),
        },
        SalsaDocTypeKindSummary::MultiLineUnion { item_type_offsets } => {
            SalsaDocTypeLoweredKind::Union {
                item_types: item_type_offsets
                    .iter()
                    .copied()
                    .map(SalsaDocTypeRef::Node)
                    .collect(),
            }
        }
        SalsaDocTypeKindSummary::Attribute { params } => SalsaDocTypeLoweredKind::Attribute {
            params: params.iter().map(lower_param).collect(),
        },
        SalsaDocTypeKindSummary::Mapped {
            key_type_offsets,
            value_type_offset,
            is_readonly,
            is_optional,
        } => SalsaDocTypeLoweredKind::Mapped {
            key_types: key_type_offsets
                .iter()
                .copied()
                .map(SalsaDocTypeRef::Node)
                .collect(),
            value_type: lower_type_ref(*value_type_offset),
            is_readonly: *is_readonly,
            is_optional: *is_optional,
        },
        SalsaDocTypeKindSummary::IndexAccess {
            base_type_offset,
            index_type_offset,
        } => SalsaDocTypeLoweredKind::IndexAccess {
            base_type: lower_type_ref(*base_type_offset),
            index_type: lower_type_ref(*index_type_offset),
        },
    }
}

fn collect_binary_chain(
    left_type_offset: Option<SalsaDocTypeNodeKey>,
    right_type_offset: Option<SalsaDocTypeNodeKey>,
    target_op: SalsaDocTypeBinaryOperatorSummary,
    node_index: &HashMap<SalsaDocTypeNodeKey, &SalsaDocTypeNodeSummary>,
) -> Vec<SalsaDocTypeRef> {
    let mut item_types = Vec::new();
    append_binary_chain_item(left_type_offset, &target_op, node_index, &mut item_types);
    append_binary_chain_item(right_type_offset, &target_op, node_index, &mut item_types);
    item_types
}

fn append_binary_chain_item(
    type_offset: Option<SalsaDocTypeNodeKey>,
    target_op: &SalsaDocTypeBinaryOperatorSummary,
    node_index: &HashMap<SalsaDocTypeNodeKey, &SalsaDocTypeNodeSummary>,
    out: &mut Vec<SalsaDocTypeRef>,
) {
    let Some(type_offset) = type_offset else {
        out.push(SalsaDocTypeRef::Incomplete);
        return;
    };

    let Some(doc_type) = node_index.get(&type_offset) else {
        out.push(SalsaDocTypeRef::Node(type_offset));
        return;
    };

    match &doc_type.kind {
        SalsaDocTypeKindSummary::Binary {
            op,
            left_type_offset,
            right_type_offset,
        } if op == target_op => {
            append_binary_chain_item(*left_type_offset, target_op, node_index, out);
            append_binary_chain_item(*right_type_offset, target_op, node_index, out);
        }
        SalsaDocTypeKindSummary::MultiLineUnion { item_type_offsets }
            if *target_op == SalsaDocTypeBinaryOperatorSummary::Union =>
        {
            out.extend(item_type_offsets.iter().copied().map(SalsaDocTypeRef::Node));
        }
        _ => out.push(SalsaDocTypeRef::Node(type_offset)),
    }
}

fn lower_type_ref<T>(type_offset: Option<T>) -> SalsaDocTypeRef
where
    T: Into<SalsaDocTypeNodeKey>,
{
    type_offset
        .map(Into::into)
        .map(SalsaDocTypeRef::Node)
        .unwrap_or(SalsaDocTypeRef::Incomplete)
}

fn lower_generic_param(param: &SalsaDocGenericParamSummary) -> SalsaDocTypeLoweredGenericParam {
    SalsaDocTypeLoweredGenericParam {
        name: param.name.clone(),
        bound: lower_type_ref(param.type_offset),
    }
}

fn lower_param(param: &crate::SalsaDocTypedParamSummary) -> SalsaDocTypeLoweredParam {
    SalsaDocTypeLoweredParam {
        name: param.name.clone(),
        doc_type: lower_type_ref(param.type_offset),
        is_dots: param.is_dots,
        is_nullable: param.is_nullable,
    }
}

fn lower_return(return_info: &crate::SalsaDocReturnTypeSummary) -> SalsaDocTypeLoweredReturn {
    SalsaDocTypeLoweredReturn {
        name: return_info.name.clone(),
        doc_type: lower_type_ref(return_info.type_offset),
    }
}

fn lower_object_field(field: &crate::SalsaDocObjectFieldSummary) -> SalsaDocTypeLoweredObjectField {
    SalsaDocTypeLoweredObjectField {
        syntax_offset: field.syntax_offset,
        key: lower_object_field_key(&field.key),
        value_type: lower_type_ref(field.value_type_offset),
        is_nullable: field.is_nullable,
    }
}

fn lower_object_field_key(
    key: &SalsaDocObjectFieldKeySummary,
) -> SalsaDocTypeLoweredObjectFieldKey {
    match key {
        SalsaDocObjectFieldKeySummary::Name(name) => {
            SalsaDocTypeLoweredObjectFieldKey::Name(name.clone())
        }
        SalsaDocObjectFieldKeySummary::String(value) => {
            SalsaDocTypeLoweredObjectFieldKey::String(value.clone())
        }
        SalsaDocObjectFieldKeySummary::Integer(value) => {
            SalsaDocTypeLoweredObjectFieldKey::Integer(*value)
        }
        SalsaDocObjectFieldKeySummary::Type(type_offset) => {
            SalsaDocTypeLoweredObjectFieldKey::Type(SalsaDocTypeRef::Node(*type_offset))
        }
    }
}
