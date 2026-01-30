use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaCallExpr, LuaExpr};
use std::collections::BTreeSet;

use crate::{
    DiagnosticCode, InferFailReason, InferGuard, LuaSemanticDeclId, LuaType, RenderLevel,
    SemanticDeclLevel, SemanticModel, diagnostic::checker::humanize_lint_type, humanize_type,
    semantic::infer_call_expr_func,
};

use super::{Checker, DiagnosticContext};

pub struct CallNonCallableChecker;

impl Checker for CallNonCallableChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::CallNonCallable];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        for node in semantic_model.get_root().descendants::<LuaAst>() {
            if let LuaAst::LuaCallExpr(call_expr) = node {
                check_call_expr(context, semantic_model, call_expr);
            }
        }
    }
}

fn check_call_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    call_expr: LuaCallExpr,
) -> Option<()> {
    let prefix_expr = call_expr.get_prefix_expr()?;
    let db = semantic_model.get_db();
    let call_expr_type = infer_call_target_type(semantic_model, &prefix_expr)?;
    let mut cache = semantic_model.get_cache().borrow_mut();
    let non_callable = is_definitely_non_callable(&call_expr_type);
    let call_result = infer_call_expr_func(
        db,
        &mut cache,
        call_expr.clone(),
        call_expr_type.clone(),
        &InferGuard::new(),
        None,
    );
    match call_result {
        Ok(_) if !non_callable => return Some(()),
        Ok(_) => {}
        Err(reason) if reason.is_need_resolve() => return Some(()),
        Err(reason)
            if !matches!(reason, InferFailReason::None | InferFailReason::RecursiveInfer) =>
        {
            return Some(());
        }
        Err(_) => {}
    }

    if !non_callable {
        return Some(());
    }

    let non_callable_types = collect_non_callable_union_types(db, &call_expr_type);
    let message = if !non_callable_types.is_empty() {
        t!(
            "Cannot call expression of type `%{full}`; non-callable type(s): %{types}.",
            full = humanize_type(db, &call_expr_type, RenderLevel::Detailed),
            types = non_callable_types.join(", "),
        )
        .to_string()
    } else {
        t!(
            "Cannot call expression of type `%{typ}`.",
            typ = humanize_lint_type(db, &call_expr_type),
        )
        .to_string()
    };
    context.add_diagnostic(
        DiagnosticCode::CallNonCallable,
        prefix_expr.get_range(),
        message,
        None,
    );
    Some(())
}

fn infer_call_target_type(
    semantic_model: &SemanticModel,
    prefix_expr: &LuaExpr,
) -> Option<LuaType> {
    let inferred = semantic_model.infer_expr(prefix_expr.clone()).ok();
    let typ = inferred.unwrap_or(LuaType::Unknown);
    if !matches!(
        typ,
        LuaType::Any | LuaType::Unknown | LuaType::SelfInfer | LuaType::Global
    ) {
        return Some(typ);
    }
    infer_decl_initializer_type(semantic_model, prefix_expr).or(Some(typ))
}

fn infer_decl_initializer_type(
    semantic_model: &SemanticModel,
    prefix_expr: &LuaExpr,
) -> Option<LuaType> {
    let db = semantic_model.get_db();
    let file_id = semantic_model.get_file_id();
    let expr_range = if let LuaExpr::NameExpr(name_expr) = prefix_expr {
        name_expr
            .get_name_token()
            .map(|token| token.get_range())
            .unwrap_or_else(|| prefix_expr.get_range())
    } else {
        prefix_expr.get_range()
    };

    let refs = db.get_reference_index().get_local_reference(&file_id);

    let decl_id = refs
        .and_then(|refs| refs.get_decl_id(&expr_range))
        .or_else(|| {
            let decl = semantic_model.find_decl(
                rowan::NodeOrToken::Node(prefix_expr.syntax().clone()),
                SemanticDeclLevel::default(),
            )?;
            match decl {
                LuaSemanticDeclId::LuaDecl(id) => Some(id),
                _ => None,
            }
        })
        .or_else(|| {
            let LuaExpr::NameExpr(name_expr) = prefix_expr else {
                return None;
            };
            let name_token = name_expr.get_name_token()?;
            let decl = db
                .get_decl_index()
                .get_decl_tree(&file_id)?
                .find_local_decl(name_token.get_name_text(), name_token.get_position())?;
            Some(decl.get_id())
        })?;

    let decl = db.get_decl_index().get_decl(&decl_id)?;
    let value_syntax_id = decl.get_value_syntax_id()?;
    let root = db
        .get_vfs()
        .get_syntax_tree(&decl_id.file_id)?
        .get_red_root();
    let node = value_syntax_id.to_node_from_root(&root)?;
    let expr = LuaExpr::cast(node)?;
    semantic_model.infer_expr(expr).ok()
}

fn is_definitely_non_callable(typ: &LuaType) -> bool {
    if typ.is_function() || typ.is_call() {
        return false;
    }

    match typ {
        LuaType::Any | LuaType::Unknown | LuaType::SelfInfer | LuaType::Global | LuaType::Nil => {
            false
        }
        LuaType::Union(union) => union.into_vec().iter().any(is_definitely_non_callable),
        LuaType::MultiLineUnion(union) => union
            .get_unions()
            .iter()
            .any(|(t, _)| is_definitely_non_callable(t)),
        LuaType::TypeGuard(inner) => is_definitely_non_callable(inner),
        _ => true,
    }
}

fn collect_non_callable_union_types(db: &crate::DbIndex, typ: &LuaType) -> Vec<String> {
    let mut types = BTreeSet::new();
    let mut insert_if_non_callable = |t: &LuaType| {
        if *t != LuaType::Nil && is_definitely_non_callable(t) {
            types.insert(humanize_lint_type(db, t));
        }
    };
    match typ {
        LuaType::Union(union) => {
            for t in union.into_vec() {
                insert_if_non_callable(&t);
            }
        }
        LuaType::MultiLineUnion(union) => {
            for (t, _) in union.get_unions().iter() {
                insert_if_non_callable(t);
            }
        }
        LuaType::TypeGuard(inner) => {
            return collect_non_callable_union_types(db, inner);
        }
        _ => {}
    }

    types.into_iter().collect()
}
