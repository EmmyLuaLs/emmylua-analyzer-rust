use std::str::FromStr;

use emmylua_code_analysis::{
    LuaCompilation, LuaDeclId, LuaMemberId, LuaMemberInfo, LuaMemberKey, LuaSemanticDeclId,
    LuaType, LuaTypeDeclId, SemanticDeclLevel, SemanticModel,
};
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallExpr, LuaExpr, LuaIndexExpr, LuaReturnStat, LuaStringToken,
    LuaSyntaxToken, LuaTableExpr, LuaTableField,
};
use itertools::Itertools;
use lsp_types::{GotoDefinitionResponse, Location, Position, Range, Uri};

use crate::handlers::{
    definition::goto_function::{find_function_call_origin, find_matching_function_definitions},
    hover::{find_all_same_named_members, find_member_origin_owner},
};

pub fn goto_def_definition(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    property_owner: LuaSemanticDeclId,
    trigger_token: &LuaSyntaxToken,
) -> Option<GotoDefinitionResponse> {
    // 首先检查属性源位置
    if let Some(property) = semantic_model
        .get_db()
        .get_property_index()
        .get_property(&property_owner)
    {
        if let Some(source) = &property.source {
            if let Some(location) = goto_source_location(source) {
                return Some(GotoDefinitionResponse::Scalar(location));
            }
        }
    }

    // 根据不同的语义声明类型处理
    match property_owner {
        LuaSemanticDeclId::LuaDecl(decl_id) => handle_decl_definition(
            semantic_model,
            compilation,
            trigger_token,
            &property_owner,
            &decl_id,
        ),
        LuaSemanticDeclId::Member(member_id) => {
            handle_member_definition(semantic_model, compilation, trigger_token, &member_id)
        }
        LuaSemanticDeclId::TypeDecl(type_decl_id) => {
            handle_type_decl_definition(semantic_model, &type_decl_id)
        }
        _ => None,
    }
}

fn handle_decl_definition(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    trigger_token: &LuaSyntaxToken,
    property_owner: &LuaSemanticDeclId,
    decl_id: &LuaDeclId,
) -> Option<GotoDefinitionResponse> {
    // 尝试查找函数调用的原始定义
    if let Some(match_semantic_decl) =
        find_function_call_origin(semantic_model, compilation, trigger_token, property_owner)
    {
        if let LuaSemanticDeclId::LuaDecl(matched_decl_id) = match_semantic_decl {
            return Some(GotoDefinitionResponse::Scalar(get_decl_location(
                semantic_model,
                &matched_decl_id,
            )?));
        }
    }

    // 返回声明的位置
    let location = get_decl_location(semantic_model, decl_id)?;
    Some(GotoDefinitionResponse::Scalar(location))
}

fn handle_member_definition(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    trigger_token: &LuaSyntaxToken,
    member_id: &LuaMemberId,
) -> Option<GotoDefinitionResponse> {
    let same_named_members =
        find_all_same_named_members(semantic_model, &Some(LuaSemanticDeclId::Member(*member_id)))?;

    let mut locations: Vec<Location> = Vec::new();

    // 尝试寻找函数调用时最匹配的定义
    if let Some(match_members) = find_matching_function_definitions(
        semantic_model,
        compilation,
        trigger_token,
        &same_named_members,
    ) {
        process_matched_members(semantic_model, compilation, &match_members, &mut locations);
        if !locations.is_empty() {
            return Some(GotoDefinitionResponse::Array(locations));
        }
    }

    // 添加原始成员的位置
    for member in same_named_members {
        if let LuaSemanticDeclId::Member(member_id) = member {
            if let Some(location) = get_member_location(semantic_model, &member_id) {
                locations.push(location);
            }
        }
    }

    // 处理实例表成员
    add_instance_table_member_locations(semantic_model, trigger_token, member_id, &mut locations);

    if !locations.is_empty() {
        Some(GotoDefinitionResponse::Array(
            locations.into_iter().unique().collect(),
        ))
    } else {
        None
    }
}

fn handle_type_decl_definition(
    semantic_model: &SemanticModel,
    type_decl_id: &LuaTypeDeclId,
) -> Option<GotoDefinitionResponse> {
    let type_decl = semantic_model
        .get_db()
        .get_type_index()
        .get_type_decl(type_decl_id)?;

    let mut locations: Vec<Location> = Vec::new();
    for lua_location in type_decl.get_locations() {
        let document = semantic_model.get_document_by_file_id(lua_location.file_id)?;
        let location = document.to_lsp_location(lua_location.range)?;
        locations.push(location);
    }

    Some(GotoDefinitionResponse::Array(locations))
}

