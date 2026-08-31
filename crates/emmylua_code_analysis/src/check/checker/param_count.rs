//! # param_count - call argument count does not match signature parameters
//!
//! M0+: callee candidate signatures = `DocFunction` primary signature + `---@overload` (type context) projection;
//! if any candidate count is compatible, don't report; otherwise pick the smallest mismatch candidate and report Missing/Redundant (on equal
//! mismatch, Missing wins). Exact variadic expansion and multi-return expansion are left for later.

use emmylua_parser::{
    LuaAstNode, LuaCallArgList, LuaCallExpr, LuaClosureExpr, LuaDocType, LuaExpr, LuaIndexExpr,
    LuaIndexKey, LuaSyntaxId, LuaTableExpr, LuaTableField, LuaTypeBinaryOperator,
};

use crate::DiagnosticCode;
use crate::check::checker::param_type_check;
use crate::semantic_model::SemanticModel;
use crate::semantic_model::infer::unify::{self, TplBindings};
use crate::{
    AsyncState, FileId, GenericTplId, LuaFunctionType, LuaGenericType, LuaMemberKey, LuaType,
    SemanticId, TypeDef, VariadicType,
};
use crate::{LuaTypeDeclId, TypeDefKind, semantic_model::member};

use super::{CheckContext, Checker};

pub struct ParamCountChecker;

