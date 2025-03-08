use rowan::TextRange;

use crate::{
    db_index::{DbIndex, LuaDeclId, LuaMemberId, LuaMemberOwner, LuaType, LuaTypeDeclId},
    InFiled,
};

pub fn merge_decl_expr_type(
    db: &mut DbIndex,
    decl_id: LuaDeclId,
    expr_type: LuaType,
) -> Option<()> {
    let decl = db.get_decl_index().get_decl(&decl_id)?;
    let decl_type = decl.get_type();
    if decl_type.is_none() {
        let decl = db.get_decl_index_mut().get_decl_mut(&decl_id)?;
        decl.set_decl_type(expr_type);
    } else {
        let decl_type = decl_type.unwrap();
        let new_type = merge_type(db, decl_type.clone(), expr_type);
        let decl = db.get_decl_index_mut().get_decl_mut(&decl_id)?;
        decl.set_decl_type(new_type);
    }

    Some(())
}

pub fn merge_member_type(
    db: &mut DbIndex,
    member_id: LuaMemberId,
    expr_type: LuaType,
) -> Option<()> {
    let member = db.get_member_index().get_member(&member_id)?;
    let member_type = member.get_decl_type();
    let new_type = merge_type(db, member_type.clone(), expr_type);
    let member = db.get_member_index_mut().get_member_mut(&member_id)?;
    member.set_decl_type(new_type);

    Some(())
}

fn merge_type(db: &mut DbIndex, decl_type: LuaType, expr_type: LuaType) -> LuaType {
    match &decl_type {
        LuaType::Unknown => expr_type,
        LuaType::Nil => LuaType::Nullable(expr_type.into()),
        LuaType::Def(def) => {
            match expr_type {
                LuaType::TableConst(in_filed_range) => {
                    merge_def_type_with_table(db, def.clone(), in_filed_range);
                }
                LuaType::Instance(instance) => {
                    let base_ref = instance.get_base();
                    match base_ref {
                        LuaType::TableConst(in_filed_range) => {
                            merge_def_type_with_table(db, def.clone(), in_filed_range.clone());
                        }
                        _ => {}
                    }
                }
                _ => {}
            }

            return LuaType::Def(def.clone());
        }
        _ => decl_type,
    }
}

fn merge_def_type_with_table(
    db: &mut DbIndex,
    def_id: LuaTypeDeclId,
    table_range: InFiled<TextRange>,
) -> Option<()> {
    let expr_member_owner = LuaMemberOwner::Element(table_range);
    let member_index = db.get_member_index_mut();
    let expr_members = member_index
        .get_member_map(expr_member_owner)?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let def_owner = LuaMemberOwner::Type(def_id);
    for member_id in expr_members {
        let member = member_index
            .get_member(&member_id)?
            .with_owner(def_owner.clone());
        member_index.add_member(member);
    }

    Some(())
}
