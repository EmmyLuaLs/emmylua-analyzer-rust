use crate::{GlobalId, LuaDeclId, compilation::analyzer::decl::DeclAnalyzer};
use emmylua_parser::{LuaAstNode, LuaExpr, LuaIndexExpr, LuaIndexKey, LuaNameExpr};

pub fn get_global_path(analyzer: &DeclAnalyzer, var_expr: LuaExpr) -> Option<GlobalId> {
    let mut prefix_expr;
    let mut paths = vec![];
    match var_expr {
        LuaExpr::NameExpr(name_expr) => {
            prefix_expr = LuaExpr::NameExpr(name_expr.clone());
        }
        LuaExpr::IndexExpr(index_expr) => {
            paths.push(index_expr.clone());
            prefix_expr = index_expr.get_prefix_expr()?;
        }
        _ => return None,
    }
    loop {
        match &prefix_expr {
            LuaExpr::NameExpr(name_expr) => {
                let global_id = get_name_expr_global_id(analyzer, &name_expr)?;
                let base_name = global_id.get_name();
                match paths.len() {
                    0 => return Some(global_id),
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
            LuaExpr::IndexExpr(index_expr) => {
                paths.push(index_expr.clone());
                prefix_expr = index_expr.get_prefix_expr()?;
            }
            _ => return None,
        }
    }
}

pub fn to_path_name(index_expr: &LuaIndexExpr) -> Option<String> {
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

fn get_name_expr_global_id(analyzer: &DeclAnalyzer, name_expr: &LuaNameExpr) -> Option<GlobalId> {
    let decl_id = LuaDeclId::new(analyzer.get_file_id(), name_expr.get_position());
    if let Some(decl) = analyzer.db.get_decl_index().get_decl(&decl_id) {
        if decl.is_global() {
            return Some(GlobalId::new(decl.get_name()));
        }
        return None;
    }

    let decl_tree = &analyzer.decl;
    let name = name_expr.get_name_text()?;
    let prev_decl = match decl_tree.find_local_decl(&name, name_expr.get_position()) {
        Some(decl) => decl,
        None => {
            return Some(GlobalId::new(&name));
        }
    };

    if !prev_decl.is_implicit_self() {
        if prev_decl.is_global() {
            return Some(GlobalId::new(prev_decl.get_name()));
        }
        return None;
    }

    let root = name_expr.get_root();
    let syntax_id = prev_decl.get_syntax_id();
    let token = syntax_id.to_token_from_root(&root)?;
    let index_expr = LuaIndexExpr::cast(token.parent()?)?;
    let LuaExpr::NameExpr(prefix_name_expr) = index_expr.get_prefix_expr()? else {
        return None;
    };

    get_name_expr_global_id(analyzer, &prefix_name_expr)
}
