mod global_members;

use std::collections::HashSet;

use emmylua_parser::{LuaAst, LuaAstNode, LuaIndexKey, LuaSyntaxId, LuaTableExpr};

use crate::{
    compilation::analyzer::{
        member::global_members::{analyze_assign_stat, analyze_func_stat}, AnalysisPipeline, AnalyzeContext
    }, DbIndex, FileId, InFiled, LuaMember, LuaMemberFeature, LuaMemberId, LuaMemberKey, LuaMemberOwner, Profile
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
                    LuaAst::LuaAssignStat(assign_stat) => {
                        analyze_assign_stat(&mut analyzer, assign_stat);
                    }
                    LuaAst::LuaFuncStat(func_stat) => {
                        analyze_func_stat(&mut analyzer, func_stat);
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

fn analyze_table_expr(analyzer: &mut MemberAnalyzer, table_expr: LuaTableExpr) -> Option<()> {
    if analyzer.visited_table.contains(&table_expr.get_syntax_id()) {
        return None;
    }

    let in_filed = InFiled::new(analyzer.file_id, table_expr.get_range().clone());
    let owner_id = LuaMemberOwner::Element(in_filed);

    if table_expr.is_object() {
        for field in table_expr.get_fields() {
            if let Some(field_key) = field.get_field_key() {
                let member_id = LuaMemberId::new(field.get_syntax_id(), analyzer.file_id);
                add_field_member(analyzer, owner_id.clone(), field_key, member_id);
            }
        }
    }

    Some(())
}

fn add_field_member(
    analyzer: &mut MemberAnalyzer,
    member_owner: LuaMemberOwner,
    field_key: LuaIndexKey,
    member_id: LuaMemberId,
) -> Option<()> {
    let decl_feature = if analyzer.is_meta {
        LuaMemberFeature::MetaDefine
    } else {
        LuaMemberFeature::FileDefine
    };

    let key: LuaMemberKey = match field_key {
        LuaIndexKey::Name(name) => LuaMemberKey::Name(name.get_name_text().into()),
        LuaIndexKey::String(str) => LuaMemberKey::Name(str.get_value().into()),
        LuaIndexKey::Integer(i) => LuaMemberKey::Integer(i.get_int_value()),
        LuaIndexKey::Idx(idx) => LuaMemberKey::Integer(idx as i64),
        LuaIndexKey::Expr(_) => {
            // let unresolve_member = UnResolveTableField {
            //     file_id: analyzer.file_id,
            //     table_expr: table_expr.clone(),
            //     field: field.clone(),
            //     decl_feature,
            // };
            // analyzer.context.add_unresolve(
            //     unresolve_member.into(),
            //     InferFailReason::UnResolveExpr(InFiled::new(
            //         analyzer.get_file_id(),
            //         field_expr.clone(),
            //     )),
            // );
            return None;
        }
    };

    let member = LuaMember::new(member_id, key.clone(), decl_feature);
    analyzer
        .db
        .get_member_index_mut()
        .add_member(member_owner, member);

    Some(())
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
