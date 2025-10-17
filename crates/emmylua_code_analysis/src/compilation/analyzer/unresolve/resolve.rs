use std::ops::Deref;

use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallExpr, LuaExpr, LuaIndexExpr, LuaLocalStat, LuaTableExpr,
};

use crate::{
    InFiled, InferFailReason, LuaDeclId, LuaMember, LuaMemberId, LuaMemberInfo, LuaMemberKey,
    LuaOperator, LuaOperatorMetaMethod, LuaOperatorOwner, LuaSemanticDeclId, LuaTypeCache,
    LuaTypeDeclId, OperatorFunction, SignatureReturnStatus, TypeOps,
    compilation::analyzer::{
        common::{add_member, bind_type},
        lua::{analyze_return_point, infer_for_range_iter_expr_func},
        unresolve::UnResolveConstructor,
    },
    db_index::{DbIndex, LuaMemberOwner, LuaType},
    find_members_with_key,
    semantic::{LuaInferCache, infer_expr},
};

use super::{
    ResolveResult, UnResolveDecl, UnResolveIterVar, UnResolveMember, UnResolveModule,
    UnResolveModuleRef, UnResolveReturn, UnResolveTableField,
};

pub fn try_resolve_decl(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    decl: &mut UnResolveDecl,
) -> ResolveResult {
    let expr = decl.expr.clone();
    let expr_type = infer_expr(db, cache, expr)?;
    let decl_id = decl.decl_id;
    let expr_type = match &expr_type {
        LuaType::Variadic(multi) => multi
            .get_type(decl.ret_idx)
            .cloned()
            .unwrap_or(LuaType::Unknown),
        _ => expr_type,
    };

    bind_type(db, decl_id.into(), LuaTypeCache::InferType(expr_type));
    Ok(())
}

pub fn try_resolve_member(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    unresolve_member: &mut UnResolveMember,
) -> ResolveResult {
    if let Some(prefix_expr) = &unresolve_member.prefix {
        let prefix_type = infer_expr(db, cache, prefix_expr.clone())?;
        let member_owner = match prefix_type {
            LuaType::TableConst(in_file_range) => LuaMemberOwner::Element(in_file_range),
            LuaType::Def(def_id) => {
                let type_decl = db
                    .get_type_index()
                    .get_type_decl(&def_id)
                    .ok_or(InferFailReason::None)?;
                // if is exact type, no need to extend field
                if type_decl.is_exact() {
                    return Ok(());
                }
                LuaMemberOwner::Type(def_id)
            }
            LuaType::Instance(instance) => LuaMemberOwner::Element(instance.get_range().clone()),
            // is ref need extend field?
            _ => {
                return Ok(()); // Changed from return None to return Ok(())
            }
        };
        let member_id = unresolve_member.member_id;
        add_member(db, member_owner, member_id);
        unresolve_member.prefix = None;
    }

    if let Some(expr) = unresolve_member.expr.clone() {
        let expr_type = infer_expr(db, cache, expr)?;
        let expr_type = match &expr_type {
            LuaType::Variadic(multi) => multi
                .get_type(unresolve_member.ret_idx)
                .cloned()
                .unwrap_or(LuaType::Unknown),
            _ => expr_type,
        };

        let member_id = unresolve_member.member_id;
        bind_type(db, member_id.into(), LuaTypeCache::InferType(expr_type));
    }

    Ok(())
}

pub fn try_resolve_table_field(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    unresolve_table_field: &mut UnResolveTableField,
) -> ResolveResult {
    let field = unresolve_table_field.field.clone();
    let field_key = field.get_field_key().ok_or(InferFailReason::None)?;
    let field_expr = field_key.get_expr().ok_or(InferFailReason::None)?;
    let field_type = infer_expr(db, cache, field_expr.clone())?;
    let member_key: LuaMemberKey = match field_type {
        LuaType::StringConst(s) => LuaMemberKey::Name((*s).clone()),
        LuaType::IntegerConst(i) => LuaMemberKey::Integer(i),
        _ => {
            if field_type.is_table() {
                LuaMemberKey::ExprType(field_type)
            } else {
                return Err(InferFailReason::None);
            }
        }
    };
    let file_id = unresolve_table_field.file_id;
    let table_expr = unresolve_table_field.table_expr.clone();
    let owner_id = LuaMemberOwner::Element(InFiled {
        file_id,
        value: table_expr.get_range(),
    });

    db.get_reference_index_mut().add_index_reference(
        member_key.clone(),
        file_id,
        field.get_syntax_id(),
    );

    let decl_type = match field.get_value_expr() {
        Some(expr) => infer_expr(db, cache, expr)?,
        None => return Err(InferFailReason::None),
    };

    let member_id = LuaMemberId::new(field.get_syntax_id(), file_id);
    let member = LuaMember::new(
        member_id,
        member_key,
        unresolve_table_field.decl_feature,
        None,
    );
    db.get_member_index_mut().add_member(owner_id, member);
    db.get_type_index_mut()
        .bind_type(member_id.into(), LuaTypeCache::InferType(decl_type.clone()));

    merge_table_field_to_def(db, cache, table_expr, member_id);
    Ok(())
}

