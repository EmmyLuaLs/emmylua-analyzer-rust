use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaAstToken, LuaExpr, LuaForRangeStat, LuaForStat, LuaFuncStat,
    LuaIndexExpr, LuaIndexKey, LuaLocalFuncStat, LuaLocalStat, LuaSyntaxId, LuaSyntaxKind,
    LuaVarExpr,
};

use crate::{
    DbIndex, GlobalId, LuaDeclExtra, LuaDeclId, LuaMember, LuaMemberFeature, LuaMemberId,
    LuaMemberKey, LuaMemberOwner, LuaSemanticDeclId, LuaSignatureId, LuaType, LuaTypeCache,
    compilation::analyzer::{
        common::bind_type,
        decl::global_path::{get_global_path, to_path_name},
    },
    db_index::{LocalAttribute, LuaDecl},
};

use super::DeclAnalyzer;

pub fn analyze_local_stat(analyzer: &mut DeclAnalyzer, stat: LuaLocalStat) -> Option<()> {
    let local_name_list = stat.get_local_name_list().collect::<Vec<_>>();
    let value_expr_list = stat.get_value_exprs().collect::<Vec<_>>();

    for (index, local_name) in local_name_list.iter().enumerate() {
        let name = if let Some(name_token) = local_name.get_name_token() {
            name_token.get_name_text().to_string()
        } else {
            continue;
        };
        let attrib = if let Some(attrib) = local_name.get_attrib() {
            if attrib.is_const() {
                Some(LocalAttribute::Const)
            } else if attrib.is_close() {
                Some(LocalAttribute::Close)
            } else {
                None
            }
        } else {
            None
        };

        let file_id = analyzer.get_file_id();
        let range = local_name.get_range();
        let expr_id = if let Some(expr) = value_expr_list.get(index) {
            Some(expr.get_syntax_id())
        } else {
            None
        };

        let decl = LuaDecl::new(
            &name,
            file_id,
            range,
            LuaDeclExtra::Local {
                kind: local_name.syntax().kind().into(),
                attrib,
            },
            expr_id,
        );
        analyzer.add_decl(decl);
    }

    Some(())
}

pub fn analyze_assign_stat(analyzer: &mut DeclAnalyzer, stat: LuaAssignStat) -> Option<()> {
    let (vars, value_exprs) = stat.get_var_and_expr_list();
    for (idx, var) in vars.iter().enumerate() {
        let value_expr_id = if let Some(expr) = value_exprs.get(idx) {
            Some(expr.get_syntax_id())
        } else {
            None
        };

        match &var {
            LuaVarExpr::NameExpr(name) => {
                let name_token = name.get_name_token()?;
                let position = name_token.get_position();
                let name = name_token.get_name_text();
                let file_id = analyzer.get_file_id();
                let range = name_token.get_range();
                if let Some(decl) = analyzer.find_decl(&name, position) {
                    let decl_id = decl.get_id();
                    analyzer
                        .db
                        .get_reference_index_mut()
                        .add_decl_reference(decl_id, file_id, range, true);
                } else {
                    let decl = LuaDecl::new(
                        name,
                        file_id,
                        range,
                        LuaDeclExtra::Global {
                            kind: LuaSyntaxKind::NameExpr.into(),
                        },
                        value_expr_id,
                    );

                    analyzer.add_decl(decl);
                }
            }
            LuaVarExpr::IndexExpr(index_expr) => {
                if analyze_maybe_global_index_expr(analyzer, index_expr, value_expr_id).is_some() {
                    continue;
                }

                let mut added_global_field = false;
                if let Some(prefix_expr) = index_expr.get_prefix_expr() {
                    if let Some(global_id) = get_global_path(analyzer, prefix_expr) {
                        if let Some(field_key) = index_expr.get_index_key() {
                            let member_id = LuaMemberId::new(
                                index_expr.get_syntax_id(),
                                analyzer.get_file_id(),
                            );
                            let base = global_id.get_name();

                            if let Some(current_name) = to_path_name(index_expr) {
                                let field_global_name = format!("{}.{}", base, current_name);
                                analyzer.db.get_member_index_mut().add_member_global_id(
                                    member_id,
                                    GlobalId::new(&field_global_name),
                                );
                            }

                            let owner_id = LuaMemberOwner::GlobalId(global_id);
                            add_field_member(
                                analyzer.db,
                                analyzer.is_meta,
                                owner_id,
                                field_key,
                                member_id,
                            );
                            added_global_field = true;
                        }
                    }
                }

                if !added_global_field {
                    let member_id =
                        LuaMemberId::new(index_expr.get_syntax_id(), analyzer.get_file_id());
                    let owner_id = LuaMemberOwner::UnResolve;
                    if let Some(field_key) = index_expr.get_index_key() {
                        add_field_member(
                            analyzer.db,
                            analyzer.is_meta,
                            owner_id,
                            field_key,
                            member_id,
                        );
                    }
                }
            }
        }
    }

    Some(())
}

