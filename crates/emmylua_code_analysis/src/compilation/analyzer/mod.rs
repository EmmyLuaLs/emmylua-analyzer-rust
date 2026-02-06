mod common;
mod decl;
mod doc;
mod flow;
mod infer_cache_manager;
mod lua;
mod unresolve;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{
    Emmyrc, FileId, InFiled, InferFailReason, WorkspaceId, db_index::DbIndex, profile::Profile,
};
use emmylua_parser::LuaChunk;
use infer_cache_manager::InferCacheManager;
use unresolve::UnResolve;

pub fn analyze(db: &mut DbIndex, need_analyzed_files: Vec<InFiled<LuaChunk>>, config: Arc<Emmyrc>) {
    if need_analyzed_files.is_empty() {
        return;
    }

    let contexts = module_analyze(db, need_analyzed_files, config);

    for (workspace_id, mut context) in contexts {
        context.workspace_id = Some(workspace_id);
        let profile_log = format!("analyze workspace {}", workspace_id);
        let _p = Profile::cond_new(&profile_log, context.tree_list.len() > 1);
        run_analysis::<decl::DeclAnalysisPipeline>(db, &mut context);
        run_analysis::<doc::DocAnalysisPipeline>(db, &mut context);
        run_analysis::<flow::FlowAnalysisPipeline>(db, &mut context);
        run_analysis::<lua::LuaAnalysisPipeline>(db, &mut context);
        run_analysis::<unresolve::UnResolveAnalysisPipeline>(db, &mut context);
    }
}

trait AnalysisPipeline {
    fn analyze(db: &mut DbIndex, context: &mut AnalyzeContext);
}

fn run_analysis<T: AnalysisPipeline>(db: &mut DbIndex, context: &mut AnalyzeContext) {
    T::analyze(db, context);
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
            let path_str = match path.to_str() {
                Some(path) => path,
                None => {
                    log::warn!("file_id {:?} path not found", file_id);
                    return vec![];
                }
            };

            let workspace_id = db
                .get_module_index_mut()
                .add_module_by_path(file_id, path_str);
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
            let path_str = match path.to_str() {
                Some(path) => path,
                None => {
                    log::warn!("file_id {:?} path not found", file_id);
                    continue;
                }
            };

            let workspace_id = db
                .get_module_index_mut()
                .add_module_by_path(file_id, path_str);
            let workspace_id = workspace_id.unwrap_or(WorkspaceId::MAIN);
            file_tree_map
                .entry(workspace_id)
                .or_default()
                .push(in_filed_tree);
        }
    }

    let mut contexts = Vec::new();
    if let Some(std_lib) = file_tree_map.remove(&WorkspaceId::STD) {
        let mut context = AnalyzeContext::new(config.clone());
        context.tree_list = std_lib;
        contexts.push((WorkspaceId::STD, context));
    }

    let mut main_vec = Vec::new();
    for (workspace_id, tree_list) in file_tree_map {
        let mut context = AnalyzeContext::new(config.clone());
        context.tree_list = tree_list;
        if workspace_id.is_library() {
            contexts.push((workspace_id, context));
        } else {
            main_vec.push((workspace_id, context));
        }
    }

    contexts.sort_by(|a, b| a.0.cmp(&b.0));

    contexts.extend(main_vec);
    contexts
}

#[derive(Debug)]
pub struct AnalyzeContext {
    tree_list: Vec<InFiled<LuaChunk>>,
    #[allow(unused)]
    config: Arc<Emmyrc>,
    metas: HashSet<FileId>,
    unresolves: Vec<(UnResolve, InferFailReason)>,
    infer_manager: InferCacheManager,
    pub workspace_id: Option<WorkspaceId>,
}

impl AnalyzeContext {
    pub fn new(emmyrc: Arc<Emmyrc>) -> Self {
        Self {
            tree_list: Vec::new(),
            config: emmyrc,
            metas: HashSet::new(),
            unresolves: Vec::new(),
            infer_manager: InferCacheManager::new(),
            workspace_id: None,
        }
    }

    pub fn add_meta(&mut self, file_id: FileId) {
        self.metas.insert(file_id);
    }

    pub fn add_tree_chunk(&mut self, tree: InFiled<LuaChunk>) {
        self.tree_list.push(tree);
    }

    pub fn add_unresolve(&mut self, un_resolve: UnResolve, reason: InferFailReason) {
        self.unresolves.push((un_resolve, reason));
    }
}