impl Checker for ParamCountChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::MissingParameter,
        DiagnosticCode::RedundantParameter,
    ];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for call_expr in root.descendants().filter_map(LuaCallExpr::cast) {
            check_call(context, semantic_model, &call_expr);
        }
        // Closure argument count vs expected `fun(...)` parameter count (`with_local(function(a) end)`).
        for closure_expr in root.descendants().filter_map(LuaClosureExpr::cast) {
            check_closure_param_count(context, semantic_model, &closure_expr);
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
    let callee_ty = semantic_model.type_of_expr(callee.get_syntax_id());
    let analysis = semantic_model.call_site_analysis(call_expr);
    let mut candidates = analysis.candidates;
    if candidates.is_empty() {
        candidates = callable_functions(semantic_model, &callee_ty);
    }
    // Higher-order generic return is not instantiated: `Mock<T>`'s T is still a TplRef, but T can be
    // inferred from the declaration initializer of `sum = fn(function(a, b) ...)`.
    if candidates.is_empty()
        && let LuaType::Generic(generic) = &callee_ty
    {
        candidates = generic_callee_fallback(semantic_model, &callee, generic);
    }
    // `---@overload` candidates: fun(...) in the signature docs are explicitly added to the argument count.
    if let LuaExpr::NameExpr(name_expr) = &callee
        && let Some(decl) = semantic_model.resolve_name(name_expr.get_position())
        && let SemanticId::Decl(decl_key) = decl
        && let Some(facts) = semantic_model.file_facts_of(decl_key.file_id)
        && let Some(decl) = facts.decl_by_id(&SemanticId::Decl(decl_key))
        && let Some(closure_syntax) = decl.value_expr_syntax
        && let Some(signature) = facts.signature_by_closure(closure_syntax)
        && let Some(docs) = signature.docs.as_ref()
    {
        for syntax in &docs.overloads {
            if let Some(func) = doc_func_from_syntax(semantic_model, decl.file_id, *syntax) {
                candidates.push(func);
            }
        }
    }
    // `A.foo`: VM only gives Function; prefer the declared member type from member resolution.
    if candidates.is_empty()
        && let Some(index_expr) = LuaIndexExpr::cast(callee.syntax().clone())
        && let Some(resolved) = semantic_model.resolve_member(&index_expr)
        && let Some(member_id) = resolved.member_id
    {
        if let Some(member_ty) = semantic_model.type_of_member(&member_id) {
            candidates = callable_functions(semantic_model, &member_ty);
        }
        if candidates.is_empty()
            && let Some(member_file) = resolved.file_id
            && let Some(facts) = semantic_model.file_facts_of(member_file)
            && let Some(member) = facts.member_by_id(&member_id)
            && let Some(value_syntax) = member.value_syntax
            && let Some(tree) = semantic_model.syntax_tree_of(member_file)
            && let Some(node) = value_syntax.to_node_from_root(&tree.get_red_root())
            && let Some(closure) = LuaClosureExpr::cast(node)
            && let Some(func) =
                semantic_model.type_of_signature_in_file(member_file, closure.get_syntax_id())
        {
            candidates.push(func);
        }
    }
    if candidates.is_empty() {
        return;
    }
    let args = call_expr
        .get_args_list()
        .map(|list| list.get_args().collect::<Vec<_>>())
        .unwrap_or_default();
    // `_nop(...)`: `...` arguments are forwarded verbatim; no count check.
    if args.iter().any(|arg| arg.syntax().text() == "...") {
        return;
    }
    let arg_range = expanded_arg_range(semantic_model, &args);
    let colon_call = analysis.colon_call;

    for candidate in &candidates {
        let range = param_count_range(semantic_model, candidate, colon_call);
        let colon_extra = usize::from(colon_call && !first_param_is_self(candidate));
        let call_min = arg_range.min + colon_extra;
        let call_max = arg_range.max.map(|max| max + colon_extra);
        let enough = call_max.is_none_or(|max| max >= range.min);
        let not_too_many = range.max.is_none_or(|max| call_min <= max);
        if enough && not_too_many {
            return;
        }
    }

    // Pick the smallest mismatch candidate; on equal mismatch, Missing wins.
    let mut best: Option<(usize, bool, usize, usize, usize)> = None; // (mismatch, is_missing, expected, found, colon_extra)
    for candidate in &candidates {
        let range = param_count_range(semantic_model, candidate, colon_call);
        let colon_extra = usize::from(colon_call && !first_param_is_self(candidate));
        let call_min = arg_range.min + colon_extra;
        let call_max = arg_range.max.map(|max| max + colon_extra);
        let candidate = if call_max.is_some_and(|max| max < range.min) {
            let max = call_max.unwrap_or(call_min);
            Some((range.min - max, true, range.min, max, colon_extra))
        } else if let Some(max) = range.max
            && call_min > max
        {
            Some((call_min - max, false, max, call_min, colon_extra))
        } else {
            None
        };
        let Some((mismatch, is_missing, expected, found, colon_extra)) = candidate else {
            continue;
        };
        let better = best.is_none_or(|(best_mismatch, best_missing, _, _, _)| {
            mismatch < best_mismatch || (mismatch == best_mismatch && is_missing && !best_missing)
        });
        if better {
            best = Some((mismatch, is_missing, expected, found, colon_extra));
        }
    }

    let Some((_, is_missing, expected, found, colon_extra)) = best else {
        return;
    };
    if is_missing {
        if let Some(args_list) = call_expr.get_args_list() {
            context.add_diagnostic(
                DiagnosticCode::MissingParameter,
                args_list.get_range(),
                t!(
                    "expected at least %{num} parameters but found %{found_num}",
                    num = expected,
                    found_num = found
                ),
            );
        }
    } else {
        // Redundant slots are at the tail of the arguments; when a colon call's implicit receiver occupies a slot, borrow forward.
        let skip = expected.saturating_sub(colon_extra);
        for arg in args.iter().skip(skip) {
            context.add_diagnostic(
                DiagnosticCode::RedundantParameter,
                arg.get_range(),
                t!(
                    "expected %{num} parameters but found %{found_num}",
                    num = expected,
                    found_num = found
                ),
            );
        }
    }
}

/// `Mock<T>` (generic argument still a TplRef) call overload candidates:
/// infer the generic arguments from the callee declaration initializer, then expand `fun(...: Parameters<T>...)` into fixed-length parameters.
fn generic_callee_fallback(
    semantic_model: &SemanticModel<'_>,
    callee: &LuaExpr,
    generic: &LuaGenericType,
) -> Vec<LuaFunctionType> {
    let Some(def) = member::type_def_of(semantic_model, &generic.get_base_type_id()) else {
        return Vec::new();
    };
    if def.call_overloads.is_empty() {
        return Vec::new();
    }
    let Some(actuals) = generic_actuals_from_decl(semantic_model, callee, &def) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for syntax in &def.call_overloads {
        if let Some(func) =
            overload_expanded_for_actuals(semantic_model, def.file_id, *syntax, &actuals)
        {
            out.push(func);
        }
    }
    out
}

