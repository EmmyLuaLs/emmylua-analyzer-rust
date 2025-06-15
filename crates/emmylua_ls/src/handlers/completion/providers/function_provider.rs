use emmylua_code_analysis::{
    get_real_type, DbIndex, InferGuard, LuaDeclLocation, LuaFunctionType, LuaMember, LuaMemberKey,
    LuaMemberOwner, LuaMultiLineUnion, LuaSemanticDeclId, LuaStringTplType, LuaType, LuaTypeCache,
    LuaTypeDeclId, LuaUnionType, RenderLevel, SemanticDeclLevel,
};
use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaAstToken, LuaCallArgList, LuaCallExpr, LuaClosureExpr,
    LuaComment, LuaDocTagParam, LuaLiteralExpr, LuaLiteralToken, LuaNameToken, LuaParamList,
    LuaStat, LuaSyntaxId, LuaSyntaxKind, LuaSyntaxToken, LuaTokenKind, LuaVarExpr,
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

fn get_token_should_type(builder: &mut CompletionBuilder) -> Option<Vec<LuaType>> {
    let token = builder.trigger_token.clone();
    let mut parent_node = token.parent()?;
    // 输入`""`时允许往上找
    if LuaLiteralExpr::can_cast(parent_node.kind().into()) {
        parent_node = parent_node.parent()?;
    }

    match parent_node.kind().into() {
        LuaSyntaxKind::CallArgList => {
            return infer_call_arg_list(builder, LuaCallArgList::cast(parent_node)?, token);
        }
        LuaSyntaxKind::ParamList => {
            if builder.is_space_trigger_character {
                return None;
            }
            return infer_param_list(builder, LuaParamList::cast(parent_node)?);
        }
        LuaSyntaxKind::Block => {
            /*
               补全以下形式:
               ```lua
               ---@class A
               ---@field func fun(a: string)

               ---@type A
               local a

               a.func =
               ```
            */
            let prev_token = token.prev_token()?;
            let assign_stat = LuaAssignStat::cast(prev_token.parent()?)?;
            let (vars, exprs) = assign_stat.get_var_and_expr_list();
            if vars.len() != 1 || !exprs.is_empty() {
                return None;
            }
            let var = vars.first()?;
            let var_type = builder.semantic_model.infer_expr(var.clone().into()).ok()?;
            let real_type = get_real_type(&builder.semantic_model.get_db(), &var_type)?;
            if real_type.is_function() {
                return Some(vec![real_type.clone()]);
            }
        }
        _ => {}
    }

    None
}

