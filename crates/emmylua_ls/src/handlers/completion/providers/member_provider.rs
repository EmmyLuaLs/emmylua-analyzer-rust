use emmylua_code_analysis::{
    DbIndex, LuaMemberInfo, LuaMemberKey, LuaSemanticDeclId, LuaType, LuaTypeDeclId, SemanticModel,
    enum_variable_is_param,
};
use emmylua_parser::{LuaAstNode, LuaAstToken, LuaIndexExpr, LuaStringToken};
use std::collections::HashMap;

use crate::handlers::completion::{
    add_completions::{CompletionTriggerStatus, add_member_completion},
    completion_builder::CompletionBuilder,
};

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let index_expr = LuaIndexExpr::cast(builder.trigger_token.parent()?)?;
    let index_token = index_expr.get_index_token()?;
    let completion_status = if index_token.is_dot() {
        CompletionTriggerStatus::Dot
    } else if index_token.is_colon() {
        CompletionTriggerStatus::Colon
    } else if LuaStringToken::can_cast(builder.trigger_token.kind().into()) {
        CompletionTriggerStatus::InString
    } else {
        CompletionTriggerStatus::LeftBracket
    };

    let prefix_expr = index_expr.get_prefix_expr()?;
    let prefix_type = builder.semantic_model.infer_expr(prefix_expr.into()).ok()?;
    // 如果是枚举类型且为函数参数, 则不进行补全
    if enum_variable_is_param(
        builder.semantic_model.get_db(),
        &mut builder.semantic_model.get_cache().borrow_mut(),
        &index_expr,
        &prefix_type,
    )
    .is_some()
    {
        return None;
    }

    let member_info_map = builder.semantic_model.get_member_info_map(&prefix_type)?;

    add_completions_for_members(builder, &member_info_map, completion_status)
}

pub fn add_completions_for_members(
    builder: &mut CompletionBuilder,
    members: &HashMap<LuaMemberKey, Vec<LuaMemberInfo>>,
    completion_status: CompletionTriggerStatus,
) -> Option<()> {
    // 排序
    let mut sorted_entries: Vec<_> = members.iter().collect();
    sorted_entries.sort_unstable_by(|(name1, _), (name2, _)| name1.cmp(name2));

    for (_, member_infos) in sorted_entries {
        add_resolve_member_infos(builder, &member_infos, completion_status);
    }

    Some(())
}

fn add_resolve_member_infos(
    builder: &mut CompletionBuilder,
    member_infos: &Vec<LuaMemberInfo>,
    completion_status: CompletionTriggerStatus,
) -> Option<()> {
    if member_infos.len() == 1 {
        let member_info = &member_infos[0];
        let overload_count = match &member_info.typ {
            LuaType::DocFunction(_) => None,
            LuaType::Signature(id) => {
                if let Some(signature) = builder
                    .semantic_model
                    .get_db()
                    .get_signature_index()
                    .get(&id)
                {
                    let count = signature.overloads.len();
                    if count == 0 { None } else { Some(count) }
                } else {
                    None
                }
            }
            _ => None,
        };
        add_member_completion(
            builder,
            member_info.clone(),
            completion_status,
            overload_count,
        );
        return Some(());
    }

    let (filtered_member_infos, overload_count) =
        filter_member_infos(&builder.semantic_model, member_infos)?;

    let resolve_state = get_resolve_state(builder.semantic_model.get_db(), &filtered_member_infos);

    for member_info in filtered_member_infos {
        match resolve_state {
            MemberResolveState::All => {
                add_member_completion(
                    builder,
                    member_info.clone(),
                    completion_status,
                    overload_count,
                );
            }
            MemberResolveState::Meta => {
                if let Some(feature) = member_info.feature {
                    if feature.is_meta_decl() {
                        add_member_completion(
                            builder,
                            member_info.clone(),
                            completion_status,
                            overload_count,
                        );
                    }
                }
            }
            MemberResolveState::FileDecl => {
                if let Some(feature) = member_info.feature {
                    if feature.is_file_decl() {
                        add_member_completion(
                            builder,
                            member_info.clone(),
                            completion_status,
                            overload_count,
                        );
                    }
                }
            }
        }
    }

    Some(())
}

