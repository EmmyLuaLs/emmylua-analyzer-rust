use emmylua_parser::{LuaAstNode, LuaExpr, LuaIndexExpr, PathTrait};
use smol_str::SmolStr;

use crate::{GlobalId, LuaMemberOwner};

use super::DeclAnalyzer;

pub fn find_index_owner(
    analyzer: &mut DeclAnalyzer,
    index_expr: LuaIndexExpr,
) -> (LuaMemberOwner, Option<GlobalId>) {
    if is_in_global_member(analyzer, &index_expr).unwrap_or(false) {
        if let Some(prefix_expr) = index_expr.get_prefix_expr() {
            match prefix_expr {
                LuaExpr::IndexExpr(parent_index_expr) => {
                    if let Some(parent_access_path) = parent_index_expr.get_access_path() {
                        if let Some(access_path) = index_expr.get_access_path() {
                            return (
                                LuaMemberOwner::GlobalPath(GlobalId(
                                    SmolStr::new(parent_access_path).into(),
                                )),
                                Some(GlobalId(SmolStr::new(access_path).into())),
                            );
                        }

                        return (
                            LuaMemberOwner::GlobalPath(GlobalId(
                                SmolStr::new(parent_access_path).into(),
                            )),
                            None,
                        );
                    }
                }
                LuaExpr::NameExpr(name) => {
                    if let Some(parent_path) = name.get_name_text() {
                        if parent_path == "self" {
                            return (LuaMemberOwner::LocalUnresolve, None);
                        }

                        if let Some(access_path) = index_expr.get_access_path() {
                            return (
                                LuaMemberOwner::GlobalPath(GlobalId(
                                    SmolStr::new(parent_path).into(),
                                )),
                                Some(GlobalId(SmolStr::new(access_path).into())),
                            );
                        }

                        return (
                            LuaMemberOwner::GlobalPath(GlobalId(SmolStr::new(parent_path).into())),
                            None,
                        );
                    }
                }
                _ => {}
            }
        } else {
            if let Some(access_path) = index_expr.get_access_path() {
                return (
                    LuaMemberOwner::LocalUnresolve,
                    Some(GlobalId(SmolStr::new(access_path).into())),
                );
            }
        }
    }

    (LuaMemberOwner::LocalUnresolve, None)
}

fn is_in_global_member(analyzer: &DeclAnalyzer, index_expr: &LuaIndexExpr) -> Option<bool> {
    let prefix = index_expr.get_prefix_expr()?;
    match prefix {
        LuaExpr::IndexExpr(index_expr) => {
            return is_in_global_member(analyzer, &index_expr);
        }
        LuaExpr::NameExpr(name) => {
            let name_text = name.get_name_text()?;
            if name_text == "self" {
                return Some(false);
            }

            let decl = analyzer.find_decl(&name_text, name.get_position());
            return Some(decl.is_none());
        }
        _ => {}
    }
    None
}
