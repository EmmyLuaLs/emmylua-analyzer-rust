use std::collections::HashMap;

use emmylua_code_analysis::{
    Decl, DeclKind, GenericTplId, LuaFunctionType, LuaMemberKey, LuaType, LuaTypeDeclId, Member,
    SalsaDatabase, SalsaMemberInfo, SalsaSemanticModel, SemanticId, TypeDef, TypeDefKind,
    VariadicType, first_param_may_not_self,
};
use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaCallExpr, LuaDocAttributeUse, LuaDocDescriptionOwner, LuaDocType,
    LuaExpr, LuaIndexExpr, LuaLocalStat, LuaSyntaxToken, LuaTableExpr, LuaTableField,
};
use lsp_types::{Hover, HoverContents, MarkupContent};
use rowan::TextRange;

use crate::handlers::common::{resolve_alias_origin, semantic_id_file};
use crate::handlers::completion::providers::callable_candidates;
use crate::handlers::hover::desc::HoverDescription;

use super::desc::{
    decl_description, decl_signature_tags, member_description, member_signature_tags,
    type_def_description,
};
use super::render::humanize;

/// Context projection result for table fields / value aliases: `show_unknown` marks
/// unbound generics that were substituted with `unknown`, so member function signatures
/// can explicitly render `unknown`.
struct MemberContextInfo {
    info: SalsaMemberInfo,
    show_unknown: bool,
}

pub fn build_semantic_info_hover(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    token: LuaSyntaxToken,
    range: TextRange,
) -> Option<Hover> {
    let info = model.semantic_info(token.clone().into())?;
    let member_call_typ = if matches!(info.decl, Some(SemanticId::Member(_)))
        && token
            .parent_ancestors()
            .any(|node| LuaCallExpr::cast(node).is_some())
    {
        member_type_at_token(model, &token)
    } else {
        None
    };
    let decl_call_fun = if matches!(info.decl, Some(SemanticId::Decl(_))) {
        token
            .parent_ancestors()
            .find_map(LuaCallExpr::cast)
            .and_then(|call| {
                if token_is_direct_call_callee(&token, &call) {
                    model.inferred_call_doc_function(call.get_syntax_id())
                } else {
                    None
                }
            })
    } else {
        None
    };
    let decl_call_bindings = if matches!(info.decl, Some(SemanticId::Decl(_))) {
        token
            .parent_ancestors()
            .find_map(LuaCallExpr::cast)
            .and_then(|call| {
                if token_is_direct_call_callee(&token, &call) {
                    model.inferred_call_bindings(call.get_syntax_id())
                } else {
                    None
                }
            })
    } else {
        None
    };
    let member_context_infos = info
        .decl
        .as_ref()
        .map(|decl| member_context_infos(model, decl, &token))
        .unwrap_or_default();
    let (mut code, description) = if let Some(decl) = info.decl.as_ref() {
        // Alias tracing: `local f = t.func` / `local a = b` show the signature and comments of the real function definition.
        let origin = resolve_alias_origin(model, decl).unwrap_or_else(|| decl.clone());
        if let Some(origin_model) =
            semantic_id_file(&origin).and_then(|file_id| model.model_for(file_id))
        {
            let origin_is_param = if let SemanticId::Decl(origin_key) = &origin {
                origin_model
                    .file_facts_of(origin_key.file_id)
                    .and_then(|facts| facts.decl_by_id(&origin))
                    .is_some_and(|decl| matches!(decl.kind, DeclKind::Param))
            } else {
                false
            };
            let hover_typ = if &origin == decl || origin_is_param {
                info.typ.clone()
            } else {
                semantic_type_of(&origin_model, &origin)
            };
            match &origin {
                SemanticId::Decl(_) => build_decl_hover(
                    &origin_model,
                    &hover_typ,
                    &origin,
                    decl_call_fun.as_ref(),
                    decl_call_bindings.as_ref(),
                ),
                SemanticId::Member(_) => build_member_hover(
                    &origin_model,
                    member_call_typ.as_ref().unwrap_or(&hover_typ),
                    &origin,
                    member_call_typ.is_some(),
                    member_context_infos.as_slice(),
                ),
                SemanticId::TypeDef(_) => {
                    build_type_hover(&origin_model, &hover_typ, &origin, &token)
                }
                _ => (humanize(&origin_model, &hover_typ), None),
            }
        } else {
            match decl {
                SemanticId::Decl(_) => build_decl_hover(
                    model,
                    &info.typ,
                    decl,
                    decl_call_fun.as_ref(),
                    decl_call_bindings.as_ref(),
                ),
                SemanticId::Member(_) => build_member_hover(
                    model,
                    member_call_typ.as_ref().unwrap_or(&info.typ),
                    decl,
                    member_call_typ.is_some(),
                    member_context_infos.as_slice(),
                ),
                SemanticId::TypeDef(_) => build_type_hover(model, &info.typ, decl, &token),
                _ => (humanize(model, &info.typ), None),
            }
        }
    } else {
        (humanize(model, &info.typ), None)
    };

    // `pcall(foo)` synthesizes the full display from the call-site callback signature (covering std's anonymous `...` signature).
    let is_pcall_hover = build_pcall_hover_code(model, &token).is_some();
    if let Some(pcall_code) = build_pcall_hover_code(model, &token) {
        code = pcall_code;
    }

    let mut value = format!("```lua\n{}\n```", code);
    if let Some(mut description) = description {
        if let Some(owner_line) = &description.owner_line {
            value.push_str("\n\n");
            value.push_str(owner_line);
        }
        let extra_tags = description.tags.clone();
        if let Some(decl) = info.decl.as_ref() {
            let origin = resolve_alias_origin(model, decl).unwrap_or_else(|| decl.clone());
            let origin_model =
                semantic_id_file(&origin).and_then(|file_id| model.model_for(file_id));
            let tags = match &origin {
                SemanticId::Decl(_) => origin_model
                    .as_ref()
                    .filter(|m| decl_should_show_signature_tags(m, &origin))
                    .map(|m| decl_signature_tags(m, &origin))
                    .unwrap_or_default(),
                SemanticId::Member(_) => origin_model
                    .as_ref()
                    .map(|m| member_signature_tags(m, &origin))
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            description.tags.extend(tags);
        }
        if description.text.is_some() || !description.tags.is_empty() {
            value.push_str("\n\n---");
        }
        let has_description_text = description.text.is_some();
        if let Some(ref text) = description.text
            && !text.is_empty()
        {
            value.push_str("\n\n");
            value.push_str(text);
        }
        let mut prev_is_param = false;
        if has_description_text && !extra_tags.is_empty() {
            value.push_str("\n\n---");
        }
        for tag in &description.tags {
            if prev_is_param && tag.starts_with("@*return") {
                value.push_str("\n\n");
            }
            value.push_str("\n\n");
            value.push_str(tag);
            prev_is_param = tag.starts_with("@*param");
        }
        if !description.overload_blocks.is_empty() {
            if description.text.is_some() || !description.tags.is_empty() {
                value.push_str("\n\n---");
            } else {
                value.push_str("\n\n---\n\n---");
            }
            for block in description.overload_blocks {
                value.push_str("\n\n");
                value.push_str(&block);
            }
        }
    }

    if is_pcall_hover {
        // Backward-compatible hover: the pcall description area gets an extra blank line, and `R...` is shown as `R ...`.
        value = value.replace("\n\n---\n\nCalls function", "\n\n---\n\n\nCalls function");
        value = value.replace("`true, R...`", "`true, R ...`");
    }

    let document = salsa.document(model.file_id())?;
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value,
        }),
        range: document.to_lsp_range(range),
    })
}

/// Whether the token is exactly the call callee's bare function name (`f(...)`), not a method-call receiver/argument.
fn token_is_direct_call_callee(token: &LuaSyntaxToken, call: &LuaCallExpr) -> bool {
    let Some(prefix) = call.get_prefix_expr() else {
        return false;
    };
    match prefix {
        LuaExpr::NameExpr(name_expr) => name_expr.get_range().contains(token.text_range().start()),
        _ => false,
    }
}

fn semantic_type_of(model: &SalsaSemanticModel<'_>, id: &SemanticId) -> LuaType {
    match id {
        SemanticId::Decl(_) => model.type_of_decl(id).unwrap_or(LuaType::Unknown),
        SemanticId::Member(_) => model.type_of_member(id).unwrap_or(LuaType::Unknown),
        SemanticId::TypeDef(_) => LuaType::Unknown,
        _ => LuaType::Unknown,
    }
}

