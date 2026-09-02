//! # generic_constraint_mismatch - generic arguments / default values violate constraints
//!
//! Covers:
//! 1. generic bindings inferred from call arguments;
//! 2. generic default values vs constraints in `---@class` / `---@alias`;
//! 3. explicit arguments vs constraints in `---@type Base<string>` / other doc generic instantiations;
//! 4. `keyof` / conditional types / `` `T` `` string template constraints.

use std::collections::HashMap;

use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaDocType, LuaExpr, LuaIndexExpr};

use crate::DiagnosticCode;
use crate::salsa_builder::def::{SalsaGenericParam, SemanticId, TypeDef};
use crate::semantic_model::SemanticModel;
use crate::semantic_model::infer::unify;
use crate::semantic_model::type_check::is_compatible;
use crate::{FileId, GenericTplId, LuaType};

use super::param_count::first_param_is_self;
use super::{CheckContext, Checker};
use crate::semantic_model::render::humanize_type;

pub struct GenericConstraintMismatchChecker;

impl Checker for GenericConstraintMismatchChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::GenericConstraintMismatch];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        check_type_definitions(context, semantic_model);
        check_type_instantiations(context, semantic_model);

        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for call_expr in root.descendants().filter_map(LuaCallExpr::cast) {
            check_call(context, semantic_model, &call_expr);
        }
    }
}

/// `---@class Base<T extends number = string>`: the default type must satisfy the constraint.
fn check_type_definitions(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
    let Some(facts) = semantic_model.file_facts() else {
        return;
    };
    for def in &facts.type_defs {
        for param in &def.generic_params {
            let (Some(constraint), Some(default)) = (param.constraint, param.default) else {
                continue;
            };
            let constraint_ty = project_doc_type_with(
                semantic_model,
                def.file_id,
                constraint,
                &def.generic_params,
                &HashMap::new(),
            );
            let default_ty = project_doc_type_with(
                semantic_model,
                def.file_id,
                default,
                &def.generic_params,
                &HashMap::new(),
            );
            let compatible = dependent_default_compatible(
                semantic_model,
                &def.generic_params,
                param,
                &default_ty,
                &constraint_ty,
            );
            if compatible {
                continue;
            }
            let range =
                doc_syntax_range(semantic_model, def.file_id, default).unwrap_or(def.name_range);
            add_constraint_diagnostic(context, range, &default_ty, &constraint_ty);
        }
    }
    // `---@generic T extends number = string` on function signatures.
    for signature in &facts.signatures {
        let Some(docs) = &signature.docs else {
            continue;
        };
        for param in &docs.generic_params {
            let (Some(constraint), Some(default)) = (param.constraint, param.default) else {
                continue;
            };
            let constraint_ty = project_doc_type_with(
                semantic_model,
                signature.file_id,
                constraint,
                &docs.generic_params,
                &HashMap::new(),
            );
            let default_ty = project_doc_type_with(
                semantic_model,
                signature.file_id,
                default,
                &docs.generic_params,
                &HashMap::new(),
            );
            let compatible = dependent_default_compatible(
                semantic_model,
                &docs.generic_params,
                param,
                &default_ty,
                &constraint_ty,
            );
            if compatible {
                continue;
            }
            let range = doc_syntax_range(semantic_model, signature.file_id, default)
                .unwrap_or(signature.closure_syntax.get_range());
            add_constraint_diagnostic(context, range, &default_ty, &constraint_ty);
        }
    }
}

