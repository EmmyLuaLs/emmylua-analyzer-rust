use hashbrown::HashSet;
use std::sync::Arc;

use crate::{
    DbIndex, LuaOperatorMetaMethod, LuaOperatorOwner, LuaTypeDeclId, LuaUnionType,
    db_index::{LuaFunctionType, LuaType},
    semantic::{
        generic::{TypeSubstitutor, instantiate_type_generic},
        infer::InferFailReason,
    },
};

pub fn collect_callable_overload_groups(
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
            let main_signature = signature.to_doc_func_type();
            let mut overloads = signature.overloads.clone();
            // 显式声明参数或返回值的主签名, 在匹配程度相同时优先于 overload.
            if signature.has_explicit_docs {
                overloads.insert(0, main_signature);
            } else {
                overloads.push(main_signature);
            }
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
    let mut declared_signatures = Vec::new();
    let mut doc_functions = Vec::new();
    let mut undeclared_signatures = Vec::new();
    for operator_id in operator_ids {
        let Some(operator) = db.get_operator_index().get_operator(operator_id) else {
            continue;
        };

        let mut func_type = operator.get_operator_func(db);
        if let Some(substitutor) = substitutor {
            func_type = instantiate_type_generic(db, &func_type, substitutor);
        }

        match func_type {
            LuaType::DocFunction(func) => doc_functions.push(func),
            LuaType::Signature(signature_id) => {
                let Some(signature) = db.get_signature_index().get(&signature_id) else {
                    continue;
                };
                // 未解析返回的 signature 不能安全转换成候选, 这里先跳过.
                if signature.is_resolve_return() {
                    let function = signature.to_call_operator_func_type();
                    if signature.has_explicit_docs {
                        declared_signatures.push(function);
                    } else {
                        undeclared_signatures.push(function);
                    }
                }
            }
            _ => {}
        }
    }

    declared_signatures.extend(doc_functions);
    declared_signatures.extend(undeclared_signatures);
    if !declared_signatures.is_empty() {
        groups.push(declared_signatures);
    }
}

fn owner_has_call_operator(db: &DbIndex, owner: &LuaOperatorOwner) -> bool {
    db.get_operator_index()
        .get_operators(owner, LuaOperatorMetaMethod::Call)
        .is_some_and(|ops| !ops.is_empty())
}

/// 如果类型可通过 `__call` 作为调用目标 (不含 signature/function 本身), 则返回 self.
pub(crate) fn call_operator_self_type(db: &DbIndex, ty: &LuaType) -> Option<LuaType> {
    let mut visiting_aliases = HashSet::new();
    call_operator_self_type_inner(db, ty, &mut visiting_aliases)
}

fn call_operator_self_type_inner(
    db: &DbIndex,
    ty: &LuaType,
    visiting_aliases: &mut HashSet<LuaTypeDeclId>,
) -> Option<LuaType> {
    match ty {
        LuaType::Ref(type_id) | LuaType::Def(type_id) => {
            call_operator_self_for_type_id(db, ty, type_id, None, visiting_aliases)
        }
        LuaType::Generic(generic) => {
            let type_id = generic.get_base_type_id();
            let substitutor = TypeSubstitutor::from_type_array(generic.get_params().to_vec());
            call_operator_self_for_type_id(db, ty, &type_id, Some(&substitutor), visiting_aliases)
        }
        LuaType::Union(union) => {
            let mut callable = Vec::new();
            for member in union.into_vec() {
                if let Some(projected) =
                    call_operator_self_type_inner(db, &member, visiting_aliases)
                {
                    callable.push(projected);
                }
            }
            match callable.len() {
                0 => None,
                1 => callable.pop(),
                _ => Some(LuaType::Union(LuaUnionType::from_vec(callable).into())),
            }
        }
        // intersection 任一成员可调用则整体可调用
        LuaType::Intersection(intersection) => intersection
            .get_types()
            .iter()
            .any(|member| call_operator_self_type_inner(db, member, visiting_aliases).is_some())
            .then(|| ty.clone()),
        LuaType::Instance(instance) => {
            call_operator_self_type_inner(db, instance.get_base(), visiting_aliases)
                .map(|_| ty.clone())
        }
        LuaType::TableConst(table) => {
            // setmetatable 产生的 __call 挂在 metatable owner 上.
            db.get_metatable_index()
                .get(table)
                .is_some_and(|meta_table| {
                    owner_has_call_operator(db, &LuaOperatorOwner::Table(meta_table.clone()))
                })
                .then(|| ty.clone())
        }
        _ => None,
    }
}

/// alias 的可调用性来自 origin, self 也按 origin 的可调用部分解析; 非 alias 则看自身是否挂了 __call.
fn call_operator_self_for_type_id(
    db: &DbIndex,
    ty: &LuaType,
    type_id: &LuaTypeDeclId,
    substitutor: Option<&TypeSubstitutor>,
    visiting_aliases: &mut HashSet<LuaTypeDeclId>,
) -> Option<LuaType> {
    if !visiting_aliases.insert(type_id.clone()) {
        return None;
    }
    let Some(type_decl) = db.get_type_index().get_type_decl(type_id) else {
        visiting_aliases.remove(type_id);
        return None;
    };

    if let Some(origin_type) = type_decl.get_alias_origin(db, substitutor)
        && let Some(projected) = call_operator_self_type_inner(db, &origin_type, visiting_aliases)
    {
        visiting_aliases.remove(type_id);
        return Some(projected);
    }

    let has_call = !type_decl.is_alias()
        && !type_decl.is_enum()
        && owner_has_call_operator(db, &type_id.clone().into());
    visiting_aliases.remove(type_id);
    has_call.then(|| ty.clone())
}
