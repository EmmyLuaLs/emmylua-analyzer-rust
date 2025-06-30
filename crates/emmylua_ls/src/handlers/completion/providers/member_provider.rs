use emmylua_code_analysis::{
    enum_variable_is_param, DbIndex, LuaMemberInfo, LuaSemanticDeclId, LuaType, LuaTypeDeclId,
};
use emmylua_parser::{LuaAstNode, LuaAstToken, LuaIndexExpr, LuaStringToken};

use crate::handlers::completion::{
    add_completions::{add_member_completion, CompletionTriggerStatus},
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
        &mut builder.semantic_model.get_config().borrow_mut(),
        &index_expr,
        &prefix_type,
    )
    .is_some()
    {
        return None;
    }

    let member_info_map = builder.semantic_model.get_member_info_map(&prefix_type)?;
    for (_, member_infos) in member_info_map.iter() {
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
        let overload_count = count_function_overloads(
            builder.semantic_model.get_db(),
            &member_infos.iter().map(|info| info).collect::<Vec<_>>(),
        );
        let member_info = &member_infos[0];
        add_member_completion(
            builder,
            member_info.clone(),
            completion_status,
            overload_count,
        );
        return Some(());
    }

    let mut resolve_state = MemberResolveState::All;
    if builder
        .semantic_model
        .get_db()
        .get_emmyrc()
        .strict
        .meta_override_file_define
    {
        for member_info in member_infos {
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

    // 屏蔽掉父类成员
    let first_owner = get_owner_type_id(builder.semantic_model.get_db(), member_infos.first()?);
    let member_infos: Vec<&LuaMemberInfo> = member_infos
        .iter()
        .filter(|member_info| {
            get_owner_type_id(builder.semantic_model.get_db(), member_info) == first_owner
        })
        .collect();

    // 当全为`DocFunction`时, 只取第一个作为补全项
    let limit_doc_function = member_infos
        .iter()
        .all(|info| matches!(info.typ, LuaType::DocFunction(_)));

    let overload_count = count_function_overloads(builder.semantic_model.get_db(), &member_infos);

    for member_info in member_infos {
        match resolve_state {
            MemberResolveState::All => {
                add_member_completion(
                    builder,
                    member_info.clone(),
                    completion_status,
                    overload_count,
                );
                if limit_doc_function {
                    break;
                }
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
                        if limit_doc_function {
                            break;
                        }
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
                        if limit_doc_function {
                            break;
                        }
                    }
                }
            }
        }
    }

    Some(())
}

fn count_function_overloads(db: &DbIndex, member_infos: &Vec<&LuaMemberInfo>) -> Option<usize> {
    let mut count = 0;
    for member_info in member_infos {
        match &member_info.typ {
            LuaType::DocFunction(_) => {
                count += 1;
            }
            LuaType::Signature(id) => {
                count += 1;
                if let Some(signature) = db.get_signature_index().get(&id) {
                    count += signature.overloads.len();
                }
            }
            _ => {}
        }
    }
    if count >= 1 {
        count -= 1;
    }
    if count == 0 {
        None
    } else {
        Some(count)
    }
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
