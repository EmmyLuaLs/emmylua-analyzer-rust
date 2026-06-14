use hashbrown::HashSet;
use std::sync::Arc;

use crate::{
    DbIndex, LuaOperatorMetaMethod, LuaOperatorOwner, LuaTypeDeclId,
    db_index::{LuaFunctionType, LuaType},
    semantic::{
        generic::{TypeSubstitutor, instantiate_type_generic},
        infer::InferFailReason,
    },
};

pub(crate) fn collect_callable_overload_groups(
    db: &DbIndex,
    callable_type: &LuaType,
    groups: &mut Vec<Vec<Arc<LuaFunctionType>>>,
) -> Result<(), InferFailReason> {
    let mut visiting_aliases = HashSet::new();
    collect_callable_overload_groups_inner(db, callable_type, groups, &mut visiting_aliases)
}

fn collect_callable_overload_groups_inner(
    db: &DbIndex,
    callable_type: &LuaType,
    groups: &mut Vec<Vec<Arc<LuaFunctionType>>>,
    visiting_aliases: &mut HashSet<LuaTypeDeclId>,
) -> Result<(), InferFailReason> {
    match callable_type {
        LuaType::Ref(type_id) | LuaType::Def(type_id) => {
            let Some(type_decl) = db.get_type_index().get_type_decl(type_id) else {
                return Ok(());
            };
            if !visiting_aliases.insert(type_id.clone()) {
                return Ok(());
            }

            let result = if let Some(origin_type) = type_decl.get_alias_origin(db, None) {
                collect_callable_overload_groups_inner(db, &origin_type, groups, visiting_aliases)
            } else {
                Ok(())
            };
            // alias 的可调用性来自 origin, 非 alias 类型再补充自身的 __call 候选
            if !type_decl.is_alias() && !type_decl.is_enum() {
                push_call_operator_overload_group(db, &type_id.clone().into(), groups, None);
            }
            visiting_aliases.remove(type_id);
            result?;
        }
        LuaType::Generic(generic) => {
            let type_id = generic.get_base_type_id();
            if !visiting_aliases.insert(type_id.clone()) {
                return Ok(());
            }
            let substitutor = TypeSubstitutor::from_type_array(generic.get_params().to_vec());
            let Some(type_decl) = db.get_type_index().get_type_decl(&type_id) else {
                visiting_aliases.remove(&type_id);
                return Ok(());
            };

            let result = if let Some(origin_type) =
                type_decl.get_alias_origin(db, Some(&substitutor))
            {
                collect_callable_overload_groups_inner(db, &origin_type, groups, visiting_aliases)
            } else {
                Ok(())
            };
            // 泛型类型的 __call 需要先替换类型模板, 否则候选会保留未实例化的 T
            if !type_decl.is_alias() && !type_decl.is_enum() {
                push_call_operator_overload_group(
                    db,
                    &type_id.clone().into(),
                    groups,
                    Some(&substitutor),
                );
            }
            visiting_aliases.remove(&type_id);
            result?;
        }
        LuaType::Union(union) => {
            for member in union.into_vec() {
                collect_callable_overload_groups_inner(db, &member, groups, visiting_aliases)?;
            }
        }
        LuaType::Intersection(intersection) => {
            for member in intersection.get_types() {
                collect_callable_overload_groups_inner(db, member, groups, visiting_aliases)?;
            }
        }
        LuaType::DocFunction(doc_func) => groups.push(vec![doc_func.clone()]),
        LuaType::Signature(sig_id) => {
            let Some(signature) = db.get_signature_index().get(sig_id) else {
                return Ok(());
            };
            let mut overloads = signature.overloads.to_vec();
            overloads.push(signature.to_doc_func_type());
            groups.push(overloads);
        }
        LuaType::Instance(instance) => {
            // instance 的可调用性由它的 base 决定.
            collect_callable_overload_groups_inner(
                db,
                instance.get_base(),
                groups,
                visiting_aliases,
            )?;
        }
        LuaType::TableConst(table) => {
            // setmetatable 产生的 __call 挂在 metatable owner 上.
            if let Some(meta_table) = db.get_metatable_index().get(table) {
                push_call_operator_overload_group(
                    db,
                    &LuaOperatorOwner::Table(meta_table.clone()),
                    groups,
                    None,
                );
            }
        }
        _ => {}
    }

    Ok(())
}

fn push_call_operator_overload_group(
    db: &DbIndex,
    owner: &LuaOperatorOwner,
    groups: &mut Vec<Vec<Arc<LuaFunctionType>>>,
    substitutor: Option<&TypeSubstitutor>,
) {
    let Some(operator_ids) = db
        .get_operator_index()
        .get_operators(owner, LuaOperatorMetaMethod::Call)
    else {
        return;
    };

    // 同一个 owner 的 call operators 作为一个 overload group, 由调用方再做参数匹配.
    let mut overloads = Vec::new();
    for operator_id in operator_ids {
        let Some(operator) = db.get_operator_index().get_operator(operator_id) else {
            continue;
        };

        let mut func_type = operator.get_operator_func(db);
        if let Some(substitutor) = substitutor {
            func_type = instantiate_type_generic(db, &func_type, substitutor);
        }

        match func_type {
            LuaType::DocFunction(func) => overloads.push(func),
            LuaType::Signature(signature_id) => {
                let Some(signature) = db.get_signature_index().get(&signature_id) else {
                    continue;
                };
                // 未解析返回的 signature 不能安全转换成候选, 这里先跳过.
                if signature.is_resolve_return() {
                    overloads.push(signature.to_call_operator_func_type());
                }
            }
            _ => {}
        }
    }

    if !overloads.is_empty() {
        groups.push(overloads);
    }
}
