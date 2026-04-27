use super::*;
use crate::{
    SalsaDeclKindSummary, SalsaExportTargetSummary, SalsaGlobalRootSummary, SalsaMemberRootSummary,
    SalsaMemberTargetSummary, SalsaScopeKindSummary, Vfs, VirtualUrlGenerator, WorkspaceId,
    WorkspaceImport,
};
use emmylua_parser::{LuaAstNode, LuaIndexKey, LuaParser, LuaTableField, ParserConfig};
use rowan::TextSize;

mod core;
mod doc;
mod flow;
mod for_range_iter;
mod module_global;
mod program_point;
mod property;
mod semantic;
mod semantic_graph;
mod semantic_solver;
mod signature_return;
mod single_file;
mod table_shape;
mod type_query;

const TEST_WORKSPACE_ROOT: &str = "C:/ws";

pub(super) fn setup_compilation() -> SalsaSummaryHost {
    let mut compilation = SalsaSummaryHost::new(Arc::new(Emmyrc::default()));
    compilation.set_workspaces(vec![Workspace::new(
        PathBuf::from(TEST_WORKSPACE_ROOT),
        WorkspaceImport::All,
        WorkspaceId::MAIN,
    )]);
    compilation
}

pub(super) fn set_test_file(
    compilation: &mut SalsaSummaryHost,
    file_id: u32,
    path: &str,
    source: &str,
) {
    compilation.set_file(
        FileId::new(file_id),
        Some(PathBuf::from(path)),
        source.to_string(),
        false,
    );
}

fn build_member_full_name(target: &SalsaMemberTargetSummary) -> String {
    let mut full_name = String::new();

    match &target.root {
        SalsaMemberRootSummary::Global(SalsaGlobalRootSummary::Env) => full_name.push_str("_ENV"),
        SalsaMemberRootSummary::Global(SalsaGlobalRootSummary::Name(name)) => {
            full_name.push_str(name)
        }
        SalsaMemberRootSummary::LocalDecl { name, .. } => full_name.push_str(name),
    }

    for segment in target.owner_segments.iter() {
        if !full_name.is_empty() {
            full_name.push('.');
        }
        full_name.push_str(segment);
    }

    if !target.member_name.is_empty() {
        if !full_name.is_empty() {
            full_name.push('.');
        }
        full_name.push_str(&target.member_name);
    }

    full_name
}
