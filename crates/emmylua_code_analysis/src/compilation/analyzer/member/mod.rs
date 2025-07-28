use crate::{
    DbIndex, Profile,
    compilation::analyzer::{AnalysisPipeline, AnalyzeContext},
};

pub struct MemberAnalysisPipeline;

impl AnalysisPipeline for MemberAnalysisPipeline {
    fn analyze(_: &mut DbIndex, context: &mut AnalyzeContext) {
        let _p = Profile::cond_new("member analyze", context.tree_list.len() > 1);
    }
}
