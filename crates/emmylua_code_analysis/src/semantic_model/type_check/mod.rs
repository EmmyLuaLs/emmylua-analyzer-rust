//! # type_check — `LuaType` type compatibility checks (standalone directory)
//!
//! Ported from the old `semantic::type_check` (isomorphic recursive descent + `TypeCheckFailReason`),
//! but resolving the type environment only through `semantic_model::SemanticModel` (salsa):
//! - **Boolean first**: `is_compatible(source, target) -> bool` (zero reason overhead);
//! - **Detail interface**: `check_type(...) -> TypeCheckResult` (includes failure reason strings);
//! - Recursion + depth guard (`TypeCheckGuard`) protects against infinite types;
//! - Parts still missing from salsa (alias origin, enum field unions, call operators) fall back to nominal checks.

mod assign;
mod complex_type;
mod context;
mod fail_reason;
mod func_type;
mod generic_type;
mod guard;
#[cfg(test)]
mod legacy_tests;
mod ref_type;
mod simple_type;
mod sub_type;
#[cfg(test)]
mod test;

use std::ops::Deref;

pub use context::TypeCheckContext;
pub use fail_reason::TypeCheckFailReason;

use crate::semantic_model::SemanticModel;
use crate::{LuaType, LuaTypeDeclId};

pub type TypeCheckResult = Result<(), TypeCheckFailReason>;

/// Boolean compatibility: whether `source` is assignable to `target`.
pub fn is_compatible(model: &SemanticModel, source: &LuaType, target: &LuaType) -> bool {
    let mut context = TypeCheckContext::new(model, false);
    check_general_type_compact(&mut context, source, target, guard::TypeCheckGuard::new()).is_ok()
}

/// Detailed compatibility: returns a reason on failure (detail mode includes human-readable strings).
pub fn check_type_detail(
    model: &SemanticModel,
    source: &LuaType,
    target: &LuaType,
) -> TypeCheckResult {
    let mut context = TypeCheckContext::new(model, true);
    check_general_type_compact(&mut context, source, target, guard::TypeCheckGuard::new())
}

/// Assignment-compatibility boolean interface: uses assignment semantics.
pub fn is_assign_compatible(model: &SemanticModel, source: &LuaType, target: &LuaType) -> bool {
    check_assign_type_detail(model, source, target).is_ok()
}

/// Strict subtype check: for cases requiring precise union/object subtype relationships.
pub fn check_type_subtype(
    model: &SemanticModel,
    source: &LuaType,
    target: &LuaType,
) -> TypeCheckResult {
    let mut context = TypeCheckContext::new(model, false);
    context.strict_union = true;
    context.strict_object = true;
    context.strict_generic = true;
    check_general_type_compact(&mut context, source, target, guard::TypeCheckGuard::new())
}

/// Assignment-compatibility detail interface: includes a detailed reason on failure.
pub fn check_assign_type_detail(
    model: &SemanticModel,
    source: &LuaType,
    target: &LuaType,
) -> TypeCheckResult {
    let mut context = TypeCheckContext::new(model, true);
    context.assign_mode = true;
    check_general_type_compact(&mut context, source, target, guard::TypeCheckGuard::new())
}

