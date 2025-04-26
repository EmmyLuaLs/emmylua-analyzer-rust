use std::collections::HashMap;

use emmylua_parser::{LuaAstNode, LuaDocTag, LuaDocTagClass};

use crate::{DiagnosticCode, LuaMember, LuaMemberFeature, LuaMemberKey, LuaType, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct DuplicateFieldChecker;

impl Checker for DuplicateFieldChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::DuplicateDocField,
        DiagnosticCode::DuplicateSetField,
    ];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();

        for tag in root.descendants::<LuaDocTag>() {
            match tag {
                LuaDocTag::Class(class_tag) => {
                    check_class_duplicate_field(context, semantic_model, class_tag);
                }
                _ => {}
            }
        }
    }
}

struct DiagnosticMemberInfo<'a> {
    typ: LuaType,
    feature: LuaMemberFeature,
    member: &'a LuaMember,
}

fn check_class_duplicate_field(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    class_tag: LuaDocTagClass,
) -> Option<()> {
    let file_id = context.file_id;
    let name_token = class_tag.get_name_token()?;
    let name = name_token.get_name_text();
    let type_decl = context
        .get_db()
        .get_type_index()
        .find_type_decl(file_id, name)?;

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
