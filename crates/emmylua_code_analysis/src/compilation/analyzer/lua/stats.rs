use emmylua_parser::{
    BinaryOperator, LuaAssignStat, LuaAstNode, LuaExpr, LuaFuncStat, LuaIndexExpr, LuaIndexKey,
    LuaLocalFuncStat, LuaLocalStat, LuaNameExpr, LuaTableExpr, LuaTableField, LuaVarExpr,
    PathTrait,
};

use crate::{
    InFiled, InferFailReason, LuaBuiltinAttributeKind, LuaLspOptimizationCode, LuaMemberKey,
    LuaSemanticDeclId, LuaTypeCache, LuaTypeOwner,
    compilation::analyzer::{
        common::{add_member, bind_type},
        unresolve::{UnResolveDecl, UnResolveMember},
    },
    db_index::{LuaDeclId, LuaMember, LuaMemberFeature, LuaMemberId, LuaMemberOwner, LuaType},
};

use super::LuaAnalyzer;

pub fn analyze_local_stat(analyzer: &mut LuaAnalyzer, local_stat: LuaLocalStat) -> Option<()> {
    let name_list: Vec<_> = local_stat.get_local_name_list().collect();
    let expr_list: Vec<_> = local_stat.get_value_exprs().collect();
    let name_count = name_list.len();
    let expr_count = expr_list.len();
    if expr_count == 0 {
        for local_name in name_list {
            let position = local_name.get_position();
            let decl_id = LuaDeclId::new(analyzer.file_id, position);
            if has_delayed_definition_attribute(analyzer, decl_id) {
                return Some(());
            }
            analyzer
                .db
                .get_type_index_mut()
                .bind_type(decl_id.into(), LuaTypeCache::InferType(LuaType::Nil));
        }

        return Some(());
    }

    for i in 0..name_count {
        let name = name_list.get(i)?;
        let position = name.get_position();
        let expr = if let Some(expr) = expr_list.get(i) {
            expr.clone()
        } else {
            break;
        };

        match analyzer.infer_expr(&expr) {
            Ok(expr_type) => {
                let expr_type = expr_type.get_result_slot_type(0).unwrap_or(expr_type);
                let decl_id = LuaDeclId::new(analyzer.file_id, position);
                if let LuaType::Instance(instance) = &expr_type
                    && instance.get_base().is_unknown()
                    && call_expr_has_effect_table_arg(&expr).is_some()
                {
                    let unresolve = UnResolveDecl {
                        file_id: analyzer.file_id,
                        decl_id,
                        expr: expr.clone(),
                        ret_idx: 0,
                    };
                    analyzer.context.add_unresolve(
                        unresolve.into(),
                        InferFailReason::UnResolveExpr(InFiled::new(
                            analyzer.file_id,
                            expr.clone(),
                        )),
                    );
                    continue;
                }

                bind_type(
                    analyzer.db,
                    decl_id.into(),
                    LuaTypeCache::InferType(expr_type),
                );
            }
            Err(InferFailReason::None) => {
                let decl_id = LuaDeclId::new(analyzer.file_id, position);
                analyzer
                    .db
                    .get_type_index_mut()
                    .bind_type(decl_id.into(), LuaTypeCache::InferType(LuaType::Nil));
            }
            Err(reason) => {
                let decl_id = LuaDeclId::new(analyzer.file_id, position);
                let unresolve = UnResolveDecl {
                    file_id: analyzer.file_id,
                    decl_id,
                    expr: expr.clone(),
                    ret_idx: 0,
                };

                analyzer.context.add_unresolve(unresolve.into(), reason);
            }
        }
    }

    if name_count > expr_count {
        let last_expr = expr_list.last();
        if let Some(last_expr) = last_expr {
            match analyzer.infer_expr(last_expr) {
                Ok(last_expr_type) => {
                    if last_expr_type.contain_multi_return() {
                        for i in expr_count..name_count {
                            let name = name_list.get(i)?;
                            let position = name.get_position();
                            let decl_id = LuaDeclId::new(analyzer.file_id, position);
                            let ret_type = last_expr_type.get_result_slot_type(i - expr_count + 1);
                            if let Some(ret_type) = ret_type {
                                bind_type(
                                    analyzer.db,
                                    decl_id.into(),
                                    LuaTypeCache::InferType(ret_type.clone()),
                                );
                            } else {
                                analyzer.db.get_type_index_mut().bind_type(
                                    decl_id.into(),
                                    LuaTypeCache::InferType(LuaType::Unknown),
                                );
                            }
                        }
                        return Some(());
                    }
                }
                Err(reason) => {
                    for i in expr_count..name_count {
                        let name = name_list.get(i)?;
                        let position = name.get_position();
                        let decl_id = LuaDeclId::new(analyzer.file_id, position);
                        let unresolve = UnResolveDecl {
                            file_id: analyzer.file_id,
                            decl_id,
                            expr: last_expr.clone(),
                            ret_idx: i - expr_count + 1,
                        };

                        analyzer
                            .context
                            .add_unresolve(unresolve.into(), reason.clone());
                    }
                }
            }
        } else {
            for i in expr_count..name_count {
                let name = name_list.get(i)?;
                let position = name.get_position();
                let decl_id = LuaDeclId::new(analyzer.file_id, position);
                analyzer
                    .db
                    .get_type_index_mut()
                    .bind_type(decl_id.into(), LuaTypeCache::InferType(LuaType::Nil));
            }
        }
    }

    Some(())
}

