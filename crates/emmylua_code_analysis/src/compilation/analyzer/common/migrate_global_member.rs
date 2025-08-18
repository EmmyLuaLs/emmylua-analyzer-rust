use crate::{
    DbIndex, GlobalId, LuaDeclId, LuaMemberId, LuaMemberOwner, LuaType, LuaTypeOwner,
    compilation::analyzer::common::add_member,
};

pub fn migrate_global_members_when_type_resolve(
    db: &mut DbIndex,
    type_owner: LuaTypeOwner,
) -> Option<()> {
    match type_owner {
        LuaTypeOwner::Decl(decl_id) => {
            migrate_global_member_to_decl(db, decl_id);
        }
        LuaTypeOwner::Member(member_id) => {
            migrate_global_member_to_member(db, member_id);
        }
        _ => {}
    }
    Some(())
}

fn migrate_global_member_to_decl(db: &mut DbIndex, decl_id: LuaDeclId) -> Option<()> {
    let decl = db.get_decl_index().get_decl(&decl_id)?;
    if !decl.is_global() {
        return None;
    }

    let owner_id = get_owner_id(db, &decl_id.clone().into())?;

    let name = decl.get_name();
    let global_id = GlobalId::new(name.into());
    let members = db
        .get_member_index()
        .get_members(&LuaMemberOwner::GlobalId(global_id))?
        .iter()
        .map(|member| member.get_id())
        .collect::<Vec<_>>();

    for member_id in members {
        add_member(db, owner_id.clone(), member_id);
    }

    Some(())
}

fn migrate_global_member_to_member(db: &mut DbIndex, member_id: LuaMemberId) -> Option<()> {
    let global_id = db.get_member_index().get_member_global_id(&member_id)?;
    let owner_id = get_owner_id(db, &member_id.clone().into())?;

    let members = db
        .get_member_index()
        .get_members(&LuaMemberOwner::GlobalId(global_id.clone()))?
        .iter()
        .map(|member| member.get_id())
        .collect::<Vec<_>>();

    let member_index = db.get_member_index_mut();
    for member_id in members {
        member_index.set_member_owner(owner_id.clone(), member_id.file_id, member_id);
        member_index.add_member_to_owner(owner_id.clone(), member_id);
    }

    Some(())
}

fn get_owner_id(db: &DbIndex, type_owner: &LuaTypeOwner) -> Option<LuaMemberOwner> {
    let type_cache = db.get_type_index().get_type_cache(&type_owner)?;
    match type_cache.as_type() {
        LuaType::Ref(type_id) => Some(LuaMemberOwner::Type(type_id.clone())),
        LuaType::TableConst(id) => Some(LuaMemberOwner::Element(id.clone())),
        LuaType::Instance(inst) => Some(LuaMemberOwner::Element(inst.get_range().clone())),
        // LuaType::GlobalTable(inst) => Some(LuaMemberOwner::GlobalId(GlobalId(inst.clone()))),
        _ => None,
    }
}
