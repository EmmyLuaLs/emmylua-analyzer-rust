use std::collections::HashMap;
use std::sync::Arc;

use emmylua_code_analysis::{
    FileId, InferGuard, LuaFunctionType, LuaMember, LuaMemberId, LuaMemberKey, LuaMemberOwner,
    LuaOperatorId, LuaOperatorMetaMethod, LuaSemanticDeclId, LuaType, LuaTypeDecl, SemanticModel,
};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallExpr, LuaExpr, LuaFuncStat, LuaIndexExpr, LuaIndexKey,
    LuaLiteralToken, LuaLocalFuncStat, LuaLocalName, LuaLocalStat, LuaStat, LuaSyntaxId,
    LuaVarExpr,
};
use emmylua_parser::{LuaAstToken, LuaTokenKind};
use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, InlayHintLabelPart, Location};
use rowan::NodeOrToken;

use rowan::TokenAtOffset;

use crate::handlers::completion::extract_index_member_alias;
use crate::handlers::definition::compare_function_types;
use crate::handlers::inlay_hint::build_function_hint::{build_closure_hint, build_label_parts};

pub fn build_inlay_hints(semantic_model: &SemanticModel) -> Option<Vec<InlayHint>> {
    let mut result = Vec::new();
    let root = semantic_model.get_root();
    for node in root.clone().descendants::<LuaAst>() {
        match node {
            LuaAst::LuaClosureExpr(closure) => {
                build_closure_hint(semantic_model, &mut result, closure);
            }
            LuaAst::LuaCallExpr(call_expr) => {
                build_call_expr_param_hint(semantic_model, &mut result, call_expr.clone());
                build_call_expr_await_hint(semantic_model, &mut result, call_expr.clone());
                build_call_expr_meta_call_hint(semantic_model, &mut result, call_expr.clone());
                build_enum_param_hint(semantic_model, &mut result, call_expr);
            }
            LuaAst::LuaLocalName(local_name) => {
                build_local_name_hint(semantic_model, &mut result, local_name);
            }
            LuaAst::LuaFuncStat(func_stat) => {
                build_func_stat_override_hint(semantic_model, &mut result, func_stat);
            }
            LuaAst::LuaIndexExpr(index_expr) => {
                build_index_expr_hint(semantic_model, &mut result, index_expr);
            }
            _ => {}
        }
    }

    Some(result)
}

fn build_call_expr_param_hint(
    semantic_model: &SemanticModel,
    result: &mut Vec<InlayHint>,
    call_expr: LuaCallExpr,
) -> Option<()> {
    if !semantic_model.get_emmyrc().hint.param_hint {
        return Some(());
    }
    let params_location = get_call_signature_param_location(semantic_model, &call_expr);
    let func = semantic_model.infer_call_expr_func(call_expr.clone(), None)?;
    let call_args_list = call_expr.get_args_list()?;
    let colon_call = call_expr.is_colon_call();
    build_call_args_for_func_type(
        semantic_model,
        result,
        call_args_list.get_args().collect(),
        colon_call,
        &func,
        params_location,
    );

    Some(())
}

fn get_call_signature_param_location(
    semantic_model: &SemanticModel,
    call_expr: &LuaCallExpr,
) -> Option<HashMap<String, Location>> {
    let prefix_expr = call_expr.get_prefix_expr()?;
    let semantic_info =
        semantic_model.get_semantic_info(NodeOrToken::Node(prefix_expr.syntax().clone()))?;
    let mut document = None;
    let closure = if let LuaType::Signature(signature_id) = &semantic_info.typ {
        let sig_file_id = signature_id.get_file_id();
        let sig_position = signature_id.get_position();
        document = semantic_model.get_document_by_file_id(sig_file_id);

        if let Some(root) = semantic_model.get_root_by_file_id(sig_file_id) {
            let token = match root.syntax().token_at_offset(sig_position) {
                TokenAtOffset::Single(token) => token,
                TokenAtOffset::Between(left, right) => {
                    if left.kind() == LuaTokenKind::TkName.into() {
                        left
                    } else {
                        right
                    }
                }
                TokenAtOffset::None => {
                    return None;
                }
            };
            let stat = token.parent_ancestors().find_map(LuaStat::cast)?;
            match stat {
                LuaStat::LocalFuncStat(local_func_stat) => local_func_stat.get_closure(),
                LuaStat::FuncStat(func_stat) => func_stat.get_closure(),
                _ => None,
            }
        } else {
            None
        }
    } else {
        None
    }?;
    let lua_params = closure.get_params_list()?;
    let document = document?;
    let url = document.get_uri();
    let mut lua_params_map: HashMap<String, Location> = HashMap::new();
    for param in lua_params.get_params() {
        if let Some(name_token) = param.get_name_token() {
            let name = name_token.get_name_text().to_string();
            let range = param.get_range();
            let lsp_range = document.to_lsp_range(range)?;
            lua_params_map.insert(name, Location::new(url.clone(), lsp_range));
        } else if param.is_dots() {
            let range = param.get_range();
            let lsp_range = document.to_lsp_range(range)?;
            lua_params_map.insert("...".to_string(), Location::new(url.clone(), lsp_range));
        }
    }
    Some(lua_params_map)
}

