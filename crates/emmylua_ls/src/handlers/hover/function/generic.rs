use emmylua_code_analysis::{
    DbIndex, LuaType, LuaTypeDeclId, TypeSubstitutor, instantiate_type_generic,
};
use emmylua_parser::LuaExpr;

use crate::handlers::hover::HoverBuilder;

pub(super) fn instantiate_type_if_needed(
    db: &DbIndex,
    typ: &LuaType,
    substitutor: &TypeSubstitutor,
) -> Option<LuaType> {
    typ.contain_tpl()
        .then(|| instantiate_type_generic(db, typ, substitutor))
}

pub(super) fn index_prefix_substitutor(
    builder: &HoverBuilder,
    expr: &LuaExpr,
) -> Option<TypeSubstitutor> {
    let LuaExpr::IndexExpr(index_expr) = expr else {
        return None;
    };
    let prefix_type = builder
        .semantic_model
        .infer_expr(index_expr.get_prefix_expr()?)
        .ok()?;
    match prefix_type {
        LuaType::Generic(generic) => Some(TypeSubstitutor::from_type_array(
            generic.get_params().clone(),
        )),
        _ => None,
    }
}

pub(super) fn owner_type_substitutor(
    db: &DbIndex,
    typ: &LuaType,
    owner_type_id: &LuaTypeDeclId,
) -> Option<TypeSubstitutor> {
    match typ {
        LuaType::Generic(generic) => {
            if generic.get_base_type_id_ref() == owner_type_id {
                Some(TypeSubstitutor::from_type_array(
                    generic.get_params().clone(),
                ))
            } else {
                None
            }
        }
        LuaType::Ref(id) | LuaType::Def(id) => {
            if id == owner_type_id {
                unknown_type_substitutor(db, owner_type_id)
            } else {
                None
            }
        }
        LuaType::Union(union) => {
            let mut substitutor = None;
            for typ in union.into_vec() {
                let Some(generic_substitutor) = owner_type_substitutor(db, &typ, owner_type_id)
                else {
                    continue;
                };
                if substitutor.is_some() {
                    return None;
                }
                substitutor = Some(generic_substitutor);
            }
            substitutor
        }
        _ => None,
    }
}

pub(super) fn unknown_type_substitutor(
    db: &DbIndex,
    owner_type_id: &LuaTypeDeclId,
) -> Option<TypeSubstitutor> {
    let generic_params = db.get_type_index().get_generic_params(owner_type_id)?;
    if generic_params.is_empty() {
        return None;
    }
    Some(TypeSubstitutor::from_type_array(vec![
        LuaType::Unknown;
        generic_params.len()
    ]))
}