fn merge_table_field_to_def(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    table_expr: LuaTableExpr,
    member_id: LuaMemberId,
) -> Option<()> {
    let file_id = cache.get_file_id();
    let local_name = table_expr
        .get_parent::<LuaLocalStat>()?
        .get_local_name_by_value(LuaExpr::TableExpr(table_expr.clone()))?;
    let decl_id = LuaDeclId::new(file_id, local_name.get_position());
    let type_cache = db.get_type_index().get_type_cache(&decl_id.into())?;
    if let LuaType::Def(id) = type_cache.deref() {
        let owner = LuaMemberOwner::Type(id.clone());
        db.get_member_index_mut()
            .set_member_owner(owner.clone(), member_id.file_id, member_id);
        db.get_member_index_mut()
            .add_member_to_owner(owner.clone(), member_id);
    }

    Some(())
}

pub fn try_resolve_module(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    module: &mut UnResolveModule,
) -> ResolveResult {
    let expr = module.expr.clone();
    let expr_type = infer_expr(db, cache, expr)?;
    let expr_type = match &expr_type {
        LuaType::Variadic(multi) => multi.get_type(0).cloned().unwrap_or(LuaType::Unknown),
        _ => expr_type,
    };
    let module_info = db
        .get_module_index_mut()
        .get_module_mut(module.file_id)
        .ok_or(InferFailReason::None)?;
    module_info.export_type = Some(expr_type);
    Ok(())
}

pub fn try_resolve_return_point(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    return_: &mut UnResolveReturn,
) -> ResolveResult {
    let return_docs = analyze_return_point(db, cache, &return_.return_points)?;

    let signature = db
        .get_signature_index_mut()
        .get_mut(&return_.signature_id)
        .ok_or(InferFailReason::None)?;

    if signature.resolve_return == SignatureReturnStatus::UnResolve {
        signature.resolve_return = SignatureReturnStatus::InferResolve;
        signature.return_docs = return_docs;
    }

    Ok(())
}

pub fn try_resolve_iter_var(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    unresolve_iter_var: &mut UnResolveIterVar,
) -> ResolveResult {
    let iter_var_types = infer_for_range_iter_expr_func(db, cache, &unresolve_iter_var.iter_exprs)?;
    for (idx, var_name) in unresolve_iter_var.iter_vars.iter().enumerate() {
        let position = var_name.get_position();
        let decl_id = LuaDeclId::new(unresolve_iter_var.file_id, position);
        let ret_type = iter_var_types
            .get_type(idx)
            .cloned()
            .unwrap_or(LuaType::Unknown);
        let ret_type = TypeOps::Remove.apply(db, &ret_type, &LuaType::Nil);

        db.get_type_index_mut()
            .bind_type(decl_id.into(), LuaTypeCache::InferType(ret_type));
    }
    Ok(())
}

pub fn try_resolve_module_ref(
    db: &mut DbIndex,
    _: &mut LuaInferCache,
    module_ref: &UnResolveModuleRef,
) -> ResolveResult {
    let module_index = db.get_module_index();
    let module = module_index
        .get_module(module_ref.module_file_id)
        .ok_or(InferFailReason::None)?;
    let export_type = module.export_type.clone().ok_or(InferFailReason::None)?;
    match &module_ref.owner_id {
        LuaSemanticDeclId::LuaDecl(decl_id) => {
            db.get_type_index_mut()
                .bind_type((*decl_id).into(), LuaTypeCache::InferType(export_type));
        }
        LuaSemanticDeclId::Member(member_id) => {
            db.get_type_index_mut()
                .bind_type((*member_id).into(), LuaTypeCache::InferType(export_type));
        }
        _ => {}
    };

    Ok(())
}

