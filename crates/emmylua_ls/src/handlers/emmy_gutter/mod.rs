mod emmy_gutter_detail_request;
mod emmy_gutter_request;

use std::str::FromStr;

use crate::{
    context::ServerContextSnapshot,
    handlers::{
        emmy_gutter::emmy_gutter_request::{EmmyGutterParams, GutterInfo},
        inlay_hint::{get_override_lsp_location, get_super_member_id},
    },
};
pub use emmy_gutter_detail_request::*;
pub use emmy_gutter_request::*;
use emmylua_code_analysis::{InferGuard, LuaMemberKey, LuaType, SemanticModel};
use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaVarExpr};
use lsp_types::Uri;
use tokio_util::sync::CancellationToken;

pub async fn on_emmy_gutter_handler(
    context: ServerContextSnapshot,
    params: EmmyGutterParams,
    _: CancellationToken,
) -> Option<Vec<GutterInfo>> {
    let uri = Uri::from_str(&params.uri).ok()?;
    let analysis = context.analysis().read().await;
    let file_id = analysis.get_file_id(&uri)?;
    let semantic_model = analysis.compilation.get_semantic_model(file_id)?;

    build_gutter_infos(&semantic_model)
}

fn build_gutter_infos(semantic_model: &SemanticModel) -> Option<Vec<GutterInfo>> {
    let root = semantic_model.get_root().clone();
    let document = semantic_model.get_document();
    let mut gutters = Vec::new();
    for tag in root.descendants::<LuaAst>() {
        match tag {
            LuaAst::LuaDocTagAlias(alias) => {
                let name_token = alias.get_name_token()?;
                let range = name_token.get_range();
                let name = name_token.get_text();
                let lsp_range = document.to_lsp_range(range)?;
                gutters.push(GutterInfo {
                    range: lsp_range,
                    kind: GutterKind::Alias,
                    detail: Some("type alias".to_string()),
                    data: Some(name.to_string()),
                });
            }
            LuaAst::LuaDocTagClass(class) => {
                let name_token = class.get_name_token()?;
                let range = name_token.get_range();
                let name = name_token.get_text();
                let lsp_range = document.to_lsp_range(range)?;
                gutters.push(GutterInfo {
                    range: lsp_range,
                    kind: GutterKind::Class,
                    detail: Some("class".to_string()),
                    data: Some(name.to_string()),
                });
            }
            LuaAst::LuaDocTagEnum(enm) => {
                let range = enm.get_name_token()?.get_range();
                let lsp_range = document.to_lsp_range(range)?;
                gutters.push(GutterInfo {
                    range: lsp_range,
                    kind: GutterKind::Enum,
                    detail: Some("enum".to_string()),
                    data: None,
                });
            }
            LuaAst::LuaFuncStat(func_stat) => {
                build_func_override_gutter_info(semantic_model, &mut gutters, func_stat);
            }
            _ => {}
        }
    }

    Some(gutters)
}

fn build_func_override_gutter_info(
    semantic_model: &SemanticModel,
    gutters: &mut Vec<GutterInfo>,
    func_stat: emmylua_parser::LuaFuncStat,
) -> Option<()> {
    if !semantic_model.get_emmyrc().hint.override_hint {
        return Some(());
    }

    let func_name = func_stat.get_func_name()?;
    let func_name_pos = func_name.get_position();
    if let LuaVarExpr::IndexExpr(index_expr) = func_name {
        let prefix_expr = index_expr.get_prefix_expr()?;
        let prefix_type = semantic_model.infer_expr(prefix_expr).ok()?;
        if let LuaType::Def(id) = prefix_type {
            let supers = semantic_model
                .get_db()
                .get_type_index()
                .get_super_types(&id)?;

            let index_key = index_expr.get_index_key()?;
            let member_key: LuaMemberKey = semantic_model.get_member_key(&index_key)?;
            let guard = InferGuard::new();
            for super_type in supers {
                if let Some(member_id) =
                    get_super_member_id(semantic_model, super_type, &member_key, &guard)
                {
                    let member = semantic_model
                        .get_db()
                        .get_member_index()
                        .get_member(&member_id)?;

                    let document = semantic_model.get_document();
                    let func_name_lsp_pos = document.to_lsp_position(func_name_pos)?;

                    let file_id = member.get_file_id();
                    let syntax_id = member.get_syntax_id();
                    let lsp_location =
                        get_override_lsp_location(semantic_model, file_id, syntax_id)?;
                    let hint = GutterInfo {
                        range: lsp_types::Range {
                            start: func_name_lsp_pos,
                            end: func_name_lsp_pos,
                        },
                        kind: GutterKind::Override,
                        detail: Some("overrides method".to_string()),
                        data: Some(format!(
                            "{}#{}#{}",
                            lsp_location.uri.get_file_path()?.display(),
                            lsp_location.range.start.line,
                            0
                        )),
                    };
                    gutters.push(hint);
                    break;
                }
            }
        }
    }

    Some(())
}

pub async fn on_emmy_gutter_detail_handler(
    context: ServerContextSnapshot,
    params: EmmyGutterDetailParams,
    _: CancellationToken,
) -> Option<GutterDetailResponse> {
    let type_name = params.data;
    let analysis = context.analysis().read().await;
    let db = &analysis.compilation.get_db();
    let type_index = db.get_type_index();

    // Find the type declaration
    let type_id = emmylua_code_analysis::LuaTypeDeclId::global(&type_name);
    type_index.get_type_decl(&type_id)?;

    // Get all subclasses
    let sub_types = type_index.get_all_sub_types(&type_id);

    // Build locations from subclasses
    let mut locations = Vec::new();
    for sub_type in sub_types {
        for location in sub_type.get_locations() {
            let file_id = location.file_id;
            if let Some(document) = db.get_vfs().get_document(&file_id) {
                if let Some(lsp_range) = document.to_lsp_range(location.range) {
                    locations.push(GutterLocation {
                        uri: document.get_uri().to_string(),
                        line: lsp_range.start.line as i32,
                        kind: GutterKind::Class,
                    });
                }
            }
        }
    }

    Some(GutterDetailResponse { locations })
}