fn call_expr_has_effect_table_arg(expr: &LuaExpr) -> Option<()> {
    if let LuaExpr::CallExpr(call_expr) = expr {
        let args_list = call_expr.get_args_list()?;
        for arg in args_list.get_args() {
            if let LuaExpr::TableExpr(table_expr) = arg
                && !table_expr.is_empty()
            {
                return Some(());
            }
        }
    }
    None
}

fn get_var_owner(analyzer: &mut LuaAnalyzer, var: LuaVarExpr) -> LuaTypeOwner {
    let file_id = analyzer.file_id;
    match var {
        LuaVarExpr::NameExpr(var_name) => {
            let position = var_name.get_position();
            let decl_id = LuaDeclId::new(file_id, position);
            LuaTypeOwner::Decl(decl_id)
        }
        LuaVarExpr::IndexExpr(index_expr) => {
            let maybe_decl_id = LuaDeclId::new(file_id, index_expr.get_position());
            if analyzer
                .db
                .get_decl_index()
                .get_decl(&maybe_decl_id)
                .is_some()
            {
                return LuaTypeOwner::Decl(maybe_decl_id);
            }

            let member_id = LuaMemberId::new(index_expr.get_syntax_id(), file_id);
            LuaTypeOwner::Member(member_id)
        }
    }
}

fn is_implicit_self_prefix(analyzer: &LuaAnalyzer, prefix_expr: &LuaExpr) -> bool {
    let LuaExpr::NameExpr(name_expr) = prefix_expr else {
        return false;
    };

    if name_expr.get_name_text().as_deref() != Some("self") {
        return false;
    }

    analyzer
        .db
        .get_decl_index()
        .get_decl_tree(&analyzer.file_id)
        .and_then(|tree| tree.find_local_decl("self", name_expr.get_position()))
        .is_some_and(|decl| decl.is_implicit_self())
}

