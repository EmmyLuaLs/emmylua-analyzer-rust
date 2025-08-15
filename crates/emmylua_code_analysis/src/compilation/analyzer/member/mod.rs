mod members;

use std::collections::HashSet;

use emmylua_parser::{LuaAst, LuaAstNode, LuaSyntaxId};

use crate::{
    DbIndex, FileId, Profile,
    compilation::analyzer::{
        AnalysisPipeline, AnalyzeContext,
        member::members::{
            analyze_assign_stat, analyze_func_stat, analyze_local_stat, analyze_table_expr,
            analyze_table_field,
        },
    },
};

/// Due to the widespread use of global variables in Lua and the various ways to define members,
/// it is impossible to fully analyze them without knowing their types.
/// Therefore, this only tries to identify as many members as possible in advance.
pub struct MemberAnalysisPipeline;

impl AnalysisPipeline for MemberAnalysisPipeline {
    fn analyze(db: &mut DbIndex, context: &mut AnalyzeContext) {
        let _p = Profile::cond_new("member analyze", context.tree_list.len() > 1);
        let tree_list = context.tree_list.clone();
        for in_filed_tree in tree_list {
            let root = &in_filed_tree.value;
            let mut analyzer = MemberAnalyzer::new(db, in_filed_tree.file_id, context);
            for node in root.descendants::<LuaAst>() {
                match node {
                    LuaAst::LuaLocalStat(local_stat) => {
                        analyze_local_stat(&mut analyzer, local_stat);
                    }
                    LuaAst::LuaAssignStat(assign_stat) => {
                        analyze_assign_stat(&mut analyzer, assign_stat);
                    }
                    LuaAst::LuaFuncStat(func_stat) => {
                        analyze_func_stat(&mut analyzer, func_stat);
                    }
                    LuaAst::LuaTableField(table_field) => {
                        analyze_table_field(&mut analyzer, table_field);
                    }
                    LuaAst::LuaTableExpr(table_expr) => {
                        analyze_table_expr(&mut analyzer, table_expr);
                    }
                    _ => {}
                }
            }
        }
    }
}

struct MemberAnalyzer<'a> {
    db: &'a mut DbIndex,
    file_id: FileId,
    #[allow(unused)]
    context: &'a mut AnalyzeContext,
    is_meta: bool,
    visited_table: HashSet<LuaSyntaxId>,
}

impl<'a> MemberAnalyzer<'a> {
    fn new(db: &'a mut DbIndex, file_id: FileId, context: &'a mut AnalyzeContext) -> Self {
        let is_meta = db
            .get_module_index()
            .get_module(file_id)
            .map_or(false, |module| module.is_meta);
        Self {
            db,
            file_id,
            context,
            is_meta,
            visited_table: HashSet::new(),
        }
    }
}
