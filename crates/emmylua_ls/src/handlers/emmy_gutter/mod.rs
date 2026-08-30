mod emmy_gutter_detail_request;
mod emmy_gutter_request;

use std::str::FromStr;

use crate::{
    context::{
        CancelStrategy, RequestOutcome, ServerContextSnapshot, analysis_query, snapshot_query,
    },
    handlers::emmy_gutter::emmy_gutter_request::{EmmyGutterParams, GutterInfo},
};
pub use emmy_gutter_detail_request::*;
pub use emmy_gutter_request::*;
use emmylua_code_analysis::{
    DocumentView, Emmyrc, LuaType, SalsaDatabase, SalsaSemanticModel, TypeScope,
};
use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaVarExpr};
use lsp_types::Uri;
use tokio_util::sync::CancellationToken;

pub async fn on_emmy_gutter_handler(
    context: ServerContextSnapshot,
    params: EmmyGutterParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<Vec<GutterInfo>> {
    let Ok(uri) = Uri::from_str(&params.uri) else {
        return RequestOutcome::Missing;
    };
    let cache_key = format!("gutter:{}", uri.as_str());
    analysis_query(
        context.analysis(),
        context.request_manager(),
        &cache_key,
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        Some(cancel_token.clone()),
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            let model = analysis.semantic_model(file_id)?;
            let document = analysis.salsa.document(file_id)?;
            let emmyrc = analysis.get_emmyrc();
            build_gutter_infos(&model, &document, &analysis.salsa, &emmyrc)
        },
    )
    .await
}

fn build_gutter_infos(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    salsa: &SalsaDatabase,
    emmyrc: &Emmyrc,
) -> Option<Vec<GutterInfo>> {
    let root = model.chunk()?;
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
                build_func_override_gutter_info(
                    model,
                    document,
                    salsa,
                    emmyrc,
                    &mut gutters,
                    func_stat,
                );
            }
            _ => {}
        }
    }

    Some(gutters)
}

fn build_func_override_gutter_info(
    model: &SalsaSemanticModel<'_>,
    document: &DocumentView,
    salsa: &SalsaDatabase,
    emmyrc: &Emmyrc,
    gutters: &mut Vec<GutterInfo>,
    func_stat: emmylua_parser::LuaFuncStat,
) -> Option<()> {
    if !emmyrc.hint.override_hint {
        return Some(());
    }

    let func_name = func_stat.get_func_name()?;
    let func_name_pos = func_name.get_position();
    if let LuaVarExpr::IndexExpr(index_expr) = func_name {
        let prefix_expr = index_expr.get_prefix_expr()?;
        let prefix_type = model.type_of_expr(prefix_expr.get_syntax_id());
        if let LuaType::Ref(id) | LuaType::Def(id) = prefix_type {
            // Full-name chain of parent types (`@class B: A`).
            let mut supers = Vec::new();
            if let Some(def) = model
                .type_defs_in_scope(TypeScope::Global, id.get_name())
                .into_iter()
                .find(|def| def.full_name.as_str() == id.get_name())
            {
                supers.extend(def.super_names.clone());
            }

            let index_key = index_expr.get_index_key()?;
            let member_name = index_key.get_path_part();

            for super_name in supers {
                let Some(super_def) = model
                    .type_defs_in_scope(TypeScope::Global, &super_name)
                    .into_iter()
                    .next()
                else {
                    continue;
                };
                let Some(member_ref) = model
                    .members_of_owner(&super_def.id)
                    .into_iter()
                    .find(|m| m.name.as_str() == member_name)
                else {
                    continue;
                };
                let Some(range) = member_ref.id.member_key_range() else {
                    continue;
                };
                let Some(target_document) = salsa.document(member_ref.file_id) else {
                    continue;
                };
                let Some(uri) = target_document.get_uri() else {
                    continue;
                };
                let Some(lsp_location_range) = target_document.to_lsp_range(range) else {
                    continue;
                };
                let lsp_location = lsp_types::Location {
                    uri: uri.clone(),
                    range: lsp_location_range,
                };
                let func_name_lsp_pos = document.to_lsp_position(func_name_pos)?;
                let hint = GutterInfo {
                    range: lsp_types::Range {
                        start: func_name_lsp_pos,
                        end: func_name_lsp_pos,
                    },
                    kind: GutterKind::Override,
                    detail: Some("overrides method".to_string()),
                    data: Some(format!("{}#{}#{}", *uri, lsp_location.range.start.line, 0)),
                };
                gutters.push(hint);
                break;
            }
        }
    }

    Some(())
}

pub async fn on_emmy_gutter_detail_handler(
    context: ServerContextSnapshot,
    params: EmmyGutterDetailParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<GutterDetailResponse> {
    let type_name = params.data;
    snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token,
        move |analysis| {
            let salsa = analysis.salsa_snapshot();
            let mut locations = Vec::new();
            for file_id in salsa.file_ids() {
                let Some(model) = SalsaSemanticModel::new(&salsa, file_id) else {
                    continue;
                };
                let Some(facts) = model.file_facts() else {
                    continue;
                };
                for def in facts.type_defs.iter() {
                    if !def
                        .super_names
                        .iter()
                        .any(|super_name| super_name.as_str() == type_name)
                    {
                        continue;
                    }
                    let Some(document) = salsa.document(file_id) else {
                        continue;
                    };
                    if let Some(lsp_range) = document.to_lsp_range(def.name_range)
                        && let Some(uri) = document.get_uri()
                    {
                        locations.push(GutterLocation {
                            uri: uri.to_string(),
                            line: lsp_range.start.line as i32,
                            kind: GutterKind::Class,
                        });
                    }
                }
            }
            Some(GutterDetailResponse { locations })
        },
    )
    .await
}
