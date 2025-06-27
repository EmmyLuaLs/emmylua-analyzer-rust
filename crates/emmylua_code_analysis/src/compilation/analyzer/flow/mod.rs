mod bind_analyze;
mod binder;

use crate::{
    compilation::analyzer::flow::{bind_analyze::bind_analyze, binder::FlowBinder},
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
        let file_id = in_filed_tree.file_id;
        let mut binder = FlowBinder::new(db, file_id);
        bind_analyze(&mut binder, chunk);
        let flow_tree = binder.finish();
        db.get_flow_index_mut().add_flow_tree(file_id, flow_tree);
    }
}
