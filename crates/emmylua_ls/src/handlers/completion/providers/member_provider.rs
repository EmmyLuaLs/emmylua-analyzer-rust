use emmylua_code_analysis::{LuaMemberInfo, LuaType};
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
    let member_info_map = builder.semantic_model.infer_member_map(&prefix_type)?;
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
        let member_info = &member_infos[0];
        add_member_completion(builder, member_info.clone(), completion_status);
        return Some(());
    }
    let first_type = &member_infos[0].typ;

    let mut resolve_state = MemberResolveState::All;
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

    // 当`DocFunction`超过5个时只取第一个作为补全项
    let doc_function_count = member_infos
        .iter()
        .filter(|info| matches!(info.typ, LuaType::DocFunction(_)))
        .count();
    let limit_doc_functions = doc_function_count > 5;
    let mut first_doc_function = false;

    for member_info in member_infos {
        if &member_info.typ != first_type {
            continue;
        }
        if limit_doc_functions && matches!(member_info.typ, LuaType::DocFunction(_)) {
            if first_doc_function {
                continue;
            }
            first_doc_function = true;
        }

        match resolve_state {
            MemberResolveState::All => {
                add_member_completion(builder, member_info.clone(), completion_status);
            }
            MemberResolveState::Meta => {
                if let Some(feature) = member_info.feature {
                    if feature.is_meta_decl() {
                        add_member_completion(builder, member_info.clone(), completion_status);
                    }
                }
            }
            MemberResolveState::FileDecl => {
                if let Some(feature) = member_info.feature {
                    if feature.is_file_decl() {
                        add_member_completion(builder, member_info.clone(), completion_status);
                    }
                }
            }
        }
    }

    Some(())
}

enum MemberResolveState {
    All,
    Meta,
    FileDecl,
}
