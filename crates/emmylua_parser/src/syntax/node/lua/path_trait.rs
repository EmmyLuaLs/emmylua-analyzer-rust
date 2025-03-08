use crate::LuaAstNode;

use super::{LuaExpr, LuaIndexKey};

pub trait PathTrait: LuaAstNode {
    fn get_access_path(&self) -> Option<String> {
        let mut paths = Vec::new();
        let mut current_node = self.syntax().clone();
        loop {
            match LuaExpr::cast(current_node)? {
                LuaExpr::NameExpr(name_expr) => {
                    let name = name_expr.get_name_text()?;
                    if paths.is_empty() {
                        return Some(name);
                    } else {
                        paths.push(name);
                        paths.reverse();
                        return Some(paths.join("."));
                    }
                }
                LuaExpr::CallExpr(call_expr) => {
                    let prefix_expr = call_expr.get_prefix_expr()?;
                    current_node = prefix_expr.syntax().clone();
                }
                LuaExpr::IndexExpr(index_expr) => {
                    match index_expr.get_index_key()? {
                        LuaIndexKey::String(s) => {
                            paths.push(s.get_value());
                        }
                        LuaIndexKey::Name(name) => {
                            paths.push(name.get_name_text().to_string());
                        }
                        LuaIndexKey::Integer(i) => {
                            paths.push(i.get_int_value().to_string());
                        }
                        LuaIndexKey::Expr(expr) => {
                            let text = format!("[{}]", expr.syntax().text());
                            paths.push(text);
                        }
                        LuaIndexKey::Idx(idx) => {
                            let text = format!("[{}]", idx);
                            paths.push(text);
                        }
                    }

                    current_node = index_expr.get_prefix_expr()?.syntax().clone();
                }
                _ => return None,
            }
        }
    }

    fn get_member_path(&self) -> Option<String> {
        let mut paths = Vec::new();
        let mut current_node = self.syntax().clone();
        loop {
            match LuaExpr::cast(current_node)? {
                LuaExpr::NameExpr(_) => {
                    if paths.is_empty() {
                        return None;
                    } else {
                        paths.reverse();
                        return Some(paths.join("."));
                    }
                }
                LuaExpr::CallExpr(call_expr) => {
                    let prefix_expr = call_expr.get_prefix_expr()?;
                    current_node = prefix_expr.syntax().clone();
                }
                LuaExpr::IndexExpr(index_expr) => {
                    let path_parts = index_expr.get_index_key()?.get_path_part();
                    paths.push(path_parts);

                    current_node = index_expr.get_prefix_expr()?.syntax().clone();
                }
                _ => return None,
            }
        }
    }
}
