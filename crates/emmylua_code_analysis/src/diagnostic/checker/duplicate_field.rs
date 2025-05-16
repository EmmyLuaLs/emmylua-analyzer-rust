use std::collections::{HashMap, HashSet};

use emmylua_parser::{
    LuaAstNode, LuaDocTagField, LuaIndexExpr, LuaStat, LuaSyntaxKind, LuaSyntaxNode,
};

use crate::{
    DiagnosticCode, LuaDecl, LuaDeclExtra, LuaMember, LuaMemberFeature, LuaMemberKey,
    LuaSemanticDeclId, LuaType, LuaTypeDeclId, SemanticDeclLevel, SemanticModel,
};

use super::{Checker, DiagnosticContext};

pub struct DuplicateFieldChecker;

impl Checker for DuplicateFieldChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::DuplicateDocField,
        DiagnosticCode::DuplicateSetField,
    ];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let decl_set = get_decl_set(semantic_model);
        if let Some(decl_set) = decl_set {
            for decl_info in decl_set {
                check_decl_duplicate_field(context, semantic_model, &decl_info);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DeclInfo {
    id: LuaTypeDeclId,
    is_require: bool,
}

fn get_decl_set(semantic_model: &SemanticModel) -> Option<HashSet<DeclInfo>> {
    let file_id = semantic_model.get_file_id();
    let Some(decl_tree) = semantic_model
        .get_db()
        .get_decl_index()
        .get_decl_tree(&file_id)
    else {
        return None;
    };
    let mut type_decl_id_set = HashSet::new();
    for (decl_id, decl) in decl_tree.get_decls() {
        if matches!(
            &decl.extra,
            LuaDeclExtra::Local { .. } | LuaDeclExtra::Global { .. }
        ) {
            let decl_type = semantic_model.get_type((*decl_id).into());
            if let LuaType::Def(id) = decl_type {
                type_decl_id_set.insert(DeclInfo {
                    id,
                    is_require: is_require_decl(&decl),
                });
            }
        }
    }

    Some(type_decl_id_set)
}

fn is_require_decl(decl: &LuaDecl) -> bool {
    let Some(expr_id) = decl.get_value_syntax_id() else {
        return false;
    };
    expr_id.get_kind() == LuaSyntaxKind::RequireCallExpr
}

struct DiagnosticMemberInfo<'a> {
    typ: LuaType,
    feature: LuaMemberFeature,
    member: &'a LuaMember,
}

fn check_decl_duplicate_field(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    decl_info: &DeclInfo,
) -> Option<()> {
    let type_decl = context
        .get_db()
        .get_type_index()
        .get_type_decl(&decl_info.id)?;
    let file_id = context.file_id;

    let members = semantic_model
        .get_db()
        .get_member_index()
        .get_members(&type_decl.get_id().into())?;

    let mut member_map: HashMap<&LuaMemberKey, Vec<&LuaMember>> = HashMap::new();

    for member in members.iter() {
        // 过滤掉 meta 定义的 signature
        match member.get_feature() {
            LuaMemberFeature::MetaMethodDecl => {
                continue;
            }
            _ => {}
        }

        member_map
            .entry(member.get_key())
            .or_insert_with(Vec::new)
            .push(*member);
    }

    for (key, members) in member_map.iter() {
        if members.len() < 2 {
            // 需要特殊处理: require("a").fun = function() end
            if let Some(member) = members.first() {
                check_one_member(context, semantic_model, member, decl_info.is_require);
            }
            continue;
        }

        let mut member_infos = Vec::with_capacity(members.len());
        for member in members.iter() {
            let typ = semantic_model.get_type(member.get_id().into());
            let feature = member.get_feature();
            member_infos.push(DiagnosticMemberInfo {
                typ,
                feature,
                member,
            });
        }

        // 1. 检查 signature
        let signatures = member_infos
            .iter()
            .filter(|info| matches!(info.typ, LuaType::Signature(_)));
        if signatures.clone().count() > 1 {
            for signature in signatures {
                if signature.member.get_file_id() != file_id {
                    continue;
                }
                context.add_diagnostic(
                    DiagnosticCode::DuplicateSetField,
                    signature.member.get_range(),
                    t!("Duplicate field `%{name}`.", name = key.to_path()).to_string(),
                    None,
                );
            }
        }

        // 2. 检查 ---@field 成员
        let field_decls = member_infos
            .iter()
            .filter(|info| info.feature.is_field_decl())
            .collect::<Vec<_>>();
        // 如果 field_decls 数量大于1，则进一步检查
        if field_decls.len() > 1 {
            // 检查是否所有 field_decls 都是 DocFunction
            let all_doc_functions = field_decls
                .iter()
                .all(|info| matches!(info.typ, LuaType::DocFunction(_)));

            // 如果不全是 DocFunction，则报错
            if !all_doc_functions {
                for field_decl in &field_decls {
                    if field_decl.member.get_file_id() == file_id {
                        context.add_diagnostic(
                            DiagnosticCode::DuplicateDocField,
                            // TODO: 范围缩小到名称而不是整个 ---@field
                            field_decl.member.get_range(),
                            t!("Duplicate field `%{name}`.", name = key.to_path()).to_string(),
                            None,
                        );
                    }
                }
            }
        }
    }

    Some(())
}

fn check_one_member(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    member: &LuaMember,
    is_require: bool,
) -> Option<()> {
    let key = member.get_key();
    let member_id = member.get_id();
    let typ = semantic_model.get_type(member.get_id().into());
    // 如果不是 signature 则不需要检查
    if !matches!(typ, LuaType::Signature(_)) {
        return None;
    }
    let references = semantic_model
        .get_db()
        .get_reference_index()
        .get_index_references(&key)?;
    let root = semantic_model.get_root().syntax();
    let property_owner = LuaSemanticDeclId::Member(member_id);

    for in_filed in references {
        // 不同文件不检查
        if in_filed.file_id != context.file_id {
            continue;
        }
        // 不需要检查自身
        if in_filed.value == *member_id.get_syntax_id() {
            continue;
        }
        let node = in_filed.value.to_node_from_root(root)?;

        if !semantic_model.is_reference_to(
            node.clone(),
            property_owner.clone(),
            SemanticDeclLevel::default(),
        ) {
            continue;
        }

        // 如果不是赋值则不需要检查
        if check_function_member_is_set(&semantic_model, &node, is_require).is_none() {
            continue;
        }

        context.add_diagnostic(
            DiagnosticCode::DuplicateSetField,
            in_filed.value.get_range(),
            t!("Duplicate field `%{name}`.", name = key.to_path()).to_string(),
            None,
        );
    }

    Some(())
}

/// 检查是否是 .member = newValue
fn check_function_member_is_set(
    semantic_model: &SemanticModel,
    node: &LuaSyntaxNode,
    is_require: bool,
) -> Option<()> {
    match node {
        expr_node if LuaIndexExpr::can_cast(expr_node.kind().into()) => {
            let expr = LuaIndexExpr::cast(expr_node.clone())?;
            let prefix_type = semantic_model
                .infer_expr(expr.get_prefix_expr()?.into())
                .ok()?;
            match prefix_type {
                LuaType::Ref(_) => {
                    return None;
                }
                _ => {}
            };
            // 往上寻找 stat 节点
            let stat = expr.ancestors::<LuaStat>().next()?;
            match stat {
                LuaStat::FuncStat(_) => {
                    return Some(());
                }
                LuaStat::AssignStat(assign_stat) => {
                    // 判断是否在左侧
                    let (vars, exprs) = assign_stat.get_var_and_expr_list();
                    for (i, var) in vars.iter().enumerate() {
                        if var
                            .syntax()
                            .text_range()
                            .contains(node.text_range().start())
                        {
                            // 如果是 require 导入的, 则直接认为是重复字段
                            if is_require {
                                return Some(());
                            }
                            // 确定右侧表达式是否是 signature
                            if let Some(expr) = exprs.get(i) {
                                let expr_type =
                                    semantic_model.infer_expr(expr.clone().into()).ok()?;
                                if matches!(expr_type, LuaType::Signature(_)) {
                                    return Some(());
                                }
                            }
                            return None;
                        }
                    }
                    return None;
                }
                _ => {
                    return None;
                }
            }
        }
        tag_field_node if LuaDocTagField::can_cast(tag_field_node.kind().into()) => {
            return Some(());
        }
        _ => {}
    }

    Some(())
}