fn process_matched_members(
    semantic_model: &SemanticModel,
    compilation: &LuaCompilation,
    match_members: &[LuaSemanticDeclId],
    locations: &mut Vec<Location>,
) {
    for member in match_members {
        match member {
            LuaSemanticDeclId::Member(member_id) => {
                if should_trace_member(semantic_model, member_id).unwrap_or(false) {
                    // 尝试搜索这个成员最原始的定义
                    match find_member_origin_owner(compilation, semantic_model, *member_id) {
                        Some(LuaSemanticDeclId::Member(origin_member_id)) => {
                            if let Some(location) =
                                get_member_location(semantic_model, &origin_member_id)
                            {
                                locations.push(location);
                                continue;
                            }
                        }
                        Some(LuaSemanticDeclId::LuaDecl(origin_decl_id)) => {
                            if let Some(location) =
                                get_decl_location(semantic_model, &origin_decl_id)
                            {
                                locations.push(location);
                                continue;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(location) = get_member_location(semantic_model, member_id) {
                    locations.push(location);
                }
            }
            LuaSemanticDeclId::LuaDecl(decl_id) => {
                if let Some(location) = get_decl_location(semantic_model, decl_id) {
                    locations.push(location);
                }
            }
            _ => {}
        }
    }
}

fn add_instance_table_member_locations(
    semantic_model: &SemanticModel,
    trigger_token: &LuaSyntaxToken,
    member_id: &LuaMemberId,
    locations: &mut Vec<Location>,
) {
    /* 对于实例的处理, 对于实例 obj
    ```lua
        ---@class T
        ---@field func fun(a: int)
        ---@field func fun(a: string)

        ---@type T
        local obj = {
            func = function() end  -- 点击`func`时需要寻找`T`的定义
        }
        obj:func(1) -- 点击`func`时, 不止需要寻找`T`的定义也需要寻找`obj`实例化时赋值的`func`
    ```
     */
    if let Some(table_field_infos) =
        find_instance_table_member(semantic_model, trigger_token, member_id)
    {
        for table_field_info in table_field_infos {
            if let Some(LuaSemanticDeclId::Member(table_member_id)) =
                table_field_info.property_owner_id
            {
                if let Some(location) = get_member_location(semantic_model, &table_member_id) {
                    locations.push(location);
                }
            }
        }
    }
}

fn goto_source_location(source: &str) -> Option<Location> {
    let source_parts = source.split('#').collect::<Vec<_>>();
    if source_parts.len() == 2 {
        let uri = source_parts[0];
        let range = source_parts[1];
        let range_parts = range.split(':').collect::<Vec<_>>();
        if range_parts.len() == 2 {
            let mut line_str = range_parts[0];
            if line_str.to_ascii_lowercase().starts_with("l") {
                line_str = &line_str[1..];
            }
            let line = line_str.parse::<u32>().ok()?;
            let col = range_parts[1].parse::<u32>().ok()?;
            let range = Range {
                start: Position::new(line, col),
                end: Position::new(line, col),
            };
            return Some(Location {
                uri: Uri::from_str(uri).ok()?,
                range,
            });
        }
    }

    None
}

pub fn goto_str_tpl_ref_definition(
    semantic_model: &SemanticModel,
    string_token: LuaStringToken,
) -> Option<GotoDefinitionResponse> {
    let name = string_token.get_value();
    let call_expr = string_token.ancestors::<LuaCallExpr>().next()?;
    let arg_exprs = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
    let string_token_idx = arg_exprs.iter().position(|arg| {
        if let LuaExpr::LiteralExpr(literal_expr) = arg {
            if literal_expr
                .syntax()
                .text_range()
                .contains(string_token.get_range().start())
            {
                true
            } else {
                false
            }
        } else {
            false
        }
    })?;
    let func = semantic_model.infer_call_expr_func(call_expr.clone(), None)?;
    let params = func.get_params();

    let target_param = match (func.is_colon_define(), call_expr.is_colon_call()) {
        (false, true) => params.get(string_token_idx + 1),
        (true, false) => {
            if string_token_idx > 0 {
                params.get(string_token_idx - 1)
            } else {
                None
            }
        }
        _ => params.get(string_token_idx),
    }?;
    // 首先尝试直接匹配StrTplRef类型
    if let Some(locations) =
        try_extract_str_tpl_ref_locations(semantic_model, &target_param.1, &name)
    {
        return Some(GotoDefinitionResponse::Array(locations));
    }

    // 如果参数类型是union，尝试从中提取StrTplRef类型
    if let Some(LuaType::Union(union_type)) = target_param.1.clone() {
        for union_member in union_type.into_vec().iter() {
            if let Some(locations) = try_extract_str_tpl_ref_locations(
                semantic_model,
                &Some(union_member.clone()),
                &name,
            ) {
                return Some(GotoDefinitionResponse::Array(locations));
            }
        }
    }

    None
}

pub fn find_instance_table_member(
    semantic_model: &SemanticModel,
    trigger_token: &LuaSyntaxToken,
    member_id: &LuaMemberId,
) -> Option<Vec<LuaMemberInfo>> {
    let member_key = semantic_model
        .get_db()
        .get_member_index()
        .get_member(&member_id)?
        .get_key();
    let parent = trigger_token.parent()?;

    match parent {
        expr_node if LuaIndexExpr::can_cast(expr_node.kind().into()) => {
            let index_expr = LuaIndexExpr::cast(expr_node)?;
            let prefix_expr = index_expr.get_prefix_expr()?;

            let decl = semantic_model.find_decl(
                prefix_expr.syntax().clone().into(),
                SemanticDeclLevel::default(),
            );

            if let Some(LuaSemanticDeclId::LuaDecl(decl_id)) = decl {
                return find_member_in_table_const(semantic_model, &decl_id, member_key);
            }
        }
        table_field_node if LuaTableField::can_cast(table_field_node.kind().into()) => {
            let table_field = LuaTableField::cast(table_field_node)?;
            let table_expr = table_field.get_parent::<LuaTableExpr>()?;
            let typ = semantic_model.infer_table_should_be(table_expr)?;
            return semantic_model.get_member_info_with_key(&typ, member_key.clone(), true);
        }
        _ => {}
    }

    None
}

fn find_member_in_table_const(
    semantic_model: &SemanticModel,
    decl_id: &LuaDeclId,
    member_key: &LuaMemberKey,
) -> Option<Vec<LuaMemberInfo>> {
    let root = semantic_model
        .get_db()
        .get_vfs()
        .get_syntax_tree(&decl_id.file_id)?
        .get_red_root();

    let node = semantic_model
        .get_db()
        .get_decl_index()
        .get_decl(decl_id)?
        .get_value_syntax_id()?
        .to_node_from_root(&root)?;

    let table_expr = LuaTableExpr::cast(node)?;
    let typ = semantic_model
        .infer_expr(LuaExpr::TableExpr(table_expr))
        .ok()?;

    semantic_model.get_member_info_with_key(&typ, member_key.clone(), true)
}

/// 是否对 member 启动追踪
fn should_trace_member(semantic_model: &SemanticModel, member_id: &LuaMemberId) -> Option<bool> {
    let root = semantic_model
        .get_db()
        .get_vfs()
        .get_syntax_tree(&member_id.file_id)?
        .get_red_root();
    let node = member_id.get_syntax_id().to_node_from_root(&root)?;
    let parent = node.parent()?.parent()?;
    // 如果成员在返回语句中, 则需要追踪
    if LuaReturnStat::can_cast(parent.kind().into()) {
        return Some(true);
    } else {
        let typ = semantic_model.get_type(member_id.clone().into());
        if typ.is_signature() {
            return Some(true);
        }
    }
    None
}

fn get_member_location(
    semantic_model: &SemanticModel,
    member_id: &LuaMemberId,
) -> Option<Location> {
    let document = semantic_model.get_document_by_file_id(member_id.file_id)?;
    document.to_lsp_location(member_id.get_syntax_id().get_range())
}

fn get_decl_location(semantic_model: &SemanticModel, decl_id: &LuaDeclId) -> Option<Location> {
    let decl = semantic_model
        .get_db()
        .get_decl_index()
        .get_decl(&decl_id)?;
    let document = semantic_model.get_document_by_file_id(decl_id.file_id)?;
    let location = document.to_lsp_location(decl.get_range())?;
    Some(location)
}

fn try_extract_str_tpl_ref_locations(
    semantic_model: &SemanticModel,
    param_type: &Option<LuaType>,
    name: &str,
) -> Option<Vec<Location>> {
    if let Some(LuaType::StrTplRef(str_tpl)) = param_type {
        let prefix = str_tpl.get_prefix();
        let suffix = str_tpl.get_suffix();
        let type_decl_id = LuaTypeDeclId::new(format!("{}{}{}", prefix, name, suffix).as_str());
        let type_decl = semantic_model
            .get_db()
            .get_type_index()
            .get_type_decl(&type_decl_id)?;
        let mut locations = Vec::new();
        for lua_location in type_decl.get_locations() {
            let document = semantic_model.get_document_by_file_id(lua_location.file_id)?;
            let location = document.to_lsp_location(lua_location.range)?;
            locations.push(location);
        }
        return Some(locations);
    }
    None
}