fn analyze_maybe_global_index_expr(
    analyzer: &mut DeclAnalyzer,
    index_expr: &LuaIndexExpr,
    value_expr_id: Option<LuaSyntaxId>,
) -> Option<LuaDeclId> {
    let file_id = analyzer.get_file_id();
    let prefix = index_expr.get_prefix_expr()?;
    if let LuaExpr::NameExpr(name_expr) = prefix {
        let prefix_name_token = name_expr.get_name_token()?;
        let prefix_name_token_text = prefix_name_token.get_name_text();
        if prefix_name_token_text == "_G" || prefix_name_token_text == "_ENV" {
            let position = index_expr.get_position();
            let index_key = index_expr.get_index_key()?;
            let index_name = match index_key {
                LuaIndexKey::Name(name) => name.get_name_text().to_string(),
                LuaIndexKey::String(str) => str.get_value(),
                _ => {
                    return None;
                }
            };
            let range = index_expr.get_range();
            if let Some(decl) = analyzer.find_decl(&index_name, position) {
                let decl_id = decl.get_id();
                analyzer
                    .db
                    .get_reference_index_mut()
                    .add_decl_reference(decl_id, file_id, range, true);
            } else {
                let decl = LuaDecl::new(
                    &index_name,
                    file_id,
                    range,
                    LuaDeclExtra::Global {
                        kind: LuaSyntaxKind::IndexExpr.into(),
                    },
                    value_expr_id,
                );
                let decl_id = decl.get_id();
                analyzer.add_decl(decl);
                return Some(decl_id);
            }
        }
    }

    None
}

pub fn analyze_for_stat(analyzer: &mut DeclAnalyzer, stat: LuaForStat) -> Option<()> {
    let it_var = stat.get_var_name()?;
    let name = it_var.get_name_text();
    let file_id = analyzer.get_file_id();
    let range = it_var.get_range();
    let decl = LuaDecl::new(
        name,
        file_id,
        range,
        LuaDeclExtra::Local {
            kind: it_var.syntax().kind().into(),
            attrib: Some(LocalAttribute::IterConst),
        },
        None,
    );
    let decl_id = decl.get_id();
    analyzer.add_decl(decl);
    bind_type(
        analyzer.db,
        decl_id.into(),
        LuaTypeCache::DocType(LuaType::Integer),
    );

    Some(())
}

pub fn analyze_for_range_stat(analyzer: &mut DeclAnalyzer, stat: LuaForRangeStat) {
    let var_list = stat.get_var_name_list();
    let file_id = analyzer.get_file_id();
    for var in var_list {
        let name = var.get_name_text();
        let range = var.get_range();

        let decl = LuaDecl::new(
            name,
            file_id,
            range,
            LuaDeclExtra::Local {
                kind: var.syntax().kind().into(),
                attrib: Some(LocalAttribute::IterConst),
            },
            None,
        );

        analyzer.add_decl(decl);
    }
}

