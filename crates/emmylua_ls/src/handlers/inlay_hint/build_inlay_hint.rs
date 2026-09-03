use emmylua_code_analysis::{
    AsyncState, LuaType, SalsaDatabase, SalsaSemanticModel, SemanticId, TypeDefKind,
};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaCommentOwner, LuaDocTag, LuaExpr, LuaFuncStat,
    LuaIndexExpr, LuaIndexKey, LuaLiteralToken, LuaLocalName, LuaNameToken, LuaTableField,
    LuaVarExpr,
};
use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, InlayHintLabelPart, Location, Range};

use crate::context::ClientId;
use crate::handlers::hover::render::humanize;

pub fn build_inlay_hints(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    client_id: ClientId,
    enum_param_hint: bool,
) -> Option<Vec<InlayHint>> {
    let mut result = Vec::new();
    let _ = client_id;
    let root = model.chunk()?;
    let document = salsa.document(model.file_id())?;
    for node in root.descendants::<LuaAst>() {
        match node {
            LuaAst::LuaClosureExpr(closure) => {
                build_closure_param_hints(model, salsa, &document, &mut result, closure);
            }
            LuaAst::LuaLocalName(local_name) => {
                build_local_name_hint(model, &document, &mut result, local_name);
            }
            LuaAst::LuaFuncStat(func_stat) => {
                build_func_stat_override_hint(model, &document, &mut result, &func_stat);
            }
            LuaAst::LuaIndexExpr(index_expr) => {
                build_index_expr_hint(model, &document, &mut result, &index_expr);
            }
            LuaAst::LuaCallExpr(call_expr) => {
                build_call_expr_await_hint(model, &document, &mut result, &call_expr);
                build_call_expr_param_hint(
                    model,
                    &document,
                    &mut result,
                    call_expr.clone(),
                    enum_param_hint,
                );
                build_meta_call_hint(model, &document, &mut result, &call_expr);
            }
            _ => {}
        }
    }
    Some(result)
}

/// Closure parameter hint: `---@param` annotated type → `: T` after the parameter name.
fn build_closure_param_hints(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    document: &emmylua_code_analysis::DocumentView,
    result: &mut Vec<InlayHint>,
    closure: emmylua_parser::LuaClosureExpr,
) -> Option<()> {
    // When overriding a parent method, do not repeat the parameter type hint (the override hint already conveys inheritance).
    if is_override_func_stat(model, &closure) {
        return None;
    }
    let params = closure.get_params_list()?;
    for param in params.get_params() {
        // Unnamed `...` has no name to hint; named variadic `...args` in Lua 5.5 is still hinted.
        if param.is_dots() && param.get_name_token().is_none() {
            continue;
        }
        let Some(name_token) = param.get_name_token() else {
            continue;
        };
        let Some(decl) = model.decl_by_offset(name_token.get_position()) else {
            continue;
        };
        let Some(ty) = model.type_of_decl(&decl) else {
            continue;
        };
        if matches!(
            ty,
            LuaType::Unknown | LuaType::Any | LuaType::Nil | LuaType::Function
        ) {
            continue;
        }
        let Some(pos) = document.to_lsp_position(name_token.get_range().end()) else {
            continue;
        };
        let label_text = format!(": {}", humanize(model, &ty));
        let location = if is_primitive_type(&ty) {
            builtin_file_location(salsa)
        } else {
            document.get_uri().map(|uri| Location {
                uri,
                range: Range::new(pos, pos),
            })
        };
        result.push(InlayHint {
            position: pos,
            label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                value: label_text,
                location,
                ..Default::default()
            }]),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(false),
            padding_right: None,
            data: None,
        });
    }
    Some(())
}

/// Whether the closure belongs to `function X:method(...)` / `function X.method(...)` and overrides a parent member.
fn is_override_func_stat(
    model: &SalsaSemanticModel<'_>,
    closure: &emmylua_parser::LuaClosureExpr,
) -> bool {
    let Some(func_stat) = closure.get_parent::<LuaFuncStat>() else {
        return false;
    };
    let Some(LuaVarExpr::IndexExpr(index_expr)) = func_stat.get_func_name() else {
        return false;
    };
    let Some(prefix_expr) = index_expr.get_prefix_expr() else {
        return false;
    };
    let type_id = match model.type_of_expr(prefix_expr.get_syntax_id()) {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        _ => return false,
    };
    let def = match model.type_def_of(&type_id) {
        Some(def) => def,
        None => return false,
    };
    let Some(name_token) = index_expr.get_index_name_token() else {
        return false;
    };
    let Some(method_name_token) = LuaNameToken::cast(name_token.clone()) else {
        return false;
    };
    let method_name = method_name_token.get_name_text().to_string();
    def.super_names.iter().any(|super_name| {
        model
            .resolve_type_def(super_name)
            .map(|super_def| {
                model
                    .members_of_owner(&super_def.id)
                    .iter()
                    .any(|m| m.name.as_str() == method_name)
            })
            .unwrap_or(false)
    })
}

