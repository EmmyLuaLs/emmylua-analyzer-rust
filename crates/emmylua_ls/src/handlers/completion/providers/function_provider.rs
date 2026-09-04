//! Function context completion: call-argument type candidates / @param completion in parameter lists / function implementations for assignment targets.

use emmylua_code_analysis::{
    LuaFunctionType, LuaType, LuaTypeDeclId, SalsaSemanticModel, SemanticId,
};
use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaCallArgList, LuaCallExpr, LuaClosureExpr, LuaDocTagParam,
    LuaExpr, LuaLiteralExpr, LuaParamList, LuaSyntaxKind, LuaTokenKind,
};
use lsp_types::{CompletionItem, CompletionItemKind};

use crate::handlers::completion::completion_builder::CompletionBuilder;
use crate::handlers::signature_helper::get_current_param_index;

use super::{CompletionProvider, ProviderDecision};

pub struct FunctionProvider;

impl CompletionProvider for FunctionProvider {
    fn name(&self) -> &'static str {
        "function"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        supports_provider(builder)
    }

    fn complete(&self, builder: &mut CompletionBuilder) -> ProviderDecision {
        complete_provider(builder).unwrap_or(ProviderDecision::NoMatch)
    }
}

fn complete_provider(builder: &mut CompletionBuilder) -> Option<ProviderDecision> {
    if builder.is_cancelled() {
        return None;
    }

    let types = get_token_should_type(builder)?;
    let before = builder.get_completion_items_mut().len();
    for typ in types {
        if dispatch_type(builder, &typ) == ProviderDecision::Stop {
            return Some(ProviderDecision::Stop);
        }
    }
    dedup_new_items(builder, before);
    Some(ProviderDecision::Continue)
}

fn dedup_new_items(builder: &mut CompletionBuilder, before: usize) {
    let items = builder.get_completion_items_mut();
    let mut seen = Vec::new();
    let mut keep = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if index < before {
            keep.push(true);
            continue;
        }
        let key = (item.label.clone(), item.kind);
        if seen.contains(&key) {
            keep.push(false);
        } else {
            seen.push(key);
            keep.push(true);
        }
    }
    for index in (0..items.len()).rev() {
        if !keep[index] {
            items.remove(index);
        }
    }
}

fn supports_provider(builder: &CompletionBuilder) -> bool {
    let token = builder.trigger_token.clone();
    let Some(mut parent_node) = token.parent() else {
        return false;
    };
    if LuaLiteralExpr::can_cast(parent_node.kind().into()) {
        let Some(next_parent) = parent_node.parent() else {
            return false;
        };
        parent_node = next_parent;
    }

    matches!(
        parent_node.kind().into(),
        LuaSyntaxKind::CallArgList | LuaSyntaxKind::ParamList | LuaSyntaxKind::Block
    )
}

fn get_token_should_type(builder: &mut CompletionBuilder) -> Option<Vec<LuaType>> {
    let token = builder.trigger_token.clone();
    let mut parent_node = token.parent()?;
    if LuaLiteralExpr::can_cast(parent_node.kind().into()) {
        parent_node = parent_node.parent()?;
    }

    match parent_node.kind().into() {
        LuaSyntaxKind::CallArgList => {
            infer_call_arg_list(builder, LuaCallArgList::cast(parent_node)?, token)
        }
        LuaSyntaxKind::ParamList => {
            if builder.is_space_trigger_character {
                return None;
            }
            infer_param_list(builder, LuaParamList::cast(parent_node)?)
        }
        LuaSyntaxKind::Block => infer_assign_target(builder),
        _ => None,
    }
}

fn infer_assign_target(builder: &CompletionBuilder) -> Option<Vec<LuaType>> {
    let prev_token = builder.trigger_token.prev_token()?;
    let assign_stat = LuaAssignStat::cast(prev_token.parent()?)?;
    let (vars, exprs) = assign_stat.get_var_and_expr_list();
    if vars.len() != 1 || !exprs.is_empty() {
        return None;
    }
    let var = vars.first()?;
    let var_type = builder
        .semantic_model
        .type_of_expr(var.to_expr().get_syntax_id());
    Some(vec![var_type])
}