fn build_call_expr_await_hint(
    semantic_model: &SemanticModel,
    result: &mut Vec<InlayHint>,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let prefix_expr = call_expr.get_prefix_expr()?;
    let semantic_info =
        semantic_model.get_semantic_info(NodeOrToken::Node(prefix_expr.syntax().clone()))?;

    match semantic_info.typ {
        LuaType::DocFunction(f) => {
            if f.is_async() {
                let range = call_expr.get_range();
                let document = semantic_model.get_document();
                let lsp_range = document.to_lsp_range(range)?;
                let hint = InlayHint {
                    kind: Some(InlayHintKind::TYPE),
                    label: InlayHintLabel::String("await".to_string()),
                    position: lsp_range.start,
                    text_edits: None,
                    tooltip: None,
                    padding_left: None,
                    padding_right: Some(true),
                    data: None,
                };
                result.push(hint);
            }
        }
        LuaType::Signature(signature_id) => {
            let signature = semantic_model
                .get_db()
                .get_signature_index()
                .get(&signature_id)?;
            if signature.is_async {
                let range = call_expr.get_range();
                let document = semantic_model.get_document();
                let lsp_range = document.to_lsp_range(range)?;
                let hint = InlayHint {
                    kind: Some(InlayHintKind::TYPE),
                    label: InlayHintLabel::String("await ".to_string()),
                    position: lsp_range.start,
                    text_edits: None,
                    tooltip: None,
                    padding_left: None,
                    padding_right: Some(true),
                    data: None,
                };
                result.push(hint);
            }
        }
        _ => {}
    }
    Some(())
}

fn build_call_args_for_func_type(
    semantic_model: &SemanticModel,
    result: &mut Vec<InlayHint>,
    call_args: Vec<LuaExpr>,
    colon_call: bool,
    func_type: &LuaFunctionType,
    params_location: Option<HashMap<String, Location>>,
) -> Option<()> {
    let call_args_len = call_args.len();
    let mut params = func_type
        .get_params()
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();

    let colon_define = func_type.is_colon_define();
    match (colon_call, colon_define) {
        (false, true) => {
            params.insert(0, "self".to_string());
        }
        (true, false) => {
            if params.len() > 0 {
                params.remove(0);
            }
        }
        _ => {}
    }

    for (idx, name) in params.iter().enumerate() {
        if idx >= call_args_len {
            break;
        }

        if name == "..." {
            for i in idx..call_args_len {
                let label_name = format!("var{}:", i - idx);
                let label = if let Some(params_location) = &params_location {
                    if let Some(location) = params_location.get(name) {
                        InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                            value: label_name,
                            location: Some(location.clone()),
                            ..Default::default()
                        }])
                    } else {
                        InlayHintLabel::String(label_name)
                    }
                } else {
                    InlayHintLabel::String(label_name)
                };

                let arg = &call_args[i];
                let range = arg.get_range();
                let document = semantic_model.get_document();
                let lsp_range = document.to_lsp_range(range)?;
                let hint = InlayHint {
                    kind: Some(InlayHintKind::PARAMETER),
                    label,
                    position: lsp_range.start,
                    text_edits: None,
                    tooltip: None,
                    padding_left: None,
                    padding_right: Some(true),
                    data: None,
                };
                result.push(hint);
            }
            break;
        }

        let arg = &call_args[idx];
        if let LuaExpr::NameExpr(name_expr) = arg {
            if let Some(param_name) = name_expr.get_name_text() {
                // optimize like rust analyzer
                if &param_name == name {
                    continue;
                }
            }
        }

        let document = semantic_model.get_document();
        let lsp_range = document.to_lsp_range(arg.get_range())?;

        let label_name = format!("{}:", name);
        let label = if let Some(params_location) = &params_location {
            if let Some(location) = params_location.get(name) {
                InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                    value: label_name,
                    location: Some(location.clone()),
                    ..Default::default()
                }])
            } else {
                InlayHintLabel::String(label_name)
            }
        } else {
            InlayHintLabel::String(label_name)
        };

        let hint = InlayHint {
            kind: Some(InlayHintKind::PARAMETER),
            label,
            position: lsp_range.start,
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: Some(true),
            data: None,
        };
        result.push(hint);
    }

    Some(())
}

