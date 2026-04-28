use emmylua_parser::{LuaAst, LuaAstNode, LuaAstToken, LuaIndexExpr, LuaNameExpr, VisibilityKind};
use rowan::TextRange;

use crate::{
    CompilationModuleInfo, DiagnosticCode, Emmyrc, LuaCommonProperty, LuaDeclId, LuaMemberId,
    LuaSemanticDeclId, SemanticDeclLevel, SemanticModel, SalsaDocVisibilityKindSummary,
    module_query::identity::find_compilation_module_by_path, parse_require_module_info,
    parse_require_module_info_by_name_expr, try_extract_signature_id_from_field,
};

use super::{Checker, DiagnosticContext};

pub struct AccessInvisibleChecker;

impl Checker for AccessInvisibleChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::AccessInvisible];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();
        for node in root.descendants::<LuaAst>() {
            match node {
                LuaAst::LuaNameExpr(name_expr) => {
                    check_name_expr(context, semantic_model, name_expr);
                }
                LuaAst::LuaIndexExpr(index_expr) => {
                    check_index_expr(context, semantic_model, index_expr);
                }
                _ => {}
            }
        }
    }
}

fn check_name_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    name_expr: LuaNameExpr,
) -> Option<()> {
    let semantic_decl = semantic_model.find_decl(
        rowan::NodeOrToken::Node(name_expr.syntax().clone()),
        SemanticDeclLevel::default(),
    )?;

    let decl_id = LuaDeclId::new(semantic_model.get_file_id(), name_expr.get_position());
    if let LuaSemanticDeclId::LuaDecl(id) = &semantic_decl
        && *id == decl_id
    {
        return Some(());
    }

    let name_token = name_expr.get_name_token()?;
    if !semantic_model.is_semantic_visible(name_token.syntax().clone(), semantic_decl.clone()) {
        let emmyrc = semantic_model.get_emmyrc();
        report_reason(context, emmyrc, name_token.get_range(), semantic_decl);
    }
    Some(())
}

fn check_index_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    index_expr: LuaIndexExpr,
) -> Option<()> {
    let exported_surface_module = find_required_export_surface_module(semantic_model, &index_expr);

    let semantic_decl = semantic_model.find_decl(
        rowan::NodeOrToken::Node(index_expr.syntax().clone()),
        SemanticDeclLevel::default(),
    );
    let member_id = LuaMemberId::new(index_expr.get_syntax_id(), semantic_model.get_file_id());
    if let Some(semantic_decl) = semantic_decl.as_ref()
        && let LuaSemanticDeclId::Member(id) = semantic_decl
        && *id == member_id
        && exported_surface_module.is_none()
    {
        return Some(());
    }

    let index_token = index_expr.get_index_name_token()?;
    if let Some(module_info) = exported_surface_module
        && let Some(visibility) = semantic_model
            .get_compilation()
            .module_export_member_visibility(module_info.file_id, index_token.text())
    {
        let emmyrc = semantic_model.get_emmyrc();
        report_summary_visibility(context, emmyrc, index_token.text_range(), visibility);
        return Some(());
    }

    if let Some(semantic_decl) = semantic_decl {
        if exported_surface_module.is_some() {
            let emmyrc = semantic_model.get_emmyrc();
            if report_reason(
                context,
                emmyrc,
                index_token.text_range(),
                semantic_decl.clone(),
            )
            .is_some()
            {
                return Some(());
            }

            if let LuaSemanticDeclId::Member(member_id) = semantic_decl.clone()
                && let Some(origin_owner) = semantic_model.get_member_origin_owner(member_id)
                && report_reason(context, emmyrc, index_token.text_range(), origin_owner).is_some()
            {
                return Some(());
            }
        }

        if !semantic_model.is_semantic_visible(index_token.clone(), semantic_decl.clone()) {
            let emmyrc = semantic_model.get_emmyrc();
            report_reason(context, emmyrc, index_token.text_range(), semantic_decl);
        }

        return Some(());
    }

    let module_info = exported_surface_module?;
    let visibility = semantic_model
        .get_compilation()
        .module_export_member_visibility(module_info.file_id, index_token.text())?;
    if matches!(
        visibility,
        SalsaDocVisibilityKindSummary::Private
            | SalsaDocVisibilityKindSummary::Protected
            | SalsaDocVisibilityKindSummary::Package
            | SalsaDocVisibilityKindSummary::Internal
    ) {
        let emmyrc = semantic_model.get_emmyrc();
        report_summary_visibility(context, emmyrc, index_token.text_range(), visibility);
    }

    Some(())
}

fn report_summary_visibility(
    context: &mut DiagnosticContext,
    _emmyrc: &Emmyrc,
    range: TextRange,
    visibility: SalsaDocVisibilityKindSummary,
) -> Option<()> {
    let message = match visibility {
        SalsaDocVisibilityKindSummary::Protected => {
            t!("The property is protected and cannot be accessed outside its subclasses.")
        }
        SalsaDocVisibilityKindSummary::Private => {
            t!("The property is private and cannot be accessed outside the class.")
        }
        SalsaDocVisibilityKindSummary::Package => {
            t!("The property is package-private and cannot be accessed outside the package.")
        }
        SalsaDocVisibilityKindSummary::Internal => {
            t!("The property is internal and cannot be accessed outside the current project.")
        }
        _ => return None,
    };

    context.add_diagnostic(
        DiagnosticCode::AccessInvisible,
        range,
        message.to_string(),
        None,
    );

    Some(())
}