fn set_index_expr_owner(analyzer: &mut LuaAnalyzer, var_expr: LuaVarExpr) -> Option<()> {
    let file_id = analyzer.file_id;
    let index_expr = LuaIndexExpr::cast(var_expr.syntax().clone())?;
    let prefix_expr = index_expr.get_prefix_expr()?;
    let should_add_ref_to_owner = is_implicit_self_prefix(analyzer, &prefix_expr);

    let prefix_type = match analyzer.infer_expr_no_flow(&prefix_expr) {
        Ok(Some(prefix_type)) => Ok(prefix_type),
        Ok(None) => analyzer.infer_expr(&prefix_expr),
        Err(reason) => Err(reason),
    };

    match prefix_type {
        Ok(prefix_type) => {
            let prefix_type = if let LuaExpr::NameExpr(name_expr) = &prefix_expr {
                let mut explicit_type = None;
                if let Some(name) = name_expr.get_name_text() {
                    let is_shadowed = analyzer
                        .db
                        .get_decl_index()
                        .get_decl_tree(&file_id)
                        .and_then(|tree| tree.find_local_decl(&name, name_expr.get_position()))
                        .map(|decl| decl.is_local() || decl.is_implicit_self())
                        .unwrap_or(false);
                    if !is_shadowed
                        && let Some(decl_ids) =
                            analyzer.db.get_global_index().get_global_decl_ids(&name)
                    {
                        for decl_id in decl_ids {
                            if let Some(type_cache) = analyzer
                                .db
                                .get_type_index()
                                .get_type_cache(&(*decl_id).into())
                            {
                                explicit_type = Some(type_cache.as_type().clone());
                                break;
                            }
                        }
                    }
                }

                explicit_type.unwrap_or(prefix_type)
            } else {
                prefix_type
            };

            index_expr.get_index_key()?;
            let member_id = LuaMemberId::new(index_expr.get_syntax_id(), file_id);
            let member_owner = match prefix_type {
                LuaType::TableConst(in_file_range) => LuaMemberOwner::Element(in_file_range),
                LuaType::Def(def_id) => LuaMemberOwner::Type(def_id),
                LuaType::Instance(instance) => {
                    LuaMemberOwner::Element(instance.get_range().clone())
                }
                LuaType::Ref(ref_id) => {
                    let member_owner = LuaMemberOwner::Type(ref_id);
                    analyzer.db.get_member_index_mut().set_member_owner(
                        member_owner.clone(),
                        member_id.file_id,
                        member_id,
                    );
                    if should_add_ref_to_owner {
                        analyzer
                            .db
                            .get_member_index_mut()
                            .add_member_to_owner(member_owner, member_id);
                    }
                    return Some(());
                }
                _ => {
                    return None;
                }
            };

            add_member(analyzer.db, member_owner, member_id);
        }
        Err(InferFailReason::None) => {}
        Err(reason) => {
            let unresolve_member = UnResolveMember {
                file_id: analyzer.file_id,
                member_id: LuaMemberId::new(var_expr.get_syntax_id(), file_id),
                expr: None,
                prefix: Some(prefix_expr),
                ret_idx: 0,
            };
            analyzer
                .context
                .add_unresolve(unresolve_member.into(), reason);
        }
    }

    Some(())
}

pub fn analyze_assign_stat(analyzer: &mut LuaAnalyzer, assign_stat: LuaAssignStat) -> Option<()> {
    let (var_list, expr_list) = assign_stat.get_var_and_expr_list();
    let expr_count = expr_list.len();
    let var_count = var_list.len();
    for i in 0..expr_count {
        let var = var_list.get(i)?;
        let expr = expr_list.get(i);
        if expr.is_none() {
            break;
        }
        let expr = expr?;
        let type_owner = get_var_owner(analyzer, var.clone());
        set_index_expr_owner(analyzer, var.clone());

        if special_assign_pattern(analyzer, type_owner.clone(), var.clone(), expr.clone()).is_some()
        {
            continue;
        }

        let expr_type = match analyzer.infer_expr(expr) {
            Ok(expr_type) => expr_type.get_result_slot_type(0).unwrap_or(expr_type),
            Err(InferFailReason::None) => LuaType::Unknown,
            Err(reason) => {
                match type_owner {
                    LuaTypeOwner::Decl(decl_id) => {
                        let unresolve_decl = UnResolveDecl {
                            file_id: analyzer.file_id,
                            decl_id,
                            expr: expr.clone(),
                            ret_idx: 0,
                        };

                        analyzer
                            .context
                            .add_unresolve(unresolve_decl.into(), reason);
                    }
                    LuaTypeOwner::Member(member_id) => {
                        let unresolve_member = UnResolveMember {
                            file_id: analyzer.file_id,
                            member_id,
                            expr: Some(expr.clone()),
                            prefix: None,
                            ret_idx: 0,
                        };
                        analyzer
                            .context
                            .add_unresolve(unresolve_member.into(), reason);
                    }
                    _ => {}
                }
                continue;
            }
        };

        if let LuaVarExpr::NameExpr(name_expr) = var {
            if let Some(decl_id) = get_delayed_definition_decl_id(analyzer, name_expr) {
                bind_type(
                    analyzer.db,
                    decl_id.into(),
                    LuaTypeCache::InferType(expr_type.clone()),
                );
            }
        }
        assign_merge_type_owner_and_expr_type(analyzer, type_owner, &expr_type, 0);
    }

    if var_count > expr_count
        && let Some(last_expr) = expr_list.last()
    {
        match analyzer.infer_expr(last_expr) {
            Ok(last_expr_type) => {
                if last_expr_type.contain_multi_return() {
                    for i in expr_count..var_count {
                        let var = var_list.get(i)?;
                        let type_owner = get_var_owner(analyzer, var.clone());
                        set_index_expr_owner(analyzer, var.clone());
                        assign_merge_type_owner_and_expr_type(
                            analyzer,
                            type_owner,
                            &last_expr_type,
                            i - expr_count + 1,
                        );
                    }
                }
            }
            Err(_) => {
                for i in expr_count..var_count {
                    let var = var_list.get(i)?;
                    let type_owner = get_var_owner(analyzer, var.clone());
                    set_index_expr_owner(analyzer, var.clone());
                    merge_type_owner_and_unresolve_expr(
                        analyzer,
                        type_owner,
                        last_expr.clone(),
                        i - expr_count + 1,
                    );
                }
            }
        }
    }

    Some(())
}

