//! # goto_string — string template reference lookup
//!
//! `f("Name")` and `f`'s corresponding parameter has `StrTplRef` (`` `T` `` / `` `prefix.T` ``) →
//! compose the type name as "prefix + string + suffix" → type definition positions.
//! Mirrors the old `goto_str_tpl_ref_definition`, now via salsa queries only.

use emmylua_code_analysis::{LuaType, SalsaDatabase, SalsaSemanticModel, SemanticId};
use emmylua_parser::{LuaAstNode, LuaAstToken, LuaCallExpr, LuaExpr, LuaStringToken};
use lsp_types::{GotoDefinitionResponse, Location};

pub fn goto_str_tpl_ref_definition(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    string_token: LuaStringToken,
) -> Option<GotoDefinitionResponse> {
    let name = string_token.get_value();
    let call_expr = string_token.ancestors::<LuaCallExpr>().next()?;
    let arg_exprs = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
    let string_token_idx = arg_exprs.iter().position(|arg| {
        if let LuaExpr::LiteralExpr(literal_expr) = arg {
            literal_expr
                .syntax()
                .text_range()
                .contains(string_token.get_range().start())
        } else {
            false
        }
    })?;

    // Callee signature (doc parameter projection; cross-file declarations resolve through their file model).
    let fun = callee_signature(model, salsa, &call_expr)?;
    let params = fun.get_params();

    // Match StrTplRef directly; try union members one by one.
    let target_param = match (fun.is_colon_define(), call_expr.is_colon_call()) {
        (false, true) => params.get(string_token_idx + 1),
        (true, false) => {
            if string_token_idx > 0 {
                params.get(string_token_idx - 1)
            } else {
                None
            }
        }
        _ => params.get(string_token_idx),
    }?;

    // Match StrTplRef directly; try union members one by one.
    if let Some(locations) = try_extract_str_tpl_ref_locations(model, salsa, &target_param.1, &name)
    {
        return Some(GotoDefinitionResponse::Array(locations));
    }
    if let Some(LuaType::Union(union_type)) = target_param.1.clone() {
        for union_member in union_type.into_vec().iter() {
            if let Some(locations) =
                try_extract_str_tpl_ref_locations(model, salsa, &Some(union_member.clone()), &name)
            {
                return Some(GotoDefinitionResponse::Array(locations));
            }
        }
    }

    None
}

/// Callee's doc signature: name expression → declared closure (cross-file via the declaration file model); closure expression → itself.
fn callee_signature(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    call_expr: &LuaCallExpr,
) -> Option<emmylua_code_analysis::LuaFunctionType> {
    let prefix = call_expr.get_prefix_expr()?;
    match &prefix {
        LuaExpr::NameExpr(name_expr) => {
            let decl = model.resolve_name(name_expr.get_position())?;
            let SemanticId::Decl(key) = &decl else {
                return None;
            };
            // The declaration may be cross-file: use the declaration file's model for the signature.
            let decl_model = if key.file_id == model.file_id() {
                None
            } else {
                SalsaSemanticModel::new(salsa, key.file_id)
            };
            let decl_model = decl_model.as_ref().unwrap_or(model);
            let decls = decl_model.decls()?;
            let decl_info = decls.iter().find(|d| d.id == decl)?;
            let closure = decl_info.value_expr_syntax?;
            decl_model.type_of_signature(closure)
        }
        LuaExpr::ClosureExpr(closure) => model.type_of_signature(closure.get_syntax_id()),
        _ => None,
    }
}

fn try_extract_str_tpl_ref_locations(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    param_type: &Option<LuaType>,
    name: &str,
) -> Option<Vec<Location>> {
    let LuaType::StrTplRef(str_tpl) = param_type.as_ref()? else {
        return None;
    };
    let type_name = format!("{}{}{}", str_tpl.get_prefix(), name, str_tpl.get_suffix());

    // Resolve in the current file scope (namespace/using aware); cross-file resolution is guaranteed by the salsa index.
    let def = model.resolve_type_def(&type_name)?;
    let mut locations = Vec::new();
    for d in model.type_defs_in_scope(def_scope(&def), &type_name) {
        let Some(document) = salsa.document(d.file_id) else {
            continue;
        };
        let Some(uri) = document.get_uri() else {
            continue;
        };
        if let Some(range) = document.to_lsp_range(d.name_range) {
            locations.push(Location { uri, range });
        }
    }
    (!locations.is_empty()).then_some(locations)
}

fn def_scope(def: &emmylua_code_analysis::TypeDef) -> emmylua_code_analysis::TypeScope {
    match &def.id {
        SemanticId::TypeDef(key) => key.scope,
        _ => emmylua_code_analysis::TypeScope::Global,
    }
}
