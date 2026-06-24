mod migrate_global_member;
use migrate_global_member::migrate_global_members_when_type_resolve;
use rowan::TextRange;

use crate::{
    InFiled, LuaMemberId, LuaTypeCache, LuaTypeOwner,
    db_index::{DbIndex, LuaMemberOwner, LuaType, LuaTypeDeclId},
};

pub fn bind_type(
    db: &mut DbIndex,
    type_owner: LuaTypeOwner,
    mut type_cache: LuaTypeCache,
) -> Option<()> {
    let decl_type_cache = db.get_type_index().get_type_cache(&type_owner);

    if decl_type_cache.is_none() {
        // type backward
        if type_cache.is_infer()
            && let LuaTypeOwner::Decl(decl_id) = &type_owner
            && let Some(decl_ref) = db
                .get_reference_index()
                .get_decl_references(&decl_id.file_id, decl_id)
            && decl_ref.mutable
        {
            match &type_cache.as_type() {
                LuaType::IntegerConst(_) => type_cache = LuaTypeCache::InferType(LuaType::Integer),
                LuaType::StringConst(_) => type_cache = LuaTypeCache::InferType(LuaType::String),
                LuaType::BooleanConst(_) => type_cache = LuaTypeCache::InferType(LuaType::Boolean),
                LuaType::FloatConst(_) => type_cache = LuaTypeCache::InferType(LuaType::Number),
                _ => {}
            }
        }

        db.get_type_index_mut()
            .bind_type(type_owner.clone(), type_cache);
        migrate_global_members_when_type_resolve(db, type_owner);
    } else {
        let decl_type = decl_type_cache?.as_type();
        merge_def_type(db, decl_type.clone(), type_cache.as_type().clone(), 0);
    }

    Some(())
}

fn merge_def_type(db: &mut DbIndex, decl_type: LuaType, expr_type: LuaType, merge_level: i32) {
    if merge_level > 1 {
        return;
    }

    if let LuaType::Def(def) = &decl_type {
        match &expr_type {
            LuaType::TableConst(in_filed_range) => {
                merge_def_type_with_table(db, def.clone(), in_filed_range.clone());
            }
            LuaType::Instance(instance) => {
                let base_ref = instance.get_base();
                merge_def_type(db, base_ref.clone(), expr_type, merge_level + 1);
            }
            _ => {}
        }
    }
}

fn merge_def_type_with_table(
    db: &mut DbIndex,
    def_id: LuaTypeDeclId,
    table_range: InFiled<TextRange>,
) -> Option<()> {
    let expr_member_owner = LuaMemberOwner::Element(table_range);
    let member_index = db.get_member_index_mut();
    let expr_member_ids = member_index
        .get_members(&expr_member_owner)?
        .iter()
        .map(|member| member.get_id())
        .collect::<Vec<_>>();
    let def_owner = LuaMemberOwner::Type(def_id);
    for table_member_id in expr_member_ids {
        add_member(db, def_owner.clone(), table_member_id);
    }

    Some(())
}

pub fn add_member(db: &mut DbIndex, owner: LuaMemberOwner, member_id: LuaMemberId) -> Option<()> {
    db.get_member_index_mut()
        .set_member_owner(owner.clone(), member_id.file_id, member_id);
    db.get_member_index_mut()
        .add_member_to_owner(owner.clone(), member_id);

    Some(())
}

fn get_owner_id(db: &DbIndex, type_owner: &LuaTypeOwner) -> Option<LuaMemberOwner> {
    let type_cache = db.get_type_index().get_type_cache(type_owner)?;
    match type_cache.as_type() {
        LuaType::Ref(type_id) => Some(LuaMemberOwner::Type(type_id.clone())),
        LuaType::TableConst(id) => Some(LuaMemberOwner::Element(id.clone())),
        LuaType::Instance(inst) => Some(LuaMemberOwner::Element(inst.get_range().clone())),
        _ => None,
    }
}
