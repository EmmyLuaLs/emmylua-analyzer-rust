//! # build_signature_helper — Salsa-based signature help
//!
//! Core subset: callee type → DocFunction → parameter/return labels + comma-count active parameter.
//! The old DbIndex version (`@overload` matching / operator calls / generic alias substitution / best parameter reordering)
//! has been retired; see `docs/SALSA_FROM_SCRATCH.md` §M3.

use emmylua_code_analysis::{LuaFunctionType, LuaType, SalsaSemanticModel};
use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr, LuaSyntaxToken, LuaTokenKind};
use lsp_types::{ParameterInformation, ParameterLabel, SignatureHelp, SignatureInformation};
use rowan::NodeOrToken;

use crate::handlers::hover::render;

pub fn build_signature_helper(
    model: &SalsaSemanticModel<'_>,
    call_expr: LuaCallExpr,
    token: LuaSyntaxToken,
) -> Option<SignatureHelp> {
    let prefix_expr = call_expr.get_prefix_expr()?;
    let prefix_type = model.type_of_expr(prefix_expr.get_syntax_id());
    let colon_call = call_expr.is_colon_call();
    let current_idx = get_current_param_index(&call_expr, &token)?;
    let prefix_text = prefix_expr.syntax().text().to_string();

    // `pcall(f, f-params...)`: std pcall has no usable doc signature, so synthesize from the callback signature.
    if matches!(
        &prefix_expr,
        LuaExpr::NameExpr(name)
            if matches!(name.get_name_text().as_deref(), Some("pcall") | Some("xpcall"))
    ) && let Some(help) = build_pcall_signature_help(model, &call_expr, current_idx)
    {
        return Some(help);
    }

    let help = match prefix_type {
        LuaType::DocFunction(func_type) => build_doc_function_signature_help(
            model,
            &func_type,
            colon_call,
            current_idx,
            &prefix_text,
        ),
        // Declared closure (name expression): query signatures.
        _ => match &prefix_expr {
            LuaExpr::NameExpr(name_expr) => {
                let decl = model.resolve_name(name_expr.get_position())?;
                let decls = model.decls()?;
                let decl_info = decls.iter().find(|d| d.id == decl)?;
                let closure = decl_info.value_expr_syntax?;
                let candidates = signature_candidates(model, closure);
                build_signature_help_candidates(
                    model,
                    &candidates,
                    colon_call,
                    current_idx,
                    &prefix_text,
                )
            }
            // Member call (`Action:id(...)` / `obj.method(...)`): resolve member declaration type.
            LuaExpr::IndexExpr(index_expr) => {
                let resolved = model.resolve_member(index_expr)?;
                let member_id = resolved.member_id?;
                let ty = model.type_of_member(&member_id)?;
                match ty {
                    LuaType::DocFunction(func_type) => build_doc_function_signature_help(
                        model,
                        &func_type,
                        colon_call,
                        current_idx,
                        &prefix_text,
                    ),
                    _ => {
                        // Runtime method (`function B:one()`): take signature from member closure.
                        let file_id = resolved.file_id?;
                        let facts = model.file_facts_of(file_id)?;
                        let member = facts.member_by_id(&member_id)?;
                        let closure = member.value_syntax?;
                        let signature = model.type_of_signature_in_file(file_id, closure)?;
                        build_doc_function_signature_help(
                            model,
                            &signature,
                            colon_call,
                            current_idx,
                            &prefix_text,
                        )
                    }
                }
            }
            _ => return None,
        },
    };
    help
}

/// Collect a closure's main signature plus `---@overload fun(...)` candidates.
fn signature_candidates(
    model: &SalsaSemanticModel<'_>,
    closure_syntax: emmylua_parser::LuaSyntaxId,
) -> Vec<LuaFunctionType> {
    let mut out = Vec::new();
    if let Some(main) = model.type_of_signature(closure_syntax) {
        out.push(main);
    }
    if let Some(signatures) = model.signatures()
        && let Some(sig) = signatures
            .iter()
            .find(|s| s.closure_syntax == closure_syntax)
        && let Some(docs) = sig.docs.as_ref()
    {
        for syntax in &docs.overloads {
            if let LuaType::DocFunction(fun) =
                model.doc_type_lua_in(model.file_id(), *syntax, &docs.generic_params)
            {
                out.push(fun.as_ref().clone());
            }
        }
    }
    out
}

/// Render multiple signature candidates and pick the one matching the current parameter position as active.
fn build_signature_help_candidates(
    model: &SalsaSemanticModel<'_>,
    candidates: &[LuaFunctionType],
    colon_call: bool,
    current_idx: usize,
    prefix_text: &str,
) -> Option<SignatureHelp> {
    let mut signatures = Vec::new();
    let mut active = 0usize;
    for (i, fun) in candidates.iter().enumerate() {
        if let Some(help) =
            build_doc_function_signature_help(model, fun, colon_call, current_idx, prefix_text)
            && let Some(info) = help.signatures.into_iter().next()
        {
            if fun.get_params().len() > current_idx {
                active = i;
            }
            signatures.push(info);
        }
    }
    if signatures.is_empty() {
        return None;
    }
    Some(SignatureHelp {
        signatures,
        active_signature: Some(active as u32),
        active_parameter: Some(current_idx as u32),
    })
}

