//! # analyze_error — errors during doc type resolution (TypeNotFound / AnnotationUsageError / MissingTypeArgument)
//!
//! The old implementation read errors accumulated during `analyze_doc_type` from the DbIndex `DiagnosticIndex`;
//! the salsa layer splits this into two paths:
//! - annotation usage errors such as `@field` without `@class` are accumulated in `FileFacts.annotation_errors` during fact extraction;
//! - `TypeNotFound` / `MissingTypeArgument` are checked by traversing all `LuaDocType` nodes in the file on demand.

use std::collections::HashSet;

use emmylua_parser::{
    LuaAstNode, LuaDocFuncType, LuaDocGenericType, LuaDocMappedType, LuaDocNameType, LuaDocType,
    LuaSyntaxKind,
};
use smol_str::SmolStr;

use crate::DiagnosticCode;
use crate::semantic_model::SemanticModel;

use super::{CheckContext, Checker};

pub struct AnalyzeErrorChecker;

impl Checker for AnalyzeErrorChecker {
    const CODES: &[DiagnosticCode] = &[
        DiagnosticCode::TypeNotFound,
        DiagnosticCode::AnnotationUsageError,
        DiagnosticCode::MissingTypeArgument,
    ];

    fn check(context: &mut CheckContext<'_>, semantic_model: &SemanticModel<'_>) {
        // Annotation usage errors accumulated during fact extraction (`@field` without `@class` context).
        if let Some(facts) = semantic_model.file_facts() {
            for error in &facts.annotation_errors {
                context.add_diagnostic(
                    DiagnosticCode::AnnotationUsageError,
                    error.range,
                    error.message.clone(),
                );
            }
        }

        let Some(tree) = semantic_model.syntax_tree() else {
            return;
        };
        let root = tree.get_red_root();
        // File-level generic name approximation (T declared in type defs and function signature docs).
        let file_generics = file_generic_names(semantic_model);
        for doc_type in root.descendants().filter_map(LuaDocType::cast) {
            check_doc_type(context, semantic_model, &doc_type, &file_generics);
        }
    }
}

fn check_doc_type(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    doc_type: &LuaDocType,
    file_generics: &HashSet<SmolStr>,
) {
    match doc_type {
        // Generic instance base names are handled by the Generic branch to avoid duplicate diagnostics.
        LuaDocType::Name(name_type) if is_generic_base(name_type) => {}
        LuaDocType::Name(name_type) => {
            check_name_type(context, semantic_model, name_type, 0, file_generics);
        }
        LuaDocType::Generic(generic_type) => {
            check_generic_type(context, semantic_model, generic_type, file_generics);
        }
        _ => {}
    }
}

fn is_generic_base(name_type: &LuaDocNameType) -> bool {
    name_type
        .syntax()
        .parent()
        .is_some_and(|parent| parent.kind() == LuaSyntaxKind::TypeGeneric.into())
}

fn check_name_type(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    name_type: &LuaDocNameType,
    arg_count: usize,
    file_generics: &HashSet<SmolStr>,
) {
    let Some(name) = name_type.get_name_text() else {
        return;
    };
    if in_mapped_type(name_type) {
        return;
    }
    if is_builtin_name(&name)
        || file_generics.contains(name.as_str())
        || in_func_generic_scope(name_type, &name)
    {
        return;
    }
    let Some(def) = semantic_model.resolve_type_def(&name) else {
        context.add_diagnostic(
            DiagnosticCode::TypeNotFound,
            name_type.get_range(),
            t!("Type '%{name}' not found", name = name),
        );
        return;
    };
    check_missing_type_args(context, &def, arg_count, name_type.get_range());
}

fn check_generic_type(
    context: &mut CheckContext<'_>,
    semantic_model: &SemanticModel<'_>,
    generic_type: &LuaDocGenericType,
    file_generics: &HashSet<SmolStr>,
) {
    let Some(name_type) = generic_type.get_name_type() else {
        return;
    };
    let Some(name) = name_type.get_name_text() else {
        return;
    };
    if !is_builtin_name(&name)
        && !file_generics.contains(name.as_str())
        && !in_func_generic_scope(&name_type, &name)
    {
        let Some(def) = semantic_model.resolve_type_def(&name) else {
            context.add_diagnostic(
                DiagnosticCode::TypeNotFound,
                generic_type.get_range(),
                t!("Type '%{name}' not found", name = name),
            );
            return;
        };
        let arg_count = generic_type
            .get_generic_types()
            .map(|list| list.get_types().count())
            .unwrap_or(0);
        check_missing_type_args(context, &def, arg_count, generic_type.get_range());
    }
}

/// Required generic argument check (mirrors legacy `complete_type_generic_args`: params with defaults may be omitted).
fn check_missing_type_args(
    context: &mut CheckContext<'_>,
    def: &crate::TypeDef,
    arg_count: usize,
    range: rowan::TextRange,
) {
    let params = &def.generic_params;
    if params.is_empty() || arg_count >= params.len() {
        return;
    }
    let missing_required = params[arg_count..]
        .iter()
        .any(|param| param.default.is_none());
    if !missing_required {
        return;
    }
    let generic_name = format!(
        "{}<{}>",
        def.full_name,
        params
            .iter()
            .map(|param| param.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    context.add_diagnostic(
        DiagnosticCode::MissingTypeArgument,
        range,
        t!(
            "Generic type '%{generic_name}' requires %{count} type argument(s)",
            generic_name = generic_name,
            count = params.len()
        ),
    );
}

fn is_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "unknown"
            | "never"
            | "nil"
            | "void"
            | "any"
            | "userdata"
            | "thread"
            | "boolean"
            | "bool"
            | "string"
            | "integer"
            | "int"
            | "number"
            | "io"
            | "self"
            | "global"
            | "function"
            | "table"
            | "..."
    )
}

/// Generic names declared on `fun<T>(...)` (apply to its parameter/return types).
/// Mapped type variables (`P` in `{ [P in K]: T[P] }`) are not global types and should not report TypeNotFound.
fn in_mapped_type(name_type: &LuaDocNameType) -> bool {
    name_type
        .syntax()
        .ancestors()
        .any(|ancestor| LuaDocMappedType::cast(ancestor).is_some())
}

fn in_func_generic_scope(name_type: &LuaDocNameType, name: &str) -> bool {
    for ancestor in name_type.syntax().ancestors() {
        if let Some(func) = LuaDocFuncType::cast(ancestor) {
            return func.get_generic_decl_list().is_some_and(|list| {
                list.get_generic_decl()
                    .filter_map(|decl| decl.get_name_token())
                    .any(|token| token.get_name_text() == name)
            });
        }
    }
    false
}

/// All generic names declared in the file (type definitions + `---@generic` function signatures; M0 approximate scope).
fn file_generic_names(semantic_model: &SemanticModel<'_>) -> HashSet<SmolStr> {
    let mut names = HashSet::new();
    let Some(facts) = semantic_model.file_facts() else {
        return names;
    };
    for def in &facts.type_defs {
        for param in &def.generic_params {
            names.insert(param.name.clone());
        }
    }
    for signature in &facts.signatures {
        if let Some(docs) = &signature.docs {
            for param in &docs.generic_params {
                names.insert(param.name.clone());
            }
        }
    }
    names
}