fn find_required_export_surface_module<'a>(
    semantic_model: &'a SemanticModel,
    index_expr: &LuaIndexExpr,
) -> Option<&'a CompilationModuleInfo> {
    let prefix_expr = index_expr.get_prefix_expr()?;
    if let Some(call_expr) = emmylua_parser::LuaCallExpr::cast(prefix_expr.syntax().clone()) {
        let args_list = call_expr.get_args_list()?;
        let first_arg = args_list.get_args().next()?;
        let require_path_type = semantic_model.infer_expr(first_arg.clone()).ok()?;
        let module_path = match &require_path_type {
            crate::LuaType::StringConst(module_path) => module_path.as_ref().to_string(),
            _ => return None,
        };

        let module_info =
            find_compilation_module_by_path(semantic_model.get_compilation(), &module_path)?;
        return semantic_model
            .get_compilation()
            .module_has_export_surface(module_info.file_id)
            .then_some(module_info);
    }

    if let Some(name_expr) = LuaNameExpr::cast(prefix_expr.syntax().clone())
        && let Some(module_info) = parse_require_module_info_by_name_expr(semantic_model, &name_expr)
    {
        return semantic_model
            .get_compilation()
            .module_has_export_surface(module_info.file_id)
            .then_some(module_info);
    }

    let semantic_decl_id = semantic_model
        .get_semantic_info(prefix_expr.syntax().clone().into())
        .and_then(|info| info.semantic_decl)
        .or_else(|| {
            semantic_model.find_decl(prefix_expr.syntax().clone().into(), SemanticDeclLevel::NoTrace)
        })?;
    let crate::LuaSemanticDeclId::LuaDecl(decl_id) = semantic_decl_id else {
        return None;
    };
    let decl = semantic_model.get_decl(&decl_id)?;
    let module_info = parse_require_module_info(semantic_model, &decl)?;
    semantic_model
        .get_compilation()
        .module_has_export_surface(module_info.file_id)
        .then_some(module_info)
}

fn report_reason(
    context: &mut DiagnosticContext,
    emmyrc: &Emmyrc,
    range: TextRange,
    property_owner_id: LuaSemanticDeclId,
) -> Option<()> {
    let property = get_visibility_property(context.db(), &property_owner_id)?;

    if let Some(version_conds) = &property.version_conds() {
        let version_number = emmyrc.runtime.version.to_lua_version_number();
        let visible = version_conds.iter().any(|cond| cond.check(&version_number));
        if !visible {
            let message = t!(
                "The current Lua version %{version} is not accessible; expected %{conds}.",
                version = version_number,
                conds = version_conds
                    .iter()
                    .map(|it| format!("{}", it))
                    .collect::<Vec<_>>()
                    .join(", ")
            );

            context.add_diagnostic(
                DiagnosticCode::AccessInvisible,
                range,
                message.to_string(),
                None,
            );
            return Some(());
        }
    }

    let message = match property.visibility {
        VisibilityKind::Protected => {
            t!("The property is protected and cannot be accessed outside its subclasses.")
        }
        VisibilityKind::Private => {
            t!("The property is private and cannot be accessed outside the class.")
        }
        VisibilityKind::Package => {
            t!("The property is package-private and cannot be accessed outside the package.")
        }
        VisibilityKind::Internal => {
            t!("The property is internal and cannot be accessed outside the current project.")
        }
        _ => {
            return None;
        }
    };

    context.add_diagnostic(
        DiagnosticCode::AccessInvisible,
        range,
        message.to_string(),
        None,
    );

    Some(())
}

fn get_visibility_property<'a>(
    db: &'a crate::DbIndex,
    property_owner: &'a LuaSemanticDeclId,
) -> Option<&'a LuaCommonProperty> {
    match db.get_property_index().get_property(property_owner) {
        Some(property) => Some(property),
        None => {
            let LuaSemanticDeclId::Member(member_id) = property_owner else {
                return None;
            };
            let member_index = db.get_member_index();
            let member = member_index.get_member(member_id)?;
            if let Some(signature_id) = try_extract_signature_id_from_field(db, member)
                && let Some(property) = db
                    .get_property_index()
                    .get_property(&LuaSemanticDeclId::Signature(signature_id))
            {
                return Some(property);
            }

            let owner = member_index.get_current_owner(member_id)?;
            let member_item = member_index.get_member_item(owner, member.get_key())?;
            for candidate_id in member_item.get_member_ids() {
                if let Some(property) = db
                    .get_property_index()
                    .get_property(&LuaSemanticDeclId::Member(candidate_id))
                {
                    return Some(property);
                }

                let candidate_member = member_index.get_member(&candidate_id)?;
                if let Some(signature_id) = try_extract_signature_id_from_field(db, candidate_member)
                    && let Some(property) = db
                        .get_property_index()
                        .get_property(&LuaSemanticDeclId::Signature(signature_id))
                {
                    return Some(property);
                }
            }

            None
        }
    }
}
