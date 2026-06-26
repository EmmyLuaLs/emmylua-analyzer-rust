use hashbrown::HashSet;
use std::sync::Arc;

use crate::{
    DbIndex, LuaTypeDeclId,
    db_index::{LuaFunctionType, LuaType},
    semantic::{generic::TypeSubstitutor, infer::InferFailReason},
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
        _ => {}
    }

    Ok(())
}
