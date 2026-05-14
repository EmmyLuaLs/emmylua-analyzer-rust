use std::collections::BTreeMap;

use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaChunk, LuaComment, LuaExpr, LuaVarExpr};
use rowan::TextSize;

use super::super::{
    SalsaBindingTargetSummary, SalsaDeclTreeSummary, SalsaDocOwnerBindingIndexSummary,
    SalsaDocOwnerBindingSummary, SalsaDocOwnerKindSummary, SalsaMemberIndexSummary,
    SalsaPropertyIndexSummary, SalsaPropertySourceSummary, SalsaSignatureIndexSummary,
};
use super::support::{extract_member_target_from_index_expr, find_visible_decl_before_offset};
use crate::compilation::summary_builder::summary::extend_property_owner_with_key;

pub fn analyze_doc_owner_binding_summary(
    decl_tree: &SalsaDeclTreeSummary,
    members: &SalsaMemberIndexSummary,
    properties: &SalsaPropertyIndexSummary,
    signatures: &SalsaSignatureIndexSummary,
    chunk: LuaChunk,
) -> SalsaDocOwnerBindingIndexSummary {
    let mut bindings = BTreeMap::<(u8, TextSize), SalsaDocOwnerBindingSummary>::new();

    for comment in chunk.descendants::<LuaComment>() {
        let Some(owner) = comment.get_owner() else {
            continue;
        };

        let owner_kind = owner_kind(&owner);
        let owner_offset = owner.get_position();
        let key = (owner_kind_key(&owner_kind), owner_offset);
        let entry = bindings
            .entry(key)
            .or_insert_with(|| SalsaDocOwnerBindingSummary {
                owner_kind: owner_kind.clone(),
                owner_offset,
                targets: Vec::new(),
            });

        for target in build_targets(owner, decl_tree, members, properties, signatures) {
            if !entry.targets.contains(&target) {
                entry.targets.push(target);
            }
        }
    }

    SalsaDocOwnerBindingIndexSummary {
        bindings: bindings.into_values().collect(),
    }
}

fn build_targets(
    owner: LuaAst,
    decl_tree: &SalsaDeclTreeSummary,
    _members: &SalsaMemberIndexSummary,
    properties: &SalsaPropertyIndexSummary,
    signatures: &SalsaSignatureIndexSummary,
) -> Vec<SalsaBindingTargetSummary> {
    match owner {
        LuaAst::LuaLocalStat(local_stat) => local_stat
            .get_local_name_list()
            .next()
            .and_then(|local_name| local_name.get_name_token())
            .and_then(|name| {
                decl_tree
                    .decls
                    .iter()
                    .find(|decl| decl.start_offset == name.get_position())
                    .map(|decl| SalsaBindingTargetSummary::Decl(decl.id))
            })
            .into_iter()
            .collect(),
        LuaAst::LuaAssignStat(assign_stat) => {
            let (vars, _) = assign_stat.get_var_and_expr_list();
            resolve_var_target(vars.first(), decl_tree)
                .into_iter()
                .collect()
        }
        LuaAst::LuaTableField(field) => properties
            .properties
            .iter()
            .filter(|property| {
                property.source == SalsaPropertySourceSummary::TableField
                    && property.syntax_offset == field.get_position()
            })
            .filter_map(|property| extend_property_owner_with_key(&property.owner, &property.key))
            .map(SalsaBindingTargetSummary::Member)
            .collect(),
        LuaAst::LuaFuncStat(func_stat) => func_stat
            .get_closure()
            .and_then(|closure| resolve_signature_target(closure.get_position(), signatures))
            .into_iter()
            .collect(),
        LuaAst::LuaLocalFuncStat(local_func_stat) => local_func_stat
            .get_closure()
            .and_then(|closure| resolve_signature_target(closure.get_position(), signatures))
            .into_iter()
            .collect(),
        LuaAst::LuaClosureExpr(closure) => {
            resolve_signature_target(closure.get_position(), signatures)
                .into_iter()
                .collect()
        }
        LuaAst::LuaCallExprStat(call_expr_stat) => call_expr_stat
            .get_call_expr()
            .and_then(|call_expr| {
                call_expr
                    .get_args_list()
                    .into_iter()
                    .flat_map(|args| args.get_args())
                    .find_map(|arg| match arg {
                        LuaExpr::ClosureExpr(closure) => {
                            resolve_signature_target(closure.get_position(), signatures)
                        }
                        _ => None,
                    })
            })
            .into_iter()
            .collect(),
        _ => owner
            .ancestors::<emmylua_parser::LuaClosureExpr>()
            .next()
            .and_then(|closure| resolve_signature_target(closure.get_position(), signatures))
            .into_iter()
            .collect(),
    }
}

