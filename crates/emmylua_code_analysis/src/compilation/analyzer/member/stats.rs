use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaExpr, LuaFuncStat, LuaIndexKey, LuaLocalStat, LuaVarExpr
};

use crate::{
    LuaDeclId, LuaMember, LuaMemberFeature, LuaMemberId, LuaMemberKey, LuaMemberOwner,
    compilation::analyzer::{common::get_name_expr_member_owner, member::MemberAnalyzer},
};

pub fn analyze_local_stat(analyzer: &mut MemberAnalyzer, local_stat: LuaLocalStat) -> Option<()> {
    let local_name_list = local_stat.get_local_name_list().collect::<Vec<_>>();
    let local_expr_list = local_stat.get_value_exprs().collect::<Vec<_>>();
    for i in 0..local_expr_list.len() {
        let local_name = local_name_list.get(i)?;
        let local_expr = local_expr_list.get(i)?;
        if let LuaExpr::TableExpr(table_expr) = local_expr {
            let decl_id = LuaDeclId::new(analyzer.file_id, local_name.get_position());
            let owner = LuaMemberOwner::DeclId(decl_id);
            if table_expr.is_object() {
                for table_field in table_expr.get_fields() {
                    if let Some(field_key) = table_field.get_field_key() {
                        let file_id = analyzer.file_id;
                        let member_id = LuaMemberId::new(table_field.get_syntax_id(), file_id);
                        add_field_member(analyzer, owner.clone(), field_key, member_id);
                    }
                }
            }
        }
    }

    Some(())
}

pub fn analyze_assign_stat(
    analyzer: &mut MemberAnalyzer,
    assign_stat: LuaAssignStat,
) -> Option<()> {
    let (vars, exprs) = assign_stat.get_var_and_expr_list();
    for i in 0..exprs.len() {
        let var = vars.get(i)?;
        let expr = exprs.get(i)?;
        match (var, expr) {
            (LuaVarExpr::IndexExpr(index_expr), _) => {
                if let Some(LuaExpr::NameExpr(prefix_name_expr)) = index_expr.get_prefix_expr() {
                    if let Some(owner) = get_name_expr_member_owner(
                        &analyzer.db,
                        analyzer.file_id,
                        &prefix_name_expr,
                    ) {
                        if let Some(field_key) = index_expr.get_index_key() {
                            let member_id =
                                LuaMemberId::new(index_expr.get_syntax_id(), analyzer.file_id);
                            add_field_member(analyzer, owner.clone(), field_key, member_id);
                        }
                    }
                }
            }
            (LuaVarExpr::NameExpr(name_expr), LuaExpr::TableExpr(table_expr)) => {
                if let Some(owner) =
                    get_name_expr_member_owner(&analyzer.db, analyzer.file_id, &name_expr)
                {
                    if table_expr.is_object() {
                        for table_field in table_expr.get_fields() {
                            if let Some(field_key) = table_field.get_field_key() {
                                let file_id = analyzer.file_id;
                                let member_id =
                                    LuaMemberId::new(table_field.get_syntax_id(), file_id);
                                add_field_member(
                                    analyzer,
                                    owner.clone(),
                                    field_key,
                                    member_id,
                                );
                            }
                        }
                    }
                }
            }
            _ => continue,
        }
    }

    Some(())
}

pub fn analyze_func_stat(
    analyzer: &mut MemberAnalyzer,
    func_stat: LuaFuncStat,
) -> Option<()> {
    let func_name_expr = func_stat.get_func_name()?;
    if let LuaVarExpr::IndexExpr(index_expr) = func_name_expr {
        if let Some(LuaExpr::NameExpr(prefix_name_expr)) = index_expr.get_prefix_expr() {
            if let Some(owner) = get_name_expr_member_owner(
                &analyzer.db,
                analyzer.file_id,
                &prefix_name_expr,
            ) {
                if let Some(field_key) = index_expr.get_index_key() {
                    let member_id =
                        LuaMemberId::new(index_expr.get_syntax_id(), analyzer.file_id);
                    add_field_member(analyzer, owner.clone(), field_key, member_id);
                }
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
            //     file_id: analyzer.get_file_id(),
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

    let member = LuaMember::new(member_id, key, decl_feature);
    analyzer
        .db
        .get_member_index_mut()
        .add_member(member_owner, member);

    Some(())
}
