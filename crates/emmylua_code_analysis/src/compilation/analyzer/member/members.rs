use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaExpr, LuaFuncStat, LuaIndexKey, LuaLocalStat, LuaTableExpr,
    LuaTableField, LuaVarExpr,
};

use crate::{
    InFiled, LuaDeclId, LuaMember, LuaMemberFeature, LuaMemberId, LuaMemberKey, LuaMemberOwner,
    LuaType, LuaTypeCache,
    compilation::analyzer::{
        common::{bind_type, get_decl_member_owner, get_global_path, get_name_expr_member_owner},
        member::MemberAnalyzer,
    },
};

pub fn analyze_local_stat(analyzer: &mut MemberAnalyzer, local_stat: LuaLocalStat) -> Option<()> {
    let local_name_list = local_stat.get_local_name_list().collect::<Vec<_>>();
    let local_expr_list = local_stat.get_value_exprs().collect::<Vec<_>>();
    for i in 0..local_expr_list.len() {
        let local_name = local_name_list.get(i)?;
        let local_expr = local_expr_list.get(i)?;
        if let LuaExpr::TableExpr(table_expr) = local_expr {
            let decl_id = LuaDeclId::new(analyzer.file_id, local_name.get_position());
            let owner = match get_decl_member_owner(analyzer.db, &decl_id) {
                Some(owner) => owner,
                None => {
                    let table_range = InFiled::new(analyzer.file_id, table_expr.get_range());
                    bind_type(
                        analyzer.db,
                        decl_id.into(),
                        LuaTypeCache::InferType(LuaType::TableConst(table_range.clone()).into()),
                    );
                    LuaMemberOwner::Element(table_range)
                }
            };

            analyzer.visited_table.insert(table_expr.get_syntax_id());

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
                if let Some(prefix_expr) = index_expr.get_prefix_expr() {
                    match prefix_expr {
                        LuaExpr::NameExpr(prefix_name_expr) => {
                            if let Some(owner) = get_name_expr_member_owner(
                                &analyzer.db,
                                analyzer.file_id,
                                &prefix_name_expr,
                            ) {
                                if let Some(field_key) = index_expr.get_index_key() {
                                    let member_id = LuaMemberId::new(
                                        index_expr.get_syntax_id(),
                                        analyzer.file_id,
                                    );
                                    add_field_member(analyzer, owner.clone(), field_key, member_id);
                                }
                            }
                        }
                        LuaExpr::IndexExpr(prefix_index_expr) => {
                            if let Some(global_id) =
                                get_global_path(&analyzer.db, analyzer.file_id, &prefix_index_expr)
                            {
                                if let Some(field_key) = index_expr.get_index_key() {
                                    let member_id = LuaMemberId::new(
                                        index_expr.get_syntax_id(),
                                        analyzer.file_id,
                                    );
                                    let owner = LuaMemberOwner::GlobalId(global_id);
                                    add_field_member(analyzer, owner, field_key, member_id);
                                    if let Some(current_global_id) =
                                        get_global_path(&analyzer.db, analyzer.file_id, &index_expr)
                                    {
                                        analyzer
                                            .db
                                            .get_member_index_mut()
                                            .add_member_global_id(member_id, current_global_id);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            (LuaVarExpr::NameExpr(name_expr), LuaExpr::TableExpr(table_expr)) => {
                if let Some(owner) =
                    get_name_expr_member_owner(&analyzer.db, analyzer.file_id, &name_expr)
                {
                    analyzer.visited_table.insert(table_expr.get_syntax_id());
                    if table_expr.is_object() {
                        for table_field in table_expr.get_fields() {
                            if let Some(field_key) = table_field.get_field_key() {
                                let file_id = analyzer.file_id;
                                let member_id =
                                    LuaMemberId::new(table_field.get_syntax_id(), file_id);
                                add_field_member(analyzer, owner.clone(), field_key, member_id);
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

pub fn analyze_func_stat(analyzer: &mut MemberAnalyzer, func_stat: LuaFuncStat) -> Option<()> {
    let func_name_expr = func_stat.get_func_name()?;
    if let LuaVarExpr::IndexExpr(index_expr) = func_name_expr {
        if let Some(LuaExpr::NameExpr(prefix_name_expr)) = index_expr.get_prefix_expr() {
            if let Some(owner) =
                get_name_expr_member_owner(&analyzer.db, analyzer.file_id, &prefix_name_expr)
            {
                if let Some(field_key) = index_expr.get_index_key() {
                    let member_id = LuaMemberId::new(index_expr.get_syntax_id(), analyzer.file_id);
                    add_field_member(analyzer, owner.clone(), field_key, member_id);
                }
            }
        }
    }

    Some(())
}

pub fn analyze_table_field(
    analyzer: &mut MemberAnalyzer,
    table_field: LuaTableField,
) -> Option<()> {
    if !table_field.is_assign_field() {
        return None;
    }

    let value_expr = table_field.get_value_expr()?;
    let LuaExpr::TableExpr(table_value) = value_expr else {
        return None;
    };

    let member_id = LuaMemberId::new(table_field.get_syntax_id(), analyzer.file_id);
    let doc_type = analyzer
        .db
        .get_type_index()
        .get_type_cache(&member_id.into())?;
    let owner = match doc_type.as_type() {
        LuaType::Def(type_id) => LuaMemberOwner::Type(type_id.clone()),
        _ => return None,
    };

    analyzer.visited_table.insert(table_value.get_syntax_id());

    for field in table_value.get_fields() {
        if let Some(field_key) = field.get_field_key() {
            add_field_member(analyzer, owner.clone(), field_key, member_id);
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

    analyzer.db.get_reference_index_mut().add_index_reference(
        key,
        member_id.file_id,
        *member_id.get_syntax_id(),
    );

    Some(())
}

pub fn analyze_table_expr(analyzer: &mut MemberAnalyzer, table_expr: LuaTableExpr) -> Option<()> {
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
