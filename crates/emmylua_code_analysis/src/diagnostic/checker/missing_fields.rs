//! Missing fields checker — pure salsa.

use std::collections::HashSet;

use emmylua_parser::{LuaAstNode, LuaTableExpr};

use crate::compilation::SalsaPropertyKeySummary;
use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, LuaType};

use super::{DiagnosticContext, humanize_lint_type_salsa};

pub fn check(context: &mut DiagnosticContext, model: &SemanticModel) {
    let root = model.get_root().clone();
    for expr in root.descendants::<LuaTableExpr>() {
        check_table(context, model, &expr);
    }
}

fn check_table(context: &mut DiagnosticContext, model: &SemanticModel, expr: &LuaTableExpr) {
    let table_type = match model.infer_table_should_be(expr.clone()) {
        Some(LuaType::Union(u)) => {
            let types: Vec<LuaType> = u
                .into_vec()
                .into_iter()
                .filter(|t| {
                    matches!(
                        t,
                        LuaType::Ref(_) | LuaType::Object(_) | LuaType::Generic(_)
                    )
                })
                .collect();
            if types.len() != 1 {
                return;
            }
            types.into_iter().next().expect("Expected exactly one type")
        }
        Some(t) => t,
        None => return,
    };

    // Get declared fields
    let fields = expr.get_fields_with_keys();
    if fields.len() > 50 {
        return;
    }
    let current: HashSet<String> = fields.iter().map(|(_, k)| k.get_path_part()).collect();

    // Get required (non-nullable) fields from type
    let type_name = match &table_type {
        LuaType::Ref(id) | LuaType::Def(id) => id.get_name().to_string(),
        LuaType::Generic(g) => g.get_base_type_id().get_name().to_string(),
        _ => return,
    };

    let Some(members) = model.get_property_entries(&type_name) else {
        return;
    };

    let mut required: HashSet<String> = HashSet::new();
    let mut optional: HashSet<String> = HashSet::new();

    for m in &members {
        if m.is_nullable {
            optional.insert(key_name(&m.key));
        } else {
            required.insert(key_name(&m.key));
        }
    }
    // Remove optionals from required
    for name in &optional {
        required.remove(name);
    }

    let missing: Vec<String> = required
        .difference(&current)
        .map(|s| format!("`{}`", s))
        .collect();
    if !missing.is_empty() {
        context.add_diagnostic(
            DiagnosticCode::MissingFields,
            expr.get_range(),
            t!(
                "Missing required fields in type `%{typ}`: %{fields}",
                typ = humanize_lint_type_salsa(
                    context.get_salsa_db(),
                    context.get_file_id(),
                    &table_type
                ),
                fields = missing.join(", ")
            )
            .to_string(),
            None,
        );
    }
}

fn key_name(key: &SalsaPropertyKeySummary) -> String {
    match key {
        SalsaPropertyKeySummary::Name(n) => n.to_string(),
        SalsaPropertyKeySummary::Integer(i) => i.to_string(),
        _ => String::new(),
    }
}