/// All `Base<string>` / `Alias<...>` generic instantiations in the file.
fn check_type_instantiations(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
    let Some(tree) = semantic_model.syntax_tree() else {
        return;
    };
    let root = tree.get_red_root();
    for generic in root.descendants().filter_map(LuaDocType::cast) {
        let LuaDocType::Generic(generic) = generic else {
            continue;
        };
        let Some(name) = generic.get_name_type().and_then(|n| n.get_name_text()) else {
            continue;
        };
        let Some(def) = semantic_model.resolve_type_def(&name) else {
            continue;
        };
        let Some(args) = generic.get_generic_types() else {
            continue;
        };
        let arg_tys: Vec<LuaType> = args
            .get_types()
            .map(|arg| {
                project_doc_type(
                    semantic_model,
                    semantic_model.file_id(),
                    arg.get_syntax_id(),
                )
            })
            .collect();
        if arg_tys.is_empty() {
            continue;
        }
        // Explicit arguments are substituted in declaration order: T in `K extends keyof T` is resolved from the first argument.
        let mut substitutions = HashMap::new();
        for (index, param) in def.generic_params.iter().enumerate() {
            if let Some(arg_ty) = arg_tys.get(index) {
                substitutions.insert(param.name.to_string(), arg_ty.clone());
            }
        }
        for (index, (arg_ty, arg)) in arg_tys.iter().zip(args.get_types()).enumerate() {
            let Some(param) = def.generic_params.get(index) else {
                break;
            };
            let Some(constraint) = param.constraint else {
                continue;
            };
            let constraint_ty = project_doc_type_with(
                semantic_model,
                def.file_id,
                constraint,
                &def.generic_params,
                &substitutions,
            );
            if constraint_compatible(semantic_model, arg_ty, &constraint_ty) {
                continue;
            }
            add_constraint_diagnostic(context, arg.get_range(), arg_ty, &constraint_ty);
        }
    }
}

fn check_call(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
) {
    let Some(callee) = call_expr.get_prefix_expr() else {
        return;
    };
    let analysis = semantic_model.call_site_analysis(call_expr);
    let owner_constraints = owner_generic_constraints(semantic_model, &callee);
    // Fast path: if neither the callee nor its owner has generic parameters, there
    // are no generic-constraint diagnostics to produce. Avoid computing/resolving
    // call signatures entirely for the vast majority of non-generic calls.
    if owner_constraints.is_empty()
        && analysis
            .candidates
            .iter()
            .all(|fun| fun.get_generic_params().is_empty())
    {
        return;
    }
    let signatures = semantic_model.call_site_signatures(call_expr);
    for (fun, bindings) in &signatures {
        // Constraints carried by the signature's generic parameters. Prefer re-projecting from the signature doc syntax using bindings:
        // `K extends keyof T` / conditional-type constraints require AST-level evaluation.
        let mut constraints: Vec<(GenericTplId, LuaType)> = Vec::new();
        if let Some((file_id, closure_syntax)) = signature_docs_of_callee(semantic_model, &callee)
            && let Some(facts) = semantic_model.file_facts_of(file_id)
            && let Some(signature) = facts.signature_by_closure(closure_syntax)
            && let Some(docs) = signature.docs.as_ref()
        {
            let mut substitutions = HashMap::new();
            for (index, param) in docs.generic_params.iter().enumerate() {
                let bound = fun
                    .get_generic_params()
                    .get(index)
                    .and_then(|generic| bindings.get(&generic.get_tpl_id()).cloned())
                    .or_else(|| {
                        param.default.map(|syntax| {
                            project_doc_type_with(
                                semantic_model,
                                file_id,
                                syntax,
                                &docs.generic_params,
                                &HashMap::new(),
                            )
                        })
                    });
                if let Some(bound) = bound {
                    substitutions.insert(param.name.to_string(), bound);
                }
            }
            for (index, param) in docs.generic_params.iter().enumerate() {
                if let Some(constraint) = param.constraint {
                    constraints.push((
                        GenericTplId::Type(index as u32),
                        project_doc_type_with(
                            semantic_model,
                            file_id,
                            constraint,
                            &docs.generic_params,
                            &substitutions,
                        ),
                    ));
                }
            }
        } else {
            constraints.extend(fun.get_generic_params().iter().filter_map(|param| {
                param
                    .get_constraint()
                    .map(|constraint| (param.get_tpl_id(), constraint.clone()))
            }));
        }
        // Class member methods: `---@param a T` on `function M.new(a)` only projects an unconstrained TplRef,
        // the constraint lives on the owner `---@class M<T: Component>`.
        for (_, param_ty) in fun.get_params() {
            let Some(param_ty) = param_ty else {
                continue;
            };
            for tpl_ref in tpl_refs_in(param_ty) {
                if constraints
                    .iter()
                    .any(|(id, _)| *id == tpl_ref.get_tpl_id())
                {
                    continue;
                }
                let Some((_, constraint)) = owner_constraints.get(tpl_ref.get_name()) else {
                    continue;
                };
                constraints.push((tpl_ref.get_tpl_id(), constraint.clone()));
            }
        }

        let range = call_expr
            .get_args_list()
            .map(|list| list.get_range())
            .unwrap_or_else(|| call_expr.get_range());
        for (tpl_id, constraint) in constraints {
            let Some(actual) = bindings.get(&tpl_id) else {
                continue;
            };
            if matches!(actual, LuaType::Unknown | LuaType::Any | LuaType::Never) {
                continue;
            }
            // In a method of `GCNode<T: table>`, passing obj: T to `add(obj: T)`:
            // when the actual argument is still an uninstantiated Ref("T"), check against T's constraint on the owner class.
            let actual = unbound_generic_actual(actual, &owner_constraints, &fun);
            // When the constraint references an uninferred T, substitute the call binding; otherwise fall back to `---@generic T = default`.
            let resolved_constraint = match &constraint {
                LuaType::TplRef(tpl) => bindings
                    .get(&tpl.get_tpl_id())
                    .cloned()
                    .or_else(|| {
                        fun.get_generic_params()
                            .iter()
                            .find(|param| param.get_tpl_id() == tpl.get_tpl_id())
                            .and_then(|param| param.get_default_type().cloned())
                    })
                    .unwrap_or(constraint),
                _ => constraint.clone(),
            };
            if call_constraint_compatible(semantic_model, &actual, &resolved_constraint) {
                continue;
            }
            add_constraint_diagnostic(context, range, &actual, &resolved_constraint);
        }

        check_str_tpl_params(
            context,
            semantic_model,
            call_expr,
            &fun,
            &owner_constraints,
            &analysis.arg_types,
        );
    }
}