fn build_local_name_hint(
    semantic_model: &SemanticModel,
    result: &mut Vec<InlayHint>,
    local_name: LuaLocalName,
) -> Option<()> {
    if !semantic_model.get_emmyrc().hint.local_hint {
        return Some(());
    }
    // local function 不显示
    if let Some(parent) = local_name.syntax().parent() {
        if LuaLocalFuncStat::can_cast(parent.kind().into()) {
            return Some(());
        }
        if LuaLocalStat::can_cast(parent.kind().into()) {
            let local_stat = LuaLocalStat::cast(parent)?;
            let local_names = local_stat.get_local_name_list();
            for (i, ln) in local_names.enumerate() {
                if local_name == ln {
                    if let Some(value_expr) = local_stat.get_value_exprs().nth(i) {
                        if let LuaExpr::ClosureExpr(_) = value_expr {
                            return Some(());
                        }
                    }
                }
            }
        }
    }

    let typ = semantic_model
        .get_semantic_info(NodeOrToken::Token(
            local_name.get_name_token()?.syntax().clone(),
        ))?
        .typ;

    // 目前没时间完善结合 ast 的类型过滤, 所以只允许一些类型显示
    match typ {
        LuaType::Ref(_) | LuaType::Generic(_) => {}
        _ => {
            return Some(());
        }
    }

    let document = semantic_model.get_document();
    let range = local_name.get_range();
    let lsp_range = document.to_lsp_range(range)?;

    let label_parts = build_label_parts(semantic_model, &typ);
    let hint = InlayHint {
        kind: Some(InlayHintKind::TYPE),
        label: InlayHintLabel::LabelParts(label_parts),
        position: lsp_range.end,
        text_edits: None,
        tooltip: None,
        padding_left: None,
        padding_right: None,
        data: None,
    };
    result.push(hint);

    Some(())
}

fn build_func_stat_override_hint(
    semantic_model: &SemanticModel,
    result: &mut Vec<InlayHint>,
    func_stat: LuaFuncStat,
) -> Option<()> {
    if !semantic_model.get_emmyrc().hint.override_hint {
        return Some(());
    }

    let func_name = func_stat.get_func_name()?;
    if let LuaVarExpr::IndexExpr(index_expr) = func_name {
        let prefix_expr = index_expr.get_prefix_expr()?;
        let prefix_type = semantic_model.infer_expr(prefix_expr.into()).ok()?;
        if let LuaType::Def(id) = prefix_type {
            let supers = semantic_model
                .get_db()
                .get_type_index()
                .get_super_types(&id)?;

            let index_key = index_expr.get_index_key()?;
            let member_key: LuaMemberKey = semantic_model.get_member_key(&index_key)?;
            let infer_guard = &mut InferGuard::new();
            for super_type in supers {
                if let Some(member_id) =
                    get_super_member_id(semantic_model, super_type, &member_key, infer_guard)
                {
                    let member = semantic_model
                        .get_db()
                        .get_member_index()
                        .get_member(&member_id)?;

                    let document = semantic_model.get_document();
                    let last_paren_pos = func_stat
                        .get_closure()?
                        .get_params_list()?
                        .get_range()
                        .end();
                    let last_paren_lsp_pos = document.to_lsp_position(last_paren_pos)?;

                    let file_id = member.get_file_id();
                    let syntax_id = member.get_syntax_id();
                    let lsp_location =
                        get_override_lsp_location(semantic_model, file_id, syntax_id)?;
                    let hint = InlayHint {
                        kind: Some(InlayHintKind::TYPE),
                        label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                            value: "override".to_string(),
                            location: Some(lsp_location),
                            ..Default::default()
                        }]),
                        position: last_paren_lsp_pos,
                        text_edits: None,
                        tooltip: None,
                        padding_left: Some(true),
                        padding_right: None,
                        data: None,
                    };
                    result.push(hint);
                    break;
                }
            }
        }
    }

    Some(())
}

