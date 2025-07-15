use emmylua_parser::{
    BinaryOperator, LuaAssignStat, LuaAstNode, LuaAstToken, LuaExpr, LuaFuncStat, LuaIndexExpr,
    LuaLocalFuncStat, LuaLocalStat, LuaTableField, LuaVarExpr, PathTrait,
};

use crate::{
    compilation::analyzer::{
        bind_type::{add_member, bind_type},
        unresolve::{UnResolveDecl, UnResolveMember},
    },
    db_index::{LuaDeclId, LuaMemberId, LuaMemberOwner, LuaType},
    InFiled, InferFailReason, LuaOperator, LuaOperatorMetaMethod, LuaOperatorOwner, LuaTypeCache,
    LuaTypeOwner, OperatorFunction,
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
            analyzer
                .db
                .get_type_index_mut()
                .bind_type(decl_id.into(), LuaTypeCache::InferType(LuaType::Unknown));
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
            Ok(mut expr_type) => {
                if let LuaType::Variadic(multi) = expr_type {
                    expr_type = multi.get_type(0)?.clone();
                }
                let decl_id = LuaDeclId::new(analyzer.file_id, position);
                // 当`call`参数包含表时, 表可能未被分析, 需要延迟
                if let LuaType::Instance(instance) = &expr_type {
                    if instance.get_base().is_unknown() {
                        if call_expr_has_effect_table_arg(&expr).is_some() {
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
                    }
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

    // The complexity brought by multiple return values is too high
    if name_count > expr_count {
        let last_expr = expr_list.last();
        if let Some(last_expr) = last_expr {
            match analyzer.infer_expr(last_expr) {
                Ok(last_expr_type) => {
                    if let LuaType::Variadic(variadic) = last_expr_type {
                        for i in expr_count..name_count {
                            let name = name_list.get(i)?;
                            let position = name.get_position();
                            let decl_id = LuaDeclId::new(analyzer.file_id, position);
                            let ret_type = variadic.get_type(i - expr_count + 1);
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
            if let LuaExpr::TableExpr(table_expr) = arg {
                if !table_expr.is_empty() {
                    return Some(());
                }
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

fn set_index_expr_owner(analyzer: &mut LuaAnalyzer, var_expr: LuaVarExpr) -> Option<()> {
    let file_id = analyzer.file_id;
    let index_expr = LuaIndexExpr::cast(var_expr.syntax().clone())?;
    let prefix_expr = index_expr.get_prefix_expr()?;

    match analyzer.infer_expr(&prefix_expr.clone().into()) {
        Ok(prefix_type) => {
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
                        member_owner,
                        member_id.file_id,
                        member_id,
                    );
                    return Some(());
                }
                // is ref need extend field?
                _ => {
                    return None;
                }
            };

            add_member(analyzer.db, member_owner, member_id);
        }
        Err(InferFailReason::None) => {}
        Err(reason) => {
            // record unresolve
            let unresolve_member = UnResolveMember {
                file_id: analyzer.file_id,
                member_id: LuaMemberId::new(var_expr.get_syntax_id(), file_id),
                expr: None,
                prefix: Some(prefix_expr.into()),
                ret_idx: 0,
            };
            analyzer
                .context
                .add_unresolve(unresolve_member.into(), reason);
        }
    }

    Some(())
}

// assign stat is toooooooooo complex
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

        match special_assign_pattern(analyzer, type_owner.clone(), var.clone(), expr.clone()) {
            Some(_) => {
                continue;
            }
            None => {}
        }

        let expr_type = match analyzer.infer_expr(expr) {
            Ok(expr_type) => match expr_type {
                LuaType::Variadic(multi) => multi.get_type(0)?.clone(),
                _ => expr_type,
            },
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
        assign_merge_type_owner_and_expr_type(analyzer, type_owner, &expr_type, 0);
    }

    // The complexity brought by multiple return values is too high
    if var_count > expr_count {
        if let Some(last_expr) = expr_list.last() {
            match analyzer.infer_expr(last_expr) {
                Ok(last_expr_type) => {
                    if last_expr_type.is_multi_return() {
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

        // Expressions like a, b are not valid
    }

    Some(())
}

fn assign_merge_type_owner_and_expr_type(
    analyzer: &mut LuaAnalyzer,
    type_owner: LuaTypeOwner,
    expr_type: &LuaType,
    idx: usize,
) -> Option<()> {
    let mut expr_type = expr_type.clone();
    if let LuaType::Variadic(multi) = expr_type {
        expr_type = multi.get_type(idx).unwrap_or(&LuaType::Nil).clone();
    }

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

    try_add_class_default_call(analyzer, func_name, signature_type);

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
    let _ = field.get_field_key()?;
    let value_expr = field.get_value_expr()?;
    let member_id = LuaMemberId::new(field.get_syntax_id(), analyzer.file_id);
    let value_type = match analyzer.infer_expr(&value_expr.clone().into()) {
        Ok(value_type) => value_type,
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

    bind_type(
        analyzer.db,
        member_id.into(),
        LuaTypeCache::InferType(value_type),
    );

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

pub fn try_add_class_default_call(
    analyzer: &mut LuaAnalyzer,
    func_name: LuaVarExpr,
    signature_type: LuaType,
) -> Option<()> {
    let LuaType::Signature(signature_id) = signature_type else {
        return None;
    };

    let default_name = &analyzer
        .get_emmyrc()
        .runtime
        .class_default_call
        .function_name;

    if default_name.is_empty() {
        return None;
    }
    if let LuaVarExpr::IndexExpr(index_expr) = func_name {
        let index_key = index_expr.get_index_key()?;
        if index_key.get_path_part() == *default_name {
            let prefix_expr = index_expr.get_prefix_expr()?;
            match analyzer.infer_expr(&prefix_expr.into()) {
                Ok(prefix_type) => match prefix_type {
                    LuaType::Def(decl_id) => {
                        // 如果已经存在, 则不添加
                        let call = analyzer.db.get_operator_index().get_operators(
                            &LuaOperatorOwner::Type(decl_id.clone()),
                            LuaOperatorMetaMethod::Call,
                        );
                        if call.is_some() {
                            return None;
                        }

                        let operator = LuaOperator::new(
                            decl_id.into(),
                            LuaOperatorMetaMethod::Call,
                            analyzer.file_id,
                            // 必须指向名称, 使用 index_expr 的完整范围不会跳转到函数上
                            index_expr.get_name_token()?.syntax().text_range(),
                            OperatorFunction::DefaultCall(signature_id),
                        );
                        analyzer.db.get_operator_index_mut().add_operator(operator);
                    }
                    _ => {}
                },
                Err(_) => {}
            }
        }
    }

    Some(())
}