pub fn try_resolve_constructor(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    unresolve_constructor: &mut UnResolveConstructor,
) -> ResolveResult {
    let (param_type, target_signature_name, root_class, strip_self, return_self) = {
        let signature = db
            .get_signature_index()
            .get(&unresolve_constructor.signature_id)
            .ok_or(InferFailReason::None)?;
        let param_info = signature
            .get_param_info_by_id(unresolve_constructor.param_idx)
            .ok_or(InferFailReason::None)?;
        let constructor_use = param_info
            .get_attribute_by_name("constructor")
            .ok_or(InferFailReason::None)?;

        // 作为构造函数的方法名
        let target_signature_name = constructor_use
            .get_param_by_name("name")
            .and_then(|typ| match typ {
                LuaType::DocStringConst(value) => Some(value.deref().clone()),
                _ => None,
            })
            .ok_or(InferFailReason::None)?;
        // 作为构造函数的根类
        let root_class =
            constructor_use
                .get_param_by_name("root_class")
                .and_then(|typ| match typ {
                    LuaType::DocStringConst(value) => Some(value.deref().clone()),
                    _ => None,
                });
        // 是否可以省略self参数
        let strip_self = constructor_use
            .get_param_by_name("strip_self")
            .and_then(|typ| match typ {
                LuaType::DocBooleanConst(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(true);
        // 是否返回self
        let return_self = constructor_use
            .get_param_by_name("return_self")
            .and_then(|typ| match typ {
                LuaType::DocBooleanConst(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(true);

        Ok::<_, InferFailReason>((
            param_info.type_ref.clone(),
            target_signature_name,
            root_class,
            strip_self,
            return_self,
        ))
    }?;

    // 需要添加构造函数的目标类型
    let target_id = get_constructor_target_type(
        db,
        cache,
        &param_type,
        unresolve_constructor.call_expr.clone(),
        unresolve_constructor.param_idx,
    )
    .ok_or(InferFailReason::None)?;

    // 添加根类
    if let Some(root_class) = root_class {
        let root_type_id = LuaTypeDeclId::new(&root_class);
        if let Some(type_decl) = db.get_type_index().get_type_decl(&root_type_id) {
            if type_decl.is_class() {
                let root_type = LuaType::Ref(root_type_id.clone());
                db.get_type_index_mut().add_super_type(
                    target_id.clone(),
                    unresolve_constructor.file_id,
                    root_type,
                );
            }
        }
    }

    // 添加构造函数
    let target_type = LuaType::Ref(target_id);
    let member_key = LuaMemberKey::Name(target_signature_name);
    let members =
        find_members_with_key(db, &target_type, member_key, false).ok_or(InferFailReason::None)?;
    let ctor_signature_member = members.first().ok_or(InferFailReason::None)?;

    set_signature_to_default_call(db, cache, ctor_signature_member, strip_self, return_self)
        .ok_or(InferFailReason::None)?;

    Ok(())
}

fn set_signature_to_default_call(
    db: &mut DbIndex,
    cache: &mut LuaInferCache,
    member_info: &LuaMemberInfo,
    strip_self: bool,
    return_self: bool,
) -> Option<()> {
    let LuaType::Signature(signature_id) = member_info.typ else {
        return None;
    };
    let Some(LuaSemanticDeclId::Member(member_id)) = member_info.property_owner_id else {
        return None;
    };
    // 我们仍然需要再做一次判断确定是否来源于`Def`类型
    let root = db
        .get_vfs()
        .get_syntax_tree(&member_id.file_id)?
        .get_red_root();
    let index_expr = LuaIndexExpr::cast(member_id.get_syntax_id().to_node_from_root(&root)?)?;
    let prefix_expr = index_expr.get_prefix_expr()?;
    let prefix_type = infer_expr(db, cache, prefix_expr.clone()).ok()?;
    let LuaType::Def(decl_id) = prefix_type else {
        return None;
    };
    // 如果已经存在显式的`__call`定义, 则不添加
    let call = db.get_operator_index().get_operators(
        &LuaOperatorOwner::Type(decl_id.clone()),
        LuaOperatorMetaMethod::Call,
    );
    if call.is_some() {
        return None;
    }

    let operator = LuaOperator::new(
        decl_id.into(),
        LuaOperatorMetaMethod::Call,
        member_id.file_id,
        // 必须指向名称, 使用 index_expr 的完整范围不会跳转到函数上
        index_expr.get_name_token()?.syntax().text_range(),
        OperatorFunction::DefaultClassCtor {
            id: signature_id,
            strip_self,
            return_self,
        },
    );
    db.get_operator_index_mut().add_operator(operator);
    Some(())
}

fn get_constructor_target_type(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    param_type: &LuaType,
    call_expr: LuaCallExpr,
    call_index: usize,
) -> Option<LuaTypeDeclId> {
    if let LuaType::StrTplRef(str_tpl) = param_type {
        let name = {
            let arg_expr = call_expr
                .get_args_list()?
                .get_args()
                .nth(call_index)?
                .clone();
            let name = infer_expr(db, cache, arg_expr).ok()?;
            match name {
                LuaType::StringConst(s) => s.to_string(),
                _ => return None,
            }
        };

        let prefix = str_tpl.get_prefix();
        let suffix = str_tpl.get_suffix();
        let type_decl_id: LuaTypeDeclId =
            LuaTypeDeclId::new(format!("{}{}{}", prefix, name, suffix).as_str());
        let type_decl = db.get_type_index().get_type_decl(&type_decl_id)?;
        if type_decl.is_class() {
            return Some(type_decl_id);
        }
    }

    None
}
