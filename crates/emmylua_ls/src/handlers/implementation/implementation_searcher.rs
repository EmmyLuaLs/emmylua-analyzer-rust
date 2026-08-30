use emmylua_code_analysis::{DeclKind, SalsaDatabase, SalsaSemanticModel, SemanticId};
use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaDocTagField, LuaExpr, LuaFuncStat, LuaIndexExpr, LuaStat,
    LuaSyntaxToken, LuaTableField,
};
use lsp_types::Location;

use crate::handlers::common::{decl_reference_ranges, member_reference_ranges};

pub fn search_implementations(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    token: LuaSyntaxToken,
) -> Option<Vec<Location>> {
    let mut result = Vec::new();
    let semantic_decl = model.find_decl(token.into())?;
    match &semantic_decl {
        SemanticId::TypeDef(key) => {
            for def in model.type_defs_in_scope(key.scope, &key.full_name) {
                push_location(salsa, def.file_id, def.name_range, &mut result);
            }
        }
        SemanticId::Member(_) => {
            search_member_implementations(salsa, &semantic_decl, &mut result);
        }
        SemanticId::Decl(_) => {
            search_decl_implementations(salsa, &semantic_decl, &mut result);
        }
        _ => {}
    }
    Some(result)
}

fn search_member_implementations(
    salsa: &SalsaDatabase,
    member: &SemanticId,
    result: &mut Vec<Location>,
) -> Option<()> {
    let mut ranges = member_reference_ranges(salsa, member, true);
    // Other definition sites of the same member key (`@field` / table field / runtime assignment / method implementation) are also implementation positions.
    if let Some(key_text) = member_key_text(salsa, member) {
        for file_id in salsa.file_ids() {
            let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
                continue;
            };
            let Some(members) = model.members() else {
                continue;
            };
            for m in members.iter() {
                if m.key.to_path() == key_text
                    && let Some(key_range) = m.id.member_key_range()
                    && !ranges.contains(&(file_id, key_range))
                {
                    ranges.push((file_id, key_range));
                }
            }
        }
    }
    let mut signatures = Vec::new();
    let mut others = Vec::new();
    for (file_id, range) in ranges {
        let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
            continue;
        };
        if !is_implementation_position(&model, range) {
            continue;
        }
        // Signature implementations (method definitions) come first.
        let is_signature = is_signature_position(&model, range);
        let target = if is_signature {
            &mut signatures
        } else {
            &mut others
        };
        push_location(salsa, file_id, range, target);
    }
    signatures.append(&mut others);
    result.append(&mut signatures);
    Some(())
}

/// Whether a position is an implementation position: an index key that is a `function T:m()` method name or a `T.x = v` lvalue.
fn is_implementation_position(model: &SalsaSemanticModel<'_>, range: rowan::TextRange) -> bool {
    let Some(chunk) = model.chunk() else {
        return false;
    };
    let Some(token) = chunk.syntax().token_at_offset(range.start()).right_biased() else {
        return false;
    };
    let Some(parent) = token.parent() else {
        return false;
    };
    if let Some(index_expr) = LuaIndexExpr::cast(parent.clone()) {
        let Some(stat) = index_expr.ancestors::<LuaStat>().next() else {
            return false;
        };
        if matches!(stat, LuaStat::FuncStat(_)) {
            return true;
        }
        if let LuaStat::AssignStat(assign_stat) = &stat {
            let (vars, _) = assign_stat.get_var_and_expr_list();
            return vars.iter().any(|var| {
                var.syntax()
                    .text_range()
                    .contains(index_expr.syntax().text_range().start())
            });
        }
        return false;
    }
    // `@field x` / table field definition positions.
    if LuaDocTagField::can_cast(parent.kind().into()) {
        return true;
    }
    if let Some(table_field) = LuaTableField::cast(parent.clone()) {
        return table_field.is_assign_field();
    }
    false
}

