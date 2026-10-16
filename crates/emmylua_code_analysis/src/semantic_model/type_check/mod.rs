//! # type_check — `LuaType` type compatibility checks (standalone directory)
//!
//! Ported from the old `semantic::type_check` (isomorphic recursive descent + `TypeCheckFailReason`),
//! but resolving the type environment only through `semantic_model::SemanticModel` (semantic):
//! - **Boolean first**: `is_compatible(source, target) -> bool` (zero reason overhead);
//! - **Detail interface**: `check_type(...) -> TypeCheckResult` (includes failure reason strings);
//! - Comparisons use a dependency graph and an explicit work queue;
//! - Parts still missing from semantic (alias origin, enum field unions, call operators) fall back to nominal checks.

mod assign;
mod comparison;
mod complex_type;
mod context;
mod fail_reason;
mod func_type;
mod generic_type;
#[cfg(test)]
mod legacy_tests;
mod ref_type;
mod simple_type;
mod sub_type;
#[cfg(test)]
mod test;

use std::ops::Deref;

pub use context::{TypeCheckContext, TypeCheckMode};
pub use fail_reason::TypeCheckFailReason;

use crate::semantic_model::SemanticModel;
use crate::semantic_model::type_eval::expand_alias_head;
use crate::{InFiled, LuaType, LuaTypeDeclId, LuaUnionType, SemanticId, TypeDef, TypeDefKind};

pub type TypeCheckResult = Result<(), TypeCheckFailReason>;

/// A definite mismatch rejects a required check even if another is unresolved.
#[derive(Default)]
pub(super) struct RequiredChecks {
    unresolved: Option<TypeCheckFailReason>,
}

impl RequiredChecks {
    pub(super) fn require(&mut self, result: TypeCheckResult) -> TypeCheckResult {
        match result {
            Err(err) if err.is_unresolved() => {
                self.unresolved = Some(err);
                Ok(())
            }
            result => result,
        }
    }

    pub(super) fn finish(self) -> TypeCheckResult {
        if let Some(err) = self.unresolved {
            Err(err)
        } else {
            Ok(())
        }
    }
}

pub(super) fn check_all(results: impl IntoIterator<Item = TypeCheckResult>) -> TypeCheckResult {
    let mut checks = RequiredChecks::default();
    for result in results {
        checks.require(result)?;
    }
    checks.finish()
}

/// Boolean compatibility: whether `source` is assignable to `target`.
pub fn is_compatible(model: &SemanticModel, source: &LuaType, target: &LuaType) -> bool {
    // Route through SemanticModel's local cache so repeated compatibility checks
    // on the same type pair in one file diagnosis do not re-run the full checker.
    model.type_check(source, target)
}

pub(crate) fn check_type(
    model: &SemanticModel,
    source: &LuaType,
    target: &LuaType,
) -> TypeCheckResult {
    let mut context = TypeCheckContext::new(model, false);
    check_general_type_compact(&mut context, source, target)
}

/// A complete alternative proves a match even if another is unresolved.
/// Without a match, an unresolved alternative prevents a definite rejection.
fn check_alternatives(
    mut failure: TypeCheckFailReason,
    checks: impl IntoIterator<Item = TypeCheckResult>,
) -> TypeCheckResult {
    for result in checks {
        match result {
            Ok(()) => return Ok(()),
            Err(err) if err.is_type_not_match() => {}
            Err(err) if err.is_unresolved() => failure = err,
            Err(err) => return Err(err),
        }
    }
    Err(failure)
}

/// Detailed compatibility: returns a reason on failure (detail mode includes human-readable strings).
pub fn check_type_detail(
    model: &SemanticModel,
    source: &LuaType,
    target: &LuaType,
) -> TypeCheckResult {
    let mut context = TypeCheckContext::new(model, true);
    check_general_type_compact(&mut context, source, target)
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
    check_general_type_compact(&mut context, source, target)
}

/// Assignment-compatibility detail interface: includes a detailed reason on failure.
pub fn check_assign_type_detail(
    model: &SemanticModel,
    source: &LuaType,
    target: &LuaType,
) -> TypeCheckResult {
    let mut context = TypeCheckContext::new(model, true);
    context.mode = TypeCheckMode::Assignment;
    check_general_type_compact(&mut context, source, target)
}

fn check_general_type_compact(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
) -> TypeCheckResult {
    comparison::check(context, source, compact_type)
}

fn check_comparison(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
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

    // Resolve outer computations before comparing their data constructors.
    for (is_source, ty) in [(false, compact_type), (true, source)] {
        let expanded = context
            .require_complete(|| expand_alias_head(context.model, ty))?
            .ok_or(TypeCheckFailReason::Unresolved)?;
        if expanded == *ty {
            continue;
        }
        return if is_source {
            check_general_type_compact(context, &expanded, compact_type)
        } else {
            check_general_type_compact(context, source, &expanded)
        };
    }
    check_general_type_compact_inner(context, source, compact_type)
}