fn get_super_member_id(
    semantic_model: &SemanticModel,
    super_type: LuaType,
    member_key: &LuaMemberKey,
    infer_guard: &mut InferGuard,
) -> Option<LuaMemberId> {
    let super_type_id = match &super_type {
        LuaType::Ref(id) => id,
        LuaType::Generic(generic) => generic.get_base_type_id_ref(),
        _ => return None,
    };
    infer_guard.check(super_type_id).ok()?;
    let member_map = semantic_model.get_member_info_map(&super_type)?;

    if let Some(member_infos) = member_map.get(&member_key) {
        let first_property = member_infos.first()?.property_owner_id.clone()?;
        if let LuaSemanticDeclId::Member(member_id) = first_property {
            return Some(member_id);
        }
    }
    None
}

fn get_override_lsp_location(
    semantic_model: &SemanticModel,
    file_id: FileId,
    syntax_id: LuaSyntaxId,
) -> Option<lsp_types::Location> {
    let document = semantic_model.get_document_by_file_id(file_id)?;
    let root = semantic_model.get_root_by_file_id(file_id)?;
    let node = syntax_id.to_node_from_root(root.syntax())?;
    let range = if let Some(index_exor) = LuaIndexExpr::cast(node.clone()) {
        index_exor.get_index_name_token()?.text_range()
    } else {
        node.text_range()
    };

    let lsp_range = document.to_lsp_location(range)?;
    Some(lsp_range)
}

fn build_call_expr_meta_call_hint(
    semantic_model: &SemanticModel,
    result: &mut Vec<InlayHint>,
    call_expr: LuaCallExpr,
) -> Option<()> {
    if !semantic_model.get_emmyrc().hint.meta_call_hint {
        return Some(());
    }

    let prefix_expr = call_expr.get_prefix_expr()?;
    let semantic_info =
        semantic_model.get_semantic_info(NodeOrToken::Node(prefix_expr.syntax().clone()))?;

    match &semantic_info.typ {
        LuaType::Ref(id) | LuaType::Def(id) => {
            let decl = semantic_model.get_db().get_type_index().get_type_decl(id)?;
            if !decl.is_class() {
                return Some(());
            }

            let call_operator_ids = semantic_model
                .get_db()
                .get_operator_index()
                .get_operators(&id.clone().into(), LuaOperatorMetaMethod::Call)?;

            set_meta_call_part(
                semantic_model,
                result,
                call_operator_ids,
                call_expr,
                semantic_info.typ,
            )?;
        }
        _ => {}
    }
    Some(())
}

fn set_meta_call_part(
    semantic_model: &SemanticModel,
    result: &mut Vec<InlayHint>,
    operator_ids: &Vec<LuaOperatorId>,
    call_expr: LuaCallExpr,
    target_type: LuaType,
) -> Option<()> {
    let (operator_id, call_func) =
        find_match_meta_call_operator_id(semantic_model, operator_ids, call_expr.clone())?;

    let operator = semantic_model
        .get_db()
        .get_operator_index()
        .get_operator(&operator_id)?;

    let location = {
        let range = operator.get_range();
        let document = semantic_model.get_document_by_file_id(operator.get_file_id())?;
        let lsp_range = document.to_lsp_range(range)?;
        Location::new(document.get_uri(), lsp_range)
    };

    let document = semantic_model.get_document();
    let parent = call_expr.syntax().parent()?;

    // 如果是 `Class(...)` 且调用返回值是 Class 类型, 则显示 `new` 提示
    let hint_new = {
        LuaStat::can_cast(parent.kind().into())
            && !matches!(call_expr.get_prefix_expr()?, LuaExpr::CallExpr(_))
            && semantic_model
                .type_check(call_func.get_ret(), &target_type)
                .is_ok()
    };

    let (value, hint_range, padding_right) = if hint_new {
        ("new".to_string(), call_expr.get_range(), Some(true))
    } else {
        (
            "⚡".to_string(),
            call_expr.get_prefix_expr()?.get_range(),
            None,
        )
    };

    let hint_position = {
        let lsp_range = document.to_lsp_range(hint_range)?;
        if hint_new {
            lsp_range.start
        } else {
            lsp_range.end
        }
    };

    let part = InlayHintLabelPart {
        value,
        location: Some(location),
        ..Default::default()
    };

    let hint = InlayHint {
        kind: Some(InlayHintKind::TYPE),
        label: InlayHintLabel::LabelParts(vec![part]),
        position: hint_position,
        text_edits: None,
        tooltip: None,
        padding_left: None,
        padding_right,
        data: None,
    };

    result.push(hint);
    Some(())
}

