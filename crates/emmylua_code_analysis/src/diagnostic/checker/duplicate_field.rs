use std::collections::{HashMap, HashSet};

use crate::{
    DiagnosticCode, LuaDeclExtra, LuaMember, LuaMemberFeature, LuaMemberKey, LuaType,
    LuaTypeDeclId, SemanticModel,
};

use super::{Checker, DiagnosticContext};

pub struct DuplicateFieldChecker;

impl Checker for DuplicateFieldChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::DuplicateDocField,
        DiagnosticCode::DuplicateSetField,
    ];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let type_decl_id_set = get_type_decl_id(semantic_model);
        if let Some(type_decl_id_set) = type_decl_id_set {
            for type_decl_id in type_decl_id_set {
                check_class_duplicate_field(context, semantic_model, &type_decl_id);
            }
        }
    }
}

fn get_type_decl_id(semantic_model: &SemanticModel) -> Option<HashSet<LuaTypeDeclId>> {
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
                type_decl_id_set.insert(id);
            }
        }
    }

    Some(type_decl_id_set)
}
struct DiagnosticMemberInfo<'a> {
    typ: LuaType,
    feature: LuaMemberFeature,
    member: &'a LuaMember,
}

fn check_class_duplicate_field(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    type_decl_id: &LuaTypeDeclId,
) -> Option<()> {
    let type_decl = context
        .get_db()
        .get_type_index()
        .get_type_decl(type_decl_id)?;
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
            continue;
        }

        let mut member_infos = Vec::new();
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
