use std::collections::HashMap;

use emmylua_parser::{LuaAstNode, LuaIndexKey, LuaTableExpr};

use crate::{DiagnosticCode, SemanticModel};

use super::{Checker, DiagnosticContext};

pub struct DuplicateIndexChecker;

impl Checker for DuplicateIndexChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::DuplicateIndex];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for table in root.descendants::<LuaTableExpr>() {
            check_table_duplicate_index(context, semantic_model, table);
        }
    }
}

fn check_table_duplicate_index(
    context: &mut DiagnosticContext,
    _: &SemanticModel,
    table: LuaTableExpr,
) -> Option<()> {
    let fields = table.get_fields();
    let mut index_map: HashMap<String, Vec<LuaIndexKey>> = HashMap::new();

    for field in fields {
        let key = field.get_field_key();
        if let Some(key) = key {
            index_map
                .entry(key.get_path_part().to_string())
                .or_insert_with(Vec::new)
                .push(key);
        }
    }

    for (name, keys) in index_map {
        if keys.len() > 1 {
            for key in keys {
                let range = if let Some(range) = key.get_range() {
                    range
                } else {
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

    Some(())
}