/// Context projection for member hover: value aliases like `local x = obj.field` or table field
/// initializers like `{ field = ... }` resolve members using the prefix/expected type at the
/// declaration or call site, producing member info with generics substituted.
fn member_context_infos(
    model: &SalsaSemanticModel<'_>,
    decl: &SemanticId,
    token: &LuaSyntaxToken,
) -> Vec<MemberContextInfo> {
    // 1. Value alias: `local f = t.func` -> project members from the prefix type of `t.func`.
    if let SemanticId::Decl(decl_key) = decl {
        let Some(facts) = model.file_facts_of(decl_key.file_id) else {
            return Vec::new();
        };
        let Some(decl_info) = facts.decl_by_id(decl) else {
            return Vec::new();
        };
        let Some(value_syntax) = decl_info.value_expr_syntax else {
            return Vec::new();
        };
        let Some(tree) = model.syntax_tree_of(decl_key.file_id) else {
            return Vec::new();
        };
        let Some(node) = value_syntax.to_node_from_root(&tree.get_red_root()) else {
            return Vec::new();
        };
        let Some(index_expr) = LuaIndexExpr::cast(node) else {
            return Vec::new();
        };
        let Some(resolved) = model.resolve_member(&index_expr) else {
            return Vec::new();
        };
        let Some(prefix) = index_expr.get_prefix_expr() else {
            return Vec::new();
        };
        let prefix_ty = model.type_of_expr(prefix.get_syntax_id());
        let key = LuaMemberKey::Name(resolved.name.clone());
        return model
            .member_infos_with_key_all(&prefix_ty, &key)
            .into_iter()
            .map(|info| MemberContextInfo {
                info,
                show_unknown: false,
            })
            .collect();
    }

    // 2. Table field initializer: `{ field = ... }` -> project by the expected type from the enclosing local declaration or call argument.
    if let SemanticId::Member(member_key) = decl {
        let Some(facts) = model.file_facts_of(member_key.file_id) else {
            return Vec::new();
        };
        let Some(member) = facts.member_by_id(decl) else {
            return Vec::new();
        };
        if let Some(context_ty) = table_context_type_for_token(model, token) {
            let mut infos = model
                .member_infos_with_key_all(&context_ty, &member.key)
                .into_iter()
                .map(|info| MemberContextInfo {
                    info,
                    show_unknown: false,
                })
                .collect::<Vec<_>>();
            if context_ty.contain_tpl() {
                // When the generic context cannot be inferred from the call site, bind generics in the
                // type-surface member using the field value's own `---@param` signatures
                // (`fun(value:T)->T` + `@param value string`
                // -> `fun(value:string)->string`); when there is no source to bind, show `unknown`.
                let runtime_fun = member_signature(model, &member);
                infos = infos
                    .into_iter()
                    .map(|ctx| {
                        let had_tpl = ctx.info.typ.contain_tpl();
                        let (typ, show_unknown) = match (&ctx.info.typ, &runtime_fun) {
                            (LuaType::DocFunction(type_fun), Some(runtime_fun)) => {
                                match bind_runtime_field_generics(model, runtime_fun, type_fun) {
                                    Some(typ) => (typ, false),
                                    None if had_tpl => (
                                        substitute_class_generics_to_unknown(
                                            model,
                                            &context_ty,
                                            ctx.info.typ,
                                        ),
                                        true,
                                    ),
                                    None => (ctx.info.typ, false),
                                }
                            }
                            _ if had_tpl => (
                                substitute_class_generics_to_unknown(
                                    model,
                                    &context_ty,
                                    ctx.info.typ,
                                ),
                                true,
                            ),
                            _ => (ctx.info.typ, false),
                        };
                        MemberContextInfo {
                            info: SalsaMemberInfo { typ, ..ctx.info },
                            show_unknown,
                        }
                    })
                    .collect();
            }
            return infos;
        }
    }

    Vec::new()
}

/// Bind direct `TplRef` generics in a type-surface member using the runtime table field function's `@param` types.
fn bind_runtime_field_generics(
    model: &SalsaSemanticModel<'_>,
    runtime_fun: &LuaFunctionType,
    type_fun: &LuaFunctionType,
) -> Option<LuaType> {
    let mut names = Vec::<String>::new();
    let mut values = Vec::<LuaType>::new();
    for (runtime_name, runtime_ty) in runtime_fun.get_params() {
        let Some(runtime_ty) = runtime_ty else {
            continue;
        };
        let Some((_, type_ty)) = type_fun
            .get_params()
            .iter()
            .find(|(name, _)| name == runtime_name)
        else {
            continue;
        };
        let Some(type_ty) = type_ty else {
            continue;
        };
        if let LuaType::TplRef(tpl) = type_ty {
            let name = tpl.get_name().to_string();
            if !names.contains(&name) {
                names.push(name);
                values.push(runtime_ty.clone());
            }
        }
    }
    if names.is_empty() {
        return None;
    }
    let name_refs = names.iter().map(String::as_str).collect::<Vec<_>>();
    Some(model.substitute_generic_params_named_str(
        &LuaType::DocFunction(std::sync::Arc::new(type_fun.clone())),
        &values,
        &name_refs,
    ))
}