/// `pcall(f, f's args...)` / `xpcall(f, err, f's args...)`:
/// Synthesize help from the first argument's function signature (the args after f are passed through to that function).
fn build_pcall_signature_help(
    model: &SalsaSemanticModel<'_>,
    call_expr: &LuaCallExpr,
    current_idx: usize,
) -> Option<SignatureHelp> {
    let args = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
    let callback = args.first()?;
    let callback_ty = model.type_of_expr(callback.get_syntax_id());
    let LuaType::DocFunction(callback_fun) = callback_ty else {
        return None;
    };

    let callback_rendered = {
        let params = callback_fun
            .get_params()
            .iter()
            .map(|(name, ty)| match ty {
                Some(ty) => format!("{name}: {}", render_type(model, ty)),
                None => name.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        if matches!(callback_fun.get_ret(), LuaType::Unknown) {
            format!("fun({params})")
        } else {
            render_type(model, &LuaType::DocFunction(callback_fun.clone()))
        }
    };
    let mut param_infos = vec![ParameterInformation {
        label: ParameterLabel::Simple(format!("f: sync {callback_rendered}")),
        documentation: None,
    }];
    let mut label = format!("pcall(f: sync {callback_rendered}");
    for (name, ty) in callback_fun.get_params() {
        let text = match ty {
            Some(ty) => format!("{name}: {}", render_type(model, ty)),
            None => name.clone(),
        };
        label.push_str(", ");
        label.push_str(&text);
        param_infos.push(ParameterInformation {
            label: ParameterLabel::Simple(text),
            documentation: None,
        });
    }
    label.push_str("): (true|false)");

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: None,
            parameters: Some(param_infos),
            active_parameter: Some(current_idx as u32),
        }],
        active_signature: Some(0),
        active_parameter: Some(current_idx as u32),
    })
}

fn build_doc_function_signature_help(
    model: &SalsaSemanticModel<'_>,
    func_type: &LuaFunctionType,
    colon_call: bool,
    current_idx: usize,
    prefix_text: &str,
) -> Option<SignatureHelp> {
    let params = func_type.get_params().to_vec();
    // Parameter information.
    let mut param_infos = vec![];
    for (index, (name, ty)) in params.iter().enumerate() {
        let param_name = if name.is_empty() {
            format!("arg{}", index)
        } else {
            name.clone()
        };
        let label = match ty {
            Some(ty) => format!("{}: {}", param_name, render_type(model, ty)),
            None => param_name,
        };
        param_infos.push(ParameterInformation {
            label: ParameterLabel::Simple(label),
            documentation: None,
        });
    }

    // Self slot adjustment (mirrors old semantics).
    if let (false, true) = (func_type.is_colon_define(), colon_call) {
        if !param_infos.is_empty() {
            param_infos.remove(0);
        }
    }

    let mut active = current_idx;
    if let Some((name, _)) = params.last()
        && name == "..."
        && current_idx >= params.len()
    {
        active = params.len() - 1;
    }

    let mut label = prefix_text.to_string();
    label.push('(');
    label.push_str(
        &param_infos
            .iter()
            .map(|info| match &info.label {
                ParameterLabel::Simple(label) => label.clone(),
                ParameterLabel::LabelOffsets(_) => String::new(),
            })
            .collect::<Vec<_>>()
            .join(", "),
    );
    label.push(')');
    match func_type.get_ret() {
        LuaType::Nil => {}
        ret => {
            label.push_str(": ");
            label.push_str(&render_type(model, ret));
        }
    }

    let signature_info = SignatureInformation {
        label,
        documentation: None,
        parameters: Some(param_infos),
        active_parameter: Some(active as u32),
    };

    Some(SignatureHelp {
        signatures: vec![signature_info],
        active_signature: Some(0),
        active_parameter: Some(active as u32),
    })
}

fn render_type(model: &SalsaSemanticModel<'_>, ty: &LuaType) -> String {
    render::humanize(model, ty)
}

pub fn get_current_param_index(call_expr: &LuaCallExpr, token: &LuaSyntaxToken) -> Option<usize> {
    let arg_list = call_expr.get_args_list()?;
    let mut current_idx = 0;
    let token_position = token.text_range().start();
    for node_or_token in arg_list.syntax().children_with_tokens() {
        if let NodeOrToken::Token(token) = node_or_token
            && token.kind() == LuaTokenKind::TkComma.into()
            && token.text_range().start() <= token_position
        {
            current_idx += 1;
        }
    }

    Some(current_idx)
}
