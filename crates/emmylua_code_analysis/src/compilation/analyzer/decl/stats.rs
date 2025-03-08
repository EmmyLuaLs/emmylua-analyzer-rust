use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaAstToken, LuaExpr, LuaForRangeStat, LuaForStat, LuaFuncStat,
    LuaIndexExpr, LuaLocalFuncStat, LuaLocalStat, LuaSyntaxId, LuaSyntaxKind, LuaVarExpr,
};

use crate::{
    db_index::{LocalAttribute, LuaDecl, LuaMember, LuaMemberKey, LuaMemberOwner},
    LuaDeclExtra, LuaPropertyOwnerId, LuaSignatureId, LuaType,
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
                decl_type: None,
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
                            decl_type: None,
                        },
                        value_expr_id,
                    );

                    analyzer.add_decl(decl);
                }
            }
            LuaVarExpr::IndexExpr(index_expr) => {
                let index_key = index_expr.get_index_key()?;
                let key: LuaMemberKey = index_key.into();
                if key.is_none() {
                    continue;
                }

                let file_id = analyzer.get_file_id();
                let member = LuaMember::new(
                    LuaMemberOwner::None,
                    key.clone(),
                    file_id,
                    index_expr.get_syntax_id(),
                    None,
                );

                analyzer.db.get_member_index_mut().add_member(member);
                if let LuaMemberKey::Name(name) = &key {
                    analyze_maybe_global_index_expr(
                        analyzer,
                        index_expr,
                        &name,
                        None,
                        value_expr_id,
                    );
                }
            }
        }
    }

    Some(())
}

fn analyze_maybe_global_index_expr(
    analyzer: &mut DeclAnalyzer,
    index_expr: &LuaIndexExpr,
    index_name: &str,
    typ: Option<LuaType>,
    value_expr_id: Option<LuaSyntaxId>,
) -> Option<()> {
    let file_id = analyzer.get_file_id();
    let prefix = index_expr.get_prefix_expr()?;
    if let LuaExpr::NameExpr(name_expr) = prefix {
        let name_token = name_expr.get_name_token()?;
        let name_token_text = name_token.get_name_text();
        if name_token_text == "_G" || name_token_text == "_ENV" {
            let position = index_expr.get_position();
            let name = name_token.get_name_text();
            let range = index_expr.get_range();
            if let Some(decl) = analyzer.find_decl(&name, position) {
                let decl_id = decl.get_id();
                analyzer
                    .db
                    .get_reference_index_mut()
                    .add_decl_reference(decl_id, file_id, range, true);
            } else {
                let decl = LuaDecl::new(
                    index_name,
                    file_id,
                    range,
                    LuaDeclExtra::Global {
                        kind: LuaSyntaxKind::IndexExpr.into(),
                        decl_type: typ,
                    },
                    value_expr_id,
                );

                analyzer.add_decl(decl);
            }
        }
    }

    Some(())
}

pub fn analyze_for_stat(analyzer: &mut DeclAnalyzer, stat: LuaForStat) -> Option<()> {
    let it_var = stat.get_var_name()?;
    let name = it_var.get_name_text();
    let pos = it_var.get_position();
    let file_id = analyzer.get_file_id();
    let range = it_var.get_range();
    if analyzer.find_decl(&name, pos).is_none() {
        let decl = LuaDecl::new(
            name,
            file_id,
            range,
            LuaDeclExtra::Local {
                kind: it_var.syntax().kind().into(),
                attrib: Some(LocalAttribute::IterConst),
                decl_type: Some(LuaType::Integer),
            },
            None,
        );

        analyzer.add_decl(decl);
    }

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
                decl_type: None,
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
                        decl_type: None,
                    },
                    None,
                );

                let decl_id = analyzer.add_decl(decl);
                LuaPropertyOwnerId::LuaDecl(decl_id)
            } else {
                return Some(());
            }
        }
        LuaVarExpr::IndexExpr(index_expr) => {
            let index_key = index_expr.get_index_key()?;
            let key: LuaMemberKey = index_key.into();
            if key.is_none() {
                return None;
            }

            let file_id = analyzer.get_file_id();
            let member = LuaMember::new(
                LuaMemberOwner::None,
                key.clone(),
                file_id,
                index_expr.get_syntax_id(),
                None,
            );

            let member_id = analyzer.db.get_member_index_mut().add_member(member);

            if let LuaMemberKey::Name(name) = &key {
                analyze_maybe_global_index_expr(analyzer, &index_expr, &name, None, None);
            }
            LuaPropertyOwnerId::Member(member_id)
        }
    };

    let closure = stat.get_closure()?;
    let file_id = analyzer.get_file_id();
    let closure_owner_id =
        LuaPropertyOwnerId::Signature(LuaSignatureId::from_closure(file_id, &closure));
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
            decl_type: None,
        },
        None,
    );

    let decl_id = analyzer.add_decl(decl);
    let closure = stat.get_closure()?;
    let closure_owner_id =
        LuaPropertyOwnerId::Signature(LuaSignatureId::from_closure(file_id, &closure));
    let property_decl_id = LuaPropertyOwnerId::LuaDecl(decl_id);
    analyzer
        .db
        .get_property_index_mut()
        .add_owner_map(property_decl_id, closure_owner_id, file_id);

    Some(())
}