/// callee expression -> the `(file_id, closure_syntax)` of the signature doc at its definition.
fn signature_docs_of_callee(
    semantic_model: &SemanticModel<'_>,
    callee: &LuaExpr,
) -> Option<(FileId, emmylua_parser::LuaSyntaxId)> {
    match callee {
        LuaExpr::NameExpr(name_expr) => {
            let decl = semantic_model.resolve_name(name_expr.get_position())?;
            let SemanticId::Decl(decl_key) = decl else {
                return None;
            };
            let facts = semantic_model.file_facts_of(decl_key.file_id)?;
            let decl = facts.decl_by_id(&SemanticId::Decl(decl_key))?;
            Some((decl.file_id, decl.value_expr_syntax?))
        }
        LuaExpr::IndexExpr(index_expr) => {
            let resolved = semantic_model.resolve_member(index_expr)?;
            let member_file = resolved.file_id?;
            let facts = semantic_model.file_facts_of(member_file)?;
            let member = facts.member_by_id(&resolved.member_id?)?;
            Some((member_file, member.value_syntax?))
        }
        _ => None,
    }
}

/// callee -> candidate signatures plus each candidate's independent generic bindings.
pub(crate) fn resolved_call_signatures(
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
    candidates: &[crate::LuaFunctionType],
    arg_types: &[LuaType],
    colon_call: bool,
    receiver_ty: &LuaType,
) -> Vec<(crate::LuaFunctionType, unify::TplBindings)> {
    let Some(callee) = call_expr.get_prefix_expr() else {
        return Vec::new();
    };
    let owner_constraints = owner_generic_constraints(semantic_model, &callee);

    let mut out = Vec::new();
    for fun in candidates {
        let fun = apply_owner_generics(fun.clone(), &owner_constraints);
        let params = fun.get_params();
        let self_param = first_param_is_self(&fun);
        let param_start = usize::from(colon_call && self_param);
        let mut bindings = unify::TplBindings::new();
        if colon_call && !self_param {
            if let Some((_, Some(param_ty))) = params.first() {
                let _ = unify::unify_bindings(param_ty, receiver_ty, &mut bindings);
            }
        }
        for ((_, param_ty), arg_ty) in params[param_start..].iter().zip(arg_types.iter()) {
            let Some(param_ty) = param_ty else {
                continue;
            };
            let _ = unify::unify_bindings(param_ty, arg_ty, &mut bindings);
        }
        out.push((fun, bindings));
    }
    out
}

