use crate::{DbIndex, LuaMemberId, LuaMemberOwner};

// pub fn get_name_expr_member_owner(
//     db: &DbIndex,
//     file_id: FileId,
//     name_expr: &LuaNameExpr,
// ) -> Option<LuaMemberOwner> {
//     let decl_id = LuaDeclId::new(file_id, name_expr.get_position());
//     if let Some(owner) = get_decl_member_owner(&db, &decl_id) {
//         return Some(owner);
//     }

//     let decl_tree = db.get_decl_index().get_decl_tree(&file_id)?;
//     let name = name_expr.get_name_text()?;
//     let prev_decl = decl_tree.find_local_decl(&name, name_expr.get_position())?;

//     if !prev_decl.is_implicit_self() {
//         return get_decl_member_owner(db, &prev_decl.get_id());
//     }

//     let root = name_expr.get_root();
//     let syntax_id = prev_decl.get_syntax_id();
//     let token = syntax_id.to_token_from_root(&root)?;
//     let index_expr = LuaIndexExpr::cast(token.parent()?)?;
//     let LuaExpr::NameExpr(prefix_name_expr) = index_expr.get_prefix_expr()? else {
//         return None;
//     };

//     get_name_expr_member_owner(db, file_id, &prefix_name_expr)
// }

// pub fn get_decl_member_owner(db: &DbIndex, decl_id: &LuaDeclId) -> Option<LuaMemberOwner> {
//     if let Some(type_cache) = db.get_type_index().get_type_cache(&decl_id.clone().into()) {
//         let decl_type = type_cache.as_type();
//         match decl_type {
//             LuaType::Def(type_id) => {
//                 return Some(LuaMemberOwner::Type(type_id.clone()));
//             }
//             LuaType::GlobalTable(global_id) => {
//                 return Some(LuaMemberOwner::GlobalId(GlobalId(global_id.clone())));
//             }
//             LuaType::TableConst(table_const) => {
//                 return Some(LuaMemberOwner::Element(table_const.clone()));
//             }
//             LuaType::Instance(inst) => {
//                 return Some(LuaMemberOwner::Element(inst.get_range().clone()));
//             }
//             _ => return None,
//         }
//     }

//     let decl = db.get_decl_index().get_decl(decl_id)?;

//     if decl.is_global() {
//         return Some(LuaMemberOwner::GlobalId(GlobalId::new(decl.get_name())));
//     }

//     None
//     // Some(LuaMemberOwner::LocalDeclId(decl_id.clone()))
// }

pub fn add_member(db: &mut DbIndex, owner: LuaMemberOwner, member_id: LuaMemberId) -> Option<()> {
    db.get_member_index_mut()
        .set_member_owner(owner.clone(), member_id.file_id, member_id);
    db.get_member_index_mut()
        .add_member_to_owner(owner.clone(), member_id);

    Some(())
}