pub fn dispatch_type(
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
        LuaType::DocFunction(func) => {
            add_lambda_completion(builder, &func);
        }
        LuaType::DocStringConst(key) => {
            add_string_completion(builder, key.as_str());
        }
        LuaType::MultiLineUnion(multi_union) => {
            add_multi_line_union_member_completion(builder, &multi_union, infer_guard);
        }
        LuaType::StrTplRef(key) => {
            add_str_tpl_ref_completion(builder, &key);
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
    infer_guard.check(&type_ref_id).ok()?;

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

        if type_decl.is_enum_key() {
            let members = builder
                .semantic_model
                .get_db()
                .get_member_index()
                .get_members(&owner_id)?;
            let mut completion_items = Vec::new();
            for member in members {
                let member_key = member.get_key();
                let label = match member_key {
                    LuaMemberKey::Name(str) => to_enum_label(builder, str.as_str()),
                    LuaMemberKey::Integer(i) => i.to_string(),
                    LuaMemberKey::None => continue,
                    LuaMemberKey::ExprType(_) => continue,
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
            add_enum_members_completion(builder, &type_ref_id, locations);
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

fn infer_param_list(
    builder: &mut CompletionBuilder,
    param_list: LuaParamList,
) -> Option<Vec<LuaType>> {
    let closure_expr = param_list.get_parent::<LuaClosureExpr>()?;

    let doc_params = get_closure_expr_comment(&closure_expr)?.children::<LuaDocTagParam>();
    let mut names = Vec::new();
    for doc_param in doc_params {
        let name = doc_param.get_name_token()?.get_name_text().to_string();
        if !names.contains(&name) {
            // 不在这里添加补全项, 拼接的优先级应在单独添加之上
            names.push(name.clone());
        }
    }
    let params = param_list
        .get_params()
        .map(|it| {
            if let Some(name_token) = it.get_name_token() {
                name_token.get_name_text().to_string()
            } else {
                "".to_string()
            }
        })
        .filter(|it| !it.is_empty())
        .collect::<Vec<_>>();

    // names 去掉 params 已有的
    names.retain(|name| !params.contains(&name));
    if names.len() > 1 {
        builder.add_completion_item(CompletionItem {
            label: format!("{}", names.iter().join(", ")),
            kind: Some(lsp_types::CompletionItemKind::INTERFACE),
            ..Default::default()
        });
    }

    for name in names {
        builder.add_completion_item(CompletionItem {
            label: name,
            kind: Some(lsp_types::CompletionItemKind::INTERFACE),
            ..Default::default()
        });
    }

    // 不返回类型, 因为字符串类型会被加上双引号, 但这里需要的是不带双引号的字符串, 我们选择直接在这里添加
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
    let typ = call_expr_func
        .get_params()
        .get(param_idx)?
        .1
        .clone()
        .unwrap_or(LuaType::Unknown);
    let mut types = Vec::new();
    types.push(typ);
    push_function_overloads_param(
        builder,
        &call_expr,
        call_expr_func.get_params(),
        param_idx,
        &mut types,
    );
    Some(types.into_iter().unique().collect()) // 需要去重
}

fn push_function_overloads_param(
    builder: &mut CompletionBuilder,
    call_expr: &LuaCallExpr,
    call_params: &[(String, Option<LuaType>)],
    param_idx: usize,
    types: &mut Vec<LuaType>,
) -> Option<()> {
    let member_index = builder.semantic_model.get_db().get_member_index();
    let prefix_expr = call_expr.get_prefix_expr()?;
    let semantic_decl = builder.semantic_model.find_decl(
        prefix_expr.syntax().clone().into(),
        SemanticDeclLevel::default(),
    )?;

    // 收集函数类型
    let functions = match semantic_decl {
        LuaSemanticDeclId::Member(member_id) => {
            let member = member_index.get_member(&member_id)?;
            let key = member.get_key().to_path();
            let owner = member_index.get_current_owner(&member_id)?;
            let members = member_index.get_members(owner)?;
            let functions = filter_function_members(builder.semantic_model.get_db(), members, key);
            Some(functions)
        }
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            let decl = builder
                .semantic_model
                .get_db()
                .get_decl_index()
                .get_decl(&decl_id)?;

            let typ = builder
                .semantic_model
                .get_db()
                .get_type_index()
                .get_type_cache(&decl_id.into())
                .map(|cache| cache.as_type().clone())
                .unwrap_or(LuaType::Unknown);
            match typ {
                LuaType::Signature(_) | LuaType::DocFunction(_) => Some(vec![typ.clone()]),
                _ => {
                    let key = decl.get_name();
                    let type_id = LuaTypeDeclId::new(decl.get_name());
                    let members = member_index.get_members(&LuaMemberOwner::Type(type_id))?;
                    let functions = filter_function_members(
                        builder.semantic_model.get_db(),
                        members,
                        key.to_string(),
                    );
                    Some(functions)
                }
            }
        }
        _ => None,
    }?;

    // 获取重载函数列表
    let signature_index = builder.semantic_model.get_db().get_signature_index();
    let mut overloads = Vec::new();
    for function in functions {
        match function {
            LuaType::Signature(signature_id) => {
                if let Some(signature) = signature_index.get(&signature_id) {
                    overloads.extend(signature.overloads.iter().cloned());
                }
            }
            LuaType::DocFunction(doc_function) => {
                overloads.push(doc_function);
            }
            _ => {}
        }
    }

    // 筛选匹配的参数类型并添加到结果中
    for overload in overloads.iter() {
        let overload_params = overload.get_params();

        // 检查前面的参数是否匹配
        if !params_match_prefix(call_params, overload_params, param_idx) {
            continue;
        }

        // 添加匹配的参数类型
        if let Some(param_type) = overload_params.get(param_idx).and_then(|p| p.1.clone()) {
            types.push(param_type);
        }
    }

    /// 过滤出函数类型的成员
    fn filter_function_members(
        db: &DbIndex,
        members: Vec<&LuaMember>,
        key: String,
    ) -> Vec<LuaType> {
        let mut result_members = vec![];
        for member in members {
            if member.get_key().to_path() == key {
                let member_type = db
                    .get_type_index()
                    .get_type_cache(&member.get_id().into())
                    .unwrap_or(&LuaTypeCache::InferType(LuaType::Unknown));
                if let LuaType::Signature(_) | LuaType::DocFunction(_) = member_type.as_type() {
                    result_members.push(member_type.as_type().clone());
                }
            }
        }

        result_members
    }

    /// 判断前面的参数是否匹配
    fn params_match_prefix(
        call_params: &[(String, Option<LuaType>)],
        overload_params: &[(String, Option<LuaType>)],
        param_idx: usize,
    ) -> bool {
        if param_idx == 0 {
            return true;
        }

        for i in 0..param_idx {
            if let (Some(call_param), Some(overload_param)) =
                (call_params.get(i), overload_params.get(i))
            {
                if call_param.1 != overload_param.1 {
                    return false;
                }
            }
        }

        true
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
    let label = format!("function({}) end", params_str.join(", "));
    let insert_text = format!("function({})\n\t$0\nend", params_str.join(", "));

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
    type_id: &LuaTypeDeclId,
    locations: Vec<LuaDeclLocation>,
) -> Option<()> {
    let owner_id = LuaMemberOwner::Type(type_id.clone());
    let members = builder
        .semantic_model
        .get_db()
        .get_member_index()
        .get_members(&owner_id)?
        .iter()
        .map(|it| {
            (
                it.get_key().clone(),
                builder
                    .semantic_model
                    .get_db()
                    .get_type_index()
                    .get_type_cache(&it.get_id().into())
                    .unwrap_or(&LuaTypeCache::InferType(LuaType::Unknown))
                    .as_type()
                    .clone(),
            )
        })
        .sorted_by(|a, b| a.0.cmp(&b.0))
        .collect::<Vec<_>>();

    // 判断是否为字符串字面量触发
    let is_string_literal_trigger = builder.get_trigger_text() == "\"\""
        && builder
            .trigger_token
            .parent()
            .and_then(LuaLiteralExpr::cast)
            .and_then(|literal_expr| literal_expr.get_literal())
            .map_or(false, |literal| {
                matches!(literal, LuaLiteralToken::String(_))
            });

    let file_id = builder.semantic_model.get_file_id();
    let is_same_file = locations.iter().all(|it| it.file_id == file_id);
    // 可能存在的本地变量名
    let variable_name = get_enum_decl_variable_name(builder, locations, is_same_file);

    // 遍历成员并生成补全项
    for (key, typ) in members {
        let label = if is_string_literal_trigger {
            let mut label =
                humanize_type(builder.semantic_model.get_db(), &typ, RenderLevel::Minimal);
            if label.starts_with("\"") {
                label = label[1..].to_string();
                if label.ends_with("\"") {
                    label = label[..label.len() - 1].to_string();
                }
            }
            label
        } else if let Some(ref var_name) = variable_name {
            let label = match key {
                LuaMemberKey::Name(str) => format!("{}.{}", var_name, str),
                LuaMemberKey::Integer(i) => format!("{}[{}]", var_name, i),
                _ => continue, // 跳过不支持的key类型
            };
            label
        } else {
            let label = humanize_type(builder.semantic_model.get_db(), &typ, RenderLevel::Minimal);
            label
        };

        let description = type_id.get_name().to_string();
        let completion_item = CompletionItem {
            label,
            kind: Some(lsp_types::CompletionItemKind::ENUM_MEMBER),
            label_details: Some(lsp_types::CompletionItemLabelDetails {
                detail: None,
                description: Some(description),
            }),
            ..Default::default()
        };

        builder.add_completion_item(completion_item);
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

fn get_closure_expr_comment(closure_expr: &LuaClosureExpr) -> Option<LuaComment> {
    let comment = closure_expr
        .ancestors::<LuaStat>()
        .next()?
        .syntax()
        .prev_sibling()?;
    match comment.kind().into() {
        LuaSyntaxKind::Comment => {
            let comment = LuaComment::cast(comment)?;
            Some(comment)
        }
        _ => None,
    }
}

fn add_str_tpl_ref_completion(
    builder: &mut CompletionBuilder,
    str_tpl: &LuaStringTplType,
) -> Option<()> {
    let db = builder.semantic_model.get_db();
    let module_index = db.get_module_index();
    let types = db.get_type_index().get_all_types();

    // 泛型约束
    let extend_type = get_str_tpl_ref_extend_type(builder, str_tpl).unwrap_or(LuaType::Any);

    let mut completion_items: Vec<_> = types
        .into_iter()
        // 过滤非类
        .filter(|type_decl| type_decl.is_class())
        .filter(|type_decl| {
            // 过滤标准库
            !type_decl
                .get_locations()
                .iter()
                .any(|loc| module_index.is_std(&loc.file_id))
        })
        .filter(|type_decl| {
            // 检查名称是否匹配
            let name = type_decl.get_full_name();
            let prefix = str_tpl.get_prefix();
            let suffix = str_tpl.get_suffix();

            (prefix.is_empty() || name.starts_with(prefix))
                && (suffix.is_empty() || name.ends_with(suffix))
        })
        .filter(|type_decl| {
            // 检查泛型约束
            let current_type = LuaType::Ref(type_decl.get_id());
            // 去掉约束自身
            if extend_type == current_type {
                return false;
            }
            builder
                .semantic_model
                .type_check(&extend_type, &current_type)
                .is_ok()
        })
        .map(|type_decl| {
            let trimmed_name = type_decl
                .get_full_name()
                .trim_start_matches(str_tpl.get_prefix())
                .trim_end_matches(str_tpl.get_suffix());

            CompletionItem {
                label: to_enum_label(builder, trimmed_name),
                kind: Some(lsp_types::CompletionItemKind::ENUM_MEMBER),
                ..Default::default()
            }
        })
        .collect();

    completion_items.sort_by(|a, b| a.label.cmp(&b.label));
    for item in completion_items {
        builder.add_completion_item(item);
    }

    Some(())
}

fn get_str_tpl_ref_extend_type(
    builder: &CompletionBuilder,
    str_tpl: &LuaStringTplType,
) -> Option<LuaType> {
    let token = builder.trigger_token.clone();
    let mut parent_node = token.parent()?;
    if LuaLiteralExpr::can_cast(parent_node.kind().into()) {
        parent_node = parent_node.parent()?;
    }
    match parent_node.kind().into() {
        LuaSyntaxKind::CallArgList => {
            let call_expr = LuaCallArgList::cast(parent_node)?.get_parent::<LuaCallExpr>()?;
            let function = builder
                .semantic_model
                .infer_expr(call_expr.get_prefix_expr()?.clone())
                .ok()?;
            if let LuaType::Signature(signature_id) = function {
                let signature = builder
                    .semantic_model
                    .get_db()
                    .get_signature_index()
                    .get(&signature_id)?;
                let generic_param = signature
                    .generic_params
                    .iter()
                    .find(|it| it.0 == str_tpl.get_name());
                if let Some(generic_param) = generic_param {
                    return Some(generic_param.1.clone().unwrap_or(LuaType::Any));
                }
            }
            None
        }
        _ => None,
    }
}