fn assign_merge_type_owner_and_expr_type(
    analyzer: &mut LuaAnalyzer,
    type_owner: LuaTypeOwner,
    expr_type: &LuaType,
    idx: usize,
) -> Option<()> {
    let expr_type = expr_type.get_result_slot_type(idx).unwrap_or(LuaType::Nil);

    bind_type(analyzer.db, type_owner, LuaTypeCache::InferType(expr_type));

    Some(())
}

fn merge_type_owner_and_unresolve_expr(
    analyzer: &mut LuaAnalyzer,
    type_owner: LuaTypeOwner,
    expr: LuaExpr,
    idx: usize,
) -> Option<()> {
    match type_owner {
        LuaTypeOwner::Decl(decl_id) => {
            let unresolve_decl = UnResolveDecl {
                file_id: analyzer.file_id,
                decl_id,
                expr: expr.clone(),
                ret_idx: idx,
            };

            analyzer.context.add_unresolve(
                unresolve_decl.into(),
                InferFailReason::UnResolveExpr(InFiled::new(analyzer.file_id, expr.clone())),
            );
        }
        LuaTypeOwner::Member(member_id) => {
            let unresolve_member = UnResolveMember {
                file_id: analyzer.file_id,
                member_id,
                expr: Some(expr.clone()),
                prefix: None,
                ret_idx: idx,
            };
            analyzer.context.add_unresolve(
                unresolve_member.into(),
                InferFailReason::UnResolveExpr(InFiled::new(analyzer.file_id, expr.clone())),
            );
        }
        _ => {}
    }

    Some(())
}

pub fn analyze_func_stat(analyzer: &mut LuaAnalyzer, func_stat: LuaFuncStat) -> Option<()> {
    let closure = func_stat.get_closure()?;
    let func_name = func_stat.get_func_name()?;
    let signature_type = analyzer.infer_expr(&closure.clone().into()).ok()?;
    let type_owner = get_var_owner(analyzer, func_name.clone());
    set_index_expr_owner(analyzer, func_name.clone());
    analyzer
        .db
        .get_type_index_mut()
        .bind_type(type_owner, LuaTypeCache::InferType(signature_type.clone()));

    Some(())
}

pub fn analyze_local_func_stat(
    analyzer: &mut LuaAnalyzer,
    local_func_stat: LuaLocalFuncStat,
) -> Option<()> {
    let closure = local_func_stat.get_closure()?;
    let func_name = local_func_stat.get_local_name()?;
    let signature_type = analyzer.infer_expr(&closure.clone().into()).ok()?;
    let position = func_name.get_position();
    let decl_id = LuaDeclId::new(analyzer.file_id, position);
    analyzer.db.get_type_index_mut().bind_type(
        decl_id.into(),
        LuaTypeCache::InferType(signature_type.clone()),
    );

    Some(())
}

