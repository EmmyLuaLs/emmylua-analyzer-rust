use emmylua_parser::{LuaAst, LuaAstNode, LuaComment, LuaDocTagType};

use crate::{
    AssignabilityResult, DiagnosticCode, LuaMemberId, LuaTypeCache, RenderLevel, SemanticModel,
    humanize_type,
};

use crate::diagnostic::checker::{
    Checker, DiagnosticContext, DiagnosticMessage, humanize_lint_type, render_diagnostic_detail,
};

pub struct TableFieldTypeMismatchChecker;

impl Checker for TableFieldTypeMismatchChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::AssignTypeMismatch];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        for type_tag in semantic_model.get_root().descendants::<LuaDocTagType>() {
            check_type_tag(context, semantic_model, &type_tag);
        }
    }
}

fn check_type_tag(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    type_tag: &LuaDocTagType,
) -> Option<()> {
    let comment = type_tag.get_parent::<LuaComment>()?;
    let LuaAst::LuaTableField(field) = comment.get_owner()? else {
        return Some(());
    };
    let value_expr = field.get_value_expr()?;
    let member_id = LuaMemberId::new(field.get_syntax_id(), semantic_model.get_file_id());
    let type_cache = semantic_model
        .get_db()
        .get_type_index()
        .get_type_cache(&member_id.into())?;
    let LuaTypeCache::DocType(annotated_type) = type_cache else {
        return Some(());
    };
    let actual_type = semantic_model.infer_expr(value_expr.clone()).ok()?;
    if actual_type.is_unknown() || actual_type.is_any() {
        return Some(());
    }

    let AssignabilityResult::NotAssignable(mismatch) =
        semantic_model.check_assignable(&actual_type, annotated_type)
    else {
        return Some(());
    };

    context.add_diagnostic(
        DiagnosticCode::AssignTypeMismatch,
        value_expr.get_range(),
        DiagnosticMessage::with_detail(
            t!(
                "Cannot assign `%{actual}` to the `@type %{annotated}` annotation.",
                annotated =
                    humanize_type(semantic_model.get_db(), annotated_type, RenderLevel::Simple),
                actual = humanize_lint_type(semantic_model.get_db(), &actual_type),
            )
            .to_string(),
            render_diagnostic_detail(
                semantic_model.get_db(),
                &mismatch,
                &actual_type,
                annotated_type,
            ),
        ),
        None,
    );

    Some(())
}
