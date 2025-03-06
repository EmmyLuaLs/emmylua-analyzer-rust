use emmylua_code_analysis::{
    InferGuard, LuaDeclLocation, LuaFunctionType, LuaMemberId, LuaMemberKey, LuaMemberOwner, LuaMultiLineUnion, LuaPropertyOwnerId, LuaType, LuaTypeDeclId, LuaUnionType, RenderLevel
};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaCallArgList, LuaCallExpr, LuaComment, LuaExpr,
    LuaNameToken, LuaSyntaxId, LuaSyntaxKind, LuaSyntaxToken, LuaTokenKind, LuaVarExpr,
};
use itertools::Itertools;
use lsp_types::{CompletionItem, Documentation};

use crate::handlers::{
    completion::completion_builder::CompletionBuilder, signature_helper::get_current_param_index,
};
use emmylua_code_analysis::humanize_type;

pub fn add_completion(builder: &mut CompletionBuilder) -> Option<()> {
    if builder.is_cancelled() {
        return None;
    }

    let types = get_token_should_type(builder)?;
    for typ in types {
        dispatch_type(builder, typ, &mut InferGuard::new());
    }
    Some(())
}

fn dispatch_type(
    builder: &mut CompletionBuilder,
    typ: LuaType,
    infer_guard: &mut InferGuard,
) -> Option<()> {
    match typ {
        LuaType::Ref(type_ref_id) => {
            add_type_ref_completion(builder, type_ref_id.clone(), infer_guard);
        }
        LuaType::Union(union_typ) => {
            add_union_member_completion(builder, &union_typ, infer_guard);
        }
        LuaType::Nullable(typ) => {
            dispatch_type(builder, (*typ).clone(), infer_guard);
        }
        LuaType::DocFunction(func) => {
            add_lambda_completion(builder, &func);
        }
        LuaType::DocStringConst(key) => {
            add_string_completion(builder, key.as_str());
        }
        LuaType::MultiLineUnion(multi_union) => {
            add_multi_line_union_member_completion(builder, &multi_union, infer_guard);
        }
        _ => {}
    }

    Some(())
}

fn add_type_ref_completion(
    builder: &mut CompletionBuilder,
    type_ref_id: LuaTypeDeclId,
    infer_guard: &mut InferGuard,
) -> Option<()> {
    infer_guard.check(&type_ref_id)?;

    let type_decl = builder
        .semantic_model
        .get_db()
        .get_type_index()
        .get_type_decl(&type_ref_id)?;
    if type_decl.is_alias() {
        let db = builder.semantic_model.get_db();
        if let Some(origin) = type_decl.get_alias_origin(db, None) {
            return dispatch_type(builder, origin.clone(), infer_guard);
        }

        builder.stop_here();
    } else if type_decl.is_enum() {
        let owner_id = LuaMemberOwner::Type(type_ref_id.clone());
        let member_map = builder
            .semantic_model
            .get_db()
            .get_member_index()
            .get_member_map(owner_id)?;

        if type_decl.is_enum_key() {
            let mut completion_items = Vec::new();
            for member_key in member_map.keys() {
                let label = match member_key {
                    LuaMemberKey::Name(str) => to_enum_label(builder, str.as_str()),
                    LuaMemberKey::Integer(i) => i.to_string(),
                    LuaMemberKey::None => continue,
                };

                let completion_item = CompletionItem {
                    label,
                    kind: Some(lsp_types::CompletionItemKind::ENUM_MEMBER),
                    ..Default::default()
                };

                completion_items.push(completion_item);
            }
            for completion_item in completion_items {
                builder.add_completion_item(completion_item);
            }
        } else {
            let locations = type_decl
                .get_locations()
                .iter()
                .map(|it| it.clone())
                .collect::<Vec<_>>();
            let member_ids = member_map.values().map(|it| it.clone()).collect::<Vec<_>>();
            add_enum_members_completion(builder, member_ids, &type_ref_id, locations);
        }

        builder.stop_here();
    }

    Some(())
}

fn add_union_member_completion(
    builder: &mut CompletionBuilder,
    union_typ: &LuaUnionType,
    infer_guard: &mut InferGuard,
) -> Option<()> {
    for union_sub_typ in union_typ.get_types() {
        let name = match union_sub_typ {
            LuaType::DocStringConst(s) => to_enum_label(builder, s),
            LuaType::DocIntegerConst(i) => i.to_string(),
            _ => {
                dispatch_type(builder, union_sub_typ.clone(), infer_guard);
                continue;
            }
        };

        let completion_item = CompletionItem {
            label: name,
            kind: Some(lsp_types::CompletionItemKind::ENUM_MEMBER),
            ..Default::default()
        };

        builder.add_completion_item(completion_item);
    }

    Some(())
}

fn add_string_completion(builder: &mut CompletionBuilder, str: &str) -> Option<()> {
    let completion_item = CompletionItem {
        label: to_enum_label(builder, str),
        kind: Some(lsp_types::CompletionItemKind::ENUM_MEMBER),
        ..Default::default()
    };

    builder.add_completion_item(completion_item);
    Some(())
}

