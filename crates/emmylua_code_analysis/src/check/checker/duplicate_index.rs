//! # duplicate_index — duplicate keys in table literals
//!
//! Pure syntax check: duplicate keys in `{ a = 1, a = 2 }`.

use std::collections::HashMap;

use emmylua_parser::{LuaAstNode, LuaIndexKey, LuaTableExpr};

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct DuplicateIndexChecker;

impl Checker for DuplicateIndexChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::DuplicateIndex];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for table in root.descendants().filter_map(LuaTableExpr::cast) {
            check_table(context, &table);
        }
    }
}

fn check_table(context: &mut CheckContext<'_>, table: &LuaTableExpr) {
    let fields: Vec<(_, LuaIndexKey)> = table.get_fields_with_keys().into_iter().collect();
    if fields.len() > 50 {
        // Skip too many fields (performance).
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
                t!("Duplicate index `%{name}`.", name = name),
            );
        }
    }
}