fn check_general_type_compact(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
    check_guard: guard::TypeCheckGuard,
) -> TypeCheckResult {
    if source == compact_type {
        return Ok(());
    }

    if is_like_any(compact_type) {
        return Ok(());
    }

    // `self` parameter accepts receiver instances (class/table/object).
    if matches!(compact_type, LuaType::SelfInfer) {
        if matches!(
            source,
            LuaType::SelfInfer
                | LuaType::Ref(_)
                | LuaType::Def(_)
                | LuaType::Generic(_)
                | LuaType::Table
                | LuaType::TableConst(_)
                | LuaType::TableGeneric(_)
                | LuaType::Object(_)
                | LuaType::Intersection(_)
        ) {
            return Ok(());
        }
        return Err(TypeCheckFailReason::TypeNotMatch);
    }

    // Alias expansion: when `compact` is an alias reference, replace it with its target type and re-check (recursive aliases are cycle-protected by the guard depth).
    if let LuaType::Ref(id) | LuaType::Def(id) = compact_type
        && context.is_alias(id)
        && let Some(target) = context.alias_target_of(id)
    {
        return check_general_type_compact(context, source, &target, check_guard.next_level()?);
    }

    // Generic alias as source: expand plain aliases (skip mapped/conditional/call; these are handled by type_eval/param checks).
    if let LuaType::Generic(generic) = source
        && context.is_alias(&generic.get_base_type_id())
        && let Some(target) = context.alias_target_of(&generic.get_base_type_id())
        && !matches!(
            target,
            LuaType::Mapped(_) | LuaType::Call(_) | LuaType::Conditional(_)
        )
    {
        let expanded =
            crate::semantic_model::type_eval::expand_alias_generic(context.model, source);
        if expanded != *source {
            return check_general_type_compact(
                context,
                &expanded,
                compact_type,
                check_guard.next_level()?,
            );
        }
    }

    if fast_eq_check(source, compact_type) {
        return Ok(());
    }

    // Assignment mode: integer is assignable to number; number is not assignable to integer.
    if context.assign_mode {
        match (source, compact_type) {
            (
                LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_),
                LuaType::Number,
            ) => return Ok(()),
            (
                LuaType::Number | LuaType::FloatConst(_),
                LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_),
            ) => return Err(context.mismatch(source, compact_type)),
            _ => {}
        }
        // Assignment mode: assigning a table literal to a named class requires nominal relationship.
        if let (LuaType::TableConst(table), LuaType::Ref(target_id) | LuaType::Def(target_id)) =
            (source, compact_type)
        {
            let source_def = table_associated_type_def(context.model, table);
            let target_def = context.model.resolve_type_def(target_id.get_name());
            if let (Some(source_def), Some(target_def)) = (source_def, target_def)
                && !defs_related_ctx(context.model, &source_def, &target_def, &mut Vec::new())
            {
                return Err(context.mismatch(source, compact_type));
            }
        }
        // Assignment mode: an integer enum target accepts broad integers; literals must be member values.
        if let LuaType::Ref(target_id) | LuaType::Def(target_id) = compact_type
            && let Some(def) = context.model.resolve_type_def(target_id.get_name())
            && def.kind == crate::TypeDefKind::Enum
        {
            match source {
                LuaType::Integer | LuaType::DocIntegerConst(_) => return Ok(()),
                LuaType::IntegerConst(value) => {
                    if enum_integer_values_ctx(context.model, &def).contains(value) {
                        return Ok(());
                    }
                    return Err(context.mismatch(source, compact_type));
                }
                _ => {}
            }
        }
        // Assignment mode: a target union only needs one compatible member.
        if let LuaType::Union(union) = compact_type {
            for component in union.into_vec() {
                if check_general_type_compact(
                    context,
                    source,
                    &component,
                    check_guard.next_level()?,
                )
                .is_ok()
                {
                    return Ok(());
                }
            }
            return Err(context.mismatch(source, compact_type));
        }
    }

    // Assignment-specific checks: semantics for object/intersection/function assignment not yet in the general path.
    if context.assign_mode
        && let Some(result) = assign::check_special(context, source, compact_type, check_guard)
    {
        return result;
    }

    // `compact` is an intersection: source satisfies all components.
    if let LuaType::Intersection(compact_intersection) = compact_type {
        if !matches!(source, LuaType::Intersection(_)) {
            for component in compact_intersection.get_types() {
                if check_general_type_compact(context, source, component, check_guard.next_level()?)
                    .is_ok()
                {
                    return Ok(());
                }
            }
            return Err(context.mismatch(source, compact_type));
        }
    }

    match source {
        LuaType::Unknown | LuaType::Any => Ok(()),
        LuaType::TplRef(tpl) => {
            if let Some(source_constraint) = tpl.get_constraint() {
                return check_general_type_compact(
                    context,
                    source_constraint,
                    compact_type,
                    check_guard.next_level()?,
                );
            }
            simple_type::check_simple_type_compact(context, source, compact_type, check_guard)
        }
        LuaType::Nil
        | LuaType::Table
        | LuaType::Userdata
        | LuaType::Function
        | LuaType::Thread
        | LuaType::Boolean
        | LuaType::String
        | LuaType::Integer
        | LuaType::Number
        | LuaType::Io
        | LuaType::Global
        | LuaType::BooleanConst(_)
        | LuaType::StringConst(_)
        | LuaType::IntegerConst(_)
        | LuaType::FloatConst(_)
        | LuaType::TableConst(_)
        | LuaType::DocStringConst(_)
        | LuaType::DocIntegerConst(_)
        | LuaType::DocBooleanConst(_)
        | LuaType::StrTplRef(_)
        | LuaType::Namespace(_)
        | LuaType::Variadic(_)
        | LuaType::Language(_) => {
            simple_type::check_simple_type_compact(context, source, compact_type, check_guard)
        }

        LuaType::Ref(type_decl_id) => {
            ref_type::check_ref_type_compact(context, type_decl_id, compact_type, check_guard)
        }
        LuaType::Def(type_decl_id) => {
            ref_type::check_ref_type_compact(context, type_decl_id, compact_type, check_guard)
        }

        LuaType::DocFunction(doc_func) => {
            func_type::check_doc_func_type_compact(context, doc_func, compact_type, check_guard)
        }
        LuaType::Signature(sig_id) => {
            func_type::check_sig_type_compact(context, sig_id, compact_type, check_guard)
        }

        LuaType::Array(_)
        | LuaType::Tuple(_)
        | LuaType::Object(_)
        | LuaType::Union(_)
        | LuaType::Intersection(_)
        | LuaType::TableGeneric(_)
        | LuaType::Call(_)
        | LuaType::MultiLineUnion(_) => {
            check_union_intersection_source(context, source, compact_type, check_guard)
        }

        LuaType::Generic(generic) => {
            generic_type::check_generic_type_compact(context, generic, compact_type, check_guard)
        }

        LuaType::Instance(instantiate) => check_general_type_compact(
            context,
            instantiate.get_base(),
            compact_type,
            check_guard.next_level()?,
        ),
        LuaType::TypeGuard(_) => {
            if compact_type.is_boolean() {
                return Ok(());
            }
            Err(context.mismatch(source, compact_type))
        }
        LuaType::Never => {
            if compact_type.is_never() {
                return Ok(());
            }
            Err(context.mismatch(source, compact_type))
        }
        LuaType::ModuleRef(_) => Ok(()),
        _ => Err(context.mismatch(source, compact_type)),
    }
}