fn get_token_should_type(builder: &mut CompletionBuilder) -> Option<Vec<LuaType>> {
    let token = builder.trigger_token.clone();
    let mut parent_node = token.parent()?;
    if LuaExpr::can_cast(parent_node.kind().into()) {
        parent_node = parent_node.parent()?;
    }

    match parent_node.kind().into() {
        LuaSyntaxKind::CallArgList => {
            return infer_call_arg_list(builder, LuaCallArgList::cast(parent_node)?, token);
        }
        LuaSyntaxKind::BinaryExpr => {
            // infer_binary_expr(builder, binary_expr)?;
        }
        _ => {}
    }

    None
}

fn infer_call_arg_list(
    builder: &mut CompletionBuilder,
    call_arg_list: LuaCallArgList,
    token: LuaSyntaxToken,
) -> Option<Vec<LuaType>> {
    let call_expr = call_arg_list.get_parent::<LuaCallExpr>()?;
    let mut param_idx = get_current_param_index(&call_expr, &token)?;
    let call_expr_func = builder
        .semantic_model
        .infer_call_expr_func(call_expr.clone(), Some(param_idx + 1))?;
    let colon_call = call_expr.is_colon_call();
    let colon_define = call_expr_func.is_colon_define();
    match (colon_call, colon_define) {
        (true, true) | (false, false) | (false, true) => {}
        (true, false) => {
            param_idx += 1;
        }
    }
    let typ = call_expr_func.get_params().get(param_idx)?.1.clone()?;
    let mut types = Vec::new();
    types.push(typ);
    infer_call_arg_list_overload(builder, &call_expr, &call_expr_func, param_idx, &mut types);
    Some(types.into_iter().unique().collect()) // 需要去重
}

