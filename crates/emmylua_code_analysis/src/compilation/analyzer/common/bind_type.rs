use rowan::TextRange;

use crate::{
    InFiled, LuaMemberId, LuaTypeCache, LuaTypeOwner,
    compilation::analyzer::common::migrate_global_member::migrate_global_members_when_type_resolve,
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
        if type_cache.is_infer() {
            if let LuaTypeOwner::Decl(decl_id) = &type_owner {
                if let Some(refs) = db
                    .get_reference_index()
                    .get_decl_references(&decl_id.file_id, decl_id)
                {
                    if refs.iter().any(|it| it.is_write) {
                        match &type_cache.as_type() {
                            LuaType::IntegerConst(_) => {
                                type_cache = LuaTypeCache::InferType(LuaType::Integer)
                            }
                            LuaType::StringConst(_) => {
                                type_cache = LuaTypeCache::InferType(LuaType::String)
                            }
                            LuaType::BooleanConst(_) => {
                                type_cache = LuaTypeCache::InferType(LuaType::Boolean)
                            }
                            LuaType::FloatConst(_) => {
                                type_cache = LuaTypeCache::InferType(LuaType::Number)
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if db
            .get_type_index_mut()
            .bind_type(type_owner.clone(), type_cache)
        {
            migrate_global_members_when_type_resolve(db, type_owner);
        }
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

    match &decl_type {
        LuaType::Def(def) => match &expr_type {
            LuaType::TableConst(in_filed_range) => {
                merge_def_type_with_table(db, def.clone(), in_filed_range.clone());
            }
            LuaType::Instance(instance) => {
                let base_ref = instance.get_base();
                merge_def_type(db, base_ref.clone(), expr_type, merge_level + 1);
            }
            _ => {}
        },
        _ => {}
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
    let old_member_owner = db.get_member_index().get_current_owner(&member_id);
    if let Some(old_owner) = old_member_owner {
        if old_owner == &owner {
            return None; // Already exists
        }
    }

    db.get_member_index_mut()
        .set_member_owner(owner.clone(), member_id.file_id, member_id);
    db.get_member_index_mut()
        .add_member_to_owner(owner.clone(), member_id);

    Some(())
}