pub fn analyze_func_stat(analyzer: &mut DeclAnalyzer, stat: LuaFuncStat) -> Option<()> {
    let func_name = stat.get_func_name()?;
    let file_id = analyzer.get_file_id();
    let property_owner_id = match func_name {
        LuaVarExpr::NameExpr(name_expr) => {
            let name_token = name_expr.get_name_token()?;
            let position = name_token.get_position();
            let name = name_token.get_name_text();
            let range = name_token.get_range();
            if analyzer.find_decl(&name, position).is_none() {
                let decl = LuaDecl::new(
                    name,
                    file_id,
                    range,
                    LuaDeclExtra::Global {
                        kind: LuaSyntaxKind::NameExpr.into(),
                    },
                    None,
                );

                let decl_id = analyzer.add_decl(decl);
                LuaSemanticDeclId::LuaDecl(decl_id)
            } else {
                return Some(());
            }
        }
        LuaVarExpr::IndexExpr(index_expr) => {
            let file_id = analyzer.get_file_id();

            if let Some(decl_id) = analyze_maybe_global_index_expr(analyzer, &index_expr, None) {
                LuaSemanticDeclId::LuaDecl(decl_id)
            } else {
                let member_id = LuaMemberId::new(index_expr.get_syntax_id(), file_id);
                let mut added_global_field = false;
                if let Some(prefix_name_expr) = index_expr.get_prefix_expr() {
                    if let Some(global_id) = get_global_path(&analyzer, prefix_name_expr.clone()) {
                        if let Some(field_key) = index_expr.get_index_key() {
                            let member_id = LuaMemberId::new(index_expr.get_syntax_id(), file_id);
                            let owner = LuaMemberOwner::GlobalId(global_id);
                            add_field_member(
                                analyzer.db,
                                analyzer.is_meta,
                                owner.clone(),
                                field_key,
                                member_id,
                            );
                            added_global_field = true;
                        }
                    }
                }

                if !added_global_field {
                    let owner = LuaMemberOwner::UnResolve;
                    let member_id = LuaMemberId::new(index_expr.get_syntax_id(), file_id);
                    if let Some(field_key) = index_expr.get_index_key() {
                        add_field_member(
                            analyzer.db,
                            analyzer.is_meta,
                            owner,
                            field_key,
                            member_id,
                        );
                    }
                }

                LuaSemanticDeclId::Member(member_id)
            }
        }
    };

    let closure = stat.get_closure()?;
    let file_id = analyzer.get_file_id();
    let closure_owner_id =
        LuaSemanticDeclId::Signature(LuaSignatureId::from_closure(file_id, &closure));
    analyzer.db.get_property_index_mut().add_owner_map(
        property_owner_id,
        closure_owner_id,
        file_id,
    );

    Some(())
}

pub fn analyze_local_func_stat(analyzer: &mut DeclAnalyzer, stat: LuaLocalFuncStat) -> Option<()> {
    let local_name = stat.get_local_name()?;
    let name_token = local_name.get_name_token()?;
    let name = name_token.get_name_text();
    let range = local_name.get_range();
    let file_id = analyzer.get_file_id();
    let decl = LuaDecl::new(
        name,
        file_id,
        range,
        LuaDeclExtra::Local {
            kind: local_name.syntax().kind().into(),
            attrib: None,
        },
        None,
    );

    let decl_id = analyzer.add_decl(decl);
    let closure = stat.get_closure()?;
    let closure_owner_id =
        LuaSemanticDeclId::Signature(LuaSignatureId::from_closure(file_id, &closure));
    let semantic_decl_id = LuaSemanticDeclId::LuaDecl(decl_id);
    analyzer
        .db
        .get_property_index_mut()
        .add_owner_map(semantic_decl_id, closure_owner_id, file_id);

    Some(())
}

pub fn add_field_member(
    db: &mut DbIndex,
    is_meta: bool,
    member_owner: LuaMemberOwner,
    field_key: LuaIndexKey,
    member_id: LuaMemberId,
) -> Option<()> {
    let decl_feature = if is_meta {
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
    db.get_member_index_mut().add_member(member_owner, member);

    Some(())
}