/// Actual type arguments of a generic callee: when the callee is a global name, take the generic bindings
/// from the declaration's initializing call (`sum = fn(function(a, b) ...)`).
fn generic_actuals_from_decl(
    semantic_model: &SemanticModel<'_>,
    callee: &LuaExpr,
    def: &TypeDef,
) -> Option<Vec<LuaType>> {
    let LuaExpr::NameExpr(name_expr) = callee else {
        return None;
    };
    let name = name_expr.get_name_text()?;
    let decl = semantic_model
        .global_decl(name.as_str())
        .or_else(|| semantic_model.resolve_name(name_expr.get_position()))?;
    let SemanticId::Decl(decl_key) = &decl else {
        return None;
    };
    let facts = semantic_model.file_facts_of(decl_key.file_id)?;
    let decl = facts.decl_by_id(&decl)?;
    let call_syntax = decl.value_expr_syntax?;
    let tree = semantic_model.syntax_tree_of(decl_key.file_id)?;
    let node = call_syntax.to_node_from_root(&tree.get_red_root())?;
    let inner_call = LuaCallExpr::cast(node)?;
    let inner_callee = inner_call.get_prefix_expr()?;
    let LuaExpr::NameExpr(inner_name) = inner_callee else {
        return None;
    };
    let inner_name = inner_name.get_name_text()?;
    let inner_decl = facts.decl_named(inner_name.as_str())?;
    let fn_closure = inner_decl.value_expr_syntax?;
    let fn_func = semantic_model.type_of_signature_in_file(decl_key.file_id, fn_closure)?;
    let arg_closure = inner_call
        .get_args_list()?
        .get_args()
        .find_map(|arg| LuaClosureExpr::cast(arg.syntax().clone()))?;
    let arg_func =
        semantic_model.type_of_signature_in_file(decl_key.file_id, arg_closure.get_syntax_id())?;
    let mut bindings = TplBindings::new();
    for (_, param_ty) in fn_func.get_params() {
        if let Some(param_ty) = param_ty {
            let _ = unify::unify_bindings(
                param_ty,
                &LuaType::DocFunction(arg_func.clone().into()),
                &mut bindings,
            );
        }
    }
    let mut actuals = Vec::with_capacity(def.generic_params.len());
    for (index, _) in def.generic_params.iter().enumerate() {
        let tpl = fn_func.get_generic_params().get(index)?;
        actuals.push(bindings.get(&tpl.get_tpl_id()).cloned()?);
    }
    Some(actuals)
}

/// Expand tuple variadics in overloads (`fun(...: MockParameters<T>...)`) into fixed-length parameter lists.
fn overload_expanded_for_actuals(
    semantic_model: &SemanticModel<'_>,
    file_id: FileId,
    syntax: LuaSyntaxId,
    actuals: &[LuaType],
) -> Option<LuaFunctionType> {
    let tree = semantic_model.syntax_tree_of(file_id)?;
    let node = syntax.to_node_from_root(&tree.get_red_root())?;
    let LuaDocType::Func(func_doc) = LuaDocType::cast(node)? else {
        return None;
    };
    let mut total = 0usize;
    for (index, param) in func_doc.get_params().enumerate() {
        let param_ty = param.get_type()?;
        match param_ty {
            LuaDocType::Variadic(variadic) => {
                let inner = variadic.get_type()?;
                let expanded = tuple_len_of_doc_type(semantic_model, file_id, &inner, actuals, 0)?;
                total = index + expanded;
                break;
            }
            _ => total = index + 1,
        }
    }
    if total == 0 {
        return None;
    }
    let params = (0..total)
        .map(|_| (String::new(), Some(LuaType::Any)))
        .collect();
    Some(LuaFunctionType::new(
        AsyncState::None,
        false,
        false,
        params,
        LuaType::Unknown,
        None,
    ))
}

