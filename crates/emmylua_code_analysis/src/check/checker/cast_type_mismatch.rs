//! # cast_type_mismatch — cast target and expression type incompatible for `---@cast key -|+ type`
//!
//! origin = key expression type; target = cast type node projection; report when `is_compatible(origin, target)`
//! fails (bidirectional super↔sub is covered by type_check ref checks). Operator casts and
//! `@cast` local/global variable declaration semantics are left for later.

use emmylua_parser::{LuaAstNode, LuaDocTagCast};

use crate::DiagnosticCode;
use crate::LuaType;
use crate::semantic_model::SemanticModel;
use crate::semantic_model::type_check::is_compatible;

use super::{CheckContext, Checker};
use crate::semantic_model::render::humanize_type;

pub struct CastTypeMismatchChecker;

impl Checker for CastTypeMismatchChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::CastTypeMismatch];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        for cast_tag in root.descendants().filter_map(LuaDocTagCast::cast) {
            check_cast_tag(context, semantic_model, &cast_tag);
        }
    }
}

fn cast_origin_constraint(semantic_model: &SemanticModel<'_>, ty: &LuaType) -> Option<LuaType> {
    match ty {
        LuaType::TplRef(tpl) => tpl.get_constraint().cloned(),
        LuaType::Ref(id) | LuaType::Def(id) => {
            // Resolved type names (Animal) are not generic parameters.
            if crate::semantic_model::member::type_def_of(semantic_model, id).is_some() {
                return None;
            }
            let name = id.get_name();
            let signatures = semantic_model.signatures()?;
            for signature in signatures {
                let docs = signature.docs.as_ref()?;
                if let Some(param) = docs
                    .generic_params
                    .iter()
                    .find(|param| param.name.as_str() == name)
                    && let Some(constraint) = param.constraint
                {
                    let constraint_ty = semantic_model.doc_type_lua(constraint);
                    if !matches!(constraint_ty, LuaType::Unknown) {
                        return Some(constraint_ty);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn check_cast_tag(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    cast_tag: &LuaDocTagCast,
) {
    let Some(key_expr) = cast_tag.get_key_expr() else {
        return;
    };
    let origin_type = semantic_model.type_of_expr(key_expr.get_syntax_id());
    // Generic parameters participate via their constraint: `---@cast animal Animal` is valid for `T: Animal`.
    let check_origin =
        cast_origin_constraint(semantic_model, &origin_type).unwrap_or_else(|| origin_type.clone());
    for op_type in cast_tag.get_op_types() {
        // Casts with operators are not checked.
        if op_type.get_op().is_some() {
            continue;
        }
        let Some(target_doc_type) = op_type.get_type() else {
            continue;
        };
        let target_type = semantic_model.doc_type_lua(target_doc_type.get_syntax_id());
        if is_compatible(semantic_model, &check_origin, &target_type)
            || cast_broad_compatible(&check_origin, &target_type)
        {
            continue;
        }
        context.add_diagnostic(
            DiagnosticCode::CastTypeMismatch,
            op_type.get_range(),
            t!(
                "Cannot cast `%{original}` to `%{target}`. %{reason}",
                original = humanize_type(semantic_model, &origin_type),
                target = humanize_type(semantic_model, &target_type),
                reason = ""
            ),
        );
    }
}

/// `table` is a supertype of tuple/array/object: casts between `---@type table?` and `[integer, integer]?`
/// do not report (legacy Unknown fallback also allowed them).
fn cast_broad_compatible(origin: &LuaType, target: &LuaType) -> bool {
    fn strip_union(ty: &LuaType) -> Option<LuaType> {
        match ty {
            LuaType::Union(union) => {
                let types = union.into_vec();
                types.iter().find(|ty| !matches!(ty, LuaType::Nil)).cloned()
            }
            other => Some(other.clone()),
        }
    }
    matches!(
        (strip_union(origin), strip_union(target)),
        (Some(LuaType::Table), Some(LuaType::Tuple(_)))
            | (Some(LuaType::Tuple(_)), Some(LuaType::Table))
    )
}
