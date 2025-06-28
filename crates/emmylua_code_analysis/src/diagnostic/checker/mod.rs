mod access_invisible;
mod analyze_error;
mod assign_type_mismatch;
mod await_in_sync;
mod cast_type_mismatch;
mod check_field;
mod check_param_count;
mod check_return_count;
mod circle_doc_class;
mod code_style;
mod code_style_check;
mod deprecated;
mod discard_returns;
mod duplicate_field;
mod duplicate_index;
mod duplicate_require;
mod duplicate_type;
mod generic;
mod incomplete_signature_doc;
mod local_const_reassign;
mod missing_fields;
mod need_check_nil;
mod param_type_check;
mod redefined_local;
mod require_module_visibility;
mod return_type_mismatch;
mod syntax_error;
mod unbalanced_assignments;
mod undefined_doc_param;
mod undefined_global;
mod unnecessary_assert;
mod unnecessary_if;
mod unused;

use emmylua_parser::{
    LuaAstNode, LuaClosureExpr, LuaComment, LuaReturnStat, LuaStat, LuaSyntaxKind,
};
use lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString};
use rowan::TextRange;
use std::sync::Arc;

use crate::{
    db_index::DbIndex, humanize_type, semantic::SemanticModel, FileId, LuaType, RenderLevel,
};

use super::{
    lua_diagnostic_code::{get_default_severity, is_code_default_enable},
    lua_diagnostic_config::LuaDiagnosticConfig,
    DiagnosticCode,
};

pub trait Checker {
    const CODES: &[DiagnosticCode];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel);
}

fn run_check<T: Checker>(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
    if T::CODES
        .iter()
        .any(|code| context.is_checker_enable_by_code(code))
    {
        T::check(context, semantic_model);
    }
}

pub fn check_file(context: &mut DiagnosticContext, semantic_model: &SemanticModel) -> Option<()> {
    run_check::<syntax_error::SyntaxErrorChecker>(context, semantic_model);
    run_check::<analyze_error::AnalyzeErrorChecker>(context, semantic_model);
    run_check::<unused::UnusedChecker>(context, semantic_model);
    run_check::<deprecated::DeprecatedChecker>(context, semantic_model);
    run_check::<undefined_global::UndefinedGlobalChecker>(context, semantic_model);
    run_check::<unnecessary_assert::UnnecessaryAssertChecker>(context, semantic_model);
    run_check::<unnecessary_if::UnnecessaryIfChecker>(context, semantic_model);
    run_check::<access_invisible::AccessInvisibleChecker>(context, semantic_model);
    run_check::<local_const_reassign::LocalConstReassignChecker>(context, semantic_model);
    run_check::<discard_returns::DiscardReturnsChecker>(context, semantic_model);
    run_check::<await_in_sync::AwaitInSyncChecker>(context, semantic_model);
    run_check::<param_type_check::ParamTypeCheckChecker>(context, semantic_model);
    run_check::<need_check_nil::NeedCheckNilChecker>(context, semantic_model);
    run_check::<code_style_check::CodeStyleCheckChecker>(context, semantic_model);
    run_check::<return_type_mismatch::ReturnTypeMismatch>(context, semantic_model);
    run_check::<undefined_doc_param::UndefinedDocParamChecker>(context, semantic_model);
    run_check::<redefined_local::RedefinedLocalChecker>(context, semantic_model);
    run_check::<missing_fields::MissingFieldsChecker>(context, semantic_model);
    run_check::<check_field::CheckFieldChecker>(context, semantic_model);
    run_check::<circle_doc_class::CircleDocClassChecker>(context, semantic_model);
    run_check::<incomplete_signature_doc::IncompleteSignatureDocChecker>(context, semantic_model);
    run_check::<assign_type_mismatch::AssignTypeMismatchChecker>(context, semantic_model);
    run_check::<duplicate_require::DuplicateRequireChecker>(context, semantic_model);
    run_check::<duplicate_type::DuplicateTypeChecker>(context, semantic_model);
    run_check::<check_return_count::CheckReturnCount>(context, semantic_model);
    run_check::<unbalanced_assignments::UnbalancedAssignmentsChecker>(context, semantic_model);
    run_check::<check_param_count::CheckParamCountChecker>(context, semantic_model);
    run_check::<duplicate_field::DuplicateFieldChecker>(context, semantic_model);
    run_check::<duplicate_index::DuplicateIndexChecker>(context, semantic_model);
    run_check::<generic::generic_constraint_mismatch::GenericConstraintMismatchChecker>(
        context,
        semantic_model,
    );
    run_check::<cast_type_mismatch::CastTypeMismatchChecker>(context, semantic_model);
    run_check::<require_module_visibility::RequireModuleVisibilityChecker>(context, semantic_model);

    run_check::<code_style::non_literal_expressions_in_assert::NonLiteralExpressionsInAssertChecker>(
        context,
        semantic_model,
    );
    Some(())
}

pub struct DiagnosticContext<'a> {
    file_id: FileId,
    db: &'a DbIndex,
    diagnostics: Vec<Diagnostic>,
    pub config: Arc<LuaDiagnosticConfig>,
}