fn check_general_type_compact_inner(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
) -> TypeCheckResult {
    if fast_eq_check(source, compact_type) {
        return Ok(());
    }

    // Check the whole boolean domain before assignment mode selects a union member.
    if matches!(source, LuaType::Boolean) {
        return simple_type::check_simple_type_compact(context, source, compact_type);
    }

    // Assignment mode: integer is assignable to number; number is not assignable to integer.
    if context.mode.uses_assignment_rules() {
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
            && def.kind == TypeDefKind::Enum
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
        // Source union members can match different target members; check them below.
        if let LuaType::Union(union) = compact_type
            && !matches!(source, LuaType::Union(_))
        {
            return check_alternatives(
                context.mismatch(source, compact_type),
                union
                    .iter()
                    .map(|component| check_general_type_compact(context, source, component)),
            );
        }
    }

    // Assignment-specific checks: semantics for object/intersection/function assignment not yet in the general path.
    if context.mode.uses_assignment_rules()
        && assign::check_special(context, source, compact_type)?.is_some()
    {
        return Ok(());
    }

    // `compact` is an intersection: source satisfies all components.
    if let LuaType::Intersection(compact_intersection) = compact_type {
        if !matches!(source, LuaType::Intersection(_)) {
            return check_alternatives(
                context.mismatch(source, compact_type),
                compact_intersection
                    .get_types()
                    .iter()
                    .map(|component| check_general_type_compact(context, source, component)),
            );
        }
    }

    match source {
        LuaType::Unknown | LuaType::Any => Ok(()),
        LuaType::TplRef(tpl) => {
            if let Some(source_constraint) = tpl.get_constraint() {
                return check_general_type_compact(context, source_constraint, compact_type);
            }
            simple_type::check_simple_type_compact(context, source, compact_type)
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
            simple_type::check_simple_type_compact(context, source, compact_type)
        }

        LuaType::Ref(type_decl_id) => {
            ref_type::check_ref_type_compact(context, type_decl_id, compact_type)
        }
        LuaType::Def(type_decl_id) => {
            ref_type::check_ref_type_compact(context, type_decl_id, compact_type)
        }

        LuaType::DocFunction(doc_func) => {
            func_type::check_doc_func_type_compact(context, doc_func, compact_type)
        }
        LuaType::Signature(sig_id) => {
            func_type::check_sig_type_compact(context, sig_id, compact_type)
        }

        LuaType::Array(_)
        | LuaType::Tuple(_)
        | LuaType::Object(_)
        | LuaType::Union(_)
        | LuaType::Intersection(_)
        | LuaType::TableGeneric(_)
        | LuaType::Call(_)
        | LuaType::MultiLineUnion(_) => {
            check_union_intersection_source(context, source, compact_type)
        }

        LuaType::Generic(generic) => {
            generic_type::check_generic_type_compact(context, generic, compact_type)
        }

        LuaType::Instance(instantiate) => {
            check_general_type_compact(context, instantiate.get_base(), compact_type)
        }
        LuaType::TypeGuard(_) => {
            if compact_type.is_boolean() {
                return Ok(());
            }
            Err(context.mismatch(source, compact_type))
        }
        // `never` is the bottom type: no value can inhabit it, so it is compatible
        // with every target (the same way `unknown`/`any` accept every source).
        LuaType::Never => Ok(()),
        LuaType::ModuleRef(_) => Ok(()),
        _ => Err(context.mismatch(source, compact_type)),
    }
}

/// Branch for when `source` is a Union/Intersection/complex type.
fn check_union_intersection_source(
    context: &mut TypeCheckContext,
    source: &LuaType,
    compact_type: &LuaType,
) -> TypeCheckResult {
    match source {
        LuaType::Union(union) => {
            // Assignment mode: every member of the source union must be assignable to the target.
            if context.mode.uses_assignment_rules() {
                return check_all(
                    union
                        .iter()
                        .map(|member| check_general_type_compact(context, member, compact_type)),
                );
            }
            // Strict subtype mode: when the target is also a union, every target component must have a matching member in the source union.
            if context.strict_union
                && let LuaType::Union(target_union) = compact_type
            {
                return check_all(target_union.iter().map(|target_component| {
                    check_general_type_compact(context, source, target_component)
                }));
            }
            // Source union: pass if any member is assignable (old semantics).
            check_alternatives(
                context.mismatch(source, compact_type),
                union
                    .iter()
                    .map(|member| check_general_type_compact(context, member, compact_type)),
            )
        }
        LuaType::Intersection(intersection) => {
            // Source intersection: any member matching is enough.
            check_alternatives(
                context.mismatch(source, compact_type),
                intersection
                    .get_types()
                    .iter()
                    .map(|member| check_general_type_compact(context, member, compact_type)),
            )
        }
        _ => complex_type::check_complex_type_compact(context, source, compact_type),
    }
}

fn table_associated_type_def(
    model: &SemanticModel,
    table: &InFiled<rowan::TextRange>,
) -> Option<TypeDef> {
    let facts = model.file_facts_of(table.file_id)?;
    let decl = facts.decl_by_value_range(table.value)?;
    facts.type_def_by_owner_syntax(decl.owner_syntax?).cloned()
}

fn defs_related_ctx(
    model: &SemanticModel,
    left: &TypeDef,
    right: &TypeDef,
    visited: &mut Vec<SemanticId>,
) -> bool {
    def_extends_ctx(model, left, right, visited)
        || def_extends_ctx(model, right, left, &mut Vec::new())
}

fn def_extends_ctx(
    model: &SemanticModel,
    source: &TypeDef,
    target: &TypeDef,
    visited: &mut Vec<SemanticId>,
) -> bool {
    if source.id == target.id {
        return true;
    }
    if visited.contains(&source.id) {
        return false;
    }
    visited.push(source.id.clone());
    for super_def in model.base_type_defs(&source.type_decl_id()) {
        if def_extends_ctx(model, &super_def, target, visited) {
            return true;
        }
    }
    false
}

fn enum_integer_values_ctx(model: &SemanticModel, def: &TypeDef) -> std::collections::HashSet<i64> {
    let mut out = std::collections::HashSet::new();
    for member_ref in model.members_of_owner(&def.id).iter() {
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
            if let LuaUnionType::Nullable(LuaType::Ref(left)) = u.deref() {
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