/// Generic constraints referenced by class member methods: `SemanticId::Member -> owner TypeDef.generic_params`.
fn owner_generic_constraints(
    semantic_model: &SemanticModel<'_>,
    callee: &LuaExpr,
) -> HashMap<smol_str::SmolStr, (usize, LuaType)> {
    let mut out = HashMap::new();
    let Some(index_expr) = LuaIndexExpr::cast(callee.syntax().clone()) else {
        return out;
    };
    let Some(resolved) = semantic_model.resolve_member(&index_expr) else {
        return out;
    };
    let (Some(member_id), Some(member_file)) = (resolved.member_id, resolved.file_id) else {
        return out;
    };
    let Some(facts) = semantic_model.file_facts_of(member_file) else {
        return out;
    };
    let Some(member) = facts.member_by_id(&member_id) else {
        return out;
    };
    let type_def_id = match &member.owner {
        SemanticId::TypeDef(type_def_id) => type_def_id.clone(),
        SemanticId::Decl(_) => {
            let Some(owner_decl) = facts.decl_by_id(&member.owner) else {
                return out;
            };
            let Some(def) = facts.type_defs.iter().find(|def| {
                def.owner_syntax.is_some() && def.owner_syntax == owner_decl.owner_syntax
            }) else {
                return out;
            };
            match &def.id {
                SemanticId::TypeDef(type_def_id) => type_def_id.clone(),
                _ => return out,
            }
        }
        _ => return out,
    };
    let Some(def) = facts.type_def_by_id(&SemanticId::TypeDef(type_def_id)) else {
        return out;
    };
    for (index, param) in def.generic_params.iter().enumerate() {
        let Some(constraint) = param.constraint else {
            continue;
        };
        let ty = project_doc_type_with(
            semantic_model,
            def.file_id,
            constraint,
            &def.generic_params,
            &HashMap::new(),
        );
        out.insert(param.name.clone(), (index, ty));
    }
    out
}

/// Uninstantiated `Ref("T")` actual argument -> the constraint of owner class generic `T`.
fn unbound_generic_actual(
    actual: &LuaType,
    owner_constraints: &HashMap<smol_str::SmolStr, (usize, LuaType)>,
    fun: &crate::LuaFunctionType,
) -> LuaType {
    let (LuaType::Ref(id) | LuaType::Def(id)) = actual else {
        return actual.clone();
    };
    let name = id.get_name();
    if let Some((_, constraint)) = owner_constraints.get(name) {
        return constraint.clone();
    }
    if let Some(param) = fun
        .get_generic_params()
        .iter()
        .find(|param| param.get_name() == name)
        && let Some(constraint) = param.get_constraint()
    {
        return constraint.clone();
    }
    actual.clone()
}

/// Convert `Ref("T")` in runtime member signatures back to `TplRef` with the owner class constraints.
fn apply_owner_generics(
    fun: crate::LuaFunctionType,
    owner_constraints: &HashMap<smol_str::SmolStr, (usize, LuaType)>,
) -> crate::LuaFunctionType {
    if owner_constraints.is_empty() {
        return fun;
    }
    let params = fun
        .get_params()
        .iter()
        .map(|(name, ty)| {
            let ty = ty
                .as_ref()
                .and_then(|ty| bind_owner_generic_type(ty, owner_constraints));
            (name.clone(), ty)
        })
        .collect();
    crate::LuaFunctionType::new(
        fun.get_async_state(),
        fun.is_colon_define(),
        fun.is_variadic(),
        params,
        fun.get_ret().clone(),
        Some(fun.get_generic_params().to_vec()),
    )
}

fn bind_owner_generic_type(
    ty: &LuaType,
    owner_constraints: &HashMap<smol_str::SmolStr, (usize, LuaType)>,
) -> Option<LuaType> {
    match ty {
        LuaType::Ref(id) | LuaType::Def(id) => {
            let name = id.get_name();
            owner_constraints
                .get(name)
                .map(|(index, constraint)| {
                    LuaType::TplRef(std::sync::Arc::new(crate::GenericTpl::new(
                        GenericTplId::Type(*index as u32),
                        smol_str::SmolStr::new(name),
                        Some(constraint.clone()),
                        None,
                        false,
                        None,
                    )))
                })
                .or_else(|| Some(ty.clone()))
        }
        LuaType::StrTplRef(str_tpl) => {
            let name = str_tpl.get_name();
            if let Some((_, constraint)) = owner_constraints.get(name) {
                Some(LuaType::StrTplRef(std::sync::Arc::new(
                    crate::LuaStringTplType::new(
                        str_tpl.get_prefix(),
                        str_tpl.get_name(),
                        str_tpl.get_tpl_id(),
                        str_tpl.get_suffix(),
                        Some(constraint.clone()),
                    ),
                )))
            } else {
                Some(ty.clone())
            }
        }
        LuaType::Union(union) => {
            let types = union
                .into_vec()
                .iter()
                .map(|component| {
                    bind_owner_generic_type(component, owner_constraints)
                        .unwrap_or_else(|| component.clone())
                })
                .collect();
            Some(LuaType::Union(std::sync::Arc::new(
                crate::LuaUnionType::from_vec(types),
            )))
        }
        _ => Some(ty.clone()),
    }
}