fn build_local_name_hint(
    model: &SalsaSemanticModel<'_>,
    document: &emmylua_code_analysis::DocumentView,
    result: &mut Vec<InlayHint>,
    local_name: LuaLocalName,
) -> Option<()> {
    let name_token = local_name.get_name_token()?;
    let decl = model.decl_by_offset(name_token.get_position())?;
    // `---@class Foo` + `local Foo`: runtime class variables do not get a type hint.
    if let Some(facts) = model.file_facts()
        && let Some(decl_info) = facts.decl_by_id(&decl)
        && let Some(owner_syntax) = decl_info.owner_syntax
        && facts
            .type_defs
            .iter()
            .any(|def| def.owner_syntax == Some(owner_syntax))
    {
        return None;
    }
    let ty = model.type_of_decl(&decl)?;
    if !should_hint_type(&ty) {
        return None;
    }
    let pos = document.to_lsp_position(name_token.get_range().end())?;
    let label_text = format!(": {}", humanize(model, &ty));
    let location = document.get_uri().map(|uri| Location {
        uri,
        range: Range::new(pos, pos),
    });
    result.push(InlayHint {
        position: pos,
        label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
            value: label_text,
            location,
            ..Default::default()
        }]),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: Some(false),
        padding_right: None,
        data: None,
    });
    Some(())
}

/// When `function B:aaa(...)` overrides a parent member, show `override` after the parameter list.
fn build_func_stat_override_hint(
    model: &SalsaSemanticModel<'_>,
    document: &emmylua_code_analysis::DocumentView,
    result: &mut Vec<InlayHint>,
    func_stat: &LuaFuncStat,
) -> Option<()> {
    let LuaVarExpr::IndexExpr(index_expr) = func_stat.get_func_name()? else {
        return None;
    };
    let prefix_expr = index_expr.get_prefix_expr()?;
    let prefix_ty = model.type_of_expr(prefix_expr.get_syntax_id());
    let type_id = match prefix_ty {
        LuaType::Ref(id) | LuaType::Def(id) => id,
        _ => return None,
    };
    let def = model.type_def_of(&type_id)?;
    let method_name = LuaNameToken::cast(index_expr.get_index_name_token()?.clone())?
        .get_name_text()
        .to_string();
    let mut has_super_member = false;
    for super_name in &def.super_names {
        let Some(super_def) = model.resolve_type_def(super_name) else {
            continue;
        };
        let members = model.members_of_owner(&super_def.id);
        if members.iter().any(|m| m.name.as_str() == method_name) {
            has_super_member = true;
            break;
        }
    }
    if !has_super_member {
        return None;
    }
    let pos = document.to_lsp_position(
        func_stat
            .get_closure()?
            .get_params_list()?
            .get_range()
            .end(),
    )?;
    let location = document.get_uri().map(|uri| Location {
        uri,
        range: Range::new(pos, pos),
    });
    result.push(InlayHint {
        position: pos,
        label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
            value: "override".to_string(),
            location,
            ..Default::default()
        }]),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: Some(true),
        padding_right: None,
        data: None,
    });
    Some(())
}

