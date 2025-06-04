use std::str::FromStr;

use emmylua_code_analysis::{
    LuaDeclId, LuaMemberId, LuaMemberInfo, LuaMemberKey, LuaSemanticDeclId, LuaType, LuaTypeDeclId,
    SemanticDeclLevel, SemanticModel,
};
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallExpr, LuaExpr, LuaIndexExpr, LuaStringToken, LuaSyntaxToken,
    LuaTableExpr,
};
use lsp_types::{GotoDefinitionResponse, Location, Position, Range, Uri};

use crate::handlers::hover::find_member_origin_owner;

pub fn goto_def_definition(
    semantic_model: &SemanticModel,
    property_owner: LuaSemanticDeclId,
    trigger_token: &LuaSyntaxToken,
) -> Option<GotoDefinitionResponse> {
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

    match property_owner {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            let decl = semantic_model
                .get_db()
                .get_decl_index()
                .get_decl(&decl_id)?;
            let document = semantic_model.get_document_by_file_id(decl_id.file_id)?;
            let location = document.to_lsp_location(decl.get_range())?;
            return Some(GotoDefinitionResponse::Scalar(location));
        }
        LuaSemanticDeclId::Member(member_id) => {
            if let Some(property_owner) = find_member_origin_owner(semantic_model, member_id) {
                if property_owner != LuaSemanticDeclId::Member(member_id) {
                    return goto_def_definition(semantic_model, property_owner, trigger_token);
                }
            }

            let member_key = semantic_model
                .get_db()
                .get_member_index()
                .get_member(&member_id)?
                .get_key();

            let mut locations: Vec<Location> = Vec::new();

            // 添加原始成员的位置
            if let Some(location) = get_member_location(semantic_model, &member_id) {
                locations.push(location);
            }

            // 查找实体的定义, 例如在 ---@field 时声明, obj = {} 时实际定义
            if let Some(table_field_info) =
                find_table_member_definition(semantic_model, trigger_token, member_key)
            {
                if let Some(LuaSemanticDeclId::Member(table_member_id)) =
                    table_field_info.property_owner_id
                {
                    if let Some(location) = get_member_location(semantic_model, &table_member_id) {
                        if !locations.contains(&location) {
                            locations.push(location);
                        }
                    }
                }
            }

            if !locations.is_empty() {
                return Some(GotoDefinitionResponse::Array(locations));
            }
        }
        LuaSemanticDeclId::TypeDecl(type_decl_id) => {
            let type_decl = semantic_model
                .get_db()
                .get_type_index()
                .get_type_decl(&type_decl_id)?;
            let mut locations: Vec<Location> = Vec::new();
            for lua_location in type_decl.get_locations() {
                let document = semantic_model.get_document_by_file_id(lua_location.file_id)?;
                let location = document.to_lsp_location(lua_location.range)?;
                locations.push(location);
            }

            return Some(GotoDefinitionResponse::Array(locations));
        }
        _ => {}
    }
    None
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
    if let Some(LuaType::StrTplRef(str_tpl)) = target_param.1.clone() {
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

        return Some(GotoDefinitionResponse::Array(locations));
    }

    None
}

pub fn find_table_member_definition(
    semantic_model: &SemanticModel,
    trigger_token: &LuaSyntaxToken,
    member_key: &LuaMemberKey,
) -> Option<LuaMemberInfo> {
    let index_expr = trigger_token.parent().and_then(LuaIndexExpr::cast)?;
    let prefix_expr = index_expr.get_prefix_expr()?;

    let decl = semantic_model.find_decl(
        prefix_expr.syntax().clone().into(),
        SemanticDeclLevel::default(),
    );

    if let Some(LuaSemanticDeclId::LuaDecl(decl_id)) = decl {
        return find_member_in_table_decl(semantic_model, &decl_id, member_key);
    }

    None
}

fn find_member_in_table_decl(
    semantic_model: &SemanticModel,
    decl_id: &LuaDeclId,
    member_key: &LuaMemberKey,
) -> Option<LuaMemberInfo> {
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

    let member_infos = semantic_model.get_member_infos(&typ)?;
    member_infos.iter().find(|m| m.key == *member_key).cloned()
}

fn get_member_location(
    semantic_model: &SemanticModel,
    member_id: &LuaMemberId,
) -> Option<Location> {
    let document = semantic_model.get_document_by_file_id(member_id.file_id)?;
    document.to_lsp_location(member_id.get_syntax_id().get_range())
}