impl<'a> DiagnosticContext<'a> {
    pub fn new(file_id: FileId, db: &'a DbIndex, config: Arc<LuaDiagnosticConfig>) -> Self {
        Self {
            file_id,
            db,
            diagnostics: Vec::new(),
            config,
        }
    }

    pub fn get_db(&self) -> &DbIndex {
        &self.db
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn add_diagnostic(
        &mut self,
        code: DiagnosticCode,
        range: TextRange,
        message: String,
        data: Option<serde_json::Value>,
    ) {
        if !self.is_checker_enable_by_code(&code) {
            return;
        }

        if !self.should_report_diagnostic(&code, &range) {
            return;
        }

        let diagnostic = Diagnostic {
            message,
            range: self.translate_range(range).unwrap_or(lsp_types::Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 0,
                },
                end: lsp_types::Position {
                    line: 0,
                    character: 0,
                },
            }),
            severity: self.get_severity(code),
            code: Some(NumberOrString::String(code.get_name().to_string())),
            source: Some("EmmyLua".into()),
            tags: self.get_tags(code),
            data,
            ..Default::default()
        };

        self.diagnostics.push(diagnostic);
    }

    fn should_report_diagnostic(&self, code: &DiagnosticCode, range: &TextRange) -> bool {
        let diagnostic_index = self.get_db().get_diagnostic_index();

        !diagnostic_index.is_file_diagnostic_code_disabled(&self.get_file_id(), code, range)
    }

    fn get_severity(&self, code: DiagnosticCode) -> Option<DiagnosticSeverity> {
        if let Some(severity) = self.config.severity.get(&code) {
            return Some(severity.clone());
        }

        Some(get_default_severity(code))
    }

    fn get_tags(&self, code: DiagnosticCode) -> Option<Vec<DiagnosticTag>> {
        match code {
            DiagnosticCode::Unused | DiagnosticCode::UnreachableCode => {
                Some(vec![DiagnosticTag::UNNECESSARY])
            }
            DiagnosticCode::Deprecated => Some(vec![DiagnosticTag::DEPRECATED]),
            _ => None,
        }
    }

    fn translate_range(&self, range: TextRange) -> Option<lsp_types::Range> {
        let document = self.db.get_vfs().get_document(&self.file_id)?;
        let (start_line, start_character) = document.get_line_col(range.start())?;
        let (end_line, end_character) = document.get_line_col(range.end())?;

        Some(lsp_types::Range {
            start: lsp_types::Position {
                line: start_line as u32,
                character: start_character as u32,
            },
            end: lsp_types::Position {
                line: end_line as u32,
                character: end_character as u32,
            },
        })
    }

    pub fn get_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub fn is_checker_enable_by_code(&self, code: &DiagnosticCode) -> bool {
        let file_id = self.get_file_id();
        let db = self.get_db();
        let diagnostic_index = db.get_diagnostic_index();
        // force enable
        if diagnostic_index.is_file_enabled(&file_id, &code) {
            return true;
        }

        // workspace force disabled
        if self.config.workspace_disabled.contains(&code) {
            return false;
        }

        let module_index = db.get_module_index();
        // ignore meta file diagnostic
        if module_index.is_meta_file(&file_id) {
            return false;
        }

        // is file disabled this code
        if diagnostic_index.is_file_disabled(&file_id, &code) {
            return false;
        }

        // workspace force enabled
        if self.config.workspace_enabled.contains(&code) {
            return true;
        }

        // default setting
        is_code_default_enable(&code)
    }
}

fn get_closure_expr_comment(closure_expr: &LuaClosureExpr) -> Option<LuaComment> {
    let comment = closure_expr
        .ancestors::<LuaStat>()
        .next()?
        .syntax()
        .prev_sibling()?;
    match comment.kind().into() {
        LuaSyntaxKind::Comment => {
            let comment = LuaComment::cast(comment)?;
            Some(comment)
        }
        _ => None,
    }
}

/// 获取属于自身的返回语句
pub fn get_return_stats(closure_expr: &LuaClosureExpr) -> impl Iterator<Item = LuaReturnStat> + '_ {
    closure_expr
        .descendants::<LuaReturnStat>()
        .filter(move |stat| {
            stat.ancestors::<LuaClosureExpr>()
                .next()
                .map_or(false, |expr| &expr == closure_expr)
        })
}

pub fn humanize_lint_type(db: &DbIndex, typ: &LuaType) -> String {
    match typ {
        // TODO: 应该仅去掉命名空间
        // LuaType::Ref(type_decl_id) => type_decl_id.get_simple_name().to_string(),
        // LuaType::Generic(generic_type) => generic_type
        //     .get_base_type_id()
        //     .get_simple_name()
        //     .to_string(),
        LuaType::IntegerConst(_) => "integer".to_string(),
        LuaType::FloatConst(_) => "number".to_string(),
        LuaType::BooleanConst(_) => "boolean".to_string(),
        LuaType::StringConst(_) => "string".to_string(),
        LuaType::DocStringConst(_) => "string".to_string(),
        LuaType::DocIntegerConst(_) => "integer".to_string(),
        LuaType::DocBooleanConst(_) => "boolean".to_string(),
        _ => humanize_type(db, typ, RenderLevel::Simple),
    }
}