/// Whether this is a method signature definition (a function member definition position).
fn is_signature_position(model: &SalsaSemanticModel<'_>, range: rowan::TextRange) -> bool {
    let Some(chunk) = model.chunk() else {
        return false;
    };
    let Some(token) = chunk.syntax().token_at_offset(range.start()).right_biased() else {
        return false;
    };
    let Some(parent) = token.parent() else {
        return false;
    };
    if let Some(index_expr) = LuaIndexExpr::cast(parent.clone()) {
        return index_expr.ancestors::<LuaFuncStat>().next().is_some();
    }
    false
}

fn search_decl_implementations(
    salsa: &SalsaDatabase,
    decl: &SemanticId,
    result: &mut Vec<Location>,
) -> Option<()> {
    // Implementation positions are declaration names plus assignment lvalues; plain reads are not implementations.
    let ranges = decl_reference_ranges(salsa, decl, true);
    for (file_id, range) in ranges {
        let Some(model) = SalsaSemanticModel::new(salsa, file_id) else {
            continue;
        };
        if is_decl_implementation_position(&model, decl, range) {
            push_location(salsa, file_id, range, result);
        }
    }
    // Same-name globals: partial classes / global variables may each use `x = {}` in multiple files,
    // and these assignments are implementation positions for the same global name.
    if let SemanticId::Decl(decl_key) = decl {
        let model = SalsaSemanticModel::new(salsa, decl_key.file_id)?;
        let decl_info = model.file_facts()?.decl_by_id(decl)?;
        if matches!(decl_info.kind, DeclKind::Global) {
            let name = decl_info.name.clone();
            for file_id in salsa.file_ids() {
                let Some(file_model) = SalsaSemanticModel::new(salsa, file_id) else {
                    continue;
                };
                let Some(facts) = file_model.file_facts() else {
                    continue;
                };
                for d in &facts.decls {
                    if d.name == name && matches!(d.kind, DeclKind::Global) {
                        push_location(salsa, file_id, d.name_range, result);
                    }
                }
            }
        }
    }
    Some(())
}

/// Whether a decl reference position is an "assignment implementation position" (the target of `x = v`, or the declaration name itself).
fn is_decl_implementation_position(
    model: &SalsaSemanticModel<'_>,
    decl: &SemanticId,
    range: rowan::TextRange,
) -> bool {
    if let SemanticId::Decl(key) = decl
        && range == key.name_range
    {
        return true;
    }
    let Some(chunk) = model.chunk() else {
        return false;
    };
    let Some(token) = chunk.syntax().token_at_offset(range.start()).right_biased() else {
        return false;
    };
    let Some(parent) = token.parent() else {
        return false;
    };
    let Some(LuaExpr::NameExpr(name_expr)) = LuaExpr::cast(parent) else {
        return false;
    };
    let Some(assign) = name_expr.syntax().parent().and_then(LuaAssignStat::cast) else {
        return false;
    };
    let (vars, _) = assign.get_var_and_expr_list();
    vars.iter()
        .any(|var| var.to_expr().get_syntax_id() == name_expr.get_syntax_id())
}

fn member_key_text(salsa: &SalsaDatabase, member: &SemanticId) -> Option<String> {
    let SemanticId::Member(key) = member else {
        return None;
    };
    let model = SalsaSemanticModel::new(salsa, key.file_id)?;
    let members = model.members()?;
    members
        .iter()
        .find(|m| &m.id == member)
        .map(|m| m.key.to_path())
}

fn push_location(
    salsa: &SalsaDatabase,
    file_id: emmylua_code_analysis::FileId,
    range: rowan::TextRange,
    result: &mut Vec<Location>,
) {
    let Some(document) = salsa.document(file_id) else {
        return;
    };
    let Some(uri) = document.get_uri() else {
        return;
    };
    let Some(lsp_range) = document.to_lsp_range(range) else {
        return;
    };
    let location = Location {
        uri,
        range: lsp_range,
    };
    if !result.contains(&location) {
        result.push(location);
    }
}
