use crate::{
    DbIndex, GenericParam, LuaMember, LuaMemberId, LuaMemberKey, LuaMemberOwner, LuaSignature,
    LuaSignatureId, LuaType, LuaTypeCache, LuaTypeDecl, LuaTypeDeclId, LuaUnionType,
    TypeSubstitutor, instantiate_type_generic,
};

pub(crate) fn get_type_decl<'a>(
    db: &'a DbIndex,
    type_decl_id: &LuaTypeDeclId,
) -> Option<&'a LuaTypeDecl> {
    db.get_type_index().get_type_decl(type_decl_id)
}

pub(crate) fn get_generic_params<'a>(
    db: &'a DbIndex,
    type_decl_id: &LuaTypeDeclId,
) -> Option<&'a Vec<GenericParam>> {
    db.get_type_index().get_generic_params(type_decl_id)
}

pub(crate) fn get_sorted_members<'a>(
    db: &'a DbIndex,
    owner: &LuaMemberOwner,
) -> Option<Vec<&'a LuaMember>> {
    db.get_member_index().get_sorted_members(owner)
}

pub(crate) fn get_type_cache<'a>(
    db: &'a DbIndex,
    member_id: &LuaMemberId,
) -> Option<&'a LuaTypeCache> {
    db.get_type_index()
        .get_type_cache(&member_id.clone().into())
}

pub(crate) fn get_real_type<'a>(db: &'a DbIndex, typ: &'a LuaType) -> Option<&'a LuaType> {
    get_real_type_with_depth(db, typ, 0)
}

fn get_real_type_with_depth<'a>(
    db: &'a DbIndex,
    typ: &'a LuaType,
    depth: u32,
) -> Option<&'a LuaType> {
    const MAX_RECURSION_DEPTH: u32 = 10;

    if depth >= MAX_RECURSION_DEPTH {
        return Some(typ);
    }

    match typ {
        LuaType::Ref(type_decl_id) => {
            let type_decl = get_type_decl(db, type_decl_id)?;
            if type_decl.is_alias() {
                return get_real_type_with_depth(db, type_decl.get_alias_ref()?, depth + 1);
            }
            Some(typ)
        }
        _ => Some(typ),
    }
}

pub(crate) fn has_userdata_super_type(db: &DbIndex, type_decl_id: &LuaTypeDeclId) -> bool {
    db.get_type_index()
        .get_super_types_iter(type_decl_id)
        .is_some_and(|mut super_types| super_types.any(LuaType::is_userdata))
}

pub(crate) fn get_signature<'a>(
    db: &'a DbIndex,
    signature_id: &LuaSignatureId,
) -> Option<&'a LuaSignature> {
    db.get_signature_index().get(signature_id)
}

pub(crate) fn get_alias_origin(
    db: &DbIndex,
    type_decl: &LuaTypeDecl,
    substitutor: Option<&TypeSubstitutor>,
) -> Option<LuaType> {
    let origin = type_decl.get_alias_ref()?.clone();
    let substitutor = match substitutor {
        Some(substitutor) => substitutor,
        None => return Some(origin),
    };

    if db
        .get_type_index()
        .get_generic_params(&type_decl.get_id())
        .is_none()
    {
        return Some(origin);
    }

    Some(instantiate_type_generic(db, &origin, substitutor))
}

pub(crate) fn get_enum_field_type(db: &DbIndex, type_decl: &LuaTypeDecl) -> Option<LuaType> {
    if !type_decl.is_enum() {
        return None;
    }

    let enum_member_owner = LuaMemberOwner::Type(type_decl.get_id());
    let enum_members = db.get_member_index().get_members(&enum_member_owner)?;

    let mut union_types = Vec::new();
    if type_decl.is_enum_key() {
        for enum_member in enum_members {
            let fake_type = match enum_member.get_key() {
                LuaMemberKey::Name(name) => LuaType::DocStringConst(name.clone().into()),
                LuaMemberKey::Integer(i) => LuaType::IntegerConst(*i),
                LuaMemberKey::ExprType(typ) => typ.clone(),
                LuaMemberKey::None => continue,
            };

            union_types.push(fake_type);
        }
    } else {
        for member in enum_members {
            let Some(type_cache) = get_type_cache(db, &member.get_id()) else {
                continue;
            };

            let member_fake_type = match type_cache {
                LuaTypeCache::InferType(typ) | LuaTypeCache::DocType(typ) => match typ {
                    LuaType::StringConst(s) => LuaType::DocStringConst(s.clone()),
                    LuaType::IntegerConst(i) => LuaType::DocIntegerConst(*i),
                    _ => typ.clone(),
                },
            };

            union_types.push(member_fake_type);
        }
    }

    Some(LuaType::Union(LuaUnionType::from_vec(union_types).into()))
}