/// Integer index `export[1]`: if the field definition has `---@[index_alias("nameX")]`, hint `: nameX` after the index.
fn build_index_expr_hint(
    model: &SalsaSemanticModel<'_>,
    document: &emmylua_code_analysis::DocumentView,
    result: &mut Vec<InlayHint>,
    index_expr: &LuaIndexExpr,
) -> Option<()> {
    let index_key = index_expr.get_index_key()?;
    if !matches!(index_key, LuaIndexKey::Integer(_)) {
        return None;
    }
    let resolved = model.resolve_member(index_expr)?;
    let member_id = resolved.member_id?;
    let facts = model.file_facts()?;
    let member = facts.member_by_id(&member_id)?;
    let key_range = member.id.member_key_range()?;
    let chunk = model.chunk()?;
    let field = chunk
        .descendants::<LuaTableField>()
        .find(|f| f.syntax().text_range().contains_range(key_range))?;
    let comment = field.get_left_comment()?;
    let alias = comment
        .get_doc_tags()
        .filter_map(|tag| match tag {
            LuaDocTag::AttributeUse(attr_tag) => Some(attr_tag.get_attribute_uses()),
            _ => None,
        })
        .flatten()
        .find_map(|attr| {
            let name = attr.get_type()?.get_name_text()?;
            if name != "index_alias" {
                return None;
            }
            let arg = attr.get_arg_list()?.get_args().next()?;
            let LuaLiteralToken::String(s) = arg.get_literal()? else {
                return None;
            };
            Some(s.get_value().to_string())
        })?;

    let index_token = index_expr.get_index_name_token()?;
    let pos = document.to_lsp_position(index_token.text_range().end())?;
    let location = document.get_uri().map(|uri| Location {
        uri,
        range: Range::new(pos, pos),
    });
    result.push(InlayHint {
        position: pos,
        label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
            value: format!(": {}", alias),
            location,
            ..Default::default()
        }]),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: Some(true),
        padding_right: None,
        data: None,
    });
    Some(())
}

/// Show `await` before async function calls.
fn build_call_expr_await_hint(
    model: &SalsaSemanticModel<'_>,
    document: &emmylua_code_analysis::DocumentView,
    result: &mut Vec<InlayHint>,
    call_expr: &LuaCallExpr,
) -> Option<()> {
    let prefix = call_expr.get_prefix_expr()?;
    let callee_ty = model.type_of_expr(prefix.get_syntax_id());
    let is_async = match &callee_ty {
        LuaType::DocFunction(fun) => fun.get_async_state() == AsyncState::Async,
        _ => false,
    };
    if !is_async {
        return None;
    }
    let pos = document.to_lsp_position(call_expr.get_range().start())?;
    result.push(InlayHint {
        position: pos,
        label: InlayHintLabel::String("await".to_string()),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: None,
        padding_right: Some(true),
        data: None,
    });
    Some(())
}

/// Show `new` before callable classes like `Hint1("a")`.
fn build_meta_call_hint(
    model: &SalsaSemanticModel<'_>,
    document: &emmylua_code_analysis::DocumentView,
    result: &mut Vec<InlayHint>,
    call_expr: &LuaCallExpr,
) -> Option<()> {
    let prefix = call_expr.get_prefix_expr()?;
    let prefix_ty = model.type_of_expr(prefix.get_syntax_id());
    let (def, is_meta_constructor) = match prefix_ty {
        LuaType::Ref(id) | LuaType::Def(id) => (model.type_def_of(&id)?, false),
        _ => (constructor_type_from_meta(model, &prefix)?, true),
    };
    if !is_meta_constructor {
        let has_call_overload = !def.call_overloads.is_empty();
        let has_constructor_member = model
            .members_of_owner(&def.id)
            .iter()
            .any(|m| m.name == "__init");
        if !has_call_overload && !has_constructor_member {
            // After `local A = meta("MyClass")` returns a concrete Ref, `__init` may only be attached
            // to the local A rather than the class TypeDef; in that case it can still be recognized as a meta constructor.
            constructor_type_from_meta(model, &prefix)?;
        }
    }
    let pos = document.to_lsp_position(call_expr.get_range().start())?;
    let location = document.get_uri().map(|uri| Location {
        uri,
        range: Range::new(pos, pos),
    });
    result.push(InlayHint {
        position: pos,
        label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
            value: "new".to_string(),
            location,
            ..Default::default()
        }]),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: None,
        padding_right: Some(true),
        data: None,
    });
    Some(())
}

/// `local A = meta("MyClass")`: infer the class definition from the string argument of a meta call.
fn constructor_type_from_meta(
    model: &SalsaSemanticModel<'_>,
    prefix: &LuaExpr,
) -> Option<emmylua_code_analysis::TypeDef> {
    let LuaExpr::NameExpr(name_expr) = prefix else {
        return None;
    };
    let decl = model.resolve_name(name_expr.get_position())?;
    let facts = model.file_facts()?;
    let decl_info = facts.decl_by_id(&decl)?;
    let value_syntax = decl_info.value_expr_syntax?;
    let chunk = model.chunk()?;
    let call = chunk
        .descendants::<LuaCallExpr>()
        .find(|call| call.get_syntax_id() == value_syntax)?;
    let args: Vec<LuaExpr> = call.get_args_list()?.get_args().collect();
    let literal = string_literal_of_expr(args.first()?)?;
    model.resolve_type_def(&literal)
}