fn infer_param_list(
    builder: &mut CompletionBuilder,
    param_list: LuaParamList,
) -> Option<Vec<LuaType>> {
    let closure_expr = param_list.get_parent::<LuaClosureExpr>()?;
    let comment = get_closure_expr_comment(&closure_expr)?;
    let mut names = Vec::new();
    for doc_param in comment.children::<LuaDocTagParam>() {
        let Some(name) = doc_param.get_name_token() else {
            continue;
        };
        let name = name.get_name_text().to_string();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let params = param_list
        .get_params()
        .filter_map(|param| {
            param
                .get_name_token()
                .map(|token| token.get_name_text().to_string())
        })
        .collect::<Vec<_>>();
    names.retain(|name| !params.contains(name));

    if names.len() > 1 {
        builder.add_completion_item(CompletionItem {
            label: names.join(", "),
            kind: Some(CompletionItemKind::INTERFACE),
            ..Default::default()
        });
    }
    for name in names {
        builder.add_completion_item(CompletionItem {
            label: name,
            kind: Some(CompletionItemKind::INTERFACE),
            ..Default::default()
        });
    }

    // Directly added, so do not fall through to string candidates.
    None
}

fn infer_call_arg_list(
    builder: &CompletionBuilder,
    call_arg_list: LuaCallArgList,
    token: emmylua_parser::LuaSyntaxToken,
) -> Option<Vec<LuaType>> {
    let call_expr = call_arg_list.get_parent::<LuaCallExpr>()?;
    let param_idx = get_current_param_index(&call_expr, &token)?;
    let prefix_expr = call_expr.get_prefix_expr()?;

    let candidates = callable_candidates(&builder.semantic_model, &prefix_expr);
    let arg_types = call_arg_list
        .get_args()
        .map(|arg| builder.semantic_model.type_of_expr(arg.get_syntax_id()))
        .collect::<Vec<_>>();
    let mut types = Vec::new();
    for func in candidates {
        let mut param_idx = param_idx;
        let colon_call = call_expr.is_colon_call();
        match (colon_call, func.is_colon_define()) {
            (true, true) | (false, false) | (false, true) => {}
            (true, false) => param_idx += 1,
        }
        if let Some(typ) = func
            .get_params()
            .get(param_idx)
            .and_then(|param| param.1.clone())
        {
            let tpl_name = tpl_name_of(&typ);
            let typ = substitute_tpl_constraints(typ, func.get_generic_params());
            let typ = if matches!(typ, LuaType::Unknown) {
                doc_keyof_param_override(
                    &builder.semantic_model,
                    &prefix_expr,
                    tpl_name.as_deref().unwrap_or(""),
                    func.get_generic_params(),
                )
                .unwrap_or(typ)
            } else {
                typ
            };
            let typ = bind_candidate_tpls(&func, &arg_types, param_idx, typ);
            if !types.contains(&typ) {
                types.push(typ);
            }
        }
    }
    (!types.is_empty()).then_some(types)
}

/// When candidate function generic parameters have constraints, replace TplRef/StrTplRef in the parameter type with the constraint type.
fn substitute_tpl_constraints(
    ty: LuaType,
    generics: &[emmylua_code_analysis::GenericTpl],
) -> LuaType {
    match ty {
        LuaType::TplRef(tpl) => {
            if let Some(constraint) = tpl.get_constraint() {
                return constraint.clone();
            }
            generics
                .iter()
                .find(|generic| generic.get_name() == tpl.get_name())
                .and_then(|generic| generic.get_constraint())
                .cloned()
                .unwrap_or(LuaType::TplRef(tpl))
        }
        LuaType::StrTplRef(str_tpl) => {
            let constraint = str_tpl.get_constraint().cloned().or_else(|| {
                generics
                    .iter()
                    .find(|generic| generic.get_name() == str_tpl.get_name())
                    .and_then(|generic| generic.get_constraint())
                    .cloned()
            });
            LuaType::StrTplRef(std::sync::Arc::new(
                emmylua_code_analysis::LuaStringTplType::new(
                    str_tpl.get_prefix(),
                    str_tpl.get_name(),
                    str_tpl.get_tpl_id(),
                    str_tpl.get_suffix(),
                    constraint,
                ),
            ))
        }
        LuaType::Union(union) => {
            let components = union
                .into_vec()
                .iter()
                .map(|component| substitute_tpl_constraints(component.clone(), generics))
                .collect();
            LuaType::Union(std::sync::Arc::new(
                emmylua_code_analysis::LuaUnionType::from_vec(components),
            ))
        }
        other => other,
    }
}

/// When `@param key K` for `K extends keyof T` projects to Unknown, reconstruct
/// `Call(KeyOf, [TplRef(T)])` from the `---@generic` constraint syntax text
/// (the old salsa layer does not lower keyof types yet).
fn doc_keyof_param_override(
    model: &SalsaSemanticModel<'_>,
    prefix_expr: &LuaExpr,
    param_name: &str,
    generics: &[emmylua_code_analysis::GenericTpl],
) -> Option<LuaType> {
    let LuaExpr::NameExpr(name_expr) = prefix_expr else {
        return None;
    };
    let decl = model.resolve_name(name_expr.get_position())?;
    let SemanticId::Decl(key) = &decl else {
        return None;
    };
    let facts = model.file_facts_of(key.file_id)?;
    let decl_info = facts.decl_by_id(&decl)?;
    let signature = facts.signature_by_closure(decl_info.value_expr_syntax?)?;
    let docs = signature.docs.as_ref()?;
    let generic_param = docs
        .generic_params
        .iter()
        .find(|generic| generic.name.as_str() == param_name)?;
    let constraint = generic_param.constraint?;
    let tree = model.syntax_tree_of(key.file_id)?;
    let node = constraint.to_node_from_root(&tree.get_red_root())?;
    let text = node.text().to_string();
    let inner = text.trim().strip_prefix("keyof ")?.trim();
    let generic = generics
        .iter()
        .find(|generic| generic.get_name() == inner)?
        .clone();
    Some(LuaType::Call(std::sync::Arc::new(
        emmylua_code_analysis::LuaAliasCallType::new(
            emmylua_code_analysis::LuaAliasCallKind::KeyOf,
            vec![LuaType::TplRef(std::sync::Arc::new(generic))],
        ),
    )))
}

/// Bind candidate function generics from already-filled arguments: in `pick(object, <??>)`, replace `T` in `keyof T` with the object type.
fn bind_candidate_tpls(
    func: &LuaFunctionType,
    arg_types: &[LuaType],
    param_idx: usize,
    ty: LuaType,
) -> LuaType {
    let mut bindings: Vec<(String, LuaType)> = Vec::new();
    let self_offset = usize::from(func.is_colon_define());
    for (index, (_, param_ty)) in func.get_params().iter().enumerate() {
        if index >= param_idx {
            break;
        }
        let Some(arg_ty) = arg_types.get(index.saturating_sub(self_offset)) else {
            continue;
        };
        let Some(name) = param_ty.as_ref().and_then(tpl_name_of) else {
            continue;
        };
        if !bindings.iter().any(|(bound, _)| bound == &name) {
            bindings.push((name, arg_ty.clone()));
        }
    }
    substitute_tpl_in_type(&ty, &bindings)
}

fn tpl_name_of(ty: &LuaType) -> Option<String> {
    match ty {
        LuaType::TplRef(tpl) => Some(tpl.get_name().to_string()),
        LuaType::StrTplRef(str_tpl) => Some(str_tpl.get_name().to_string()),
        _ => None,
    }
}

fn substitute_tpl_in_type(ty: &LuaType, bindings: &[(String, LuaType)]) -> LuaType {
    match ty {
        LuaType::TplRef(tpl) => bindings
            .iter()
            .find(|(name, _)| name == tpl.get_name())
            .map(|(_, bound)| bound.clone())
            .unwrap_or_else(|| ty.clone()),
        LuaType::StrTplRef(str_tpl) => bindings
            .iter()
            .find(|(name, _)| name == str_tpl.get_name())
            .map(|(_, bound)| bound.clone())
            .unwrap_or_else(|| ty.clone()),
        LuaType::Union(union) => {
            let components = union
                .into_vec()
                .iter()
                .map(|component| substitute_tpl_in_type(component, bindings))
                .collect();
            LuaType::Union(std::sync::Arc::new(
                emmylua_code_analysis::LuaUnionType::from_vec(components),
            ))
        }
        LuaType::Array(array) => {
            let base = substitute_tpl_in_type(array.get_base(), bindings);
            LuaType::Array(std::sync::Arc::new(
                emmylua_code_analysis::LuaArrayType::from_base_type(base),
            ))
        }
        LuaType::Call(call) => {
            let operands = call
                .get_operands()
                .iter()
                .map(|operand| substitute_tpl_in_type(operand, bindings))
                .collect();
            LuaType::Call(std::sync::Arc::new(
                emmylua_code_analysis::LuaAliasCallType::new(call.get_call_kind(), operands),
            ))
        }
        _ => ty.clone(),
    }
}

/// Call prefix to function candidates: declarations' signatures (main + `---@overload`) take
/// priority; fall back to the projected function value type (with unions expanded).
pub(crate) fn callable_candidates(
    model: &SalsaSemanticModel<'_>,
    prefix_expr: &LuaExpr,
) -> Vec<LuaFunctionType> {
    // Name/member declarations: read the overload list in signature facts.
    if let LuaExpr::NameExpr(name_expr) = prefix_expr
        && let Some(decl) = model.resolve_name(name_expr.get_position())
        && let Some(SemanticId::Decl(key)) = Some(&decl).cloned()
        && let Some(facts) = model.file_facts_of(key.file_id)
        && let Some(decl_info) = facts.decl_by_id(&decl)
        && let Some(closure) = decl_info.value_expr_syntax
        && let Some(signature) = facts.signature_by_closure(closure)
    {
        let mut out = Vec::new();
        if let Some(docs) = signature.docs.as_ref() {
            for overload in &docs.overloads {
                if let LuaType::DocFunction(func) =
                    model.doc_type_lua_in(key.file_id, *overload, &[])
                {
                    out.push(func.as_ref().clone());
                }
            }
        }
        if let Some(main) = model.type_of_decl_signature(&decl) {
            out.push(main);
        }
        if !out.is_empty() {
            return out;
        }
    }

    // Member calls: `---@field event fun(...)` / runtime closure overloads for `T.event(...)`.
    if let LuaExpr::IndexExpr(index_expr) = prefix_expr
        && let Some(resolved) = model.resolve_member(index_expr)
    {
        let Some(member_id) = &resolved.member_id else {
            // When the runtime value and type definition have different names
            // (`---@class Test1 local Test = {}`), resolve_member may not yield a declaration id,
            // so collect same-named candidates in the file by member name.
            return member_candidates_by_name(model, &resolved.name);
        };
        let file_id = match &resolved.file_id {
            Some(file_id) => *file_id,
            None => match member_id {
                SemanticId::Member(key) => key.file_id,
                _ => {
                    return expand_callable_types(
                        model,
                        &resolved.member_type(model).unwrap_or(LuaType::Unknown),
                    );
                }
            },
        };
        if let Some(facts) = model.file_facts_of(file_id)
            && let Some(member) = facts.member_by_id(member_id)
        {
            let mut out = Vec::new();
            let key_text = member.key.to_path();
            let owner = member.owner.clone();
            for candidate in facts
                .members
                .iter()
                .filter(|candidate| candidate.owner == owner && candidate.key.to_path() == key_text)
            {
                if let Some(value) = candidate.value_syntax
                    && let Some(signature) = facts.signature_by_closure(value)
                    && let Some(docs) = signature.docs.as_ref()
                {
                    for overload in &docs.overloads {
                        if let LuaType::DocFunction(func) =
                            model.doc_type_lua_in(file_id, *overload, &[])
                        {
                            out.push(func.as_ref().clone());
                        }
                    }
                }
                if let Some(LuaType::DocFunction(func)) = model.type_of_member(&candidate.id) {
                    out.push(func.as_ref().clone());
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
    }

    let ty = model.type_of_expr(prefix_expr.get_syntax_id());
    expand_callable_types(model, &ty)
}

fn member_candidates_by_name(model: &SalsaSemanticModel<'_>, name: &str) -> Vec<LuaFunctionType> {
    let Some(facts) = model.file_facts() else {
        return Vec::new();
    };
    let file_id = model.file_id();
    let mut out = Vec::new();
    for member in facts
        .members
        .iter()
        .filter(|member| member.key.to_path() == name)
    {
        if let Some(value) = member.value_syntax
            && let Some(signature) = facts.signature_by_closure(value)
            && let Some(docs) = signature.docs.as_ref()
        {
            for overload in &docs.overloads {
                if let LuaType::DocFunction(func) = model.doc_type_lua_in(file_id, *overload, &[]) {
                    out.push(func.as_ref().clone());
                }
            }
        }
        if let Some(LuaType::DocFunction(func)) = model.type_of_member(&member.id) {
            out.push(func.as_ref().clone());
        }
    }
    out
}

fn expand_callable_types(_model: &SalsaSemanticModel<'_>, ty: &LuaType) -> Vec<LuaFunctionType> {
    match ty {
        LuaType::DocFunction(func) => vec![func.as_ref().clone()],
        LuaType::Function => vec![LuaFunctionType::new(
            emmylua_code_analysis::AsyncState::None,
            false,
            false,
            Vec::new(),
            LuaType::Unknown,
            None,
        )],
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .flat_map(|ty| expand_callable_types(_model, ty))
            .collect(),
        _ => Vec::new(),
    }
}

pub(super) fn dispatch_type(builder: &mut CompletionBuilder, typ: &LuaType) -> ProviderDecision {
    dispatch_type_visited(builder, typ, &mut Vec::new())
}

fn dispatch_type_visited(
    builder: &mut CompletionBuilder,
    typ: &LuaType,
    visited: &mut Vec<LuaTypeDeclId>,
) -> ProviderDecision {
    match typ {
        LuaType::DocFunction(func) => add_lambda_completion(builder, func),
        LuaType::DocStringConst(key) | LuaType::StringConst(key) => {
            add_string_completion(builder, key.as_str())
        }
        LuaType::DocIntegerConst(value) | LuaType::IntegerConst(value) => {
            add_integer_completion(builder, *value)
        }
        LuaType::StrTplRef(str_tpl) => add_str_tpl_ref_completion(builder, str_tpl),
        LuaType::Call(call)
            if call.get_call_kind() == emmylua_code_analysis::LuaAliasCallKind::KeyOf =>
        {
            add_keyof_completion(builder, call.get_operands().first())
        }
        LuaType::TplRef(tpl) => {
            if let Some(constraint) = tpl.get_constraint() {
                dispatch_type_visited(builder, constraint, visited)
            } else {
                ProviderDecision::Continue
            }
        }
        LuaType::Ref(type_id) | LuaType::Def(type_id) => {
            if let Some(def) = builder.semantic_model.type_def_of(type_id) {
                if def.kind == emmylua_code_analysis::TypeDefKind::Alias
                    && let Some(target) = builder.semantic_model.alias_target(&def)
                {
                    if visited.contains(type_id) {
                        return ProviderDecision::Continue;
                    }
                    visited.push(type_id.clone());
                    let decision = dispatch_type_visited(builder, &target, visited);
                    visited.pop();
                    return decision;
                }
            }
            add_named_type_completion(builder, type_id)
        }
        LuaType::Union(union) => {
            for component in union.into_vec().iter() {
                match component {
                    LuaType::DocStringConst(s) | LuaType::StringConst(s) => {
                        add_string_completion(builder, s.as_str());
                    }
                    LuaType::DocIntegerConst(i) | LuaType::IntegerConst(i) => {
                        add_integer_completion(builder, *i);
                    }
                    other => {
                        dispatch_type_visited(builder, other, visited);
                    }
                }
            }
            ProviderDecision::Continue
        }
        LuaType::MultiLineUnion(union) => {
            for (component, description) in union.get_unions() {
                if let Some(description) = description {
                    // First add the plain enum items, then overwrite with the description.
                    let before = builder.get_completion_items_mut().len();
                    dispatch_type_visited(builder, component, visited);
                    for item in builder.get_completion_items_mut().iter_mut().skip(before) {
                        item.label_details = Some(lsp_types::CompletionItemLabelDetails {
                            detail: None,
                            description: Some(description.clone()),
                        });
                        item.documentation =
                            Some(lsp_types::Documentation::String(description.clone()));
                    }
                } else {
                    dispatch_type_visited(builder, component, visited);
                }
            }
            ProviderDecision::Continue
        }
        _ => ProviderDecision::Continue,
    }
}

fn enum_is_key_def(builder: &CompletionBuilder, def: &emmylua_code_analysis::TypeDef) -> bool {
    let Some(document) = builder.semantic_model.document(def.file_id) else {
        return false;
    };
    let range = rowan::TextRange::up_to(def.name_range.start());
    let prefix = document.get_text_slice(range);
    let start = prefix.len().saturating_sub(32);
    prefix[start..].contains("(key)")
}

fn add_named_type_completion(
    builder: &mut CompletionBuilder,
    type_id: &LuaTypeDeclId,
) -> ProviderDecision {
    let Some(def) = builder.semantic_model.type_def_of(type_id) else {
        return ProviderDecision::Continue;
    };
    if def.kind != emmylua_code_analysis::TypeDefKind::Enum {
        return ProviderDecision::Continue;
    }

    let Some(facts) = builder.semantic_model.file_facts_of(def.file_id) else {
        return ProviderDecision::Continue;
    };
    // `---@enum C6.Param` followed by `local EP = {...}`: runtime member owner is the EP declaration.
    let runtime_decl = facts
        .decls
        .iter()
        .find(|decl| decl.owner_syntax == def.owner_syntax);
    let runtime_name = runtime_decl.map(|decl| decl.name.to_string());
    let mut members: Vec<_> = facts
        .members
        .iter()
        .filter(|member| {
            member.owner == def.id || runtime_decl.is_some_and(|decl| member.owner == decl.id)
        })
        .filter_map(|member| member.key.name().map(|name| name.to_string()))
        .collect();
    members.sort();

    let in_string = matches!(
        builder.trigger_token.kind().into(),
        LuaTokenKind::TkString | LuaTokenKind::TkLongString
    );
    let key_enum = enum_is_key_def(builder, &def);
    for name in members {
        let label = if in_string || key_enum {
            if in_string {
                name.clone()
            } else {
                format!("\"{}\"", name)
            }
        } else if let Some(variable) = &runtime_name {
            format!("{variable}.{name}")
        } else {
            format!("\"{}\"", name)
        };
        builder.add_completion_item(CompletionItem {
            label,
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            label_details: Some(lsp_types::CompletionItemLabelDetails {
                detail: None,
                description: Some(def.name.to_string()),
            }),
            ..Default::default()
        });
    }
    ProviderDecision::Stop
}

fn add_keyof_completion(
    builder: &mut CompletionBuilder,
    operand: Option<&LuaType>,
) -> ProviderDecision {
    let Some(operand) = operand else {
        return ProviderDecision::Continue;
    };
    let mut keys: Vec<String> = builder
        .semantic_model
        .member_infos(operand)
        .into_iter()
        .filter_map(|info| match info.key {
            emmylua_code_analysis::LuaMemberKey::Name(name) => Some(name.to_string()),
            emmylua_code_analysis::LuaMemberKey::Integer(index) => Some(index.to_string()),
            _ => None,
        })
        .collect();
    keys.sort();
    for key in keys {
        add_string_completion(builder, &key);
    }
    ProviderDecision::Continue
}

fn add_str_tpl_ref_completion(
    builder: &mut CompletionBuilder,
    str_tpl: &emmylua_code_analysis::LuaStringTplType,
) -> ProviderDecision {
    let prefix = str_tpl.get_prefix();
    let suffix = str_tpl.get_suffix();
    let constraint = str_tpl.get_constraint();
    let mut names = Vec::new();
    for file_id in builder.semantic_model.file_ids() {
        let Some(model) = builder.semantic_model.model_for(file_id) else {
            continue;
        };
        let Some(facts) = model.file_facts() else {
            continue;
        };
        for def in facts
            .type_defs
            .iter()
            .filter(|def| def.kind == emmylua_code_analysis::TypeDefKind::Class && !def.flags.meta)
        {
            let full_name = def.full_name.as_str();
            if (!prefix.is_empty() && !full_name.starts_with(prefix))
                || (!suffix.is_empty() && !full_name.ends_with(suffix))
            {
                continue;
            }
            if let Some(constraint) = constraint
                && !type_def_matches_constraint(&model, def, constraint)
            {
                continue;
            }
            let trimmed = full_name
                .trim_start_matches(prefix)
                .trim_end_matches(suffix);
            if !names.contains(&trimmed.to_string()) {
                names.push(trimmed.to_string());
            }
        }
    }
    names.sort();
    for name in names {
        add_string_completion(builder, &name);
    }
    ProviderDecision::Continue
}

/// Whether a type definition satisfies a `` `T`` constraint (including inheritance chains / unions / primitive supers).
fn type_def_matches_constraint(
    model: &SalsaSemanticModel<'_>,
    def: &emmylua_code_analysis::TypeDef,
    constraint: &LuaType,
) -> bool {
    match constraint {
        LuaType::Ref(id) | LuaType::Def(id) => {
            let Some(target) = model.type_def_of(id) else {
                return true;
            };
            if def.id == target.id {
                return true;
            }
            let mut stack = vec![def.clone()];
            let mut visited = Vec::new();
            while let Some(current) = stack.pop() {
                if current.id == target.id {
                    return true;
                }
                if visited.contains(&current.id) {
                    continue;
                }
                visited.push(current.id.clone());
                for super_name in &current.super_names {
                    if let Some(parent) =
                        model.resolve_type_def_in(current.file_id, super_name.as_str())
                    {
                        stack.push(parent);
                    }
                }
            }
            false
        }
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .any(|component| type_def_matches_constraint(model, def, component)),
        LuaType::Any | LuaType::Unknown => true,
        primitive => {
            let primitive_name = crate::handlers::hover::render::humanize(model, primitive);
            let mut stack = vec![def.clone()];
            let mut visited = Vec::new();
            while let Some(current) = stack.pop() {
                if visited.contains(&current.id) {
                    continue;
                }
                visited.push(current.id.clone());
                if current
                    .super_names
                    .iter()
                    .any(|name| name.as_str() == primitive_name)
                {
                    return true;
                }
                for super_name in &current.super_names {
                    if let Some(parent) =
                        model.resolve_type_def_in(current.file_id, super_name.as_str())
                    {
                        stack.push(parent);
                    }
                }
            }
            false
        }
    }
}

fn add_lambda_completion(
    builder: &mut CompletionBuilder,
    func: &LuaFunctionType,
) -> ProviderDecision {
    let params_str = func
        .get_params()
        .iter()
        .map(|p| p.0.clone())
        .collect::<Vec<_>>();
    let label = format!("function({}) end", params_str.join(", "));
    let insert_text = format!("function({})\n\t$0\nend", params_str.join(", "));
    builder.add_completion_item(CompletionItem {
        label,
        kind: Some(CompletionItemKind::FUNCTION),
        sort_text: Some("0".to_string()),
        insert_text: Some(insert_text),
        insert_text_format: Some(lsp_types::InsertTextFormat::SNIPPET),
        ..Default::default()
    });
    ProviderDecision::Continue
}

fn add_string_completion(builder: &mut CompletionBuilder, value: &str) -> ProviderDecision {
    let label = if matches!(
        builder.trigger_token.kind().into(),
        LuaTokenKind::TkString | LuaTokenKind::TkLongString
    ) {
        value.to_string()
    } else {
        format!("\"{}\"", value)
    };
    builder.add_completion_item(CompletionItem {
        label,
        kind: Some(CompletionItemKind::ENUM_MEMBER),
        ..Default::default()
    });
    ProviderDecision::Continue
}

fn add_integer_completion(builder: &mut CompletionBuilder, value: i64) -> ProviderDecision {
    builder.add_completion_item(CompletionItem {
        label: value.to_string(),
        kind: Some(CompletionItemKind::ENUM_MEMBER),
        ..Default::default()
    });
    ProviderDecision::Continue
}

/// Function implementation snippet for an assignment target like `c1.on_add = <??>`.
pub fn add_function_impl_completion(builder: &mut CompletionBuilder, ty: &LuaType) -> bool {
    let func = resolve_function_type(builder, ty, &mut Vec::new());
    let Some(func) = func else {
        return false;
    };
    let params = func
        .get_params()
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let label = format!("function({}) end", params.join(", "));
    builder
        .add_completion_item(CompletionItem {
            label: label.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            insert_text: Some(label),
            ..Default::default()
        })
        .is_some()
}

/// Expand `fun?` / alias `fun(...)` into a function type.
fn resolve_function_type(
    builder: &CompletionBuilder,
    ty: &LuaType,
    visited: &mut Vec<LuaTypeDeclId>,
) -> Option<LuaFunctionType> {
    match ty {
        LuaType::DocFunction(func) => Some(func.as_ref().clone()),
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .find_map(|component| resolve_function_type(builder, component, visited)),
        LuaType::Ref(id) | LuaType::Def(id) => {
            if visited.contains(id) {
                return None;
            }
            let def = builder.semantic_model.type_def_of(id)?;
            visited.push(id.clone());
            let target = builder.semantic_model.alias_target(&def)?;
            let result = resolve_function_type(builder, &target, visited);
            visited.pop();
            result
        }
        _ => None,
    }
}

fn get_closure_expr_comment(closure_expr: &LuaClosureExpr) -> Option<emmylua_parser::LuaComment> {
    let comment = closure_expr
        .ancestors::<emmylua_parser::LuaStat>()
        .next()?
        .syntax()
        .prev_sibling()?;
    match comment.kind().into() {
        LuaSyntaxKind::Comment => emmylua_parser::LuaComment::cast(comment),
        _ => None,
    }
}