/// 过滤成员信息，返回需要的成员列表和重载数量
fn filter_member_infos<'a>(
    semantic_model: &SemanticModel,
    member_infos: &'a Vec<LuaMemberInfo>,
) -> Option<(Vec<&'a LuaMemberInfo>, Option<usize>)> {
    if member_infos.is_empty() {
        return None;
    }

    let mut file_decl_member: Option<&LuaMemberInfo> = None;
    let mut member_with_owners: Vec<(&LuaMemberInfo, Option<LuaTypeDeclId>)> =
        Vec::with_capacity(member_infos.len());
    let mut all_doc_function = true;
    let mut overload_count = 0;

    // 一次遍历收集所有信息
    for member_info in member_infos {
        let owner_id = get_owner_type_id(semantic_model.get_db(), member_info);
        member_with_owners.push((member_info, owner_id.clone()));

        // 寻找第一个 file_decl 作为参考，如果没有则使用第一个
        if file_decl_member.is_none() {
            if let Some(feature) = member_info.feature {
                if feature.is_file_decl() {
                    file_decl_member = Some(member_info);
                }
            }
        }

        // 检查是否全为 DocFunction，同时计算重载数量
        match &member_info.typ {
            LuaType::DocFunction(_) => {
                overload_count += 1;
            }
            LuaType::Signature(id) => {
                all_doc_function = false;
                overload_count += 1;
                if let Some(signature) = semantic_model.get_db().get_signature_index().get(&id) {
                    overload_count += signature.overloads.len();
                }
            }
            _ => {
                all_doc_function = false;
            }
        }
    }

    // 确定最终使用的参考 owner
    let final_reference_owner = if let Some(file_decl_member_info) = file_decl_member {
        // 与第一个成员进行类型检查, 确保子类成员的类型与父类成员的类型一致
        if let Some((first_member, first_owner)) = member_with_owners.first() {
            let type_check_result =
                semantic_model.type_check(&file_decl_member_info.typ, &first_member.typ);
            if type_check_result.is_ok() {
                get_owner_type_id(semantic_model.get_db(), file_decl_member_info)
            } else {
                first_owner.clone()
            }
        } else {
            get_owner_type_id(semantic_model.get_db(), file_decl_member_info)
        }
    } else {
        // 没有找到 file_decl，使用第一个成员作为参考
        member_with_owners
            .first()
            .map(|(_, owner)| owner.clone())
            .flatten()
    };

    // 过滤出相同 owner_type_id 的成员
    let mut filtered_member_infos: Vec<&LuaMemberInfo> = member_with_owners
        .into_iter()
        .filter_map(|(member_info, owner_id)| {
            if owner_id == final_reference_owner {
                Some(member_info)
            } else {
                None
            }
        })
        .collect();

    // 处理重载计数
    let final_overload_count = if overload_count >= 1 {
        let count = overload_count - 1;
        if count == 0 { None } else { Some(count) }
    } else {
        None
    };

    // 如果全为 DocFunction, 只保留第一个
    if all_doc_function && !filtered_member_infos.is_empty() {
        filtered_member_infos.truncate(1);
    }

    Some((filtered_member_infos, final_overload_count))
}

enum MemberResolveState {
    All,
    Meta,
    FileDecl,
}

fn get_owner_type_id(db: &DbIndex, info: &LuaMemberInfo) -> Option<LuaTypeDeclId> {
    match &info.property_owner_id {
        Some(LuaSemanticDeclId::Member(member_id)) => {
            if let Some(owner) = db.get_member_index().get_current_owner(member_id) {
                return owner.get_type_id().cloned();
            }
            None
        }
        _ => None,
    }
}

fn get_resolve_state(db: &DbIndex, member_infos: &Vec<&LuaMemberInfo>) -> MemberResolveState {
    let mut resolve_state = MemberResolveState::All;
    if db.get_emmyrc().strict.meta_override_file_define {
        for member_info in member_infos.iter() {
            match member_info.feature {
                Some(feature) => {
                    if feature.is_meta_decl() {
                        resolve_state = MemberResolveState::Meta;
                        break;
                    } else if feature.is_file_decl() {
                        resolve_state = MemberResolveState::FileDecl;
                    }
                }
                None => {}
            }
        }
    }
    resolve_state
}
