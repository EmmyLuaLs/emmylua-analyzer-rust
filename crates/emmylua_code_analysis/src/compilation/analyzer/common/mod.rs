mod bind_type;
mod member;
mod migrate_global_member;

pub use bind_type::{add_member, bind_type};
pub use member::*;

use crate::{DbIndex, GlobalId, LuaMemberOwner, LuaType, LuaTypeOwner};


fn get_owner_id(db: &DbIndex, type_owner: &LuaTypeOwner) -> Option<LuaMemberOwner> {
    let type_cache = db.get_type_index().get_type_cache(&type_owner)?;
    match type_cache.as_type() {
        LuaType::Ref(type_id) => Some(LuaMemberOwner::Type(type_id.clone())),
        LuaType::TableConst(id) => Some(LuaMemberOwner::Element(id.clone())),
        LuaType::Instance(inst) => Some(LuaMemberOwner::Element(inst.get_range().clone())),
        LuaType::GlobalTable(inst) => Some(LuaMemberOwner::GlobalId(GlobalId(inst.clone()))),
        _ => None,
    }
}