/// Tuple length of conditional aliases like `MockParameters<T>`.
fn tuple_len_of_doc_type(
    semantic_model: &SemanticModel<'_>,
    file_id: FileId,
    ty: &LuaDocType,
    actuals: &[LuaType],
    depth: usize,
) -> Option<usize> {
    if depth > 8 {
        return None;
    }
    match ty {
        LuaDocType::Generic(generic) => {
            let name = generic.get_name_type()?.get_name_text()?;
            let def = semantic_model.resolve_type_def_in(file_id, &name)?;
            alias_tuple_len(semantic_model, &def, actuals, depth + 1)
        }
        _ => None,
    }
}

/// Tuple length of a conditional alias (`T extends ... and P or never`) under the given actual arguments.
fn alias_tuple_len(
    semantic_model: &SemanticModel<'_>,
    def: &TypeDef,
    actuals: &[LuaType],
    depth: usize,
) -> Option<usize> {
    if depth > 8 {
        return None;
    }
    let syntax = def.alias_type?;
    let tree = semantic_model.syntax_tree_of(def.file_id)?;
    let node = syntax.to_node_from_root(&tree.get_red_root())?;
    let LuaDocType::Conditional(conditional) = LuaDocType::cast(node)? else {
        return None;
    };
    let (condition, true_ty, _false_ty) = conditional.get_types()?;
    let LuaDocType::Binary(binary) = condition else {
        return None;
    };
    if binary.get_op_token().map(|op| op.get_op()) != Some(LuaTypeBinaryOperator::Extends) {
        return None;
    }
    let (_, right) = binary.get_types()?;
    match right {
        LuaDocType::Name(name_ty) if name_ty.get_name_text().as_deref() == Some("Procedure") => {
            // `T extends Procedure`: T is a function -> take the true branch.
            if !matches!(actuals.first(), Some(LuaType::DocFunction(_))) {
                return None;
            }
            tuple_len_of_doc_type(semantic_model, def.file_id, &true_ty, actuals, depth)
        }
        LuaDocType::Func(_) => {
            // `T extends fun(...: infer P): any`: P is a tuple composed of the actual function parameters.
            let Some(LuaType::DocFunction(actual_func)) = actuals.first() else {
                return None;
            };
            if matches!(
                &true_ty,
                LuaDocType::Name(name) if name.get_name_text().as_deref() == Some("P")
            ) {
                Some(actual_func.get_params().len())
            } else {
                tuple_len_of_doc_type(semantic_model, def.file_id, &true_ty, actuals, depth)
            }
        }
        _ => None,
    }
}

/// Closure arguments vs expected function type: if the closure declares more parameters than the expected `fun(...)`, report redundant parameters.
fn check_closure_param_count(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    closure_expr: &LuaClosureExpr,
) {
    let expected = closure_expected_functions(semantic_model, closure_expr);
    if expected.is_empty() {
        return;
    }
    // Any expected function is variadic -> indefinite count, so skip.
    let mut max_expected = 0usize;
    for func in &expected {
        let Some(len) = fixed_param_len(func) else {
            return;
        };
        max_expected = max_expected.max(len);
    }
    let Some(params_list) = closure_expr.get_params_list() else {
        return;
    };
    let params = params_list.get_params().collect::<Vec<_>>();
    if params.len() <= max_expected {
        return;
    }
    let found = params.len();
    for param in params.iter().skip(max_expected) {
        context.add_diagnostic(
            DiagnosticCode::RedundantParameter,
            param.get_range(),
            t!(
                "expected %{num} parameters but found %{found_num}",
                num = max_expected,
                found_num = found
            ),
        );
    }
}