pub fn analyze_table_field(analyzer: &mut LuaAnalyzer, field: LuaTableField) -> Option<()> {
    if !field.is_assign_field() {
        return Some(());
    }

    if let Some(field_key) = field.get_field_key() {
        if let LuaIndexKey::Expr(_) = &field_key {
            let db = &mut *analyzer.db;
            let member_id = LuaMemberId::new(field.get_syntax_id(), analyzer.file_id);
            if db.get_member_index().get_member(&member_id).is_none() {
                let cache = analyzer
                    .context
                    .infer_manager
                    .get_infer_cache(analyzer.file_id);
                if let Ok(member_key) = LuaMemberKey::from_index_key(db, cache, &field_key) {
                    if !matches!(member_key, LuaMemberKey::ExprType(ref typ) if typ.is_unknown()) {
                        if let Some(table_expr) = field.get_parent::<LuaTableExpr>() {
                            let owner_id = LuaMemberOwner::Element(InFiled::new(
                                analyzer.file_id,
                                table_expr.get_range(),
                            ));
                            let decl_feature = if analyzer.context.metas.contains(&analyzer.file_id)
                            {
                                LuaMemberFeature::MetaDefine
                            } else {
                                LuaMemberFeature::FileDefine
                            };
                            let member = LuaMember::new(member_id, member_key, decl_feature, None);
                            db.get_member_index_mut().add_member(owner_id, member);
                        }
                    }
                }
            }
        }
    }

    let value_expr = field.get_value_expr()?;
    let member_id = LuaMemberId::new(field.get_syntax_id(), analyzer.file_id);
    let value_type = match analyzer.infer_expr(&value_expr.clone()) {
        Ok(value_type) => match value_type {
            LuaType::Def(ref_id) => LuaType::Ref(ref_id),
            _ => value_type,
        },
        Err(InferFailReason::None) => LuaType::Unknown,
        Err(reason) => {
            let unresolve = UnResolveMember {
                file_id: analyzer.file_id,
                member_id,
                expr: Some(value_expr.clone()),
                prefix: None,
                ret_idx: 0,
            };

            analyzer.context.add_unresolve(unresolve.into(), reason);
            return None;
        }
    };

    let cache = LuaTypeCache::InferType(value_type);
    bind_type(analyzer.db, member_id.into(), cache);

    Some(())
}

fn special_assign_pattern(
    analyzer: &mut LuaAnalyzer,
    type_owner: LuaTypeOwner,
    var: LuaVarExpr,
    expr: LuaExpr,
) -> Option<()> {
    let access_path = var.get_access_path()?;
    let binary_expr = if let LuaExpr::BinaryExpr(binary_expr) = expr {
        binary_expr
    } else {
        return None;
    };

    if binary_expr.get_op_token()?.get_op() != BinaryOperator::OpOr {
        return None;
    }

    let (left, right) = binary_expr.get_exprs()?;
    let left_var = LuaVarExpr::cast(left.syntax().clone())?;
    let left_access_path = left_var.get_access_path()?;
    if access_path != left_access_path {
        return None;
    }

    match analyzer.infer_expr(&right) {
        Ok(right_expr_type) => {
            assign_merge_type_owner_and_expr_type(analyzer, type_owner, &right_expr_type, 0);
        }
        Err(_) => return None,
    }

    Some(())
}

fn has_delayed_definition_attribute(analyzer: &LuaAnalyzer, decl_id: LuaDeclId) -> bool {
    if let Some(property) = analyzer
        .db
        .get_property_index()
        .get_property(&LuaSemanticDeclId::LuaDecl(decl_id))
    {
        return property
            .find_builtin_attribute(LuaBuiltinAttributeKind::LspOptimization)
            .and_then(|attribute_use| attribute_use.as_lsp_optimization())
            .is_some_and(|attribute| attribute.code == LuaLspOptimizationCode::DelayedDefinition);
    }
    false
}

fn get_delayed_definition_decl_id(
    analyzer: &LuaAnalyzer,
    name_expr: &LuaNameExpr,
) -> Option<LuaDeclId> {
    let file_id = analyzer.file_id;
    let references_index = analyzer.db.get_reference_index();
    let range = name_expr.get_range();
    let file_ref = references_index.get_local_reference(&file_id)?;
    let decl_id = file_ref.get_decl_id(&range)?;
    if !has_delayed_definition_attribute(analyzer, decl_id) {
        return None;
    }
    Some(decl_id)
}