fn string_literal_of_expr(expr: &LuaExpr) -> Option<String> {
    let token = expr
        .syntax()
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
        .find_map(LuaLiteralToken::cast)?;
    match token {
        LuaLiteralToken::String(string) => Some(string.get_value()),
        _ => None,
    }
}

fn build_call_expr_param_hint(
    model: &SalsaSemanticModel<'_>,
    document: &emmylua_code_analysis::DocumentView,
    result: &mut Vec<InlayHint>,
    call_expr: LuaCallExpr,
    enum_param_hint: bool,
) -> Option<()> {
    let prefix = call_expr.get_prefix_expr()?;
    let callee_ty = model.type_of_expr(prefix.get_syntax_id());
    let mut is_class_call = false;
    let fun = match callee_ty {
        LuaType::DocFunction(fun) => fun.as_ref().clone(),
        // Callable class: use `---@overload fun(...)` as the parameter signature.
        LuaType::Ref(id) | LuaType::Def(id) => {
            is_class_call = true;
            let def = model.type_def_of(&id)?;
            let overload_syntax = def.call_overloads.first().copied()?;
            match model.doc_type_lua_in(def.file_id, overload_syntax, &def.generic_params) {
                LuaType::DocFunction(fun) => fun.as_ref().clone(),
                _ => return None,
            }
        }
        // Named function: project according to the declared signature.
        LuaType::Function | LuaType::Unknown => match &prefix {
            LuaExpr::NameExpr(name_expr) => {
                let decl = model.resolve_name(name_expr.get_position())?;
                let decls = model.decls()?;
                let decl_info = decls.iter().find(|d| d.id == decl)?;
                let closure = decl_info.value_expr_syntax?;
                model.type_of_signature(closure)?
            }
            LuaExpr::IndexExpr(index_expr) => {
                let resolved = model.resolve_member(index_expr)?;
                let member_id = resolved.member_id?;
                let member_file = match &member_id {
                    SemanticId::Member(key) => key.file_id,
                    _ => return None,
                };
                // Most stdlib/doc members already resolve to `DocFunction`. Runtime members such
                // as `function string.format(fmt, ...) end` resolve to plain `Function`; for those,
                // fall back to the closure signature so parameter inlay hints still work.
                if let LuaType::DocFunction(fun) = model.type_of_member(&member_id)? {
                    fun.as_ref().clone()
                } else if let Some(facts) = model.file_facts_of(member_file)
                    && let Some(member) = facts.member_by_id(&member_id)
                    && let Some(value_syntax) = member.value_syntax
                    && let Some(sig) = model.type_of_signature_in_file(member_file, value_syntax)
                {
                    sig
                } else {
                    return None;
                }
            }
            _ => return None,
        },
        _ => return None,
    };

    let args = call_expr.get_args_list()?.get_args().collect::<Vec<_>>();
    let colon_call = call_expr.is_colon_call();
    let param_offset = usize::from(!fun.is_colon_define() && colon_call);
    let params = fun.get_params();
    let is_variadic = fun.is_variadic();
    for (index, arg) in args.iter().enumerate() {
        let param_index = index + param_offset;
        let param = if param_index < params.len() {
            Some(&params[param_index])
        } else if is_variadic && !params.is_empty() {
            params.last()
        } else {
            None
        };
        let Some((name, param_ty)) = param else {
            break;
        };
        if name.is_empty() {
            continue;
        }
        // Omit the parameter label when the argument name matches the parameter name (`test(a)` with argument `a`).
        if name != "..."
            && let LuaExpr::NameExpr(arg_name) = arg
            && arg_name.get_name_text().as_deref() == Some(name.as_str())
        {
            continue;
        }
        let Some(pos) = document.to_lsp_position(arg.get_position()) else {
            continue;
        };
        let location = if is_class_call {
            None
        } else {
            document.get_uri().map(|uri| Location {
                uri,
                range: Range::new(pos, pos),
            })
        };
        // Extra arguments for variadic `...` are shown as `var0:` / `var1:`.
        let label_text = if name == "..." {
            let var_index = param_index.saturating_sub(params.len().saturating_sub(1));
            format!("var{}:", var_index)
        } else {
            format!("{}:", name)
        };
        result.push(InlayHint {
            position: pos,
            label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                value: label_text,
                location,
                ..Default::default()
            }]),
            kind: Some(InlayHintKind::PARAMETER),
            text_edits: None,
            tooltip: None,
            padding_left: Some(false),
            padding_right: Some(true),
            data: None,
        });

        // Enum argument: hint a valid enum value after the literal (`test(1)` → `Status.Done`).
        if enum_param_hint
            && let Some(param_ty) = param_ty
            && let Some(value_hint) = build_enum_value_hint(model, document, arg, param_ty)
        {
            result.push(value_hint);
        }
    }
    Some(())
}