/// Expected function types for the closure's syntactic context:
/// 1. Call argument -> function type in the callee parameter;
/// 2. Table field value -> the type of the corresponding member in the table's expected type (`@field event fun()`).
fn closure_expected_functions(
    semantic_model: &SemanticModel<'_>,
    closure_expr: &LuaClosureExpr,
) -> Vec<LuaFunctionType> {
    if let Some((call_expr, param_idx)) = closure_call_arg_info(closure_expr) {
        let Some(callee) = call_expr.get_prefix_expr() else {
            return Vec::new();
        };
        let candidates = param_type_check::callable_candidates(semantic_model, &callee);
        let colon_call = call_expr.is_colon_call();
        let mut out = Vec::new();
        for func in &candidates {
            let idx = match (func.is_colon_define(), colon_call) {
                (true, false) => param_idx + 1,
                (false, true) => {
                    if param_idx == 0 {
                        continue;
                    }
                    param_idx - 1
                }
                _ => param_idx,
            };
            let Some(ty) = func.get_params().get(idx).and_then(|(_, ty)| ty.as_ref()) else {
                continue;
            };
            out.extend(function_types(semantic_model, ty));
        }
        return out;
    }
    if let Some((table_expr, field_key)) = closure_table_field_info(closure_expr) {
        let Some(table_ty) = table_expected_type(semantic_model, &table_expr) else {
            return Vec::new();
        };
        let Some(member_ty) = semantic_model.member_type(&table_ty, &field_key) else {
            return Vec::new();
        };
        return function_types(semantic_model, &member_ty);
    }
    Vec::new()
}

/// Position of a closure used as a call argument.
fn closure_call_arg_info(closure_expr: &LuaClosureExpr) -> Option<(LuaCallExpr, usize)> {
    let arg_list = closure_expr.get_parent::<LuaCallArgList>()?;
    let call_expr = arg_list.get_parent::<LuaCallExpr>()?;
    let position = closure_expr.get_position();
    let index = arg_list
        .get_args()
        .position(|arg| arg.get_position() == position)?;
    Some((call_expr, index))
}

/// Position of a closure used as a table field value, plus the field name key.
fn closure_table_field_info(closure_expr: &LuaClosureExpr) -> Option<(LuaTableExpr, LuaMemberKey)> {
    let field = closure_expr.get_parent::<LuaTableField>()?;
    let table_expr = field.get_parent::<LuaTableExpr>()?;
    let field_key = match field.get_field_key()? {
        LuaIndexKey::Name(name) => LuaMemberKey::Name(name.get_name_text().into()),
        LuaIndexKey::String(str) => LuaMemberKey::Name(str.get_value().into()),
        _ => return None,
    };
    Some((table_expr, field_key))
}

/// The `---@type` target type on the assignment containing the table literal (`---@type A local a = { ... }`).
fn table_expected_type(
    semantic_model: &SemanticModel<'_>,
    table_expr: &LuaTableExpr,
) -> Option<LuaType> {
    let facts = semantic_model.file_facts()?;
    let decl = facts.decls.iter().find(|decl| {
        decl.value_expr_syntax == Some(table_expr.get_syntax_id()) && decl.doc_type_syntax.is_some()
    })?;
    let syntax = decl.doc_type_syntax?;
    let ty = semantic_model.doc_type_lua(syntax);
    (!matches!(ty, LuaType::Unknown)).then_some(ty)
}