/// `---@param t \`T\``: the string argument must be a declared type name; if constrained, also check it against that constraint.
fn check_str_tpl_params(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    call_expr: &LuaCallExpr,
    fun: &crate::LuaFunctionType,
    owner_constraints: &HashMap<smol_str::SmolStr, (usize, LuaType)>,
    arg_types: &[LuaType],
) {
    let args = call_expr
        .get_args_list()
        .map(|list| list.get_args().collect::<Vec<_>>())
        .unwrap_or_default();
    if args.is_empty() {
        return;
    }
    let colon_call = call_expr.is_colon_call();
    let self_param = first_param_is_self(fun);
    let param_start = usize::from(colon_call && self_param);
    let params = fun.get_params();
    for (index, (_, param_ty)) in params[param_start..].iter().enumerate() {
        let Some(param_ty) = param_ty else {
            continue;
        };
        let Some(arg) = args.get(index) else {
            continue;
        };
        for str_tpl in str_tpl_refs_in(param_ty) {
            let arg_ty = arg_types
                .get(index)
                .cloned()
                .unwrap_or_else(|| semantic_model.type_of_expr(arg.get_syntax_id()));
            // For a union parameter `` `T`|T ``: when the argument is not a string, only the T branch applies, so don't report a template type error.
            if matches!(param_ty, LuaType::Union(_))
                && !matches!(
                    arg_ty,
                    LuaType::String
                        | LuaType::StringConst(_)
                        | LuaType::DocStringConst(_)
                        | LuaType::StrTplRef(_)
                )
            {
                continue;
            }
            let string_value = match &arg_ty {
                LuaType::StringConst(s) | LuaType::DocStringConst(s) => s.as_str(),
                LuaType::String | LuaType::Any | LuaType::Unknown | LuaType::StrTplRef(_) => {
                    continue;
                }
                _ => {
                    add_constraint_diagnostic(context, arg.get_range(), &arg_ty, &LuaType::String);
                    continue;
                }
            };
            let full_type_name = format!(
                "{}{}{}",
                str_tpl.get_prefix(),
                string_value,
                str_tpl.get_suffix()
            );
            let Some(def) = semantic_model.resolve_type_def(&full_type_name) else {
                context.add_diagnostic(
                    DiagnosticCode::GenericConstraintMismatch,
                    arg.get_range(),
                    t!(
                        "the string template type `%{name}` does not match any type declaration",
                        name = full_type_name
                    ),
                );
                continue;
            };
            let constraint = str_tpl
                .get_constraint()
                .cloned()
                .or_else(|| {
                    owner_constraints
                        .get(str_tpl.get_name())
                        .map(|(_, constraint)| constraint.clone())
                })
                .or_else(|| {
                    fun.get_generic_params()
                        .iter()
                        .find(|param| param.get_name() == str_tpl.get_name())
                        .and_then(|param| param.get_constraint().cloned())
                });
            if let Some(constraint) = constraint {
                let resolved_ty = semantic_model.type_def_ref(&def);
                if !constraint_compatible(semantic_model, &resolved_ty, &constraint) {
                    add_constraint_diagnostic(context, arg.get_range(), &resolved_ty, &constraint);
                }
            }
        }
    }
}

fn tpl_refs_in(ty: &LuaType) -> Vec<std::sync::Arc<crate::GenericTpl>> {
    let mut out = Vec::new();
    collect_tpl_refs(ty, &mut out, 0);
    out
}

fn collect_tpl_refs(ty: &LuaType, out: &mut Vec<std::sync::Arc<crate::GenericTpl>>, depth: usize) {
    if depth > 16 {
        return;
    }
    match ty {
        LuaType::TplRef(tpl) => out.push(tpl.clone()),
        LuaType::Union(union) => {
            for component in union.into_vec() {
                collect_tpl_refs(&component, out, depth + 1);
            }
        }
        LuaType::Intersection(intersection) => {
            for component in intersection.get_types() {
                collect_tpl_refs(component, out, depth + 1);
            }
        }
        LuaType::Array(array) => collect_tpl_refs(array.get_base(), out, depth + 1),
        _ => {}
    }
}