/// Replace unbound class generics (`T`/`TplRef`) in a generic context with `unknown`.
fn substitute_class_generics_to_unknown(
    model: &SalsaSemanticModel<'_>,
    context_ty: &LuaType,
    ty: LuaType,
) -> LuaType {
    if !ty.contain_tpl() {
        return ty;
    }
    let names = match context_ty {
        LuaType::Generic(generic) => model
            .type_def_of(generic.get_base_type_id_ref())
            .map(|def| {
                def.generic_params
                    .iter()
                    .map(|param| param.name.to_string())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    if names.is_empty() {
        return ty;
    }
    let params = vec![LuaType::Unknown; names.len()];
    let name_refs = names.iter().map(String::as_str).collect::<Vec<_>>();
    model.substitute_generic_params_named_str(&ty, &params, &name_refs)
}

fn table_context_type_for_token(
    model: &SalsaSemanticModel<'_>,
    token: &LuaSyntaxToken,
) -> Option<LuaType> {
    let field = token.parent_ancestors().find_map(LuaTableField::cast)?;
    let table_expr = field.ancestors::<LuaTableExpr>().next()?;
    table_context_type_for_expr(model, &table_expr)
}

fn table_context_type_for_expr(
    model: &SalsaSemanticModel<'_>,
    table_expr: &LuaTableExpr,
) -> Option<LuaType> {
    let syntax_id = table_expr.get_syntax_id();

    // `---@type T local t = { ... }`: type from the local declaration.
    if let Some(local) = table_expr.ancestors::<LuaLocalStat>().next() {
        if local
            .get_value_exprs()
            .any(|expr| expr.get_syntax_id() == syntax_id)
            && let Some(name) = local.get_local_name_list().next()
            && let Some(decl) = model.decl_by_offset(name.get_position())
        {
            if let Some(ty) = model.type_of_decl(&decl)
                && !matches!(ty, LuaType::Unknown)
            {
                return Some(ty);
            }
        }
    }

    // `x = { ... }`: type of the assignment target.
    if let Some(assign) = table_expr.ancestors::<LuaAssignStat>().next() {
        let (vars, values) = assign.get_var_and_expr_list();
        if values.iter().any(|expr| expr.get_syntax_id() == syntax_id)
            && let Some(var) = vars.first()
        {
            let ty = model.type_of_expr(var.to_expr().get_syntax_id());
            if !matches!(ty, LuaType::Unknown) {
                return Some(ty);
            }
        }
    }

    // Call argument: `observe({ ... })` -> parameter type.
    if let Some(call) = table_expr.ancestors::<LuaCallExpr>().next() {
        let arg_idx = call
            .get_args_list()?
            .get_args()
            .enumerate()
            .find(|(_, arg)| arg.get_syntax_id() == syntax_id)
            .map(|(idx, _)| idx)?;
        // Prefer call-site generic projection when available (`observe("x", { ... })` with `T -> string`).
        if let Some(fun) = model.inferred_call_doc_function(call.get_syntax_id()) {
            let mut param_idx = arg_idx;
            if call.is_colon_call() && !fun.is_colon_define() {
                param_idx += 1;
            }
            if let Some(ty) = fun
                .get_params()
                .get(param_idx)
                .and_then(|(_, ty)| ty.clone())
                && !matches!(ty, LuaType::Unknown)
            {
                return Some(ty);
            }
        }
        let prefix = call.get_prefix_expr()?;
        let mut types = Vec::new();
        for func in callable_candidates(model, &prefix) {
            let mut param_idx = arg_idx;
            if call.is_colon_call() && !func.is_colon_define() {
                param_idx += 1;
            }
            if let Some(ty) = func
                .get_params()
                .get(param_idx)
                .and_then(|(_, ty)| ty.clone())
            {
                if !types.contains(&ty) {
                    types.push(ty);
                }
            }
        }
        if let Some(ty) = types.into_iter().next() {
            return Some(ty);
        }
    }

    let ty = model.type_of_expr(syntax_id);
    (!matches!(ty, LuaType::Unknown)).then_some(ty)
}

/// Generic-projected member type for the member access (`x.foo` / `x:foo`) containing the current token.
fn member_type_at_token(model: &SalsaSemanticModel<'_>, token: &LuaSyntaxToken) -> Option<LuaType> {
    let index_expr = token.parent_ancestors().find_map(LuaIndexExpr::cast)?;
    let prefix = index_expr.get_prefix_expr()?;
    let prefix_ty = model.type_of_expr(prefix.get_syntax_id());
    let resolved = model.resolve_member(&index_expr)?;
    let member_id = resolved.member_id?;
    let member_file = match &member_id {
        SemanticId::Member(key) => key.file_id,
        _ => return None,
    };
    let member = model.file_facts_of(member_file)?.member_by_id(&member_id)?;
    let mut fun = match model.type_of_member(&member_id) {
        Some(LuaType::DocFunction(fun)) => fun.as_ref().clone(),
        _ => member_signature(model, &member)?,
    };

    // For function/method calls, prefer inferring function-level generics from arguments (`A.add(B)` with `T -> B`).
    if let Some(call) = token.parent_ancestors().find_map(LuaCallExpr::cast)
        && let Some(call_fun) = model.inferred_call_doc_function(call.get_syntax_id())
    {
        fun = call_fun;
    }

    let doc_fun = LuaType::DocFunction(std::sync::Arc::new(fun));
    match &prefix_ty {
        LuaType::Generic(generic) => {
            let base_id = generic.get_base_type_id_ref();
            let names = model
                .type_def_of(base_id)
                .map(|def| {
                    def.generic_params
                        .iter()
                        .map(|param| param.name.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Some(model.substitute_generic_params_named(&doc_fun, generic.get_params(), &names))
        }
        _ => Some(doc_fun),
    }
}

/// Render function parameters/returns (`is_method=true` strips the implicit self first parameter).
/// `show_unknown=true` is used for field/member type-surface display: still render explicitly when unbound generics become `unknown`.
fn render_function_params_and_ret_mode(
    model: &SalsaSemanticModel<'_>,
    func: &LuaFunctionType,
    is_method: bool,
    multiline_returns: bool,
    show_unknown: bool,
) -> (String, String) {
    let last_idx = func.get_params().len().saturating_sub(1);
    let params = func
        .get_params()
        .iter()
        .enumerate()
        .filter_map(|(index, (param_name, param_ty))| {
            if index == 0 && is_method && !func.is_colon_define() {
                return None;
            }
            let mut name = if param_name.is_empty() {
                format!("arg{}", index)
            } else {
                param_name.clone()
            };
            if func.is_variadic() && index == last_idx && name != "..." {
                name = format!("...{}", name);
            }
            match param_ty {
                Some(ty) if !matches!(ty, LuaType::Unknown) => {
                    Some(format!("{}: {}", name, humanize(model, ty)))
                }
                Some(ty) if show_unknown && matches!(ty, LuaType::Unknown) => {
                    Some(format!("{}: unknown", name))
                }
                _ => Some(name),
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    let ret_text = match func.get_ret() {
        LuaType::Nil => String::new(),
        LuaType::Unknown if show_unknown => "unknown".to_string(),
        LuaType::Unknown => String::new(),
        LuaType::Variadic(variadic) => match variadic.as_ref() {
            VariadicType::Multi(types) => types
                .iter()
                .map(|ty| humanize(model, ty))
                .collect::<Vec<_>>()
                .join(", "),
            VariadicType::Base(ty) => humanize(model, ty),
        },
        ty => humanize(model, ty),
    };

    let ret = if ret_text.is_empty() {
        String::new()
    } else if multiline_returns {
        format!(
            "
  -> {}
",
            ret_text
        )
    } else {
        format!(" -> {}", ret_text)
    };

    (params, ret)
}

fn decl_overload_count(
    model: &SalsaSemanticModel<'_>,
    closure_syntax: emmylua_parser::LuaSyntaxId,
) -> usize {
    model
        .signatures()
        .and_then(|signatures| {
            signatures
                .iter()
                .find(|signature| signature.closure_syntax == closure_syntax)
        })
        .and_then(|signature| signature.docs.as_ref())
        .map(|docs| docs.overloads.len())
        .unwrap_or(0)
}

/// Render `---@overload fun(...)` as separate function signature blocks (substituting call-site generic bindings when possible).
fn render_decl_overload_blocks(
    model: &SalsaSemanticModel<'_>,
    name: &str,
    closure_syntax: emmylua_parser::LuaSyntaxId,
    is_local: bool,
    call_bindings: Option<&HashMap<GenericTplId, LuaType>>,
) -> Vec<String> {
    let Some(signatures) = model.signatures() else {
        return Vec::new();
    };
    let Some(signature) = signatures
        .iter()
        .find(|signature| signature.closure_syntax == closure_syntax)
    else {
        return Vec::new();
    };
    let Some(docs) = signature.docs.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for overload_syntax in &docs.overloads {
        let LuaType::DocFunction(fun) =
            model.doc_type_lua_in(model.file_id(), *overload_syntax, &[])
        else {
            continue;
        };
        let mut fun = fun.as_ref().clone();
        if let Some(bindings) = call_bindings {
            let params = (0..docs.generic_params.len())
                .map(|index| {
                    bindings
                        .get(&GenericTplId::Type(index as u32))
                        .cloned()
                        .unwrap_or(LuaType::Unknown)
                })
                .collect::<Vec<_>>();
            let names = docs
                .generic_params
                .iter()
                .map(|param| param.name.as_str())
                .collect::<Vec<_>>();
            if let LuaType::DocFunction(substituted) = model.substitute_generic_params_named_str(
                &LuaType::DocFunction(std::sync::Arc::new(fun.clone())),
                &params,
                &names,
            ) {
                fun = substituted.as_ref().clone();
            }
        }
        let (params, ret) = render_function_params_and_ret_mode(model, &fun, false, false, false);
        let code = if is_local {
            format!("local function {}({}){}", name, params, ret)
        } else {
            format!("function {}({}){}", name, params, ret)
        };
        out.push(format!("```lua\n{}\n```", code));
    }
    out
}

fn signature_has_named_returns(
    model: &SalsaSemanticModel<'_>,
    closure_syntax: Option<emmylua_parser::LuaSyntaxId>,
) -> bool {
    let Some(closure_syntax) = closure_syntax else {
        return false;
    };
    model
        .signatures()
        .and_then(|signatures| {
            signatures
                .iter()
                .find(|signature| signature.closure_syntax == closure_syntax)
        })
        .and_then(|signature| signature.docs.as_ref())
        .is_some_and(|docs| !docs.named_returns.is_empty())
}

/// Render named `---@return name type` as a multiline return block.
fn render_named_returns(
    model: &SalsaSemanticModel<'_>,
    closure_syntax: Option<emmylua_parser::LuaSyntaxId>,
) -> String {
    let Some(closure_syntax) = closure_syntax else {
        return String::new();
    };
    let Some(signature) = model.signatures().and_then(|signatures| {
        signatures
            .iter()
            .find(|signature| signature.closure_syntax == closure_syntax)
    }) else {
        return String::new();
    };
    let Some(docs) = &signature.docs else {
        return String::new();
    };
    let mut lines = Vec::new();
    for type_syntax in &docs.returns {
        let ty = model.doc_type_lua(*type_syntax);
        let type_text = humanize(model, &ty);
        if let Some((name, _)) = docs
            .named_returns
            .iter()
            .find(|(_, syntax)| syntax == type_syntax)
        {
            lines.push(format!("  -> {}: {}", name, type_text));
        } else {
            lines.push(format!("  -> {}", type_text));
        }
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!(
            "
{}
",
            lines.join(
                "
"
            )
        )
    }
}

/// Member owner (TypeDef) -> display name and reference type.
fn member_owner_type_and_name(
    model: &SalsaSemanticModel<'_>,
    owner: &SemanticId,
) -> (Option<LuaType>, Option<String>) {
    if let SemanticId::Decl(decl_key) = owner {
        let facts = model.file_facts_of(decl_key.file_id);
        if let Some(def) = facts.and_then(|facts| {
            let decl = facts.decl_by_id(owner)?;
            facts
                .type_defs
                .iter()
                .find(|def| def.owner_syntax.is_some() && def.owner_syntax == decl.owner_syntax)
        }) {
            let ty = model.type_def_ref(&def);
            let simple_name = def
                .name
                .rsplit('.')
                .next()
                .unwrap_or(def.name.as_str())
                .to_string();
            return (Some(ty), Some(simple_name));
        }
        return (
            None,
            facts
                .and_then(|facts| facts.decl_by_id(owner))
                .map(|decl| decl.name.to_string()),
        );
    }
    let SemanticId::TypeDef(owner_key) = owner else {
        return (None, None);
    };
    let def = model
        .resolve_type_def(owner_key.full_name.as_str())
        .or_else(|| {
            model
                .type_defs_in_scope(owner_key.scope, owner_key.full_name.as_str())
                .into_iter()
                .find(|def| def.id == *owner)
        });
    match def {
        Some(def) => {
            let ty = model.type_def_ref(&def);
            let simple_name = def
                .name
                .rsplit('.')
                .next()
                .unwrap_or(def.name.as_str())
                .to_string();
            (Some(ty), Some(simple_name))
        }
        None => (
            None,
            owner_key
                .full_name
                .rsplit('.')
                .next()
                .map(|name| name.to_string()),
        ),
    }
}

/// Old humanize semantics: `fun(self: Owner, ...)` is displayed as a method, `fun(self: string, ...)` as a field.
fn display_as_method(
    model: &SalsaSemanticModel<'_>,
    func: &LuaFunctionType,
    owner_ty: Option<&LuaType>,
) -> bool {
    if func.is_colon_define() {
        return true;
    }
    let Some((name, ty)) = func.get_params().first() else {
        return false;
    };
    match ty {
        Some(ty) => {
            if ty.is_self_infer() {
                return true;
            }
            match owner_ty {
                Some(owner_ty) => {
                    if matches!(owner_ty, LuaType::Ref(_) | LuaType::Def(_))
                        && first_param_may_not_self(ty)
                    {
                        return false;
                    }
                    if model.type_check(owner_ty, ty) {
                        return true;
                    }
                    name == "self" && model.type_check(ty, owner_ty)
                }
                None => name == "self",
            }
        }
        None => name == "self",
    }
}

/// Number of other member declarations with the same key (shown as `(+N overloads)` for `---@field` overloads).
fn member_overload_count(
    model: &SalsaSemanticModel<'_>,
    member_info: &Member,
    _owner_ty: &LuaType,
) -> usize {
    model
        .members_of_owner(&member_info.owner)
        .into_iter()
        .filter(|member_ref| member_ref.name.as_str() == member_info.key.to_path().as_str())
        .count()
        .saturating_sub(1)
}

/// Replace function generic parameters still unbound at the call site with `unknown` (call-site display semantics);
/// already bound generics keep their substituted actual types.
fn substitute_unbound_call_generics(
    model: &SalsaSemanticModel<'_>,
    fun: LuaFunctionType,
    bindings: &HashMap<GenericTplId, LuaType>,
) -> LuaFunctionType {
    let generic_params = fun.get_generic_params().to_vec();
    if generic_params.is_empty() {
        return fun;
    }
    let params = generic_params
        .iter()
        .map(|param| {
            bindings
                .get(&param.get_tpl_id())
                .cloned()
                .unwrap_or(LuaType::Unknown)
        })
        .collect::<Vec<_>>();
    let names = generic_params
        .iter()
        .map(|param| param.get_name())
        .collect::<Vec<_>>();
    if let LuaType::DocFunction(substituted) = model.substitute_generic_params_named_str(
        &LuaType::DocFunction(std::sync::Arc::new(fun.clone())),
        &params,
        &names,
    ) {
        return substituted.as_ref().clone();
    }
    fun
}

fn decl_should_show_signature_tags(model: &SalsaSemanticModel<'_>, decl: &SemanticId) -> bool {
    model
        .decls()
        .and_then(|decls| decls.iter().find(|d| &d.id == decl))
        .is_some_and(|d| !matches!(d.kind, DeclKind::Param))
}

fn build_decl_hover(
    model: &SalsaSemanticModel<'_>,
    typ: &LuaType,
    decl: &SemanticId,
    call_fun: Option<&LuaFunctionType>,
    call_bindings: Option<&HashMap<GenericTplId, LuaType>>,
) -> (String, Option<HoverDescription>) {
    let Some(decls) = model.decls() else {
        return (humanize(model, typ), None);
    };
    let Some(decl_info) = decls.iter().find(|d| &d.id == decl) else {
        return (humanize(model, typ), None);
    };
    let name = decl_info.name.to_string();
    let prefix = match decl_info.kind {
        DeclKind::Param => "(parameter) ",
        DeclKind::Local { .. } => "local ",
        DeclKind::Global => "(global) ",
    };

    // Parameter declaration: prefer the passed hover type (may carry flow narrowing at an alias),
    // otherwise fall back to the signature `@param` projection.
    if matches!(decl_info.kind, DeclKind::Param) {
        let param_info = param_decl_type(model, &decl_info);
        if let Some((param_ty, closure_syntax)) = param_info {
            let type_text = if !matches!(typ, LuaType::Unknown) {
                humanize(model, typ)
            } else {
                humanize(model, &param_ty)
            };
            let constraint = param_generic_constraint(model, closure_syntax, &param_ty);
            let code = if let Some(constraint) = constraint {
                format!("(parameter) {}: {} extends {}", name, type_text, constraint)
            } else {
                format!("(parameter) {}: {}", name, type_text)
            };
            return (code, Some(decl_description(model, decl)));
        }
    }

    // Function declaration: `local function name(a: string, ...) -> ret (+N overloads)`.
    if let Some(closure) = decl_info.value_expr_syntax
        && let Some(signature) = call_fun
            .cloned()
            .or_else(|| model.type_of_signature(closure))
    {
        let signature = if let Some(bindings) = call_bindings {
            substitute_unbound_call_generics(model, signature, bindings)
        } else {
            signature
        };
        let has_named_returns =
            !call_fun.is_some() && signature_has_named_returns(model, Some(closure));
        let (params, _) =
            render_function_params_and_ret_mode(model, &signature, false, false, false);
        let ret = if has_named_returns {
            render_named_returns(model, Some(closure))
        } else {
            render_function_params_and_ret_mode(model, &signature, false, false, false).1
        };
        let overloads = decl_overload_count(model, closure);
        let overloads = if overloads > 0 {
            format!(" (+{} overloads)", overloads)
        } else {
            String::new()
        };
        let code = if matches!(decl_info.kind, DeclKind::Local { .. }) {
            format!("local function {}({}){}{}", name, params, ret, overloads)
        } else {
            format!("function {}({}){}{}", name, params, ret, overloads)
        };
        let mut description = decl_description(model, decl);
        description.overload_blocks = render_decl_overload_blocks(
            model,
            &name,
            closure,
            matches!(decl_info.kind, DeclKind::Local { .. }),
            call_bindings,
        );
        return (code, Some(description));
    }

    let description = decl_description(model, decl);
    // Named type declaration -> expand class member list (`local node: Node { ... }`).
    if let Some(class_lines) = class_member_lines(model, typ)
        && !class_lines.is_empty()
    {
        let code = format!(
            "{}{}: {} {{\n{}\n}}",
            prefix,
            name,
            humanize(model, typ),
            class_lines.join("\n")
        );
        return (code, Some(description));
    }
    let display_typ = hover_expand_type(model, typ);
    let code = format!(
        "{}{}: {}",
        prefix,
        name,
        humanize(model, &widen_decl_const(&display_typ))
    );
    (code, Some(description))
}

/// Call-site signature for `pcall(f, ...)`: synthesize the old full signature using the first
/// argument callback's `@param` / `@return_overload` (source order preserved, generic types kept as raw text).
fn build_pcall_hover_code(
    model: &SalsaSemanticModel<'_>,
    token: &LuaSyntaxToken,
) -> Option<String> {
    let call = token.parent_ancestors().find_map(LuaCallExpr::cast)?;
    let prefix = call.get_prefix_expr()?;
    let LuaExpr::NameExpr(name_expr) = prefix else {
        return None;
    };
    let callee = name_expr.get_name_text()?;
    if !matches!(callee.as_str(), "pcall" | "xpcall") {
        return None;
    }
    let callback = call.get_args_list()?.get_args().next()?;
    let LuaExpr::NameExpr(callback_name) = callback else {
        return None;
    };
    let decl = model.resolve_name(callback_name.get_position())?;
    let SemanticId::Decl(decl_key) = &decl else {
        return None;
    };
    let facts = model.file_facts_of(decl_key.file_id)?;
    let decl_info = facts.decl_by_id(&decl)?;
    let closure = decl_info.value_expr_syntax?;
    let signature = facts.signature_by_closure(closure)?;
    let docs = signature.docs.as_ref()?;

    let params = signature
        .param_names
        .iter()
        .filter_map(|name| {
            let ty_text = docs
                .param_types
                .iter()
                .find(|(n, _)| n == name)
                .and_then(|(_, syntax)| raw_type_text(model, decl_key.file_id, *syntax))?;
            Some(format!("{}: {}", name, ty_text))
        })
        .collect::<Vec<_>>();
    let params_text = params.join(", ");

    // Expand `---@return_overload` by row/slot: the i-th type in a row merges into the i-th slot's union.
    let mut slots: Vec<Vec<String>> = Vec::new();
    let mut index = 0usize;
    for row_len in &docs.return_overload_rows {
        for slot in 0..*row_len {
            let (_, syntax) = docs.return_overloads.get(index)?;
            index += 1;
            let text = normalize_tuple_brackets(
                &raw_type_text(model, decl_key.file_id, *syntax).unwrap_or_default(),
            );
            if slots.len() <= slot {
                slots.push(Vec::new());
            }
            if !slots[slot].contains(&text) {
                slots[slot].push(text);
            }
        }
    }
    if slots.is_empty() {
        return None;
    }

    let slot_joined = |slot: usize| -> String {
        slots
            .get(slot)
            .map(|types| types.join("|"))
            .unwrap_or_default()
    };
    let slot_wrapped = |slot: usize| -> String {
        let joined = slot_joined(slot);
        if joined.is_empty() {
            joined
        } else {
            format!("({})", joined)
        }
    };
    let callback_ret = format!("({},{})", slot_wrapped(0), slot_wrapped(1));
    let callback_fun = format!("sync fun({}) -> {}", params_text, callback_ret);
    let pcall_params = format!("f: {}, {}", callback_fun, params_text);

    let slot0 = slot_joined(0);
    let slot0_ordered = if slot0.contains("false") && slot0.contains("true") {
        "true|false".to_string()
    } else {
        slot0.clone()
    };
    let ret_parts = [
        format!("({})", slot0_ordered),
        format!("({}|string)", slot0),
        format!("(({}))?", slot_joined(1)),
    ];
    let pcall_ret = ret_parts.join(", ");

    Some(format!(
        "function {}({}) -> {}",
        callee, pcall_params, pcall_ret
    ))
}

fn normalize_tuple_brackets(text: &str) -> String {
    text.trim()
        .replace('[', "(")
        .replace(']', ")")
        .replace(' ', "")
}

/// Hover-display only: evaluate generic alias instances / conditional types first; this affects only display, not the semantic layer's default nominal form.
fn hover_expand_type(model: &SalsaSemanticModel<'_>, ty: &LuaType) -> LuaType {
    let expanded = model.expand_alias_for_hover(ty);
    let ty = if matches!(expanded, LuaType::Unknown | LuaType::Any) {
        ty.clone()
    } else {
        expanded
    };
    model.eval_conditionals_for_hover(&ty)
}

/// Local/global variable declaration hover widens literal constants to base types (`0` -> `integer`);
/// member paths such as table fields keep their literal types through their own rendering.
fn widen_decl_const(ty: &LuaType) -> LuaType {
    match ty {
        LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => LuaType::Integer,
        LuaType::FloatConst(_) => LuaType::Number,
        LuaType::StringConst(_) | LuaType::DocStringConst(_) => LuaType::String,
        LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_) => LuaType::Boolean,
        _ => ty.clone(),
    }
}

/// Rendered member list lines for a named type (`    field: number?,`; function members -> `function`).
fn class_member_lines(model: &SalsaSemanticModel<'_>, ty: &LuaType) -> Option<Vec<String>> {
    if !matches!(ty, LuaType::Ref(_) | LuaType::Def(_)) {
        return None;
    }
    let lines = model
        .member_infos(ty)
        .into_iter()
        .filter_map(|info| {
            let key_display = member_class_key_display(model, &info);
            if key_display.as_deref() == Some("[nil]") {
                return None;
            }
            let name = match key_display {
                Some(raw) => raw,
                None => match &info.key {
                    LuaMemberKey::Name(name) => name.to_string(),
                    LuaMemberKey::Integer(i) => format!("[{}]", i),
                    _ => return None,
                },
            };
            let rendered = match &info.typ {
                LuaType::DocFunction(_) => "function".to_string(),
                ty => render_member_typ_with_default(model, ty),
            };
            Some(format!("    {}: {},", name, rendered))
        })
        .collect::<Vec<_>>();
    Some(lines)
}

/// Raw key text for `@field` index signatures in class expansion (`[integer]` / `[true]` / `[nil]`).
fn member_class_key_display(
    model: &SalsaSemanticModel<'_>,
    info: &SalsaMemberInfo,
) -> Option<String> {
    let id = info.id.as_ref()?;
    let range = id.member_key_range()?;
    let SemanticId::Member(key) = id else {
        return None;
    };
    let member = model.file_facts_of(key.file_id)?.member_by_id(id)?;
    if !member.is_index_signature {
        return None;
    }
    let document = model.db().document(key.file_id)?;
    let raw = document.get_text_slice(range).trim().to_string();
    Some(format!(
        "[{}]",
        raw.trim_start_matches('[').trim_end_matches(']')
    ))
}

fn member_is_nullable(model: &SalsaSemanticModel<'_>, id: &SemanticId) -> bool {
    let SemanticId::Member(key) = id else {
        return false;
    };
    model
        .file_facts_of(key.file_id)
        .and_then(|facts| facts.member_by_id(id))
        .map(|member| member.is_nullable)
        .unwrap_or(false)
}

/// Render literal types in hover as `base_type = literal` (`1` -> `integer = 1`).
fn render_member_typ_with_default(model: &SalsaSemanticModel<'_>, ty: &LuaType) -> String {
    let value = humanize(model, ty);
    let base = match ty {
        LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => Some("integer"),
        LuaType::FloatConst(_) => Some("number"),
        LuaType::StringConst(_) | LuaType::DocStringConst(_) => Some("string"),
        LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_) => Some("boolean"),
        _ => None,
    };
    match base {
        Some(base) => format!("{} = {}", base, value),
        None => value,
    }
}

/// Remove `nil` from nullable types (`T | nil` / `T?`) for display when the current assignment is non-nil.
fn strip_nullable(ty: &LuaType) -> LuaType {
    match ty {
        LuaType::Union(union) => {
            let mut types = union.into_vec();
            types.retain(|t| !matches!(t, LuaType::Nil));
            LuaType::from_vec(types)
        }
        LuaType::MultiLineUnion(union) => strip_nullable(&union.to_union()),
        _ => ty.clone(),
    }
}

/// Type for a parameter declaration: closure-signature `@param` projection.
fn param_decl_type(
    model: &SalsaSemanticModel<'_>,
    decl_info: &Decl,
) -> Option<(LuaType, emmylua_parser::LuaSyntaxId)> {
    let chunk = model.chunk()?;
    let token = chunk
        .syntax()
        .token_at_offset(decl_info.name_range.start())
        .right_biased()?;
    let closure = token
        .parent_ancestors()
        .find_map(emmylua_parser::LuaClosureExpr::cast)?;
    let closure_syntax = closure.get_syntax_id();
    let signature = model.type_of_signature(closure_syntax)?;
    let index = signature
        .get_params()
        .iter()
        .position(|(param_name, _)| param_name == decl_info.name)?;
    let ty = model.param_type(closure_syntax, index)?;
    Some((ty, closure_syntax))
}

fn param_generic_constraint(
    model: &SalsaSemanticModel<'_>,
    closure_syntax: emmylua_parser::LuaSyntaxId,
    param_ty: &LuaType,
) -> Option<String> {
    let LuaType::TplRef(tpl) = param_ty else {
        return None;
    };
    let name = tpl.get_name();
    let signature = model
        .signatures()?
        .iter()
        .find(|s| s.closure_syntax == closure_syntax)?;
    let docs = signature.docs.as_ref()?;
    let generic = docs.generic_params.iter().find(|g| g.name == name)?;
    let constraint_syntax = generic.constraint?;
    let constraint_ty = model.doc_type_lua(constraint_syntax);
    Some(humanize(model, &constraint_ty))
}

/// Member closure signature (cross-file fallback when a runtime member type projects to `Function`).
fn member_signature(
    model: &SalsaSemanticModel<'_>,
    member_info: &Member,
) -> Option<LuaFunctionType> {
    let SemanticId::Member(member_key) = &member_info.id else {
        return None;
    };
    let value_syntax = member_info.value_syntax?;
    model.type_of_signature_in_file(member_key.file_id, value_syntax)
}

/// Member owner display name (TypeDef / Decl / Name / nested Member).
fn member_owner_display_name(model: &SalsaSemanticModel<'_>, owner: &SemanticId) -> Option<String> {
    match owner {
        SemanticId::TypeDef(_) => member_owner_type_and_name(model, owner).1,
        SemanticId::Decl(decl_key) => model.file_facts_of(decl_key.file_id).and_then(|facts| {
            let decl = facts.decl_by_id(owner)?;
            let def = facts
                .type_defs
                .iter()
                .find(|def| def.owner_syntax.is_some() && def.owner_syntax == decl.owner_syntax);
            Some(
                def.map(|def| {
                    def.name
                        .rsplit('.')
                        .next()
                        .unwrap_or(def.name.as_str())
                        .to_string()
                })
                .unwrap_or_else(|| decl.name.to_string()),
            )
        }),
        SemanticId::Name(name) => {
            if name.contains('.') {
                None
            } else {
                Some(name.to_string())
            }
        }
        // Nested table members (`M.K.Value = function`) are usually shown by their final segment, not the full access path.
        SemanticId::Member(_) => None,
        _ => None,
    }
}

/// Type of the member owner (used for type-surface members / generic projection).
fn runtime_owner_type(model: &SalsaSemanticModel<'_>, member: &Member) -> Option<LuaType> {
    match &member.owner {
        SemanticId::Decl(decl_key) => model.type_of_decl(&SemanticId::Decl(decl_key.clone())),
        SemanticId::TypeDef(key) => model
            .type_defs_in_scope(key.scope, &key.full_name)
            .into_iter()
            .next()
            .map(|def| model.type_def_ref(&def)),
        _ => None,
    }
}

/// If a runtime member definition has a same-key `@field` type member, prefer it for rendering (`function Test.e(a,b)`
/// shows the class-field signature instead of runtime `any` params).
fn member_typed_field(model: &SalsaSemanticModel<'_>, member_info: &Member) -> Option<Member> {
    let owner_ty = match &member_info.owner {
        SemanticId::Decl(decl_key) => {
            let facts = model.file_facts_of(decl_key.file_id)?;
            let decl_info = facts.decl_by_id(&member_info.owner)?;
            if let Some(owner_syntax) = decl_info.owner_syntax
                && let Some(def) = facts
                    .type_defs
                    .iter()
                    .find(|def| def.owner_syntax == Some(owner_syntax))
            {
                model.type_def_ref(def)
            } else {
                model.type_of_decl(&SemanticId::Decl(decl_key.clone()))?
            }
        }
        SemanticId::TypeDef(key) => {
            let def = model
                .type_defs_in_scope(key.scope, &key.full_name)
                .into_iter()
                .next()?;
            model.type_def_ref(&def)
        }
        _ => return None,
    };
    let mut def_ids = Vec::new();
    collect_type_def_ids(model, &owner_ty, &mut def_ids);
    let key = member_info.key.to_path();
    for id in def_ids {
        let Some(def) = model.type_def_of(&id) else {
            continue;
        };
        for member_ref in model.members_of_owner(&def.id) {
            if member_ref.name != key {
                continue;
            }
            let facts = model.file_facts_of(member_ref.file_id)?;
            let member = facts.member_by_id(&member_ref.id)?;
            // `members_of_owner(TypeDef)` entries are all `@field` type members.
            return Some(member.clone());
        }
    }
    None
}

/// Collect named type definitions that a type may correspond to (union/intersection/generic/constrained generic expansion).
fn collect_type_def_ids(
    _model: &SalsaSemanticModel<'_>,
    ty: &LuaType,
    out: &mut Vec<LuaTypeDeclId>,
) {
    match ty {
        LuaType::Ref(id) | LuaType::Def(id) => out.push(id.clone()),
        LuaType::Union(union) => {
            for component in union.into_vec() {
                collect_type_def_ids(_model, &component, out);
            }
        }
        LuaType::Intersection(intersection) => {
            for component in intersection.get_types() {
                collect_type_def_ids(_model, component, out);
            }
        }
        LuaType::Generic(generic) => {
            collect_type_def_ids(
                _model,
                &LuaType::Ref(generic.get_base_type_id_ref().clone()),
                out,
            );
        }
        LuaType::TplRef(tpl) => {
            if let Some(constraint) = tpl.get_constraint() {
                collect_type_def_ids(_model, constraint, out);
            }
        }
        _ => {}
    }
}

/// Runtime member value type: prefer direct expression inference; when the VM cannot infer
/// `table<K,V>[index]`, fall back to the generic table built-in value type V (`t[p]` for
/// `---@type table<string, number>` -> number).
fn runtime_non_nil_value_type(
    model: &SalsaSemanticModel<'_>,
    runtime_member: &Member,
) -> Option<LuaType> {
    let syntax = runtime_member.value_syntax?;
    let direct = model.type_of_expr(syntax);
    if !matches!(direct, LuaType::Unknown | LuaType::Nil | LuaType::Any) {
        return Some(direct);
    }
    let file_id = match &runtime_member.id {
        SemanticId::Member(key) => key.file_id,
        _ => model.file_id(),
    };
    let tree = model.syntax_tree_of(file_id)?;
    let node = syntax.to_node_from_root(&tree.get_red_root())?;
    let LuaExpr::IndexExpr(index_expr) = LuaExpr::cast(node)? else {
        return None;
    };
    let prefix = index_expr.get_prefix_expr()?;
    let prefix_ty = model.type_of_expr(prefix.get_syntax_id());
    match &prefix_ty {
        LuaType::TableGeneric(params) if params.len() >= 2 => Some(params[1].clone()),
        LuaType::Generic(generic)
            if generic.get_base_type_id().get_name() == "table"
                && generic.get_params().len() >= 2 =>
        {
            Some(generic.get_params()[1].clone())
        }
        _ => None,
    }
}

fn build_member_hover(
    model: &SalsaSemanticModel<'_>,
    typ: &LuaType,
    member: &SemanticId,
    call_site_substituted: bool,
    context_infos: &[MemberContextInfo],
) -> (String, Option<HoverDescription>) {
    let Some(members) = model.members() else {
        return (humanize(model, typ), None);
    };
    let Some(member_info) = members.iter().find(|m| &m.id == member) else {
        return (humanize(model, typ), None);
    };
    let mut runtime_member = member_info.clone();
    // The member use site may exist as a standalone member fact (no doc/value). For hover, prefer falling back to
    // the real defining member with the same key and owner (which has `---@type` or a value expression) to get type and comments.
    if runtime_member.doc_type_syntax.is_none() && runtime_member.value_syntax.is_none() {
        if let Some(canonical) = members.iter().find(|m| {
            m.key == runtime_member.key
                && m.owner == runtime_member.owner
                && (m.doc_type_syntax.is_some() || m.value_syntax.is_some())
        }) {
            runtime_member = canonical.clone();
        }
    }

    // Context projection (table field initializer / value alias): prefer the type-surface member as the display subject.
    let mut typed_member: Option<Member> = None;
    let mut effective_typ_override: Option<LuaType> = None;
    let mut overload_types: Vec<LuaType> = Vec::new();
    let mut overload_ids: Vec<Option<SemanticId>> = Vec::new();
    let show_unknown = context_infos.iter().any(|ctx| ctx.show_unknown);
    if !call_site_substituted && !context_infos.is_empty() {
        let first = &context_infos[0];
        if let (Some(id), Some(file_id)) = (&first.info.id, first.info.file_id)
            && let Some(facts) = model.file_facts_of(file_id)
            && let Some(context_member) = facts.member_by_id(id)
        {
            typed_member = Some(context_member.clone());
            effective_typ_override = Some(first.info.typ.clone());
            overload_types = context_infos
                .iter()
                .map(|info| info.info.typ.clone())
                .collect();
            overload_ids = context_infos
                .iter()
                .map(|info| info.info.id.clone())
                .collect();
        }
    } else if call_site_substituted {
        // A call-site overload was already selected: show that signature itself, not all overloads.
        if let Some(owner_ty) = runtime_owner_type(model, &runtime_member) {
            let infos = model.member_infos_with_key_all(&owner_ty, &runtime_member.key);
            if let Some(selected) = infos
                .iter()
                .find(|info| info.typ == *typ)
                .or_else(|| {
                    infos.iter().find(|info| {
                        matches!(
                            (&info.typ, typ),
                            (LuaType::DocFunction(a), LuaType::DocFunction(b)) if a == b
                        )
                    })
                })
                .and_then(|info| info.id.as_ref())
                && let Some(file_id) = match selected {
                    SemanticId::Member(key) => Some(key.file_id),
                    _ => None,
                }
                && let Some(facts) = model.file_facts_of(file_id)
                && let Some(context_member) = facts.member_by_id(selected)
            {
                typed_member = Some(context_member.clone());
            }
        }
    } else {
        let owner_ty = runtime_owner_type(model, &runtime_member);
        let mut function_members = Vec::new();
        if let Some(owner_ty) = owner_ty {
            let type_members = model
                .member_infos_with_key_all(&owner_ty, &runtime_member.key)
                .into_iter()
                .filter(|info| {
                    info.id.as_ref().is_some_and(|id| {
                        let file_id = match id {
                            SemanticId::Member(key) => key.file_id,
                            _ => return false,
                        };
                        model
                            .file_facts_of(file_id)
                            .and_then(|facts| facts.member_by_id(id))
                            .is_some_and(|member| matches!(member.owner, SemanticId::TypeDef(_)))
                    })
                })
                .collect::<Vec<_>>();
            // When same-name members include both normal fields and function fields, prefer function candidates for business purposes.
            function_members = type_members
                .iter()
                .filter(|info| matches!(info.typ, LuaType::DocFunction(_)))
                .cloned()
                .collect();
            overload_types = function_members
                .iter()
                .map(|info| info.typ.clone())
                .collect();
        }
        // Method definitions (`function M:event`) or runtime functions without a function field prefer the
        // implementation signature, with class fields as overloads; normal field/dot definitions still use the
        // existing "prefer same-key `@field` type member" rule.
        let prefer_runtime_definition = runtime_member.value_syntax.is_some()
            && (runtime_member.is_method || function_members.is_empty());
        if !prefer_runtime_definition {
            typed_member = member_typed_field(model, &runtime_member);
            if let Some(preferred) = function_members.first()
                && let Some(preferred_id) = &preferred.id
            {
                let file_id = match preferred_id {
                    SemanticId::Member(key) => Some(key.file_id),
                    _ => None,
                };
                if let Some(file_id) = file_id
                    && let Some(facts) = model.file_facts_of(file_id)
                    && let Some(context_member) = facts.member_by_id(preferred_id)
                {
                    typed_member = Some(context_member.clone());
                    effective_typ_override = Some(preferred.typ.clone());
                }
            }
        }
    }

    let member_info = typed_member.as_ref().unwrap_or(&runtime_member);
    let display_member_id = typed_member
        .as_ref()
        .map(|m| m.id.clone())
        .unwrap_or_else(|| runtime_member.id.clone());
    let member_name = member_info.key.to_path();
    let mut description = member_description(model, &display_member_id);
    let doc_effective_typ = match &member_info.id {
        SemanticId::Member(key) => member_info.doc_type_syntax.and_then(|syntax| {
            let ty = model.doc_type_lua_in(key.file_id, syntax, &[]);
            if !matches!(ty, LuaType::Unknown) {
                Some(ty)
            } else {
                None
            }
        }),
        _ => None,
    };
    let class_assoc_ty = (|| {
        let SemanticId::Member(key) = &runtime_member.id else {
            return None;
        };
        let facts = model.file_facts_of(key.file_id)?;
        let tree = model.syntax_tree_of(key.file_id)?;
        let root = tree.get_red_root();
        for def in &facts.type_defs {
            let owner_syntax = def.owner_syntax?;
            let node = owner_syntax.to_node_from_root(&root)?;
            if node.text_range().contains_range(key.key_range) {
                return Some(model.type_def_ref(def));
            }
        }
        None
    })();
    let runtime_owner_ty = runtime_owner_type(model, &runtime_member);
    let runtime_member_info = if typed_member.is_none() && !runtime_member.is_method {
        runtime_owner_ty.as_ref().and_then(|owner_ty| {
            let infos = model.member_infos(&owner_ty);
            infos
                .into_iter()
                .find(|info| info.key == runtime_member.key)
        })
    } else {
        None
    };
    let effective_typ = if let Some(override_ty) = effective_typ_override {
        override_ty
    } else if call_site_substituted && !matches!(typ, LuaType::Unknown) {
        typ.clone()
    } else if typed_member.is_some() {
        model
            .type_of_member(&display_member_id)
            .unwrap_or_else(|| typ.clone())
    } else if let Some(doc_ty) = doc_effective_typ {
        doc_ty
    } else if typed_member.is_none()
        && runtime_member.value_syntax.is_some()
        && member_signature(model, &runtime_member).is_some()
    {
        // Without a `@field` type-surface member, runtime function definitions take the `member_signature`
        // branch and are rendered as `function Owner.name(...)` / `(method) Owner:name(...)`.
        LuaType::Unknown
    } else if matches!(typ, LuaType::DocFunction(_)) {
        typ.clone()
    } else if let Some(class_ty) = class_assoc_ty
        && (matches!(
            typ,
            LuaType::Table | LuaType::Unknown | LuaType::TableConst(_)
        ))
    {
        class_ty
    } else if let Some(member_ty) = runtime_member_info {
        member_ty.typ
    } else {
        typ.clone()
    };
    let runtime_literal_ty = runtime_member.value_syntax.and_then(|syntax| {
        let ty = model.type_of_expr(syntax);
        matches!(
            ty,
            LuaType::StringConst(_)
                | LuaType::DocStringConst(_)
                | LuaType::IntegerConst(_)
                | LuaType::DocIntegerConst(_)
                | LuaType::FloatConst(_)
                | LuaType::BooleanConst(_)
                | LuaType::DocBooleanConst(_)
        )
        .then_some(ty)
    });
    // When a runtime member assignment/table field initializer is a non-nil value, nullable `@field x? T` hover
    // is shown as the non-null base type based on the current actual value (`x = "a"` / `x = create()` no longer shows `T?`).
    let runtime_non_nil_ty = runtime_non_nil_value_type(model, &runtime_member);
    let effective_typ = if matches!(effective_typ, LuaType::Unknown)
        && let Some(runtime_ty) = &runtime_non_nil_ty
    {
        runtime_ty.clone()
    } else if runtime_non_nil_ty.is_some() && effective_typ.is_nullable() {
        strip_nullable(&effective_typ)
    } else {
        effective_typ
    };
    if let Some(typed) = &typed_member
        && let SemanticId::TypeDef(key) = &typed.owner
        && key.full_name.contains('.')
    {
        description.owner_line = Some(format!("&nbsp;&nbsp;in class `{}`", key.full_name));
    }

    // Method definitions no longer return early: continue to signature rendering so `(method) Owner:name(...)` and params/returns are shown.
    // Function-valued fields (`@field f fun(...)`): signature rendering; type-definition owner -> `(field)/(method) Class.name()`.
    if let LuaType::DocFunction(fun) = &effective_typ {
        if let (Some(owner_ty), Some(owner_name)) =
            member_owner_type_and_name(model, &member_info.owner)
        {
            let is_method = display_as_method(model, fun, Some(&owner_ty));
            let has_named_returns = !call_site_substituted
                && signature_has_named_returns(model, member_info.value_syntax);
            let (params, _) =
                render_function_params_and_ret_mode(model, fun, is_method, false, show_unknown);
            let ret = if has_named_returns {
                render_named_returns(model, member_info.value_syntax)
            } else {
                render_function_params_and_ret_mode(model, fun, is_method, false, show_unknown).1
            };
            let overloads = if call_site_substituted {
                0
            } else if overload_types.is_empty() {
                member_overload_count(model, &member_info, &owner_ty)
            } else if typed_member.is_some() {
                overload_types.len().saturating_sub(1)
            } else {
                overload_types.len()
            };
            let overloads = if overloads > 0 {
                format!(" (+{} overloads)", overloads)
            } else {
                String::new()
            };
            let separator = if is_method { ":" } else { "." };
            let prefix = if is_method {
                "(method) "
            } else if typed_member.is_none() && runtime_member.value_syntax.is_some() {
                "function "
            } else {
                "(field) "
            };
            render_member_overload_blocks(
                model,
                &member_info,
                &owner_ty,
                &overload_types,
                &overload_ids,
                typed_member.is_some(),
                is_method,
                &mut description,
            );
            return (
                format!(
                    "{}{}{}{}({}){}{}",
                    prefix, owner_name, separator, member_name, params, ret, overloads
                ),
                Some(description),
            );
        } else if let Some(owner_name) = member_owner_display_name(model, &member_info.owner) {
            let is_method = fun.is_colon_define() || member_info.is_method;
            let has_named_returns = !call_site_substituted
                && signature_has_named_returns(model, member_info.value_syntax);
            let (params, _) =
                render_function_params_and_ret_mode(model, fun, is_method, false, show_unknown);
            let ret = if has_named_returns {
                render_named_returns(model, member_info.value_syntax)
            } else {
                render_function_params_and_ret_mode(model, fun, is_method, false, show_unknown).1
            };
            let overloads = member_overload_count(model, &member_info, &LuaType::Unknown);
            let overloads = if overloads > 0 {
                format!(" (+{} overloads)", overloads)
            } else {
                String::new()
            };
            let separator = if is_method { ":" } else { "." };
            let code = if is_method {
                format!(
                    "(method) {}{}{}({}){}{}",
                    owner_name, separator, member_name, params, ret, overloads
                )
            } else {
                format!(
                    "function {}.{}({}){}{}",
                    owner_name, member_name, params, ret, overloads
                )
            };
            return (code, Some(description));
        }

        let has_named_returns =
            !call_site_substituted && signature_has_named_returns(model, member_info.value_syntax);
        let (params, _) =
            render_function_params_and_ret_mode(model, fun, false, has_named_returns, show_unknown);
        let code = format!("(field) {}({})", member_name, params);
        return (code, Some(description));
    }

    // Runtime function members (`function CO.running()`): the semantic type may project to a bare `Function`,
    // so re-render using the member closure signature; after `---@version` filtering, this is already a visible definition.
    if let Some(fun) = member_signature(model, member_info) {
        let (owner_ty, owner_name) = member_owner_type_and_name(model, &member_info.owner);
        let is_method = member_info.is_method || display_as_method(model, &fun, owner_ty.as_ref());
        let has_named_returns =
            !call_site_substituted && signature_has_named_returns(model, member_info.value_syntax);
        let (params, _) =
            render_function_params_and_ret_mode(model, &fun, is_method, false, show_unknown);
        let ret = if has_named_returns {
            render_named_returns(model, member_info.value_syntax)
        } else {
            render_function_params_and_ret_mode(model, &fun, is_method, false, show_unknown).1
        };
        if let (Some(owner_ty), Some(owner_name)) = (owner_ty, owner_name) {
            let overloads = if call_site_substituted {
                0
            } else if overload_types.is_empty() {
                member_overload_count(model, &member_info, &owner_ty)
            } else if typed_member.is_some() {
                overload_types.len().saturating_sub(1)
            } else {
                overload_types.len()
            };
            let overloads = if overloads > 0 {
                format!(" (+{} overloads)", overloads)
            } else {
                String::new()
            };
            let separator = if is_method { ":" } else { "." };
            let prefix = if is_method { "(method) " } else { "function " };
            render_member_overload_blocks(
                model,
                &member_info,
                &owner_ty,
                &overload_types,
                &overload_ids,
                typed_member.is_some(),
                is_method,
                &mut description,
            );
            return (
                format!(
                    "{}{}{}{}({}){}{}",
                    prefix, owner_name, separator, member_name, params, ret, overloads
                ),
                Some(description),
            );
        }
        if let Some(owner_name) = member_owner_display_name(model, &member_info.owner) {
            let code = if is_method {
                format!("(method) {}:{}({}){}", owner_name, member_name, params, ret)
            } else {
                format!("function {}.{}({}){}", owner_name, member_name, params, ret)
            };
            return (code, Some(description));
        }
        let code = format!("function {}({}){}", member_name, params, ret);
        return (code, Some(description));
    }
    let mut rendered_typ = render_member_typ_with_default(model, &effective_typ);
    if runtime_literal_ty.is_none()
        && member_is_nullable(model, &display_member_id)
        && !rendered_typ.ends_with('?')
    {
        rendered_typ.push('?');
    }
    if let Some(literal_ty) = runtime_literal_ty
        && !matches!(
            effective_typ,
            LuaType::StringConst(_)
                | LuaType::DocStringConst(_)
                | LuaType::IntegerConst(_)
                | LuaType::DocIntegerConst(_)
                | LuaType::FloatConst(_)
                | LuaType::BooleanConst(_)
                | LuaType::DocBooleanConst(_)
        )
    {
        rendered_typ.push_str(" = ");
        rendered_typ.push_str(&humanize(model, &literal_ty));
    }
    let code = format!("(field) {}: {}", member_name, rendered_typ);
    (code, Some(description))
}

/// Append same-key member overload blocks (each rendered as a code block with a trailing `-- description` comment).
fn render_member_overload_blocks(
    model: &SalsaSemanticModel<'_>,
    member_info: &Member,
    owner_ty: &LuaType,
    overload_types: &[LuaType],
    overload_ids: &[Option<SemanticId>],
    skip_first: bool,
    _is_method: bool,
    description: &mut HoverDescription,
) {
    let (_, owner_name_opt) = member_owner_type_and_name(model, &member_info.owner);
    let Some(owner_name) = owner_name_opt else {
        return;
    };
    let member_name = member_info.key.to_path();
    let start = if skip_first { 1 } else { 0 };
    for (index, typ) in overload_types.iter().enumerate().skip(start) {
        let LuaType::DocFunction(fun) = typ else {
            continue;
        };
        let overload_is_method = display_as_method(model, fun, Some(owner_ty));
        let (params, ret) =
            render_function_params_and_ret_mode(model, fun, overload_is_method, false, false);
        let separator = if overload_is_method { ":" } else { "." };
        let prefix = if overload_is_method {
            "(method) "
        } else {
            "(field) "
        };
        let mut block = format!(
            "{}{}{}{}({}){}",
            prefix, owner_name, separator, member_name, params, ret
        );
        if let Some(Some(id)) = overload_ids.get(index)
            && let Some(text) = member_description(model, id).text
        {
            block.push_str(" -- ");
            block.push_str(&text);
        }
        description
            .overload_blocks
            .push(format!("```lua\n{}\n```", block));
    }
}

/// Display text from the raw type syntax in the document (used for fidelity when alias generic constraints/targets cannot be evaluated).
fn raw_type_text(
    model: &SalsaSemanticModel<'_>,
    file_id: emmylua_code_analysis::FileId,
    syntax: emmylua_parser::LuaSyntaxId,
) -> Option<String> {
    let document = model.db().document(file_id)?;
    Some(
        document
            .get_text_slice(syntax.get_range())
            .trim()
            .to_string(),
    )
}

/// Display alias generic parameters: `<K extends keyof T, T>`.
fn alias_generic_suffix(model: &SalsaSemanticModel<'_>, def: &TypeDef) -> String {
    if def.generic_params.is_empty() {
        return String::new();
    }
    let parts = def
        .generic_params
        .iter()
        .map(|param| {
            let mut text = param.name.to_string();
            if let Some(constraint) = param.constraint {
                let ty = model.doc_type_lua_in(def.file_id, constraint, &def.generic_params);
                let constraint_text = if matches!(ty, LuaType::Unknown | LuaType::Any) {
                    raw_type_text(model, def.file_id, constraint)
                        .unwrap_or_else(|| "unknown".into())
                } else {
                    humanize(model, &ty)
                };
                text.push_str(" extends ");
                text.push_str(&constraint_text);
            }
            if let Some(default) = param.default {
                let ty = model.doc_type_lua_in(def.file_id, default, &def.generic_params);
                let default_text = if matches!(ty, LuaType::Unknown | LuaType::Any) {
                    raw_type_text(model, def.file_id, default).unwrap_or_else(|| "unknown".into())
                } else {
                    humanize(model, &ty)
                };
                text.push_str(" = ");
                text.push_str(&default_text);
            }
            text
        })
        .collect::<Vec<_>>();
    format!("<{}>", parts.join(", "))
}

/// Render an alias: normal types show `(alias) Name = Type`;
/// multiline unions additionally append trailing comments like `| "A" -- A1`.
fn render_alias_hover(model: &SalsaSemanticModel<'_>, def: &TypeDef) -> String {
    let generic_suffix = alias_generic_suffix(model, def);
    let Some(alias_syntax) = def.alias_type else {
        return format!("(alias) {}{}", def.name, generic_suffix);
    };
    let alias_ty = model.doc_type_lua_in(def.file_id, alias_syntax, &def.generic_params);
    let mut alias_text = if matches!(alias_ty, LuaType::Unknown | LuaType::Any) {
        raw_type_text(model, def.file_id, alias_syntax)
            .unwrap_or_else(|| humanize(model, &alias_ty))
    } else {
        humanize(model, &alias_ty)
    };
    // When object/mapped alias raw text lacks a trailing `;`, add one to keep the `{ ...; }` display.
    let trimmed = alias_text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') && !trimmed.ends_with(";}") {
        let without_brace = trimmed[..trimmed.len() - 1].trim_end();
        alias_text = format!("{}; }}", without_brace);
    }
    let mut code = format!("(alias) {}{} = {}", def.name, generic_suffix, alias_text);

    if let Some(tree) = model.syntax_tree_of(def.file_id)
        && let Some(node) = alias_syntax.to_node_from_root(&tree.get_red_root())
        && let Some(LuaDocType::MultiLineUnion(union)) = LuaDocType::cast(node)
    {
        let mut lines = Vec::new();
        for field in union.get_fields() {
            let type_text = field
                .get_type()
                .map(|ty| {
                    let lua_ty =
                        model.doc_type_lua_in(def.file_id, ty.get_syntax_id(), &def.generic_params);
                    humanize(model, &lua_ty)
                })
                .unwrap_or_default();
            let desc = field
                .get_description()
                .map(|desc| {
                    desc.get_description_text()
                        .trim()
                        .trim_start_matches('#')
                        .trim()
                        .to_string()
                })
                .unwrap_or_default();
            if desc.is_empty() {
                lines.push(format!("    | {}", type_text));
            } else {
                lines.push(format!("    | {} -- {}", type_text, desc));
            }
        }
        if !lines.is_empty() {
            code.push('\n');
            code.push_str(&lines.join("\n"));
            code.push('\n');
        }
    }

    code
}

/// Select the overload on a type definition by argument types at the attribute use site (`---@[custom_attribute(1)]`).
fn attribute_overload_for_type(
    model: &SalsaSemanticModel<'_>,
    token: &LuaSyntaxToken,
    def: &TypeDef,
) -> Option<LuaFunctionType> {
    let attribute_use = token
        .parent_ancestors()
        .find_map(LuaDocAttributeUse::cast)?;
    let attribute_name = attribute_use.get_type()?.get_name_text()?;
    if attribute_name != def.name && attribute_name != def.full_name {
        return None;
    }
    let args = attribute_use
        .get_arg_list()
        .map(|list| list.get_args().collect::<Vec<_>>())
        .unwrap_or_default();
    let arg_types = args
        .iter()
        .map(|arg| model.type_of_expr(LuaExpr::LiteralExpr(arg.clone()).get_syntax_id()))
        .collect::<Vec<_>>();
    let overloads: Vec<LuaFunctionType> = def
        .call_overloads
        .iter()
        .filter_map(|syntax| match model.doc_type_lua(*syntax) {
            LuaType::DocFunction(fun) => Some(fun.as_ref().clone()),
            _ => None,
        })
        .collect();
    if overloads.is_empty() {
        return None;
    }
    let arg_count = arg_types.len();
    let only_candidate = overloads.len() == 1;
    let mut fallback = None;
    let mut count_fallback = None;
    for func in &overloads {
        fallback.get_or_insert_with(|| func.clone());
        if !attribute_params_accept_arg_count(func.get_params(), arg_count) {
            continue;
        }
        count_fallback.get_or_insert_with(|| func.clone());
        if only_candidate || attribute_callable_accepts_types(model, func, &arg_types) {
            return Some(func.clone());
        }
    }
    count_fallback.or(fallback)
}

fn attribute_params_accept_arg_count(
    def_params: &[(String, Option<LuaType>)],
    arg_count: usize,
) -> bool {
    let required_count = def_params
        .iter()
        .take_while(|(name, typ)| name != "..." && !typ.as_ref().is_some_and(LuaType::is_variadic))
        .filter(|(_, typ)| !typ.as_ref().is_some_and(LuaType::is_optional))
        .count();
    let allows_more = def_params
        .last()
        .is_some_and(|(name, typ)| name == "..." || typ.as_ref().is_some_and(LuaType::is_variadic));
    arg_count >= required_count && (allows_more || arg_count <= def_params.len())
}

fn attribute_callable_accepts_types(
    model: &SalsaSemanticModel<'_>,
    func: &LuaFunctionType,
    arg_types: &[LuaType],
) -> bool {
    for (index, (name, param_ty)) in func.get_params().iter().enumerate() {
        if name == "..." {
            if let Some(param_ty) = param_ty {
                return arg_types[index..]
                    .iter()
                    .all(|arg| model.type_check(arg, param_ty));
            }
            return true;
        }
        let Some(param_ty) = param_ty else {
            continue;
        };
        let Some(arg) = arg_types.get(index) else {
            return true;
        };
        if !model.type_check(arg, param_ty) {
            return false;
        }
    }
    true
}

fn build_type_hover(
    model: &SalsaSemanticModel<'_>,
    typ: &LuaType,
    decl: &SemanticId,
    token: &LuaSyntaxToken,
) -> (String, Option<HoverDescription>) {
    let SemanticId::TypeDef(key) = decl else {
        return (humanize(model, typ), None);
    };
    let defs = model.type_defs_in_scope(key.scope, &key.full_name);
    let Some(def) = defs.iter().find(|d| d.id == *decl) else {
        return (humanize(model, typ), None);
    };
    let description = type_def_description(model, def);
    let prefix = match def.kind {
        TypeDefKind::Class => "(class) ",
        TypeDefKind::Alias => "(alias) ",
        TypeDefKind::Enum => "(enum) ",
    };

    // Choose overload by arguments at the attribute use site: `---@[custom_attribute(1)]` ->
    // `(class) custom_attribute(value: integer)`.
    if def.kind == TypeDefKind::Class
        && let Some(attribute_fun) = attribute_overload_for_type(model, token, def)
    {
        let (params, _) =
            render_function_params_and_ret_mode(model, &attribute_fun, false, false, false);
        return (
            format!("{}{}({})", prefix, def.name, params),
            Some(description),
        );
    }

    // Class declaration hover: when there are generic parameters, show the declaration form `(class) Name<...>`
    // instead of expanding the member list (member expansion is for instance/type use sites, not the definition site).
    if def.kind == TypeDefKind::Class && !def.generic_params.is_empty() {
        return (
            format!("{}{}{}", prefix, def.name, alias_generic_suffix(model, def)),
            Some(description),
        );
    }

    // Class member list: `(class) A {\n    a: number,\n}`.
    let class_ty = LuaType::Ref(LuaTypeDeclId::global(def.full_name.as_str()));
    let member_lines = model
        .member_infos(&class_ty)
        .into_iter()
        .filter_map(|info| {
            let key_display = member_class_key_display(model, &info);
            if key_display.as_deref() == Some("[nil]") {
                return None;
            }
            let name = match key_display {
                Some(raw) => raw,
                None => match &info.key {
                    LuaMemberKey::Name(name) => name.to_string(),
                    LuaMemberKey::Integer(i) => format!("[{}]", i),
                    _ => return None,
                },
            };
            Some(format!(
                "    {}: {},",
                name,
                render_member_typ_with_default(model, &info.typ)
            ))
        })
        .collect::<Vec<_>>();
    let code = match def.kind {
        TypeDefKind::Alias => render_alias_hover(model, def),
        _ if member_lines.is_empty() => format!("{}{}", prefix, def.name),
        _ => format!("{}{} {{\n{}\n}}", prefix, def.name, member_lines.join("\n")),
    };
    (code, Some(description))
}
