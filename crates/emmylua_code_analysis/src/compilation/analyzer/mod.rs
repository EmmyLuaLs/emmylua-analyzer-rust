mod decl;
mod doc;
mod flow;
mod lua;
mod unresolve;

use std::{collections::HashMap, sync::Arc};

use crate::{db_index::DbIndex, profile::Profile, Emmyrc, InFiled, WorkspaceId};
use emmylua_parser::LuaChunk;
use unresolve::UnResolve;

pub fn analyze(db: &mut DbIndex, need_analyzed_files: Vec<InFiled<LuaChunk>>, config: Arc<Emmyrc>) {
    if need_analyzed_files.is_empty() {
        return;
    }

    let contexts = module_analyze(db, need_analyzed_files, config);

    for (workspace_id, mut context) in contexts {
        let profile_log = format!("analyze workspace {}", workspace_id);
        let _p = Profile::cond_new(&profile_log, context.tree_list.len() > 1);
        decl::analyze(db, &mut context);
        flow::analyze(db, &mut context);
        doc::analyze(db, &mut context);
        lua::analyze(db, &mut context);
        unresolve::analyze(db, &mut context);
    }
}

fn module_analyze(
    db: &mut DbIndex,
    need_analyzed_files: Vec<InFiled<LuaChunk>>,
    config: Arc<Emmyrc>,
) -> Vec<(WorkspaceId, AnalyzeContext)> {
    if need_analyzed_files.len() == 1 {
        let in_filed_tree = need_analyzed_files[0].clone();
        let file_id = in_filed_tree.file_id;
        if let Some(path) = db.get_vfs().get_file_path(&file_id).cloned() {
            let workspace_id = db
                .get_module_index_mut()
                .add_module_by_path(file_id, path.to_str().unwrap());
            let workspace_id = workspace_id.unwrap_or(WorkspaceId::MAIN);
            let mut context = AnalyzeContext::new(config);
            context.add_tree_chunk(in_filed_tree);
            return vec![(workspace_id, context)];
        }

        return vec![];
    }

    let _p = Profile::new("module analyze");
    let mut file_tree_map: HashMap<WorkspaceId, Vec<InFiled<LuaChunk>>> = HashMap::new();
    for in_filed_tree in need_analyzed_files {
        let file_id = in_filed_tree.file_id;
        if let Some(path) = db.get_vfs().get_file_path(&file_id).cloned() {
            let workspace_id = db
                .get_module_index_mut()
                .add_module_by_path(file_id, path.to_str().unwrap());
            let workspace_id = workspace_id.unwrap_or(WorkspaceId::MAIN);
            file_tree_map
                .entry(workspace_id)
                .or_default()
                .push(in_filed_tree);
        }
    }

    let mut contexts = Vec::new();
    for (workspace_id, tree_list) in file_tree_map {
        let mut context = AnalyzeContext::new(config.clone());
        context.tree_list = tree_list;
        contexts.push((workspace_id, context));
    }
    contexts
}

#[derive(Debug)]
pub struct AnalyzeContext {
    tree_list: Vec<InFiled<LuaChunk>>,
    config: Arc<Emmyrc>,
    unresolves: Vec<UnResolve>,
}

impl AnalyzeContext {
    pub fn new(emmyrc: Arc<Emmyrc>) -> Self {
        Self {
            tree_list: Vec::new(),
            config: emmyrc,
            unresolves: Vec::new(),
        }
    }

    pub fn add_tree_chunk(&mut self, tree: InFiled<LuaChunk>) {
        self.tree_list.push(tree);
    }

    pub fn add_unresolve(&mut self, un_resolve: UnResolve) {
        self.unresolves.push(un_resolve);
    }
}