fn resolve_var_target(
    var: Option<&LuaVarExpr>,
    decl_tree: &SalsaDeclTreeSummary,
) -> Option<SalsaBindingTargetSummary> {
    match var? {
        LuaVarExpr::NameExpr(name_expr) => {
            let name = name_expr.get_name_text()?;
            let decl = find_visible_decl_before_offset(decl_tree, &name, name_expr.get_position())
                .or_else(|| {
                    decl_tree.decls.iter().find(|decl| {
                        decl.start_offset == name_expr.get_position() && decl.name == name
                    })
                })?;
            Some(SalsaBindingTargetSummary::Decl(decl.id))
        }
        LuaVarExpr::IndexExpr(index_expr) => {
            extract_member_target_from_index_expr(decl_tree, index_expr)
                .map(|target| SalsaBindingTargetSummary::Member(target.into()))
        }
    }
}

fn resolve_signature_target(
    signature_offset: TextSize,
    signatures: &SalsaSignatureIndexSummary,
) -> Option<SalsaBindingTargetSummary> {
    signatures
        .signatures
        .iter()
        .find(|signature| signature.syntax_offset == signature_offset)
        .map(|signature| SalsaBindingTargetSummary::Signature(signature.syntax_offset))
}

fn owner_kind(owner: &LuaAst) -> SalsaDocOwnerKindSummary {
    match owner {
        LuaAst::LuaAssignStat(_) => SalsaDocOwnerKindSummary::AssignStat,
        LuaAst::LuaLocalStat(_) => SalsaDocOwnerKindSummary::LocalStat,
        LuaAst::LuaFuncStat(_) => SalsaDocOwnerKindSummary::FuncStat,
        LuaAst::LuaLocalFuncStat(_) => SalsaDocOwnerKindSummary::LocalFuncStat,
        LuaAst::LuaTableField(_) => SalsaDocOwnerKindSummary::TableField,
        LuaAst::LuaClosureExpr(_) => SalsaDocOwnerKindSummary::Closure,
        LuaAst::LuaCallExprStat(_) => SalsaDocOwnerKindSummary::CallExprStat,
        LuaAst::LuaReturnStat(_) => SalsaDocOwnerKindSummary::ReturnStat,
        _ => SalsaDocOwnerKindSummary::Other,
    }
}

fn owner_kind_key(kind: &SalsaDocOwnerKindSummary) -> u8 {
    match kind {
        SalsaDocOwnerKindSummary::None => 0,
        SalsaDocOwnerKindSummary::AssignStat => 1,
        SalsaDocOwnerKindSummary::LocalStat => 2,
        SalsaDocOwnerKindSummary::FuncStat => 3,
        SalsaDocOwnerKindSummary::LocalFuncStat => 4,
        SalsaDocOwnerKindSummary::TableField => 5,
        SalsaDocOwnerKindSummary::Closure => 6,
        SalsaDocOwnerKindSummary::CallExprStat => 7,
        SalsaDocOwnerKindSummary::ReturnStat => 8,
        SalsaDocOwnerKindSummary::Other => 9,
    }
}
