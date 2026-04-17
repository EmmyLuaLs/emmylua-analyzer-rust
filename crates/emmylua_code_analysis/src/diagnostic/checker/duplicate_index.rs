use hashbrown::HashMap;

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
    let fields = table.get_fields_with_keys();
    if fields.len() > 50 {
        // Skip checking if there are too many fields to avoid performance issues
        return Some(());
    }

    let mut index_map: HashMap<String, Vec<LuaIndexKey>> = HashMap::new();

    for (_, key) in fields {
        index_map.entry(key.get_path_part()).or_default().push(key);
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
