use emmylua_parser::{LuaAstNode, LuaChunk, LuaExpr, LuaIndexExpr, LuaIndexKey};
use smol_str::SmolStr;

use crate::{
    FileId, SalsaExportTargetSummary, SalsaMemberPathRootSummary, SalsaMemberPathSummary,
    SalsaModuleResolveIndex, analyze_module_return_points, build_module_resolve_index,
    resolve_module_export_in_index,
};

use super::super::{
    SalsaDeclTreeSummary, SalsaGlobalSummary, SalsaMemberIndexSummary, SalsaModuleSummary,
};

pub fn analyze_module_summary(
    file_id: FileId,
    decl_tree: &SalsaDeclTreeSummary,
    globals: &SalsaGlobalSummary,
    members: &SalsaMemberIndexSummary,
    chunk: LuaChunk,
) -> Option<SalsaModuleSummary> {
    let resolve_index = build_module_resolve_index(decl_tree, globals, members);
    build_module_summary_with_index(file_id, &resolve_index, chunk)
}

pub fn build_module_summary_with_index(
    file_id: FileId,
    resolve_index: &SalsaModuleResolveIndex,
    chunk: LuaChunk,
) -> Option<SalsaModuleSummary> {
    let export_target = analyze_module_return_points(chunk.get_block()?)
        .into_iter()
        .find_map(summarize_export_expr);
    let export = export_target
        .as_ref()
        .and_then(|target| resolve_module_export_in_index(target, &resolve_index));

    Some(SalsaModuleSummary {
        file_id,
        export_target,
        export,
    })
}

fn summarize_export_expr(expr: LuaExpr) -> Option<SalsaExportTargetSummary> {
    match expr {
        LuaExpr::NameExpr(name_expr) => Some(SalsaExportTargetSummary::LocalName(
            name_expr.get_name_text()?.into(),
        )),
        LuaExpr::IndexExpr(index_expr) => summarize_member_export(index_expr),
        LuaExpr::ClosureExpr(closure) => {
            Some(SalsaExportTargetSummary::Closure(closure.get_position()))
        }
        LuaExpr::TableExpr(table_expr) => {
            Some(SalsaExportTargetSummary::Table(table_expr.get_position()))
        }
        LuaExpr::ParenExpr(paren_expr) => summarize_export_expr(paren_expr.get_expr()?),
        _ => None,
    }
}

fn summarize_member_export(index_expr: LuaIndexExpr) -> Option<SalsaExportTargetSummary> {
    let path = extract_member_target_from_index_expr(&index_expr)?;
    Some(SalsaExportTargetSummary::Member(path))
}

fn extract_member_target_from_index_expr(
    index_expr: &LuaIndexExpr,
) -> Option<SalsaMemberPathSummary> {
    let (root, segments) = extract_member_path_from_index_expr(index_expr)?;
    let (member_name, owner_segments) = split_member_path(segments)?;
    Some(SalsaMemberPathSummary {
        root,
        owner_segments: owner_segments.into(),
        member_name,
    })
}

fn extract_member_path_from_index_expr(
    index_expr: &LuaIndexExpr,
) -> Option<(SalsaMemberPathRootSummary, Vec<SmolStr>)> {
    let (root, mut segments) = extract_member_path_from_expr(&index_expr.get_prefix_expr()?)?;
    segments.push(get_member_name(index_expr)?);
    Some((root, segments))
}

fn extract_member_path_from_expr(
    expr: &LuaExpr,
) -> Option<(SalsaMemberPathRootSummary, Vec<SmolStr>)> {
    match expr {
        LuaExpr::NameExpr(name_expr) => {
            let name = name_expr.get_name_text()?;
            let root = if name == "_G" || name == "_ENV" {
                SalsaMemberPathRootSummary::Env
            } else {
                SalsaMemberPathRootSummary::Name(name.into())
            };
            Some((root, Vec::new()))
        }
        LuaExpr::IndexExpr(index_expr) => extract_member_path_from_index_expr(index_expr),
        LuaExpr::ParenExpr(paren_expr) => extract_member_path_from_expr(&paren_expr.get_expr()?),
        _ => None,
    }
}

fn split_member_path(segments: Vec<SmolStr>) -> Option<(SmolStr, Vec<SmolStr>)> {
    let member_name = segments.last()?.clone();
    let owner_segments = if segments.len() > 1 {
        segments[..segments.len() - 1].to_vec()
    } else {
        Vec::new()
    };
    Some((member_name, owner_segments))
}

fn get_member_name(index_expr: &LuaIndexExpr) -> Option<SmolStr> {
    match index_expr.get_index_key()? {
        LuaIndexKey::Name(name) => Some(name.get_name_text().into()),
        LuaIndexKey::String(string) => Some(string.get_value().into()),
        _ => None,
    }
}