fn build_enum_value_hint(
    model: &SalsaSemanticModel<'_>,
    document: &emmylua_code_analysis::DocumentView,
    arg: &LuaExpr,
    param_ty: &LuaType,
) -> Option<InlayHint> {
    let LuaType::Ref(type_id) = param_ty else {
        return None;
    };
    let def = model.type_def_of(type_id)?;
    if def.kind != TypeDefKind::Enum {
        return None;
    }
    // `test(Status.Done)`: the argument is already an enum member, so do not hint the value.
    if let LuaExpr::IndexExpr(index_expr) = arg
        && let Some(prefix_expr) = index_expr.get_prefix_expr()
        && let LuaExpr::NameExpr(name_expr) = prefix_expr
        && name_expr.get_name_text().as_deref() == Some(def.name.as_str())
    {
        return None;
    }
    // Enum fields belong to the runtime table (`Status = { Done = 1 }`); collect them by declarations with the same name or owner as def.
    let facts = model.file_facts_of(def.file_id)?;
    let runtime_table_range = facts
        .decls
        .iter()
        .find(|decl| decl.name == def.name || decl.owner_syntax == def.owner_syntax)
        .and_then(|decl| decl.value_expr_syntax)
        .map(|syntax| syntax.get_range());
    let mut member_names = facts
        .members
        .iter()
        .filter(|member| {
            member.key.name().is_some()
                && (member.owner == def.id
                    || facts.decl_by_id(&member.owner).is_some_and(|decl| {
                        decl.name == def.name || decl.owner_syntax == def.owner_syntax
                    })
                    || matches!(
                        (&member.owner, runtime_table_range),
                        (SemanticId::Member(table), Some(range)) if table.key_range == range
                    ))
        })
        .filter_map(|member| member.key.name())
        .collect::<Vec<_>>();
    member_names.sort();
    // `local Done = 1; test(Done)`: the variable name is already an enum member name, so do not hint the value.
    if let LuaExpr::NameExpr(name_expr) = arg
        && let Some(arg_name) = name_expr.get_name_text()
        && member_names.contains(&arg_name.as_str())
    {
        return None;
    }
    // String-key enums: when `test("Done")` is already a member name, do not hint the value.
    if let LuaExpr::LiteralExpr(lit) = arg
        && let Some(LuaLiteralToken::String(s)) = lit.get_literal()
        && member_names.iter().any(|m| *m == s.get_value())
    {
        return None;
    }
    let member_name = member_names.into_iter().next()?;
    let pos = document.to_lsp_position(arg.get_range().end())?;
    Some(InlayHint {
        position: pos,
        label: InlayHintLabel::String(format!("{}.{}", def.name, member_name)),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: Some(false),
        padding_right: None,
        data: None,
    })
}

fn is_primitive_type(ty: &LuaType) -> bool {
    matches!(
        ty,
        LuaType::Nil
            | LuaType::Boolean
            | LuaType::Integer
            | LuaType::Number
            | LuaType::String
            | LuaType::Thread
            | LuaType::Userdata
            | LuaType::Io
            | LuaType::Table
    )
}

/// Find the built-in library file (`builtin.lua`) location; return `None` if not found.
fn builtin_file_location(salsa: &SalsaDatabase) -> Option<Location> {
    for file_id in salsa.file_ids() {
        let document = salsa.document(file_id)?;
        if document
            .get_uri()
            .is_some_and(|uri| uri.as_str().ends_with("builtin.lua"))
        {
            let range = Range::new(
                lsp_types::Position::new(0, 0),
                lsp_types::Position::new(0, 0),
            );
            return document.get_uri().map(|uri| Location { uri, range });
        }
    }
    None
}

/// Types worth hinting: named types (class/union/array/generic instance); skip constants / primitives / functions / table identities.
fn should_hint_type(ty: &LuaType) -> bool {
    matches!(
        ty,
        LuaType::Ref(_)
            | LuaType::Def(_)
            | LuaType::Union(_)
            | LuaType::Array(_)
            | LuaType::Generic(_)
            | LuaType::Instance(_)
            | LuaType::Intersection(_)
    )
}