fn infer_call_arg_list_overload(
    builder: &mut CompletionBuilder,
    call_expr: &LuaCallExpr,
    call_expr_func: &LuaFunctionType,
    param_idx: usize,
    types: &mut Vec<LuaType>,
) -> Option<()> {
    let prefix_expr = call_expr.get_prefix_expr()?;
    let property_owner_id = builder
        .semantic_model
        .get_property_owner_id(prefix_expr.syntax().clone().into())?;

    let signature_id = match property_owner_id {
        LuaPropertyOwnerId::Member(member_id) => {
            let member = builder
                .semantic_model
                .get_db()
                .get_member_index()
                .get_member(&member_id)?;
            if let LuaType::Signature(signature_id) = member.get_decl_type() {
                Some(signature_id)
            } else {
                None
            }
        }
        LuaPropertyOwnerId::LuaDecl(decl_id) => {
            let decl = builder
                .semantic_model
                .get_db()
                .get_decl_index()
                .get_decl(&decl_id)?;
            if let LuaType::Signature(signature_id) = decl.get_type()? {
                Some(signature_id)
            } else {
                None
            }
        }
        _ => None,
    }?;

    let signature = builder
        .semantic_model
        .get_db()
        .get_signature_index()
        .get(&signature_id)?;

    let call_params = call_expr_func.get_params();
    for overload in signature.overloads.iter() {
        let overload_param = overload.get_params();
        // 前面的参数必须相同
        let mut is_match = true;
        if param_idx != 0 {
            for (i, param) in call_params.iter().enumerate() {
                if i < param_idx {
                    if let Some(overload_param_type) = overload_param.get(i) {
                        if param.1 != overload_param_type.1 {
                            is_match = false;
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
        }
        if !is_match {
            continue;
        }
        if let Some(param_type) = overload.get_params().get(param_idx)?.1.clone() {
            types.push(param_type);
        }
    }

    Some(())
}

fn add_multi_line_union_member_completion(
    builder: &mut CompletionBuilder,
    union_typ: &LuaMultiLineUnion,
    infer_guard: &mut InferGuard,
) -> Option<()> {
    for (union_sub_typ, description) in union_typ.get_unions() {
        let name = match union_sub_typ {
            LuaType::DocStringConst(s) => to_enum_label(builder, s),
            LuaType::DocIntegerConst(i) => i.to_string(),
            _ => {
                dispatch_type(builder, union_sub_typ.clone(), infer_guard);
                continue;
            }
        };

        let documentation = if let Some(description) = description {
            Some(Documentation::String(description.clone()))
        } else {
            None
        };

        let label_details = if let Some(description) = description {
            Some(lsp_types::CompletionItemLabelDetails {
                detail: None,
                description: Some(description.clone()),
            })
        } else {
            None
        };

        let completion_item = CompletionItem {
            label: name,
            kind: Some(lsp_types::CompletionItemKind::ENUM_MEMBER),
            label_details,
            documentation,
            ..Default::default()
        };

        builder.add_completion_item(completion_item);
    }

    Some(())
}

fn to_enum_label(builder: &CompletionBuilder, str: &str) -> String {
    if matches!(
        builder.trigger_token.kind().into(),
        LuaTokenKind::TkString | LuaTokenKind::TkLongString
    ) {
        str.to_string()
    } else {
        format!("\"{}\"", str)
    }
}

fn add_lambda_completion(builder: &mut CompletionBuilder, func: &LuaFunctionType) -> Option<()> {
    let params_str = func
        .get_params()
        .iter()
        .map(|p| p.0.clone())
        .collect::<Vec<_>>();
    let label = format!("function ({}) end", params_str.join(", "));
    let insert_text = format!("function ({})\n\t$0\nend", params_str.join(", "));

    let completion_item = CompletionItem {
        label,
        kind: Some(lsp_types::CompletionItemKind::FUNCTION),
        sort_text: Some("0".to_string()),
        insert_text: Some(insert_text),
        insert_text_format: Some(lsp_types::InsertTextFormat::SNIPPET),
        ..Default::default()
    };

    builder.add_completion_item(completion_item);
    Some(())
}

fn add_enum_members_completion(
    builder: &mut CompletionBuilder,
    member_ids: Vec<LuaMemberId>,
    type_id: &LuaTypeDeclId,
    locations: Vec<LuaDeclLocation>,
) -> Option<()> {
    let file_id = builder.semantic_model.get_file_id();
    let is_same_file = locations.iter().all(|it| it.file_id == file_id);
    if let Some(variable_name) = get_enum_decl_variable_name(builder, locations, is_same_file) {
        for member_id in member_ids {
            let member = builder
                .semantic_model
                .get_db()
                .get_member_index()
                .get_member(&member_id)?;
            let key = member.get_key();
            let label = match key {
                LuaMemberKey::Name(str) => format!("{}.{}", variable_name, str.to_string()),
                LuaMemberKey::Integer(i) => format!("{}[{}]", variable_name, i),
                LuaMemberKey::None => continue,
            };

            let description = format!("{}", type_id.get_name());
            let completion_item = CompletionItem {
                label,
                kind: Some(lsp_types::CompletionItemKind::ENUM_MEMBER),
                label_details: Some(lsp_types::CompletionItemLabelDetails {
                    detail: None,
                    description: Some(description.clone()),
                }),
                ..Default::default()
            };

            builder.add_completion_item(completion_item);
        }
    } else {
        for member_id in member_ids {
            let member = builder
                .semantic_model
                .get_db()
                .get_member_index()
                .get_member(&member_id)?;
            let label = humanize_type(
                builder.semantic_model.get_db(),
                member.get_decl_type(),
                RenderLevel::Minimal,
            );
            let description = format!("{}", type_id.get_name());
            let completion_item = CompletionItem {
                label,
                kind: Some(lsp_types::CompletionItemKind::ENUM_MEMBER),
                label_details: Some(lsp_types::CompletionItemLabelDetails {
                    detail: None,
                    description: Some(description.clone()),
                }),
                ..Default::default()
            };

            builder.add_completion_item(completion_item);
        }
    }
    Some(())
}

fn get_enum_decl_variable_name(
    builder: &CompletionBuilder,
    locations: Vec<LuaDeclLocation>,
    is_same_file: bool,
) -> Option<String> {
    let completion_file_id = builder.semantic_model.get_file_id();
    if is_same_file {
        let same_location = locations
            .iter()
            .find(|it| it.file_id == completion_file_id)?;
        let root = builder
            .semantic_model
            .get_root_by_file_id(same_location.file_id)?;
        let syntax_id = LuaSyntaxId::new(LuaTokenKind::TkName.into(), same_location.range);
        let token = LuaNameToken::cast(syntax_id.to_token_from_root(root.syntax())?)?;
        let comment = token.ancestors::<LuaComment>().next()?;
        let comment_owner = comment.get_owner()?;
        match comment_owner {
            LuaAst::LuaLocalStat(local_stat) => {
                return Some(
                    local_stat
                        .get_local_name_list()
                        .next()?
                        .get_name_token()?
                        .get_name_text()
                        .to_string(),
                )
            }
            LuaAst::LuaAssignStat(assign_stat) => {
                return Some(
                    assign_stat
                        .child::<LuaVarExpr>()?
                        .syntax()
                        .text()
                        .to_string(),
                )
            }
            _ => {}
        }
    } else {
        for location in locations {
            let root = builder
                .semantic_model
                .get_root_by_file_id(location.file_id)?;
            let syntax_id = LuaSyntaxId::new(LuaTokenKind::TkName.into(), location.range);
            let token = LuaNameToken::cast(syntax_id.to_token_from_root(root.syntax())?)?;
            let comment = token.ancestors::<LuaComment>().next()?;
            let comment_owner = comment.get_owner()?;
            match comment_owner {
                LuaAst::LuaLocalStat(_) => return None,
                LuaAst::LuaAssignStat(assign_stat) => {
                    return Some(
                        assign_stat
                            .child::<LuaVarExpr>()?
                            .syntax()
                            .text()
                            .to_string(),
                    );
                }
                _ => {}
            }
        }
    }

    None
}
