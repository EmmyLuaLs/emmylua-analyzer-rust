use std::vec;

use emmylua_parser::{LuaAstNode, LuaExpr, LuaIndexExpr, LuaIndexKey, LuaNameExpr};

use crate::{DbIndex, FileId, GlobalId, LuaDeclId, LuaMemberOwner, LuaType};

// todo

pub fn get_name_expr_member_owner(
    db: &DbIndex,
    file_id: FileId,
    name_expr: &LuaNameExpr,
) -> Option<LuaMemberOwner> {
    let decl_id = LuaDeclId::new(file_id, name_expr.get_position());
    if let Some(owner) = get_decl_member_owner(&db, &decl_id) {
        return Some(owner);
    }

    let decl_tree = db.get_decl_index().get_decl_tree(&file_id)?;
    let name = name_expr.get_name_text()?;
    let prev_decl = decl_tree.find_local_decl(&name, name_expr.get_position())?;

    if !prev_decl.is_implicit_self() {
        return get_decl_member_owner(db, &prev_decl.get_id())
    }

    let root = name_expr.get_root();
    let syntax_id = prev_decl.get_syntax_id();
    let token = syntax_id.to_token_from_root(&root)?;
    let index_expr = LuaIndexExpr::cast(token.parent()?)?;
    let LuaExpr::NameExpr(prefix_name_expr) = index_expr.get_prefix_expr()? else {
        return None;
    };

    get_name_expr_member_owner(db, file_id, &prefix_name_expr)
}

pub fn get_decl_member_owner(db: &DbIndex, decl_id: &LuaDeclId) -> Option<LuaMemberOwner> {
    if let Some(type_cache) = db.get_type_index().get_type_cache(&decl_id.clone().into()) {
        let decl_type = type_cache.as_type();
        match decl_type {
            LuaType::Def(type_id) => {
                return Some(LuaMemberOwner::Type(type_id.clone()));
            }
            LuaType::GlobalTable(global_id) => {
                return Some(LuaMemberOwner::GlobalId(GlobalId(global_id.clone())));
            }
            LuaType::TableConst(table_const) => {
                return Some(LuaMemberOwner::Element(table_const.clone()));
            }
            LuaType::Instance(inst) => {
                return Some(LuaMemberOwner::Element(inst.get_range().clone()))
            }
            _ => return None,
        }
    }

    let decl = db.get_decl_index().get_decl(decl_id)?;

    if decl.is_global() {
        return Some(LuaMemberOwner::GlobalId(GlobalId::new(decl.get_name())));
    }

    None
    // Some(LuaMemberOwner::LocalDeclId(decl_id.clone()))
}

pub fn get_global_path(
    db: &DbIndex,
    file_id: FileId,
    index_expr: &LuaIndexExpr,
) -> Option<GlobalId> {
    let mut prefix_expr = index_expr.get_prefix_expr()?;
    let mut paths = vec![index_expr.clone()];
    loop {
        match &prefix_expr {
            LuaExpr::NameExpr(name_expr) => {
                let owner_id = get_name_expr_member_owner(db, file_id, &name_expr)?;
                match owner_id {
                    LuaMemberOwner::GlobalId(global_id) => {
                        let base_name = global_id.get_name();
                        match paths.len() {
                            0 => return Some(GlobalId::new(base_name)),
                            1 => {
                                if let Some(name) = to_path_name(&paths[0]) {
                                    return Some(GlobalId::new(&format!("{}.{}", base_name, name)));
                                } else {
                                    return None;
                                }
                            }
                            _ => {
                                let mut path = base_name.to_string();
                                for path_expr in paths.iter().rev() {
                                    if let Some(name) = to_path_name(path_expr) {
                                        // general this path is not too long
                                        path.push_str(&format!(".{}", name));
                                    }
                                }
                                return Some(GlobalId::new(&path));
                            }
                        }
                    }
                    _ => return None,
                };
            }
            LuaExpr::IndexExpr(index_expr) => {
                paths.push(index_expr.clone());
                prefix_expr = index_expr.get_prefix_expr()?;
            }
            _ => return None,
        }
    }
}

fn to_path_name(index_expr: &LuaIndexExpr) -> Option<String> {
    match index_expr.get_index_key()? {
        LuaIndexKey::String(s) => {
            return Some(s.get_value());
        }
        LuaIndexKey::Name(name) => {
            return Some(name.get_name_text().to_string());
        }
        LuaIndexKey::Integer(i) => {
            return Some(i.get_int_value().to_string());
        }
        LuaIndexKey::Idx(idx) => {
            let text = format!("[{}]", idx);
            return Some(text);
        }
        _ => return None,
    }
}