/// Branch for when `source` is a Union/Intersection/complex type.
fn check_union_intersection_source(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
    check_guard: guard::TypeCheckGuard,
) -> TypeCheckResult {
    match source {
        LuaType::Union(union) => {
            // Assignment mode: every member of the source union must be assignable to the target.
            if context.assign_mode {
                for member in union.into_vec() {
                    check_general_type_compact(
                        context,
                        &member,
                        compact_type,
                        check_guard.next_level()?,
                    )?;
                }
                return Ok(());
            }
            // Strict subtype mode: when the target is also a union, every target component must have a matching member in the source union.
            if context.strict_union
                && let LuaType::Union(target_union) = compact_type
            {
                let next_guard = check_guard.next_level()?;
                for target_component in target_union.into_vec() {
                    check_general_type_compact(context, source, &target_component, next_guard)?;
                }
                return Ok(());
            }
            // Source union: pass if any member is assignable (old semantics).
            for member in union.into_vec() {
                match check_general_type_compact(
                    context,
                    &member,
                    compact_type,
                    check_guard.next_level()?,
                ) {
                    Ok(()) => return Ok(()),
                    Err(err) if err.is_type_not_match() => {}
                    Err(err) => return Err(err),
                }
            }
            Err(context.mismatch(source, compact_type))
        }
        LuaType::Intersection(intersection) => {
            // Source intersection: any member matching is enough.
            for member in intersection.get_types() {
                if check_general_type_compact(
                    context,
                    member,
                    compact_type,
                    check_guard.next_level()?,
                )
                .is_ok()
                {
                    return Ok(());
                }
            }
            Err(context.mismatch(source, compact_type))
        }
        _ => complex_type::check_complex_type_compact(context, source, compact_type, check_guard),
    }
}

fn table_associated_type_def(
    model: &SemanticModel,
    table: &crate::InFiled<rowan::TextRange>,
) -> Option<crate::TypeDef> {
    let facts = model.file_facts_of(table.file_id)?;
    let decl = facts.decls.iter().find(|decl| {
        decl.value_expr_syntax
            .map(|syntax| syntax.get_range())
            .is_some_and(|range| range == table.value)
    })?;
    facts
        .type_defs
        .iter()
        .find(|def| def.owner_syntax.is_some() && def.owner_syntax == decl.owner_syntax)
        .cloned()
}