/// Function components within a type (`DocFunction` / `Signature` / union).
fn function_types(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> Vec<LuaFunctionType> {
    callable_functions(semantic_model, ty)
}

/// Fixed-length function parameter count (last parameter variadic -> None).
fn fixed_param_len(func: &LuaFunctionType) -> Option<usize> {
    if func.is_variadic()
        || func.get_params().last().is_some_and(|(name, ty)| {
            name == "..."
                || ty
                    .as_ref()
                    .is_some_and(|ty| matches!(ty, LuaType::Variadic(_)))
        })
    {
        return None;
    }
    Some(func.get_params().len())
}

fn doc_func_from_syntax(
    semantic_model: &SemanticModel<'_>,
    file_id: FileId,
    syntax: LuaSyntaxId,
) -> Option<LuaFunctionType> {
    let tree = semantic_model.syntax_tree_of(file_id)?;
    let node = syntax.to_node_from_root(&tree.get_red_root())?;
    let LuaDocType::Func(func) = LuaDocType::cast(node)? else {
        return None;
    };
    let params = func
        .get_params()
        .map(|param| {
            (
                String::new(),
                param
                    .get_type()
                    .map(|ty| semantic_model.doc_type_lua_rich_in(file_id, ty.get_syntax_id())),
            )
        })
        .collect();
    Some(LuaFunctionType::new(
        AsyncState::None,
        false,
        false,
        params,
        LuaType::Unknown,
        None,
    ))
}

/// Call-side argument slot count range: trailing multi-return calls expand per `Variadic`;
/// `max = None` means the upper bound is unknown (`T...` / `table.unpack`).
#[derive(Debug, Clone, Copy)]
struct ArgCountRange {
    min: usize,
    max: Option<usize>,
}

fn expanded_arg_range(semantic_model: &SemanticModel<'_>, args: &[LuaExpr]) -> ArgCountRange {
    let base = args.len().saturating_sub(1);
    if let Some(last) = args.last()
        && let Some(variadic) = last_arg_variadic(semantic_model, last)
    {
        return ArgCountRange {
            min: base + variadic.get_min_len().unwrap_or(0),
            max: variadic.get_max_len().map(|len| base + len),
        };
    }
    ArgCountRange {
        min: args.len(),
        max: Some(args.len()),
    }
}

/// Multi-return type of the trailing argument; falls back to the callee declaration doc return type when VM inference fails.
fn last_arg_variadic(semantic_model: &SemanticModel<'_>, expr: &LuaExpr) -> Option<VariadicType> {
    let ty = semantic_model.type_of_expr(expr.get_syntax_id());
    if let LuaType::Variadic(variadic) = &ty {
        return Some(variadic.as_ref().clone());
    }
    if !matches!(ty, LuaType::Unknown) {
        return None;
    }
    let LuaExpr::CallExpr(inner_call) = expr else {
        return None;
    };
    let callee = inner_call.get_prefix_expr()?;
    let candidates = param_type_check::callable_candidates(semantic_model, &callee);
    for func in candidates {
        if let LuaType::Variadic(variadic) = func.get_ret() {
            return Some(variadic.as_ref().clone());
        }
    }
    // Candidate signatures project `---@return T...` to Unknown (TypeShell has no Variadic yet);
    // look directly at doc return nodes and fall back to an unbounded variadic expansion.
    if callee_has_variadic_doc_return(semantic_model, &callee).unwrap_or(false) {
        return Some(VariadicType::Base(LuaType::Any));
    }
    None
}

/// Whether the callee declaration doc `---@return` contains a `T...` variadic node.
fn callee_has_variadic_doc_return(
    semantic_model: &SemanticModel<'_>,
    callee: &LuaExpr,
) -> Option<bool> {
    let (file_id, closure_syntax) = match callee {
        LuaExpr::NameExpr(name_expr) => {
            let name = name_expr.get_name_text()?;
            let decl = semantic_model
                .resolve_name(name_expr.get_position())
                .or_else(|| semantic_model.global_decl(name.as_str()))?;
            let SemanticId::Decl(decl_key) = &decl else {
                return Some(false);
            };
            let facts = semantic_model.file_facts_of(decl_key.file_id)?;
            let decl = facts.decl_by_id(&decl)?;
            (decl_key.file_id, decl.value_expr_syntax?)
        }
        LuaExpr::IndexExpr(index_expr) => {
            let resolved = semantic_model.resolve_member(index_expr)?;
            let member_id = resolved.member_id?;
            let file_id = resolved.file_id?;
            let facts = semantic_model.file_facts_of(file_id)?;
            let member = facts.member_by_id(&member_id)?;
            (file_id, member.value_syntax?)
        }
        _ => return Some(false),
    };
    let facts = semantic_model.file_facts_of(file_id)?;
    let signature = facts.signature_by_closure(closure_syntax)?;
    let docs = signature.docs.as_ref()?;
    let tree = semantic_model.syntax_tree_of(file_id)?;
    Some(docs.returns.iter().any(|syntax| {
        syntax
            .to_node_from_root(&tree.get_red_root())
            .and_then(LuaDocType::cast)
            .is_some_and(|doc_ty| matches!(doc_ty, LuaDocType::Variadic(_)))
    }))
}

pub(crate) fn first_param_is_self(func: &LuaFunctionType) -> bool {
    func.get_params().first().is_some_and(|(name, ty)| {
        name == "self"
            || ty.as_ref().is_some_and(|t| {
                t.is_self_infer() || matches!(t, LuaType::Ref(id) if id.get_name() == "self")
            })
    })
}

struct ParamCountRange {
    min: usize,
    max: Option<usize>,
    #[allow(dead_code)]
    variadic: bool,
}

fn param_count_range(
    semantic_model: &SemanticModel<'_>,
    func: &LuaFunctionType,
    colon_call: bool,
) -> ParamCountRange {
    let params = func.get_params();
    let self_param = first_param_is_self(func);
    let explicit_self_in_params = self_param;
    let params = if colon_call && self_param {
        &params[1..]
    } else {
        params
    };

    // Lua 5.5 named vararg (`...args`) 的最后一个参数名不是 "..."，但仍应视为可变参数槽。
    // 优先使用 LuaFunctionType::is_variadic()；doc/operator 投影若没传该标志，则回退到
    // 参数名或参数类型为 Variadic 的启发式判断。
    let variadic_index = if func.is_variadic() {
        Some(params.len().saturating_sub(1))
    } else {
        params.iter().rposition(|(name, ty)| {
            name == "..."
                || ty
                    .as_ref()
                    .is_some_and(|ty| matches!(ty, LuaType::Variadic(_)))
        })
    };
    let variadic_slot = variadic_index.and_then(|i| params.get(i));

    let mut min = params
        .iter()
        .enumerate()
        .filter(|(idx, (name, ty))| {
            Some(*idx) != variadic_index
                && name != "..."
                && !matches!(ty, None | Some(LuaType::Any | LuaType::Unknown))
                && !ty
                    .as_ref()
                    .is_some_and(|ty| is_optional(semantic_model, ty))
        })
        .count();

    // 未实例化的 `fun(...: T...)`：T 仍然未知或命名引用时，不能当作无界可变参数。
    let unresolved_variadic = variadic_slot.is_some_and(|(_, ty)| {
        ty.as_ref().is_some_and(|ty| {
            matches!(ty, LuaType::Variadic(variadic) if matches!(
                variadic.as_ref(),
                VariadicType::Base(LuaType::Ref(id) | LuaType::Def(id))
                    if member::type_def_of(semantic_model, id).is_none()
            ))
        })
    });

    // 已实例化的 `T...`（T 是 tuple）或 `Variadic::Multi` 可展开为固定长度参数。
    let instantiated_variadic_len = variadic_slot.and_then(|(_, ty)| match ty.as_ref()? {
        LuaType::Variadic(variadic) => match variadic.as_ref() {
            VariadicType::Base(base) => {
                if let LuaType::Tuple(tuple) = base {
                    Some(tuple.get_types().len())
                } else {
                    None
                }
            }
            VariadicType::Multi(types) => Some(types.len()),
        },
        _ => None,
    });

    let has_variadic = func.is_variadic() || variadic_index.is_some();
    let mut max = if let Some(len) = instantiated_variadic_len {
        Some(params.len() - 1 + len)
    } else if !unresolved_variadic && has_variadic {
        None
    } else {
        Some(params.len())
    };

    // Method definitions (implicit self) need an extra required self slot in non-colon calls;
    // explicit `fun(self: self)` parameter lists already contain self, so don't double-count.
    if !colon_call && !explicit_self_in_params && (func.is_colon_define() || self_param) {
        min += 1;
        max = max.map(|max| max + 1);
    }
    ParamCountRange {
        min,
        max,
        variadic: max.is_none(),
    }
}

/// Optional determination after nullable / alias expansion (recursive alias cycle guard).
fn is_optional(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> bool {
    is_optional_inner(semantic_model, ty, &mut Vec::new())
}

fn is_optional_inner(
    semantic_model: &SemanticModel<'_>,
    ty: &LuaType,
    visited: &mut Vec<LuaTypeDeclId>,
) -> bool {
    if ty.is_optional() {
        return true;
    }
    let (LuaType::Ref(id) | LuaType::Def(id)) = ty else {
        return false;
    };
    if visited.contains(id) {
        return false;
    }
    visited.push(id.clone());
    let Some(def) = member::type_def_of(semantic_model, id) else {
        return false;
    };
    if def.kind != TypeDefKind::Alias {
        return false;
    }
    semantic_model
        .alias_target(&def)
        .is_some_and(|target| is_optional_inner(semantic_model, &target, visited))
}

/// Callee type -> candidate function types (primary signature + type-context `---@overload`).
pub(crate) fn callable_functions(
    semantic_model: &SemanticModel<'_>,
    ty: &LuaType,
) -> Vec<LuaFunctionType> {
    match ty {
        LuaType::DocFunction(func) => vec![func.as_ref().clone()],
        LuaType::Signature(signature_id) => semantic_model
            .signature_lua_by_legacy_id(signature_id)
            .into_iter()
            .collect(),
        LuaType::Ref(id) | LuaType::Def(id) => {
            let Some(def) = member::type_def_of(semantic_model, id) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            for syntax in &def.call_overloads {
                if let LuaType::DocFunction(func) = semantic_model.doc_type_lua(*syntax) {
                    out.push(func.as_ref().clone());
                }
            }
            // `---@operator call(...)`: like `---@overload`, it is a callable candidate.
            if let Some(facts) = semantic_model.file_facts_of(def.file_id)
                && let Some(op) = facts.operator_of(&def.id, "call")
            {
                let params = op
                    .params
                    .iter()
                    .map(|syntax| {
                        (
                            String::new(),
                            Some(semantic_model.doc_type_lua_rich_in(def.file_id, *syntax)),
                        )
                    })
                    .collect();
                out.push(LuaFunctionType::new(
                    AsyncState::None,
                    false,
                    false,
                    params,
                    semantic_model.doc_type_lua_rich_in(def.file_id, op.returns),
                    None,
                ));
            }
            out
        }
        LuaType::TableConst(table) => {
            crate::semantic_model::infer::vm::InferVm::setmetatable_call_candidate_for_table(
                semantic_model,
                table,
            )
            .into_iter()
            .collect()
        }
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .flat_map(|component| callable_functions(semantic_model, component))
            .collect(),
        LuaType::Generic(generic) => {
            let Some(def) = member::type_def_of(semantic_model, &generic.get_base_type_id()) else {
                return Vec::new();
            };
            if def.kind == TypeDefKind::Alias {
                if let Some(target) = semantic_model.alias_target(&def) {
                    let bindings: TplBindings = generic
                        .get_params()
                        .iter()
                        .enumerate()
                        .map(|(index, ty)| (GenericTplId::Type(index as u32), ty.clone()))
                        .collect();
                    let instantiated = unify::substitute(&target, &bindings);
                    return callable_functions(semantic_model, &instantiated);
                }
            }
            // Non-alias generics (classes etc.): fall back to call overload / operator call candidates.
            let mut out = Vec::new();
            for syntax in &def.call_overloads {
                if let LuaType::DocFunction(func) = semantic_model.doc_type_lua(*syntax) {
                    out.push(func.as_ref().clone());
                }
            }
            if let Some(facts) = semantic_model.file_facts_of(def.file_id)
                && let Some(op) = facts.operator_of(&def.id, "call")
            {
                let params = op
                    .params
                    .iter()
                    .map(|syntax| {
                        (
                            String::new(),
                            Some(semantic_model.doc_type_lua_rich_in(def.file_id, *syntax)),
                        )
                    })
                    .collect();
                out.push(LuaFunctionType::new(
                    AsyncState::None,
                    false,
                    false,
                    params,
                    semantic_model.doc_type_lua_rich_in(def.file_id, op.returns),
                    None,
                ));
            }
            out
        }
        _ => Vec::new(),
    }
}