fn str_tpl_refs_in(ty: &LuaType) -> Vec<std::sync::Arc<crate::LuaStringTplType>> {
    let mut out = Vec::new();
    collect_str_tpl_refs(ty, &mut out, 0);
    out
}

fn collect_str_tpl_refs(
    ty: &LuaType,
    out: &mut Vec<std::sync::Arc<crate::LuaStringTplType>>,
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    match ty {
        LuaType::StrTplRef(str_tpl) => out.push(str_tpl.clone()),
        LuaType::Union(union) => {
            for component in union.into_vec() {
                collect_str_tpl_refs(&component, out, depth + 1);
            }
        }
        _ => {}
    }
}

/// doc type node -> projected `LuaType`.
fn project_doc_type(
    semantic_model: &SemanticModel<'_>,
    file_id: FileId,
    syntax: emmylua_parser::LuaSyntaxId,
) -> LuaType {
    project_doc_type_with(semantic_model, file_id, syntax, &[], &HashMap::new())
}

/// Projection with generic context + explicit argument substitution:
/// - literal -> constant type;
/// - `keyof A` -> union of member keys; unsubstituted `keyof T` -> rigid TplRef marker;
/// - conditional types are evaluated according to bindings.
fn project_doc_type_with(
    semantic_model: &SemanticModel<'_>,
    file_id: FileId,
    syntax: emmylua_parser::LuaSyntaxId,
    generics: &[SalsaGenericParam],
    substitutions: &HashMap<String, LuaType>,
) -> LuaType {
    let Some(tree) = semantic_model.syntax_tree_of(file_id) else {
        return semantic_model.doc_type_lua_rich_in(file_id, syntax);
    };
    let Some(node) = syntax.to_node_from_root(&tree.get_red_root()) else {
        return semantic_model.doc_type_lua_rich_in(file_id, syntax);
    };
    let Some(doc_ty) = LuaDocType::cast(node) else {
        return semantic_model.doc_type_lua_rich_in(file_id, syntax);
    };
    match &doc_ty {
        LuaDocType::Name(name_ty) => {
            let Some(name) = name_ty.get_name_text() else {
                return semantic_model.doc_type_lua_rich_in(file_id, syntax);
            };
            if let Some(bound) = substitutions.get(&name) {
                return bound.clone();
            }
            if let Some((index, _)) = generics
                .iter()
                .enumerate()
                .find(|(_, param)| param.name == name)
            {
                return LuaType::TplRef(std::sync::Arc::new(crate::GenericTpl::new(
                    GenericTplId::Type(index as u32),
                    smol_str::SmolStr::new(name),
                    None,
                    None,
                    false,
                    None,
                )));
            }
            semantic_model.doc_type_lua_rich_in(file_id, syntax)
        }
        LuaDocType::Unary(unary) => {
            if !unary
                .get_op_token()
                .is_some_and(|op| op.get_op() == emmylua_parser::LuaTypeUnaryOperator::Keyof)
            {
                return semantic_model.doc_type_lua_rich_in(file_id, syntax);
            }
            let Some(target) = unary.get_type() else {
                return semantic_model.doc_type_lua_rich_in(file_id, syntax);
            };
            let LuaDocType::Name(name_ty) = &target else {
                return semantic_model.doc_type_lua_rich_in(file_id, syntax);
            };
            let Some(name) = name_ty.get_name_text() else {
                return semantic_model.doc_type_lua_rich_in(file_id, syntax);
            };
            if let Some(bound) = substitutions.get(&name) {
                return keyof_type(semantic_model, bound);
            }
            if generics.iter().any(|param| param.name == name) {
                // Unsubstituted `keyof T`: rigid placeholder (consumed by default-value checks).
                let index = generics
                    .iter()
                    .position(|param| param.name == name)
                    .unwrap_or(0);
                return LuaType::TplRef(std::sync::Arc::new(crate::GenericTpl::new(
                    GenericTplId::Type(index as u32),
                    smol_str::SmolStr::new(name),
                    None,
                    None,
                    false,
                    None,
                )));
            }
            keyof_name(semantic_model, file_id, &name)
                .unwrap_or_else(|| semantic_model.doc_type_lua_rich_in(file_id, syntax))
        }
        LuaDocType::Binary(binary) => {
            let Some((left, right)) = binary.get_types() else {
                return semantic_model.doc_type_lua_rich_in(file_id, syntax);
            };
            let left_ty = project_doc_type_with(
                semantic_model,
                file_id,
                left.get_syntax_id(),
                generics,
                substitutions,
            );
            let right_ty = project_doc_type_with(
                semantic_model,
                file_id,
                right.get_syntax_id(),
                generics,
                substitutions,
            );
            match binary.get_op_token().map(|op| op.get_op()) {
                Some(emmylua_parser::LuaTypeBinaryOperator::Intersection) => LuaType::Intersection(
                    std::sync::Arc::new(crate::LuaIntersectionType::new(vec![left_ty, right_ty])),
                ),
                Some(emmylua_parser::LuaTypeBinaryOperator::Union) => {
                    let mut types = Vec::new();
                    for ty in [left_ty, right_ty] {
                        match ty {
                            LuaType::Union(union) => types.extend(union.into_vec()),
                            other => types.push(other),
                        }
                    }
                    LuaType::Union(std::sync::Arc::new(crate::LuaUnionType::from_vec(types)))
                }
                _ => semantic_model.doc_type_lua_rich_in(file_id, syntax),
            }
        }
        LuaDocType::Conditional(conditional) => {
            let Some((condition, true_ty, false_ty)) = conditional.get_types() else {
                return semantic_model.doc_type_lua_rich_in(file_id, syntax);
            };
            let take_true = if let LuaDocType::Binary(condition_binary) = &condition {
                condition_binary.get_types().is_some_and(|(left, right)| {
                    condition_binary.get_op_token().is_some_and(|op| {
                        op.get_op() == emmylua_parser::LuaTypeBinaryOperator::Extends
                    }) && {
                        let left_ty = project_doc_type_with(
                            semantic_model,
                            file_id,
                            left.get_syntax_id(),
                            generics,
                            substitutions,
                        );
                        let right_ty = project_doc_type_with(
                            semantic_model,
                            file_id,
                            right.get_syntax_id(),
                            generics,
                            substitutions,
                        );
                        constraint_compatible(semantic_model, &left_ty, &right_ty)
                    }
                })
            } else {
                false
            };
            let branch = if take_true { true_ty } else { false_ty };
            project_doc_type_with(
                semantic_model,
                file_id,
                branch.get_syntax_id(),
                generics,
                substitutions,
            )
        }
        _ => semantic_model.doc_type_lua_rich_in(file_id, syntax),
    }
}

