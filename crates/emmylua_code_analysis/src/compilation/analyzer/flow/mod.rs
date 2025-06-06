mod bind_file;
mod binder;
mod flow_node;

use crate::{
    compilation::analyzer::flow::{bind_file::bind_file, binder::FlowBinder},
    db_index::DbIndex,
    profile::Profile,
};

use super::AnalyzeContext;

pub(crate) fn analyze(db: &mut DbIndex, context: &mut AnalyzeContext) {
    let _p = Profile::cond_new("flow analyze", context.tree_list.len() > 1);
    let tree_list = context.tree_list.clone();
    // build decl and ref flow chain
    for in_filed_tree in &tree_list {
        let chunk = in_filed_tree.value.clone();
        // let file_id = in_filed_tree.file_id;
        let mut binder = FlowBinder::new();
        bind_file(&mut binder, chunk);
    }
}
