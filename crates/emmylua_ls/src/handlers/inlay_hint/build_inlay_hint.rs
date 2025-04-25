use std::collections::HashMap;

use emmylua_code_analysis::{
    FileId, InferGuard, LuaFunctionType, LuaMemberId, LuaMemberKey, LuaSemanticDeclId,
    LuaSignatureId, LuaType, RenderLevel, SemanticModel,
};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaCallExpr, LuaClosureExpr, LuaExpr, LuaFuncStat, LuaIndexExpr,
    LuaLocalName, LuaSyntaxId, LuaVarExpr,
};
use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, InlayHintLabelPart, Location};
use rowan::NodeOrToken;

use emmylua_code_analysis::humanize_type;

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
                build_call_expr_await_hint(semantic_model, &mut result, call_expr);
            }
            LuaAst::LuaLocalName(local_name) => {
                build_local_name_hint(semantic_model, &mut result, local_name);
            }
            LuaAst::LuaFuncStat(func_stat) => {
                build_func_stat_override_hint(semantic_model, &mut result, func_stat);
            }
            _ => {}
        }
    }

    Some(result)
}

fn build_closure_hint(
    semantic_model: &SemanticModel,
    result: &mut Vec<InlayHint>,
    closure: LuaClosureExpr,
) -> Option<()> {
    if !semantic_model.get_emmyrc().hint.param_hint {
        return Some(());
    }
    let file_id = semantic_model.get_file_id();
    let signature_id = LuaSignatureId::from_closure(file_id, &closure);
    let signature = semantic_model
        .get_db()
        .get_signature_index()
        .get(&signature_id)?;

    let lua_params = closure.get_params_list()?;
    let signature_params = signature.get_type_params();
    let mut lua_params_map = HashMap::new();
    for param in lua_params.get_params() {
        if let Some(name_token) = param.get_name_token() {
            let name = name_token.get_name_text().to_string();
            lua_params_map.insert(name, param);
        } else if param.is_dots() {
            lua_params_map.insert("...".to_string(), param);
        }
    }

    let document = semantic_model.get_document();
    let db = semantic_model.get_db();
    for (signature_param_name, typ) in &signature_params {
        if let Some(typ) = typ {
            if typ.is_any() {
                continue;
            }

            if let Some(lua_param) = lua_params_map.get(signature_param_name) {
                let lsp_range = document.to_lsp_range(lua_param.get_range())?;
                let typ_desc = format!(": {}", humanize_type(db, &typ, RenderLevel::Simple));
                let hint = InlayHint {
                    kind: Some(InlayHintKind::TYPE),
                    label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                        value: typ_desc,
                        location: Some(Location::new(document.get_uri(), lsp_range)),
                        ..Default::default()
                    }]),
                    position: lsp_range.end,
                    text_edits: None,
                    tooltip: None,
                    padding_left: Some(true),
                    padding_right: None,
                    data: None,
                };
                result.push(hint);
            }
        }
    }

    Some(())
}

fn build_call_expr_param_hint(
    semantic_model: &SemanticModel,
    result: &mut Vec<InlayHint>,
    call_expr: LuaCallExpr,
) -> Option<()> {
    if !semantic_model.get_emmyrc().hint.param_hint {
        return Some(());
    }

    let func = semantic_model.infer_call_expr_func(call_expr.clone(), None)?;
    let call_args_list = call_expr.get_args_list()?;
    let colon_call = call_expr.is_colon_call();
    build_call_args_for_func_type(
        semantic_model,
        result,
        call_args_list.get_args().collect(),
        colon_call,
        &func,
    );

    Some(())
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
                let arg = &call_args[i];
                let range = arg.get_range();
                let document = semantic_model.get_document();
                let lsp_range = document.to_lsp_range(range)?;
                let hint = InlayHint {
                    kind: Some(InlayHintKind::PARAMETER),
                    label: InlayHintLabel::String(format!("var{}:", i - idx)),
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

        let range = arg.get_range();
        let document = semantic_model.get_document();
        let lsp_range = document.to_lsp_range(range)?;
        let hint = InlayHint {
            kind: Some(InlayHintKind::PARAMETER),
            label: InlayHintLabel::String(format!("{}:", name)),
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

    let typ = semantic_model
        .get_semantic_info(NodeOrToken::Node(local_name.syntax().clone()))?
        .typ
        .clone();

    let document = semantic_model.get_document();
    let db = semantic_model.get_db();
    let range = local_name.get_range();
    let lsp_range = document.to_lsp_range(range)?;

    let typ_desc = humanize_type(db, &typ, RenderLevel::Simple);
    let hint = InlayHint {
        kind: Some(InlayHintKind::TYPE),
        label: InlayHintLabel::String(format!(": {}", typ_desc)),
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
    if let LuaType::Ref(super_type_id) = &super_type {
        infer_guard.check(super_type_id).ok()?;
        let member_map = semantic_model.infer_member_map(&super_type)?;

        if let Some(member_infos) = member_map.get(&member_key) {
            let first_property = member_infos.first()?.property_owner_id.clone()?;
            if let LuaSemanticDeclId::Member(member_id) = first_property {
                return Some(member_id);
            }
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