/// `keyof` of a named type: union of member names (including parent type members).
fn keyof_name(semantic_model: &SemanticModel<'_>, file_id: FileId, name: &str) -> Option<LuaType> {
    let def = semantic_model.resolve_type_def_in(file_id, name)?;
    Some(keyof_type_def(semantic_model, &def))
}

fn keyof_type(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> LuaType {
    let (LuaType::Ref(id) | LuaType::Def(id)) = ty else {
        return LuaType::Unknown;
    };
    let Some(def) = semantic_model.type_def_of(id) else {
        return LuaType::Unknown;
    };
    keyof_type_def(semantic_model, &def)
}

fn keyof_type_def(semantic_model: &SemanticModel<'_>, def: &TypeDef) -> LuaType {
    let mut keys = Vec::new();
    collect_key_names(semantic_model, def, &mut keys, &mut Vec::new());
    if keys.is_empty() {
        return LuaType::Unknown;
    }
    let types = keys
        .into_iter()
        .map(|name| LuaType::StringConst(smol_str::SmolStr::new(name).into()))
        .collect();
    LuaType::Union(std::sync::Arc::new(crate::LuaUnionType::from_vec(types)))
}

fn collect_key_names(
    semantic_model: &SemanticModel<'_>,
    def: &TypeDef,
    keys: &mut Vec<String>,
    visited: &mut Vec<smol_str::SmolStr>,
) {
    if visited.contains(&def.full_name) {
        return;
    }
    visited.push(def.full_name.clone());
    for member in semantic_model.members_of_owner(&def.id) {
        if !keys.iter().any(|key| key == member.name.as_str()) {
            keys.push(member.name.to_string());
        }
    }
    for super_name in &def.super_names {
        if let Some(super_def) = semantic_model.resolve_type_def_in(def.file_id, super_name) {
            collect_key_names(semantic_model, &super_def, keys, visited);
        }
    }
}

/// Dependent generic default-value determination:
/// - when the default references an earlier `T`, substitute T's constraint before checking (`U extends string = T`);
/// - when the constraint itself is a rigid `T` / `keyof T`, the default must also reference that `T` (`U extends T = "x"` is an error).
fn dependent_default_compatible(
    semantic_model: &SemanticModel<'_>,
    params: &[SalsaGenericParam],
    current: &SalsaGenericParam,
    actual: &LuaType,
    constraint: &LuaType,
) -> bool {
    let actual_ref = generic_param_name(actual);
    let constraint_ref = generic_param_name(constraint);

    if let Some(constraint_name) = constraint_ref {
        // `U extends T = T`; other defaults cannot claim to be that rigid T.
        return actual_ref == Some(constraint_name);
    }

    if let Some(actual_name) = actual_ref {
        // `U extends string = T`: T's constraint is string, so treat it as string.
        if let Some(previous) = params
            .iter()
            .take_while(|p| p.name != current.name)
            .find(|p| p.name.as_str() == actual_name)
            && let Some(previous_constraint) = previous.constraint
        {
            let previous_ty = project_doc_type_with(
                semantic_model,
                semantic_model.file_id(),
                previous_constraint,
                params,
                &HashMap::new(),
            );
            return constraint_compatible(semantic_model, &previous_ty, constraint);
        }
        return false;
    }

    constraint_compatible(semantic_model, actual, constraint)
}

fn generic_param_name(ty: &LuaType) -> Option<&str> {
    match ty {
        LuaType::Ref(id) | LuaType::Def(id) => {
            if id.get_name().len() == 1 {
                Some(id.get_name())
            } else {
                None
            }
        }
        LuaType::TplRef(tpl) => Some(tpl.get_name()),
        LuaType::StrTplRef(str_tpl) => Some(str_tpl.get_name()),
        _ => None,
    }
}

/// Call-argument constraint compatibility: any member of an argument union suffices (the T branch in `T|string` is compatible with table).
fn call_constraint_compatible(
    semantic_model: &SemanticModel<'_>,
    actual: &LuaType,
    constraint: &LuaType,
) -> bool {
    if matches!(
        actual,
        LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_)
    ) && matches!(constraint, LuaType::Number)
    {
        return true;
    }
    if let LuaType::Union(actual_union) = actual {
        return actual_union
            .into_vec()
            .iter()
            .any(|component| constraint_compatible(semantic_model, component, constraint));
    }
    constraint_compatible(semantic_model, actual, constraint)
}

