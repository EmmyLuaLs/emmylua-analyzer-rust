use emmylua_parser::{LuaAstNode, LuaChunk, LuaExpr};
use rowan::TextSize;
use smol_str::SmolStr;

use super::super::{SalsaLookupBucket, build_lookup_buckets, find_bucket_indices};
use super::doc_owner::collect_doc_owner_resolves_for_signature;
use super::doc_tag::{
    build_doc_tag_query_index_from_properties, collect_doc_tag_properties_for_owners_in_index,
    collect_doc_tags_for_owner_in_index,
};
use super::lexical::{find_call_use_in_index, find_member_use_at, find_name_use_at};
use super::{
    SalsaDeclTypeQueryIndex, SalsaDocTagQueryIndex, SalsaMemberTypeQueryIndex,
    SalsaProgramPointMemberTypeInfoSummary, SalsaProgramPointTypeInfoSummary,
    SalsaTypeNarrowSummary, build_semantic_value_shell_from_candidates,
    collect_active_type_narrows, find_member_type_at_program_point,
    find_name_type_at_program_point,
};
pub use crate::{SalsaSignatureReturnQueryIndex, SalsaSignatureReturnQuerySummary};

use crate::{
    SalsaCallSummary, SalsaCallUseSummary, SalsaDeclTreeSummary, SalsaDocGenericSummary,
    SalsaDocOperatorSummary, SalsaDocOwnerResolveIndex, SalsaDocOwnerResolveSummary,
    SalsaDocOwnerSummary, SalsaDocParamSummary, SalsaDocReturnSummary, SalsaDocSummary,
    SalsaDocTagKindSummary, SalsaDocTagPropertySummary, SalsaDocTypeIndexSummary,
    SalsaDocTypeLoweredIndex, SalsaDocTypeLoweredNode, SalsaDocTypeNodeKey, SalsaDocTypeRef,
    SalsaLexicalUseIndex, SalsaLocalAssignmentQueryIndex, SalsaMemberRootSummary,
    SalsaMemberTargetSummary, SalsaNameUseResolutionSummary, SalsaPropertyIndexSummary,
    SalsaSemanticResolveStateSummary, SalsaSignatureIndexSummary, SalsaSignatureParamSummary,
    SalsaSignatureReturnExprKindSummary, SalsaSignatureReturnResolveStateSummary,
    SalsaSignatureReturnValueSummary, SalsaSignatureSummary, SalsaSyntaxIdSummary,
    SalsaUseSiteIndexSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
struct SalsaSignatureSupportIndex {
    generics: Vec<SalsaDocGenericSummary>,
    params: Vec<SalsaDocParamSummary>,
    returns: Vec<SalsaDocReturnSummary>,
    operators: Vec<SalsaDocOperatorSummary>,
    lowered_types: Vec<SalsaDocTypeLoweredNode>,
    by_generic_offset: Vec<SalsaLookupBucket<TextSize>>,
    by_param_offset: Vec<SalsaLookupBucket<TextSize>>,
    by_return_offset: Vec<SalsaLookupBucket<TextSize>>,
    by_operator_offset: Vec<SalsaLookupBucket<TextSize>>,
    by_lowered_type_offset: Vec<SalsaLookupBucket<TextSize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureTypeExplainSummary {
    pub type_ref: SalsaDocTypeRef,
    pub lowered: Option<SalsaDocTypeLoweredNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureGenericParamExplainSummary {
    pub name: SmolStr,
    pub bound_type: Option<SalsaSignatureTypeExplainSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureGenericExplainSummary {
    pub syntax_offset: TextSize,
    pub params: Vec<SalsaSignatureGenericParamExplainSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureParamExplainSummary {
    pub name: SmolStr,
    pub syntax_offset: TextSize,
    pub is_vararg: bool,
    pub doc_param_offset: Option<TextSize>,
    pub doc_type: Option<SalsaSignatureTypeExplainSummary>,
    pub is_nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureReturnItemExplainSummary {
    pub name: Option<SmolStr>,
    pub doc_type: SalsaSignatureTypeExplainSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureReturnExplainSummary {
    pub syntax_offset: TextSize,
    pub items: Vec<SalsaSignatureReturnItemExplainSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureOperatorExplainSummary {
    pub name: SmolStr,
    pub syntax_offset: TextSize,
    pub param_types: Vec<SalsaSignatureTypeExplainSummary>,
    pub return_type: Option<SalsaSignatureTypeExplainSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureExplainSummary {
    pub signature: SalsaSignatureSummary,
    pub doc_owners: Vec<SalsaDocOwnerResolveSummary>,
    pub tag_properties: Vec<SalsaDocTagPropertySummary>,
    pub generics: Vec<SalsaSignatureGenericExplainSummary>,
    pub params: Vec<SalsaSignatureParamExplainSummary>,
    pub returns: Vec<SalsaSignatureReturnExplainSummary>,
    pub operators: Vec<SalsaSignatureOperatorExplainSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaCallExplainSummary {
    pub call: SalsaCallSummary,
    pub lexical_call: Option<SalsaCallUseSummary>,
    pub call_generic_types: Vec<SalsaSignatureTypeExplainSummary>,
    pub candidate_signature_offsets: Vec<TextSize>,
    pub resolved_signature_offset: Option<TextSize>,
    pub resolved_signature: Option<SalsaSignatureExplainSummary>,
    pub args: Vec<SalsaCallArgExplainSummary>,
    pub overload_returns: Vec<SalsaSignatureReturnExplainSummary>,
    pub returns: Vec<SalsaSignatureReturnExplainSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaCallArgExplainSummary {
    pub arg_index: usize,
    pub expr_offset: TextSize,
    pub expected_param: Option<SalsaSignatureParamExplainSummary>,
    pub expected_doc_type: Option<SalsaSignatureTypeExplainSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaSignatureExplainIndex {
    pub signatures: Vec<SalsaSignatureExplainSummary>,
    pub calls: Vec<SalsaCallExplainSummary>,
    by_signature_offset: Vec<SalsaLookupBucket<TextSize>>,
    by_call_offset: Vec<SalsaLookupBucket<TextSize>>,
    by_call_syntax_id: Vec<SalsaLookupBucket<SalsaSyntaxIdSummary>>,
    by_signature_name: Vec<SalsaLookupBucket<SmolStr>>,
}

pub fn build_signature_explain_index(
    signatures_index: &SalsaSignatureIndexSummary,
    owner_resolves: &SalsaDocOwnerResolveIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    tag_properties: &[SalsaDocTagPropertySummary],
    lowered_types: &SalsaDocTypeLoweredIndex,
    lexical_uses: &SalsaLexicalUseIndex,
    doc_generics: &[SalsaDocGenericSummary],
    doc_params: &[SalsaDocParamSummary],
    doc_returns: &[SalsaDocReturnSummary],
    doc_operators: &[SalsaDocOperatorSummary],
) -> SalsaSignatureExplainIndex {
    let tag_property_index = build_doc_tag_query_index_from_properties(tag_properties);
    let support_index = build_signature_support_index(
        lowered_types,
        doc_generics,
        doc_params,
        doc_returns,
        doc_operators,
    );
    let signatures = signatures_index
        .signatures
        .iter()
        .map(|signature| {
            build_signature_explain(
                signature,
                owner_resolves,
                &tag_property_index,
                &support_index,
            )
        })
        .collect::<Vec<_>>();

    let signature_offset_entries = signatures
        .iter()
        .enumerate()
        .map(|(index, signature)| (signature.signature.syntax_offset, index))
        .collect::<Vec<_>>();
    let signature_name_entries = signatures
        .iter()
        .enumerate()
        .filter_map(|(index, signature)| signature.signature.name.clone().map(|name| (name, index)))
        .collect::<Vec<_>>();
    let by_signature_offset = build_lookup_buckets(signature_offset_entries);
    let by_signature_name = build_lookup_buckets(signature_name_entries);
    let calls = signatures_index
        .calls
        .iter()
        .map(|call| {
            build_call_explain(
                call,
                &signatures,
                &by_signature_offset,
                &by_signature_name,
                doc_tag_query_index,
                &support_index,
                lexical_uses,
            )
        })
        .collect::<Vec<_>>();
    let by_call_offset = calls
        .iter()
        .enumerate()
        .map(|(index, call)| (call.call.syntax_offset, index))
        .collect::<Vec<_>>();
    let by_call_syntax_id = calls
        .iter()
        .enumerate()
        .map(|(index, call)| (call.call.syntax_id, index))
        .collect::<Vec<_>>();

    SalsaSignatureExplainIndex {
        signatures,
        calls,
        by_signature_offset,
        by_call_offset: build_lookup_buckets(by_call_offset),
        by_call_syntax_id: build_lookup_buckets(by_call_syntax_id),
        by_signature_name,
    }
}

pub fn find_signature_explain_at(
    index: &SalsaSignatureExplainIndex,
    signature_offset: TextSize,
) -> Option<SalsaSignatureExplainSummary> {
    find_signature_from_index(
        &index.signatures,
        &index.by_signature_offset,
        signature_offset,
    )
}

pub fn find_call_explain_at(
    index: &SalsaSignatureExplainIndex,
    call_offset: TextSize,
) -> Option<SalsaCallExplainSummary> {
    find_bucket_indices(&index.by_call_offset, &call_offset)
        .and_then(|indices| indices.first().copied())
        .map(|call_index| index.calls[call_index].clone())
}

pub fn find_call_explain_by_syntax_id(
    index: &SalsaSignatureExplainIndex,
    call_syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaCallExplainSummary> {
    find_bucket_indices(&index.by_call_syntax_id, &call_syntax_id)
        .and_then(|indices| indices.first().copied())
        .map(|call_index| index.calls[call_index].clone())
}

pub fn build_signature_return_query_index(
    signatures_index: &SalsaSignatureIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    decl_index: &SalsaDeclTypeQueryIndex,
    member_index: &SalsaMemberTypeQueryIndex,
    assignments: &SalsaLocalAssignmentQueryIndex,
    property_index: &SalsaPropertyIndexSummary,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
    lowered_types: &SalsaDocTypeLoweredIndex,
) -> SalsaSignatureReturnQueryIndex {
    SalsaSignatureReturnQueryIndex {
        signatures: signatures_index
            .signatures
            .iter()
            .map(|signature| {
                build_signature_return_query(
                    signature,
                    signature_explain_index,
                    use_sites,
                    decl_index,
                    member_index,
                    assignments,
                    property_index,
                    doc,
                    doc_types,
                    doc_tag_query_index,
                    decl_tree,
                    chunk,
                    lowered_types,
                )
            })
            .collect(),
    }
}

pub fn find_signature_return_query_at(
    index: &SalsaSignatureReturnQueryIndex,
    signature_offset: TextSize,
) -> Option<SalsaSignatureReturnQuerySummary> {
    index
        .signatures
        .iter()
        .find(|summary| summary.signature_offset == signature_offset)
        .cloned()
}

fn build_signature_return_query(
    signature: &SalsaSignatureSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    decl_index: &SalsaDeclTypeQueryIndex,
    member_index: &SalsaMemberTypeQueryIndex,
    assignments: &SalsaLocalAssignmentQueryIndex,
    property_index: &SalsaPropertyIndexSummary,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
    lowered_types: &SalsaDocTypeLoweredIndex,
) -> SalsaSignatureReturnQuerySummary {
    let doc_returns = find_signature_explain_at(signature_explain_index, signature.syntax_offset)
        .map(|summary| summary.returns)
        .unwrap_or_default();
    let values = signature
        .return_expr_offsets
        .iter()
        .enumerate()
        .map(|(return_slot_index, expr_offset)| {
            build_signature_return_value(
                *expr_offset,
                return_slot_index,
                &doc_returns,
                signature_explain_index,
                use_sites,
                decl_index,
                member_index,
                assignments,
                property_index,
                doc,
                doc_types,
                doc_tag_query_index,
                decl_tree,
                chunk,
                lowered_types,
            )
        })
        .collect::<Vec<_>>();
    let has_recursive_dependency = values.iter().any(|value| {
        value.call.as_ref().is_some_and(|call| {
            call.resolved_signature_offset == Some(signature.syntax_offset)
                || call
                    .candidate_signature_offsets
                    .contains(&signature.syntax_offset)
        })
    });
    let has_partial_value = values
        .iter()
        .any(|value| !signature_return_value_is_resolved(value));
    let state = if has_recursive_dependency {
        SalsaSignatureReturnResolveStateSummary::RecursiveDependency
    } else if has_partial_value {
        SalsaSignatureReturnResolveStateSummary::Partial
    } else {
        SalsaSignatureReturnResolveStateSummary::Resolved
    };

    SalsaSignatureReturnQuerySummary {
        signature_offset: signature.syntax_offset,
        state,
        doc_returns,
        values,
    }
}

fn build_signature_return_value(
    expr_offset: TextSize,
    return_slot_index: usize,
    doc_returns: &[SalsaSignatureReturnExplainSummary],
    signature_explain_index: &SalsaSignatureExplainIndex,
    use_sites: &SalsaUseSiteIndexSummary,
    decl_index: &SalsaDeclTypeQueryIndex,
    member_index: &SalsaMemberTypeQueryIndex,
    assignments: &SalsaLocalAssignmentQueryIndex,
    property_index: &SalsaPropertyIndexSummary,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
    lowered_types: &SalsaDocTypeLoweredIndex,
) -> SalsaSignatureReturnValueSummary {
    let doc_return_type_offsets =
        collect_signature_return_doc_slot_offsets(doc_returns, return_slot_index);

    let Some(expr) = find_expr_at_offset(chunk, expr_offset) else {
        return SalsaSignatureReturnValueSummary {
            expr_offset,
            kind: SalsaSignatureReturnExprKindSummary::Other,
            doc_return_type_offsets,
            name_type: None,
            member_type: None,
            call: None,
        };
    };

    match expr {
        LuaExpr::NameExpr(_) => SalsaSignatureReturnValueSummary {
            expr_offset,
            kind: SalsaSignatureReturnExprKindSummary::Name,
            doc_return_type_offsets,
            name_type: build_return_name_type(
                expr_offset,
                use_sites,
                decl_index,
                assignments,
                property_index,
                doc_types,
                signature_explain_index,
                doc_tag_query_index,
                decl_tree,
                chunk,
                lowered_types,
            ),
            member_type: None,
            call: None,
        },
        LuaExpr::IndexExpr(_) => SalsaSignatureReturnValueSummary {
            expr_offset,
            kind: SalsaSignatureReturnExprKindSummary::Member,
            doc_return_type_offsets,
            name_type: None,
            member_type: build_return_member_type(
                expr_offset,
                use_sites,
                member_index,
                decl_index,
                property_index,
                doc,
                doc_types,
                assignments,
                decl_tree,
                chunk,
                lowered_types,
                signature_explain_index,
                doc_tag_query_index,
            ),
            call: None,
        },
        LuaExpr::CallExpr(_) => SalsaSignatureReturnValueSummary {
            expr_offset,
            kind: SalsaSignatureReturnExprKindSummary::Call,
            doc_return_type_offsets,
            name_type: None,
            member_type: None,
            call: find_call_explain_at(signature_explain_index, expr_offset),
        },
        LuaExpr::LiteralExpr(_) => SalsaSignatureReturnValueSummary {
            expr_offset,
            kind: SalsaSignatureReturnExprKindSummary::Literal,
            doc_return_type_offsets,
            name_type: None,
            member_type: None,
            call: None,
        },
        LuaExpr::ClosureExpr(_) => SalsaSignatureReturnValueSummary {
            expr_offset,
            kind: SalsaSignatureReturnExprKindSummary::Closure,
            doc_return_type_offsets,
            name_type: None,
            member_type: None,
            call: None,
        },
        LuaExpr::TableExpr(_) => SalsaSignatureReturnValueSummary {
            expr_offset,
            kind: SalsaSignatureReturnExprKindSummary::Table,
            doc_return_type_offsets,
            name_type: None,
            member_type: None,
            call: None,
        },
        _ => SalsaSignatureReturnValueSummary {
            expr_offset,
            kind: SalsaSignatureReturnExprKindSummary::Other,
            doc_return_type_offsets,
            name_type: None,
            member_type: None,
            call: None,
        },
    }
}

fn collect_signature_return_doc_slot_offsets(
    doc_returns: &[SalsaSignatureReturnExplainSummary],
    return_slot_index: usize,
) -> Vec<SalsaDocTypeNodeKey> {
    doc_returns
        .iter()
        .filter_map(|return_row| return_row.items.get(return_slot_index))
        .filter_map(|item| match item.doc_type.type_ref {
            SalsaDocTypeRef::Node(type_offset) => Some(type_offset),
            SalsaDocTypeRef::Incomplete => None,
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn build_return_name_type(
    expr_offset: TextSize,
    use_sites: &SalsaUseSiteIndexSummary,
    decl_index: &SalsaDeclTypeQueryIndex,
    assignments: &SalsaLocalAssignmentQueryIndex,
    property_index: &SalsaPropertyIndexSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    signature_explain_index: &SalsaSignatureExplainIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
    lowered_types: &SalsaDocTypeLoweredIndex,
) -> Option<SalsaProgramPointTypeInfoSummary> {
    let name_use = find_name_use_at(use_sites, expr_offset)?;
    let active_narrows = match name_use.resolution {
        SalsaNameUseResolutionSummary::LocalDecl(decl_id) => {
            collect_active_type_narrows(decl_tree, chunk.clone(), decl_id, expr_offset)
        }
        SalsaNameUseResolutionSummary::Global => Vec::<SalsaTypeNarrowSummary>::new(),
    };

    Some(find_name_type_at_program_point(
        decl_index,
        &name_use,
        expr_offset,
        assignments,
        property_index,
        doc_types,
        signature_explain_index,
        doc_tag_query_index,
        decl_tree,
        chunk,
        lowered_types,
        &active_narrows,
    ))
}

fn build_return_member_type(
    expr_offset: TextSize,
    use_sites: &SalsaUseSiteIndexSummary,
    member_index: &SalsaMemberTypeQueryIndex,
    decl_index: &SalsaDeclTypeQueryIndex,
    property_index: &SalsaPropertyIndexSummary,
    doc: &SalsaDocSummary,
    doc_types: &SalsaDocTypeIndexSummary,
    assignments: &SalsaLocalAssignmentQueryIndex,
    decl_tree: &SalsaDeclTreeSummary,
    chunk: &LuaChunk,
    lowered_types: &SalsaDocTypeLoweredIndex,
    signature_explain_index: &SalsaSignatureExplainIndex,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
) -> Option<SalsaProgramPointMemberTypeInfoSummary> {
    let member_use = find_member_use_at(use_sites, expr_offset)?;
    Some(find_member_type_at_program_point(
        member_index,
        property_index,
        decl_index,
        doc,
        doc_types,
        &member_use,
        expr_offset,
        assignments,
        decl_tree,
        chunk,
        lowered_types,
        signature_explain_index,
        doc_tag_query_index,
    ))
}

fn signature_return_value_is_resolved(value: &SalsaSignatureReturnValueSummary) -> bool {
    if !value.doc_return_type_offsets.is_empty() {
        return true;
    }

    match value.kind {
        SalsaSignatureReturnExprKindSummary::Name => value.name_type.as_ref().is_some_and(|info| {
            build_semantic_value_shell_from_candidates(&info.candidates).state
                == SalsaSemanticResolveStateSummary::Resolved
        }),
        SalsaSignatureReturnExprKindSummary::Member => {
            value.member_type.as_ref().is_some_and(|info| {
                build_semantic_value_shell_from_candidates(&info.candidates).state
                    == SalsaSemanticResolveStateSummary::Resolved
            })
        }
        SalsaSignatureReturnExprKindSummary::Call => {
            value.call.as_ref().is_some_and(call_explain_is_resolved)
        }
        SalsaSignatureReturnExprKindSummary::Literal
        | SalsaSignatureReturnExprKindSummary::Closure
        | SalsaSignatureReturnExprKindSummary::Table => true,
        _ => false,
    }
}

pub fn call_explain_is_resolved(call: &SalsaCallExplainSummary) -> bool {
    call.resolved_signature_offset.is_some() || !call.returns.is_empty()
}

fn find_expr_at_offset(chunk: &LuaChunk, expr_offset: TextSize) -> Option<LuaExpr> {
    chunk
        .descendants::<LuaExpr>()
        .find(|expr| TextSize::from(u32::from(expr.get_position())) == expr_offset)
}

fn build_signature_explain(
    signature: &SalsaSignatureSummary,
    owner_resolves: &SalsaDocOwnerResolveIndex,
    tag_properties: &super::doc_tag::SalsaDocTagQueryIndex,
    support_index: &SalsaSignatureSupportIndex,
) -> SalsaSignatureExplainSummary {
    let doc_owners =
        collect_doc_owner_resolves_for_signature(owner_resolves, signature.syntax_offset);
    let tag_properties = collect_tag_properties_for_resolves(&doc_owners, tag_properties);
    let generics = signature
        .doc_generic_offsets
        .iter()
        .filter_map(|offset| find_generic_at(support_index, *offset))
        .map(|generic| SalsaSignatureGenericExplainSummary {
            syntax_offset: generic.syntax_offset,
            params: generic
                .params
                .iter()
                .map(|param| SalsaSignatureGenericParamExplainSummary {
                    name: param.name.clone(),
                    bound_type: param.type_offset.map(|type_offset| {
                        build_type_explain(SalsaDocTypeRef::Node(type_offset), support_index)
                    }),
                })
                .collect(),
        })
        .collect();
    let params = signature
        .params
        .iter()
        .map(|param| build_param_explain(param, signature, support_index))
        .collect();
    let returns = signature
        .doc_return_offsets
        .iter()
        .filter_map(|offset| find_return_at(support_index, *offset))
        .map(|return_info| SalsaSignatureReturnExplainSummary {
            syntax_offset: return_info.syntax_offset,
            items: return_info
                .items
                .iter()
                .map(|item| SalsaSignatureReturnItemExplainSummary {
                    name: item.name.clone(),
                    doc_type: build_type_explain(
                        SalsaDocTypeRef::Node(item.type_offset),
                        support_index,
                    ),
                })
                .collect(),
        })
        .collect();
    let operators = signature
        .doc_operator_offsets
        .iter()
        .filter_map(|offset| find_operator_at(support_index, *offset))
        .map(|operator| SalsaSignatureOperatorExplainSummary {
            name: operator.name.clone(),
            syntax_offset: operator.syntax_offset,
            param_types: operator
                .param_type_offsets
                .iter()
                .map(|type_offset| {
                    build_type_explain(SalsaDocTypeRef::Node(*type_offset), support_index)
                })
                .collect(),
            return_type: operator.return_type_offset.map(|type_offset| {
                build_type_explain(SalsaDocTypeRef::Node(type_offset), support_index)
            }),
        })
        .collect();

    SalsaSignatureExplainSummary {
        signature: signature.clone(),
        doc_owners,
        tag_properties,
        generics,
        params,
        returns,
        operators,
    }
}

fn build_param_explain(
    param: &SalsaSignatureParamSummary,
    signature: &SalsaSignatureSummary,
    support_index: &SalsaSignatureSupportIndex,
) -> SalsaSignatureParamExplainSummary {
    let doc_param = signature
        .doc_param_offsets
        .iter()
        .filter_map(|offset| find_param_at(support_index, *offset))
        .find(|doc_param| doc_param.name == param.name || (param.is_vararg && doc_param.is_vararg));

    SalsaSignatureParamExplainSummary {
        name: param.name.clone(),
        syntax_offset: param.syntax_offset,
        is_vararg: param.is_vararg,
        doc_param_offset: doc_param.as_ref().map(|doc_param| doc_param.syntax_offset),
        doc_type: doc_param.as_ref().map(|doc_param| {
            build_type_explain(
                doc_param
                    .type_offset
                    .map(SalsaDocTypeRef::Node)
                    .unwrap_or(SalsaDocTypeRef::Incomplete),
                support_index,
            )
        }),
        is_nullable: doc_param
            .as_ref()
            .is_some_and(|doc_param| doc_param.is_nullable),
    }
}

fn build_call_explain(
    call: &SalsaCallSummary,
    signatures: &[SalsaSignatureExplainSummary],
    signature_offsets: &[SalsaLookupBucket<TextSize>],
    signature_names: &[SalsaLookupBucket<SmolStr>],
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    support_index: &SalsaSignatureSupportIndex,
    lexical_uses: &SalsaLexicalUseIndex,
) -> SalsaCallExplainSummary {
    let lexical_call = find_call_use_in_index(lexical_uses, call.syntax_offset);
    let all_candidate_signature_offsets =
        collect_candidate_signature_offsets(lexical_call.as_ref(), signatures, signature_names);
    let candidate_signature_offsets = select_best_candidate_signature_offsets(
        &all_candidate_signature_offsets,
        lexical_call.as_ref(),
        signatures,
        signature_offsets,
    );
    let resolved_signature_offset = if candidate_signature_offsets.len() == 1 {
        Some(candidate_signature_offsets[0])
    } else {
        None
    };
    let resolved_signature = resolved_signature_offset.and_then(|signature_offset| {
        find_signature_from_index(signatures, signature_offsets, signature_offset)
    });
    let overload_returns = resolved_signature
        .as_ref()
        .map(|signature| {
            collect_signature_return_overload_rows(signature, doc_tag_query_index, support_index)
        })
        .unwrap_or_default();
    let args = call
        .arg_expr_offsets
        .iter()
        .enumerate()
        .map(|(arg_index, expr_offset)| {
            let expected_param = resolved_signature.as_ref().and_then(|signature| {
                signature_call_param_index(signature, call.is_colon_call, arg_index)
                    .and_then(|param_index| signature.params.get(param_index).cloned())
            });

            SalsaCallArgExplainSummary {
                arg_index,
                expr_offset: *expr_offset,
                expected_doc_type: expected_param
                    .as_ref()
                    .and_then(|param| param.doc_type.clone()),
                expected_param,
            }
        })
        .collect();
    let doc_returns = resolved_signature
        .as_ref()
        .map(|signature| signature.returns.clone())
        .unwrap_or_default();
    let returns = if !overload_returns.is_empty() {
        overload_returns.clone()
    } else {
        doc_returns
    };

    SalsaCallExplainSummary {
        call: call.clone(),
        lexical_call,
        call_generic_types: call
            .call_generic_type_offsets
            .iter()
            .map(|type_offset| {
                build_type_explain(SalsaDocTypeRef::Node(*type_offset), support_index)
            })
            .collect(),
        candidate_signature_offsets,
        resolved_signature_offset,
        resolved_signature,
        args,
        overload_returns,
        returns,
    }
}

fn collect_signature_return_overload_rows(
    signature: &SalsaSignatureExplainSummary,
    doc_tag_query_index: &SalsaDocTagQueryIndex,
    support_index: &SalsaSignatureSupportIndex,
) -> Vec<SalsaSignatureReturnExplainSummary> {
    let mut overload_tags = signature
        .doc_owners
        .iter()
        .flat_map(|owner| {
            collect_doc_tags_for_owner_in_index(
                doc_tag_query_index,
                &SalsaDocOwnerSummary {
                    kind: owner.owner_kind.clone(),
                    syntax_offset: Some(owner.owner_offset),
                },
            )
        })
        .filter(|tag| tag.kind == SalsaDocTagKindSummary::ReturnOverload)
        .collect::<Vec<_>>();
    overload_tags.sort_by_key(|tag| tag.syntax_offset);
    overload_tags.dedup_by_key(|tag| tag.syntax_offset);

    overload_tags
        .into_iter()
        .map(|tag| SalsaSignatureReturnExplainSummary {
            syntax_offset: tag.syntax_offset,
            items: tag
                .type_offsets()
                .iter()
                .map(|type_offset| SalsaSignatureReturnItemExplainSummary {
                    name: None,
                    doc_type: build_type_explain(
                        SalsaDocTypeRef::Node(*type_offset),
                        support_index,
                    ),
                })
                .collect(),
        })
        .filter(|row| !row.items.is_empty())
        .collect()
}

fn collect_candidate_signature_offsets(
    call: Option<&SalsaCallUseSummary>,
    signatures: &[SalsaSignatureExplainSummary],
    signature_names: &[SalsaLookupBucket<SmolStr>],
) -> Vec<TextSize> {
    let Some(call) = call else {
        return Vec::new();
    };

    let target_name = if let Some(member_target) = &call.callee_member {
        Some(build_member_full_name(member_target))
    } else {
        call.callee_name.clone()
    };
    let Some(target_name) = target_name else {
        return Vec::new();
    };

    find_bucket_indices(signature_names, &target_name)
        .into_iter()
        .flatten()
        .map(|signature_index| signatures[*signature_index].signature.syntax_offset)
        .collect()
}

fn select_best_candidate_signature_offsets(
    candidate_signature_offsets: &[TextSize],
    call: Option<&SalsaCallUseSummary>,
    signatures: &[SalsaSignatureExplainSummary],
    signature_offsets: &[SalsaLookupBucket<TextSize>],
) -> Vec<TextSize> {
    let mut best_score = None;
    let mut best_offsets = Vec::new();

    for signature_offset in candidate_signature_offsets {
        let Some(signature) =
            find_signature_from_index(signatures, signature_offsets, *signature_offset)
        else {
            continue;
        };
        let Some(score) = signature_call_candidate_score(&signature, call) else {
            continue;
        };

        match best_score {
            Some(current_best) if score < current_best => {}
            Some(current_best) if score == current_best => best_offsets.push(*signature_offset),
            _ => {
                best_score = Some(score);
                best_offsets.clear();
                best_offsets.push(*signature_offset);
            }
        }
    }

    if best_offsets.is_empty() {
        candidate_signature_offsets.to_vec()
    } else {
        best_offsets
    }
}

fn signature_call_candidate_score(
    signature: &SalsaSignatureExplainSummary,
    call: Option<&SalsaCallUseSummary>,
) -> Option<(u8, u8, u8, u8, u8, usize)> {
    let call = call?;
    let adjusted_arg_count =
        signature_call_adjusted_arg_count(signature, call.is_colon_call, call.arg_count);
    let param_count = signature.params.len();
    let has_vararg = signature.params.last().is_some_and(|param| param.is_vararg);
    let fixed_param_count = if has_vararg {
        param_count.saturating_sub(1)
    } else {
        param_count
    };
    let missing_param_start = adjusted_arg_count.min(param_count);

    if adjusted_arg_count < fixed_param_count
        && !signature.params[adjusted_arg_count..fixed_param_count]
            .iter()
            .all(|param| param.is_nullable)
    {
        return None;
    }

    if adjusted_arg_count < param_count
        && !signature.params[missing_param_start..]
            .iter()
            .all(|param| param.is_vararg || param.is_nullable)
    {
        return None;
    }

    if adjusted_arg_count > param_count && !has_vararg {
        return None;
    }

    let has_missing_non_vararg_params = signature.params[missing_param_start..]
        .iter()
        .any(|param| !param.is_vararg);

    let compatibility_class = if !has_vararg && adjusted_arg_count == param_count {
        6
    } else if adjusted_arg_count < param_count && has_missing_non_vararg_params {
        5
    } else if has_vararg && adjusted_arg_count == param_count {
        4
    } else if has_vararg && adjusted_arg_count == fixed_param_count {
        3
    } else if has_vararg && adjusted_arg_count > fixed_param_count {
        2
    } else {
        1
    };
    let colon_shape_match = u8::from(signature.signature.is_method == call.is_colon_call);
    let exact_param_alignment = u8::from(adjusted_arg_count == param_count);
    let matched_non_vararg_params = adjusted_arg_count.min(fixed_param_count);
    let (typed_param_matches, specific_param_matches) =
        signature_call_doc_param_quality(signature, matched_non_vararg_params);

    Some((
        compatibility_class,
        colon_shape_match,
        exact_param_alignment,
        specific_param_matches,
        typed_param_matches,
        matched_non_vararg_params,
    ))
}

fn signature_call_doc_param_quality(
    signature: &SalsaSignatureExplainSummary,
    matched_non_vararg_params: usize,
) -> (u8, u8) {
    let mut typed_param_matches = 0u8;
    let mut specific_param_matches = 0u8;

    for param in signature.params.iter().take(matched_non_vararg_params) {
        if let Some(doc_type) = &param.doc_type {
            typed_param_matches = typed_param_matches.saturating_add(1);
            if !signature_type_is_any_like(doc_type) {
                specific_param_matches = specific_param_matches.saturating_add(1);
            }
        }
    }

    (typed_param_matches, specific_param_matches)
}

fn signature_type_is_any_like(doc_type: &SalsaSignatureTypeExplainSummary) -> bool {
    match doc_type.lowered.as_ref().map(|lowered| &lowered.kind) {
        Some(crate::SalsaDocTypeLoweredKind::Unknown) => true,
        Some(crate::SalsaDocTypeLoweredKind::Name { name }) => {
            name.eq_ignore_ascii_case("any") || name.eq_ignore_ascii_case("unknown")
        }
        _ => false,
    }
}

fn signature_call_adjusted_arg_count(
    signature: &SalsaSignatureExplainSummary,
    is_colon_call: bool,
    arg_count: usize,
) -> usize {
    match (signature.signature.is_method, is_colon_call) {
        (true, false) => arg_count.saturating_sub(1),
        (false, true) => arg_count.saturating_add(1),
        _ => arg_count,
    }
}

fn signature_call_param_index(
    signature: &SalsaSignatureExplainSummary,
    is_colon_call: bool,
    arg_index: usize,
) -> Option<usize> {
    match (signature.signature.is_method, is_colon_call) {
        (true, false) => arg_index.checked_sub(1),
        (false, true) => arg_index.checked_add(1),
        _ => Some(arg_index),
    }
}

fn build_type_explain(
    type_ref: SalsaDocTypeRef,
    support_index: &SalsaSignatureSupportIndex,
) -> SalsaSignatureTypeExplainSummary {
    let lowered = match type_ref {
        SalsaDocTypeRef::Node(type_key) => find_lowered_type(type_key, support_index),
        SalsaDocTypeRef::Incomplete => None,
    };

    SalsaSignatureTypeExplainSummary { type_ref, lowered }
}

fn find_lowered_type(
    type_key: SalsaDocTypeNodeKey,
    support_index: &SalsaSignatureSupportIndex,
) -> Option<SalsaDocTypeLoweredNode> {
    let syntax_offset: TextSize = type_key.into();
    find_bucket_indices(&support_index.by_lowered_type_offset, &syntax_offset)
        .and_then(|indices| indices.first().copied())
        .map(|type_index| support_index.lowered_types[type_index].clone())
}

fn collect_tag_properties_for_resolves(
    resolves: &[SalsaDocOwnerResolveSummary],
    tag_properties: &super::doc_tag::SalsaDocTagQueryIndex,
) -> Vec<SalsaDocTagPropertySummary> {
    let owners = resolves
        .iter()
        .map(resolve_owner_summary)
        .collect::<Vec<_>>();
    collect_doc_tag_properties_for_owners_in_index(tag_properties, &owners)
}

fn build_signature_support_index(
    lowered_types: &SalsaDocTypeLoweredIndex,
    doc_generics: &[SalsaDocGenericSummary],
    doc_params: &[SalsaDocParamSummary],
    doc_returns: &[SalsaDocReturnSummary],
    doc_operators: &[SalsaDocOperatorSummary],
) -> SalsaSignatureSupportIndex {
    SalsaSignatureSupportIndex {
        generics: doc_generics.to_vec(),
        params: doc_params.to_vec(),
        returns: doc_returns.to_vec(),
        operators: doc_operators.to_vec(),
        lowered_types: lowered_types.types.clone(),
        by_generic_offset: build_lookup_buckets(
            doc_generics
                .iter()
                .enumerate()
                .map(|(index, generic)| (generic.syntax_offset, index))
                .collect(),
        ),
        by_param_offset: build_lookup_buckets(
            doc_params
                .iter()
                .enumerate()
                .map(|(index, param)| (param.syntax_offset, index))
                .collect(),
        ),
        by_return_offset: build_lookup_buckets(
            doc_returns
                .iter()
                .enumerate()
                .map(|(index, return_info)| (return_info.syntax_offset, index))
                .collect(),
        ),
        by_operator_offset: build_lookup_buckets(
            doc_operators
                .iter()
                .enumerate()
                .map(|(index, operator)| (operator.syntax_offset, index))
                .collect(),
        ),
        by_lowered_type_offset: build_lookup_buckets(
            lowered_types
                .types
                .iter()
                .enumerate()
                .map(|(index, doc_type)| (doc_type.syntax_offset, index))
                .collect(),
        ),
    }
}

fn find_generic_at(
    support_index: &SalsaSignatureSupportIndex,
    syntax_offset: TextSize,
) -> Option<SalsaDocGenericSummary> {
    find_bucket_indices(&support_index.by_generic_offset, &syntax_offset)
        .and_then(|indices| indices.first().copied())
        .map(|generic_index| support_index.generics[generic_index].clone())
}

fn find_param_at(
    support_index: &SalsaSignatureSupportIndex,
    syntax_offset: TextSize,
) -> Option<SalsaDocParamSummary> {
    find_bucket_indices(&support_index.by_param_offset, &syntax_offset)
        .and_then(|indices| indices.first().copied())
        .map(|param_index| support_index.params[param_index].clone())
}

fn find_return_at(
    support_index: &SalsaSignatureSupportIndex,
    syntax_offset: TextSize,
) -> Option<SalsaDocReturnSummary> {
    find_bucket_indices(&support_index.by_return_offset, &syntax_offset)
        .and_then(|indices| indices.first().copied())
        .map(|return_index| support_index.returns[return_index].clone())
}

fn find_operator_at(
    support_index: &SalsaSignatureSupportIndex,
    syntax_offset: TextSize,
) -> Option<SalsaDocOperatorSummary> {
    find_bucket_indices(&support_index.by_operator_offset, &syntax_offset)
        .and_then(|indices| indices.first().copied())
        .map(|operator_index| support_index.operators[operator_index].clone())
}

fn find_signature_from_index(
    signatures: &[SalsaSignatureExplainSummary],
    signature_offsets: &[SalsaLookupBucket<TextSize>],
    signature_offset: TextSize,
) -> Option<SalsaSignatureExplainSummary> {
    find_bucket_indices(signature_offsets, &signature_offset)
        .and_then(|indices| indices.first().copied())
        .map(|signature_index| signatures[signature_index].clone())
}

fn resolve_owner_summary(resolve: &SalsaDocOwnerResolveSummary) -> SalsaDocOwnerSummary {
    SalsaDocOwnerSummary {
        kind: resolve.owner_kind.clone(),
        syntax_offset: Some(resolve.owner_offset),
    }
}

fn build_member_full_name(target: &SalsaMemberTargetSummary) -> SmolStr {
    let mut full_name = String::new();

    match &target.root {
        SalsaMemberRootSummary::Global(crate::SalsaGlobalRootSummary::Env) => {
            full_name.push_str("_ENV")
        }
        SalsaMemberRootSummary::Global(crate::SalsaGlobalRootSummary::Name(name)) => {
            full_name.push_str(name)
        }
        SalsaMemberRootSummary::LocalDecl { name, .. } => full_name.push_str(name),
    }

    for segment in target.owner_segments.iter() {
        if !full_name.is_empty() {
            full_name.push('.');
        }
        full_name.push_str(segment);
    }

    if !target.member_name.is_empty() {
        if !full_name.is_empty() {
            full_name.push('.');
        }
        full_name.push_str(&target.member_name);
    }

    full_name.into()
}