fn find_match_meta_call_operator_id(
    semantic_model: &SemanticModel,
    operator_ids: &Vec<LuaOperatorId>,
    call_expr: LuaCallExpr,
) -> Option<(LuaOperatorId, Arc<LuaFunctionType>)> {
    let call_func = semantic_model.infer_call_expr_func(call_expr.clone(), None)?;
    if operator_ids.len() == 1 {
        return Some((operator_ids.first().cloned()?, call_func));
    }
    for operator_id in operator_ids {
        let operator = semantic_model
            .get_db()
            .get_operator_index()
            .get_operator(operator_id)?;
        let operator_func = {
            let operator_type = operator.get_operator_func(semantic_model.get_db());
            match operator_type {
                LuaType::DocFunction(func) => func,
                LuaType::Signature(signature_id) => {
                    let signature = semantic_model
                        .get_db()
                        .get_signature_index()
                        .get(&signature_id)?;
                    signature.to_doc_func_type()
                }
                _ => return None,
            }
        };
        let is_match =
            compare_function_types(semantic_model, &call_func, &operator_func, &call_expr)
                .unwrap_or(false);

        if is_match {
            return Some((operator_id.clone(), operator_func));
        }
    }
    operator_ids.first().cloned().map(|id| (id, call_func))
}

fn build_index_expr_hint(
    semantic_model: &SemanticModel,
    result: &mut Vec<InlayHint>,
    index_expr: LuaIndexExpr,
) -> Option<()> {
    if !semantic_model.get_emmyrc().hint.index_hint {
        return Some(());
    }

    // 只处理整数索引
    let index_key = index_expr.get_index_key()?;
    if !matches!(index_key, LuaIndexKey::Integer(_)) {
        return Some(());
    }

    // 获取前缀表达式的类型信息
    let prefix_expr = index_expr.get_prefix_expr()?;
    let prefix_type = semantic_model.infer_expr(prefix_expr.into()).ok()?;
    let member_key = semantic_model.get_member_key(&index_key)?;

    let member_infos = semantic_model.get_member_info_with_key(&prefix_type, member_key, false)?;
    let member_info = member_infos.first()?;
    // 尝试提取别名
    let alias = extract_index_member_alias(semantic_model, member_info)?;
    // 创建 hint
    let document = semantic_model.get_document();
    let position = {
        let index_token = index_expr.get_index_name_token()?;
        let range = index_token.text_range();
        let lsp_range = document.to_lsp_range(range)?;
        lsp_range.end
    };

    let label_location = {
        let range = index_expr.get_index_key()?.get_range()?;
        let lsp_range = document.to_lsp_range(range)?;
        Location::new(document.get_uri(), lsp_range)
    };

    let hint = InlayHint {
        kind: Some(InlayHintKind::TYPE),
        label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
            value: format!(": {}", alias),
            location: Some(label_location),
            ..Default::default()
        }]),
        position,
        text_edits: None,
        tooltip: None,
        padding_left: Some(true),
        padding_right: None,
        data: None,
    };

    result.push(hint);
    Some(())
}

fn build_enum_param_hint(
    semantic_model: &SemanticModel,
    result: &mut Vec<InlayHint>,
    call_expr: LuaCallExpr,
) -> Option<()> {
    if !semantic_model.get_emmyrc().hint.enum_param_hint {
        return Some(());
    }

    let func_type = semantic_model.infer_call_expr_func(call_expr.clone(), None)?;
    let call_args = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
    let params = func_type.get_params();

    let colon_call = call_expr.is_colon_call();
    let colon_define = func_type.is_colon_define();

    let param_offset: i32 = match (colon_call, colon_define) {
        (true, false) => 1,
        (false, true) => -1,
        _ => 0,
    };

    for (i, arg) in call_args.iter().enumerate() {
        let param_index = i as i32 + param_offset;
        if param_index < 0 {
            continue;
        }
        process_enum_hint_for_arg(semantic_model, result, arg, params, param_index as usize);
    }

    Some(())
}