/// Constraint compatibility: every member of an argument union must satisfy the constraint; any member of the target union is sufficient;
/// literal constants must match exactly.
fn constraint_compatible(
    semantic_model: &SemanticModel<'_>,
    actual: &LuaType,
    constraint: &LuaType,
) -> bool {
    if actual == constraint {
        return true;
    }
    if let (LuaType::StringConst(a), LuaType::StringConst(b)) = (actual, constraint) {
        return a == b;
    }
    if let LuaType::Union(actual_union) = actual {
        return actual_union
            .into_vec()
            .iter()
            .all(|component| constraint_compatible(semantic_model, component, constraint));
    }
    if let LuaType::Union(constraint_union) = constraint {
        return constraint_union
            .into_vec()
            .iter()
            .any(|component| constraint_compatible(semantic_model, actual, component));
    }
    if is_compatible(semantic_model, actual, constraint) {
        return true;
    }
    false
}

fn doc_syntax_range(
    semantic_model: &SemanticModel<'_>,
    file_id: FileId,
    syntax: emmylua_parser::LuaSyntaxId,
) -> Option<rowan::TextRange> {
    semantic_model
        .syntax_tree_of(file_id)
        .and_then(|tree| syntax.to_node_from_root(&tree.get_red_root()))
        .map(|node| node.text_range())
}

fn add_constraint_diagnostic(
    context: &mut CheckContext<'_>,
    range: rowan::TextRange,
    actual: &LuaType,
    constraint: &LuaType,
) {
    let actual_name = humanize_type(context.semantic_model, actual);
    context.add_diagnostic(
        DiagnosticCode::GenericConstraintMismatch,
        range,
        t!(
            "type `%{found}` does not satisfy the constraint `%{source}`. %{reason}",
            found = actual_name,
            source = humanize_type(context.semantic_model, constraint),
            reason = ""
        ),
    );
}