fn defs_related_ctx(
    model: &SemanticModel,
    left: &crate::TypeDef,
    right: &crate::TypeDef,
    visited: &mut Vec<crate::SemanticId>,
) -> bool {
    def_extends_ctx(model, left, right, visited)
        || def_extends_ctx(model, right, left, &mut Vec::new())
}

fn def_extends_ctx(
    model: &SemanticModel,
    source: &crate::TypeDef,
    target: &crate::TypeDef,
    visited: &mut Vec<crate::SemanticId>,
) -> bool {
    if source.id == target.id {
        return true;
    }
    if visited.contains(&source.id) {
        return false;
    }
    visited.push(source.id.clone());
    for super_name in &source.super_names {
        let super_def = model
            .resolve_type_def_in(source.file_id, super_name.as_str())
            .or_else(|| {
                model
                    .type_defs_in_scope(crate::TypeScope::Global, super_name.as_str())
                    .into_iter()
                    .next()
            });
        if let Some(super_def) = super_def
            && def_extends_ctx(model, &super_def, target, visited)
        {
            return true;
        }
    }
    false
}

fn enum_integer_values_ctx(
    model: &SemanticModel,
    def: &crate::TypeDef,
) -> std::collections::HashSet<i64> {
    let mut out = std::collections::HashSet::new();
    for member_ref in model.members_of_owner(&def.id) {
        let Some(facts) = model.file_facts_of(member_ref.file_id) else {
            continue;
        };
        let Some(member) = facts.member_by_id(&member_ref.id) else {
            continue;
        };
        let Some(value_syntax) = member.value_syntax else {
            continue;
        };
        let Some(tree) = model.syntax_tree_of(member_ref.file_id) else {
            continue;
        };
        let Some(node) = value_syntax.to_node_from_root(&tree.get_red_root()) else {
            continue;
        };
        if let Some(value) = eval_enum_integer_expr(node.text().to_string()) {
            out.insert(value);
        }
    }
    out
}

fn eval_enum_integer_expr(text: String) -> Option<i64> {
    let text = text.trim();
    if let Ok(value) = text.parse::<i64>() {
        return Some(value);
    }
    if let Some((left, right)) = text.split_once("<<") {
        let left = left.trim().parse::<i64>().ok()?;
        let right = right.trim().parse::<u32>().ok()?;
        return left.checked_shl(right);
    }
    None
}

fn is_like_any(ty: &LuaType) -> bool {
    match ty {
        LuaType::Any | LuaType::Unknown => true,
        LuaType::TplRef(tpl) => tpl.get_constraint().is_none(),
        _ => false,
    }
}

fn fast_eq_check(a: &LuaType, b: &LuaType) -> bool {
    match (a, b) {
        (LuaType::Nil, LuaType::Nil)
        | (LuaType::Table, LuaType::Table)
        | (LuaType::Userdata, LuaType::Userdata)
        | (LuaType::Function, LuaType::Function)
        | (LuaType::Thread, LuaType::Thread)
        | (LuaType::Boolean, LuaType::Boolean)
        | (LuaType::String, LuaType::String)
        | (LuaType::Integer, LuaType::Integer)
        | (LuaType::Number, LuaType::Number)
        | (LuaType::Io, LuaType::Io)
        | (LuaType::Global, LuaType::Global)
        | (LuaType::Unknown, LuaType::Unknown)
        | (LuaType::Any, LuaType::Any) => true,
        (LuaType::Ref(left), LuaType::Ref(right)) => left == right,
        (LuaType::Union(u), LuaType::Ref(id)) => {
            if let crate::LuaUnionType::Nullable(LuaType::Ref(left)) = u.deref() {
                return left == id;
            }
            false
        }
        (LuaType::Generic(left), LuaType::Generic(right)) => left == right,
        _ => false,
    }
}

// Ensure LuaTypeDeclId can be constructed globally (used for tests/base type names).
#[allow(unused)]
fn _assert_decl_id(id: LuaTypeDeclId) {
    let _ = id;
}
