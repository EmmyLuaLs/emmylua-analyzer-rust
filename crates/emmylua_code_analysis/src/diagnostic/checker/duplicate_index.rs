//! Duplicate index checker — salsa-native.

use hashbrown::HashMap;

use emmylua_parser::{LuaAstNode, LuaIndexKey, LuaTableExpr};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::DiagnosticContext;

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for table in root.descendants::<LuaTableExpr>() {
        check_table(context, table);
    }
}

fn check_table(context: &mut DiagnosticContext, table: LuaTableExpr) {
    let fields = table.get_fields_with_keys();
    if fields.len() > 50 {
        return;
    }
    let mut index_map: HashMap<String, Vec<LuaIndexKey>> = HashMap::new();
    for (_, key) in fields {
        index_map.entry(key.get_path_part()).or_default().push(key);
    }
    for (name, keys) in index_map {
        if keys.len() <= 1 {
            continue;
        }
        for key in keys {
            let Some(range) = key.get_range() else {
                continue;
            };
            context.add_diagnostic(
                DiagnosticCode::DuplicateIndex,
                range,
                t!("Duplicate index `%{name}`.", name = name).to_string(),
                None,
            );
        }
    }
}