fn process_enum_hint_for_arg(
    semantic_model: &SemanticModel,
    result: &mut Vec<InlayHint>,
    arg: &LuaExpr,
    params: &[(String, Option<LuaType>)],
    param_index: usize,
) -> Option<()> {
    let (_, param_type) = params.get(param_index)?;
    let param_type = param_type.as_ref()?;

    let type_id = match param_type {
        LuaType::Ref(id) => id,
        _ => return None,
    };

    let type_decl = semantic_model
        .get_db()
        .get_type_index()
        .get_type_decl(type_id)?;
    if !type_decl.is_enum() {
        return None;
    }

    // 推断参数类型
    let arg_type = semantic_model.infer_expr(arg.clone()).ok()?;

    // 查找对应的枚举成员
    let member_decl = find_matching_enum_member(semantic_model, type_decl, &arg_type)?;
    let member_name = member_decl.get_key().to_path();

    match arg {
        LuaExpr::LiteralExpr(literal_expr) => {
            if let Some(literal_token) = literal_expr.get_literal() {
                match literal_token {
                    LuaLiteralToken::String(string_token) => {
                        if string_token.get_value() == member_name {
                            return None;
                        }
                    }
                    LuaLiteralToken::Number(number_token) => {
                        if number_token.is_int() {
                            let number_value = format!("[{}]", number_token.get_int_value());
                            if number_value == member_name {
                                return None;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        LuaExpr::NameExpr(name_expr) => {
            if let Some(arg_name) = name_expr.get_name_text() {
                if member_name == arg_name {
                    return None;
                }
                // 名称里包含了枚举名和成员名(忽略大小写)也不显示提示
                let lower_arg_name = arg_name.to_lowercase();
                let lower_enum_name = type_decl.get_name().to_lowercase();
                let lower_member_name = member_name.to_lowercase();
                if lower_arg_name.contains(&lower_enum_name)
                    && lower_arg_name.contains(&lower_member_name)
                {
                    return None;
                }
            }
        }
        LuaExpr::IndexExpr(index_expr) => {
            // 对索引访问需要完全匹配尾名称
            if let Some(index_name_token) = index_expr.get_index_name_token() {
                if let Some(name_token) =
                    emmylua_parser::LuaNameToken::cast(index_name_token.clone())
                {
                    let index_name = name_token.get_name_text();
                    if index_name == member_name {
                        return None;
                    }
                }
            }
        }
        _ => {}
    }

    let enum_name = type_decl.get_name();
    let hint_text = format!("{}.{}", enum_name, member_name);

    let document = semantic_model.get_document();
    let range = arg.get_range();
    let lsp_range = document.to_lsp_range(range)?;

    let hint = InlayHint {
        kind: Some(InlayHintKind::PARAMETER),
        label: InlayHintLabel::String(hint_text),
        position: lsp_range.end,
        text_edits: None,
        tooltip: None,
        padding_left: Some(true),
        padding_right: None,
        data: None,
    };
    result.push(hint);

    Some(())
}

fn find_matching_enum_member<'a>(
    semantic_model: &'a SemanticModel,
    type_decl: &LuaTypeDecl,
    arg_type: &LuaType,
) -> Option<&'a LuaMember> {
    let enum_member_owner = LuaMemberOwner::Type(type_decl.get_id());
    let enum_members = semantic_model
        .get_db()
        .get_member_index()
        .get_members(&enum_member_owner)?;
    let is_enum_key = type_decl.is_enum_key();

    for member_decl in enum_members {
        let is_match = if is_enum_key {
            let member_key = member_decl.get_key();
            match (member_key, arg_type) {
                (LuaMemberKey::Name(s), LuaType::StringConst(arg_s)) => s == arg_s.as_ref(),
                (LuaMemberKey::Integer(i), LuaType::IntegerConst(arg_i)) => *i == *arg_i,
                (LuaMemberKey::ExprType(typ), _) => typ == arg_type,
                _ => false,
            }
        } else {
            if let Some(type_cache) = semantic_model
                .get_db()
                .get_type_index()
                .get_type_cache(&member_decl.get_id().into())
            {
                type_cache.as_type() == arg_type
            } else {
                false
            }
        };

        if is_match {
            return Some(member_decl);
        }
    }
    None
}
