use emmylua_parser::{LuaAstNode, LuaNameExpr};

use crate::{DbIndex, FileId, GlobalId, LuaDeclId, LuaMemberOwner, LuaType};

pub fn get_name_expr_member_owner(db: &DbIndex, file_id: FileId, name_expr: &LuaNameExpr) -> Option<LuaMemberOwner> {
    let decl_id = LuaDeclId::new(file_id, name_expr.get_position());
    if let Some(owner) = get_decl_member_owner(&db, &decl_id) {
        return Some(owner);
    }

    let decl_tree = db.get_decl_index().get_decl_tree(&file_id)?;
    let name = name_expr.get_name_text()?;
    let prev_decl = decl_tree.find_local_decl(&name, name_expr.get_position())?;

    Some(LuaMemberOwner::DeclId(prev_decl.get_id()))
}

pub fn get_decl_member_owner(db: &DbIndex, decl_id: &LuaDeclId) -> Option<LuaMemberOwner> {
    if let Some(type_cache) = db.get_type_index().get_type_cache(&decl_id.clone().into()) {
        let decl_type = type_cache.as_type();
        match decl_type {
            LuaType::Def(type_id) => {
                return Some(LuaMemberOwner::Type(type_id.clone()));
            }
            LuaType::GlobalTable(global_id) => {
                return Some(LuaMemberOwner::Global(GlobalId(global_id.clone())));
            }
            LuaType::LocalDecl(decl_id) => {
                return Some(LuaMemberOwner::DeclId(decl_id.clone()));
            }

            _ => return None,
        }
    }

    let decl = db.get_decl_index().get_decl(decl_id)?;

    if decl.is_global() {
        return Some(LuaMemberOwner::Global(GlobalId::new(decl.get_name())));
    }

    Some(LuaMemberOwner::DeclId(decl_id.clone()))
}
