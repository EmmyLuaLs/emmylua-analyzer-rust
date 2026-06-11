//! Checker pipeline.
//!
//! 每个 checker 是一个独立函数，接收 `&mut DiagnosticContext` 和 `&SemanticModel`。
//! 已迁移到新架构的 checker 直接在这里调用，未迁移的暂时注释。

// ✅ 已迁移 (new model)
pub(crate) mod access_invisible;
pub(crate) mod await_in_sync;
pub(crate) mod code_style;
pub(crate) mod analyze_error;
pub(crate) mod call_non_callable;
pub(crate) mod check_field;
pub(crate) mod check_param_count;
pub(crate) mod check_return_count;
pub(crate) mod circle_doc_class;
pub(crate) mod duplicate_index;
pub(crate) mod discard_returns;
pub(crate) mod deprecated;
pub(crate) mod duplicate_require;
pub(crate) mod local_const_reassign;
pub(crate) mod need_check_nil;
pub(crate) mod readonly_check;
pub(crate) mod return_type_mismatch;
pub(crate) mod param_type_check;
pub(crate) mod undefined_doc_param;
pub(crate) mod redefined_local;
pub(crate) mod syntax_error;
pub(crate) mod unbalanced_assignments;
pub(crate) mod undefined_global;
pub(crate) mod unnecessary_assert;
pub(crate) mod unknown_doc_tag;
pub(crate) mod unnecessary_if;
pub(crate) mod unused;

// ⏳ 待迁移 (Checker trait bridge)
mod assign_type_mismatch;
mod attribute_check;
mod cast_type_mismatch;
// mod check_export; // needs check_field::is_valid_member (old API)
// mod check_param_count; // migrated
mod duplicate_field;
mod duplicate_type;
mod enum_value_mismatch;
mod generic;
mod global_non_module;
mod incomplete_signature_doc;
mod missing_fields;
mod require_module_visibility;
mod type_access_modifier;

use emmylua_parser::{LuaAstNode, LuaClosureExpr, LuaComment, LuaReturnStat, LuaStat, LuaSyntaxKind};
use lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString};
use rowan::TextRange;
use std::sync::Arc;

use crate::db_index::DbIndex;
use crate::semantic_model::SemanticModel;
use crate::{DiagnosticCode, FileId, LuaType, RenderLevel, Vfs, humanize_type};

use super::lua_diagnostic_code::{get_default_severity, is_code_default_enable};
use super::lua_diagnostic_config::LuaDiagnosticConfig;

/// Old checker trait — retained for bridge migration.
pub trait Checker {
    const CODES: &[DiagnosticCode];
    fn check(context: &mut DiagnosticContext, semantic_model: &crate::semantic::SemanticModel);
}

fn run_check<T: Checker>(context: &mut DiagnosticContext, semantic_model: &crate::semantic::SemanticModel) {
    if T::CODES.iter().any(|code| context.is_checker_enable_by_code(code)) {
        T::check(context, semantic_model);
    }
}

pub struct DiagnosticContext<'a> {
    file_id: FileId,
    pub db: &'a DbIndex,
    diagnostics: Vec<Diagnostic>,
    pub config: Arc<LuaDiagnosticConfig>,
}

impl<'a> DiagnosticContext<'a> {
    pub fn new(file_id: FileId, db: &'a DbIndex, config: Arc<LuaDiagnosticConfig>) -> Self {
        Self { file_id, db, diagnostics: Vec::new(), config }
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn get_db(&self) -> &DbIndex {
        self.db
    }

    pub fn add_diagnostic(
        &mut self,
        code: DiagnosticCode,
        range: TextRange,
        message: String,
        data: Option<serde_json::Value>,
    ) {
        if !self.is_checker_enable_by_code(&code) || !self.should_report(&code, &range) {
            return;
        }
        let diagnostic = Diagnostic {
            message,
            range: self.translate_range(range).unwrap_or(lsp_types::Range {
                start: lsp_types::Position { line: 0, character: 0 },
                end: lsp_types::Position { line: 0, character: 0 },
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

    fn should_report(&self, code: &DiagnosticCode, range: &TextRange) -> bool {
        !self.db.get_diagnostic_index().is_file_diagnostic_code_disabled(&self.file_id, code, range)
    }

    fn get_severity(&self, code: DiagnosticCode) -> Option<DiagnosticSeverity> {
        self.config.severity.get(&code).copied().or_else(|| Some(get_default_severity(code)))
    }

    fn get_tags(&self, code: DiagnosticCode) -> Option<Vec<DiagnosticTag>> {
        match code {
            DiagnosticCode::Unused | DiagnosticCode::UnreachableCode => Some(vec![DiagnosticTag::UNNECESSARY]),
            DiagnosticCode::Deprecated => Some(vec![DiagnosticTag::DEPRECATED]),
            _ => None,
        }
    }

    fn translate_range(&self, range: TextRange) -> Option<lsp_types::Range> {
        let document = self.db.get_vfs().get_document(&self.file_id)?;
        let (start_line, start_character) = document.get_line_col(range.start())?;
        let (end_line, end_character) = document.get_line_col(range.end())?;
        Some(lsp_types::Range {
            start: lsp_types::Position { line: start_line as u32, character: start_character as u32 },
            end: lsp_types::Position { line: end_line as u32, character: end_character as u32 },
        })
    }

    pub fn get_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub fn is_checker_enable_by_code(&self, code: &DiagnosticCode) -> bool {
        let diag_index = self.db.get_diagnostic_index();
        if diag_index.is_file_enabled(&self.file_id, code) {
            return true;
        }
        if self.config.workspace_disabled.contains(code) {
            return false;
        }
        let module_index = self.db.get_module_index();
        if module_index.is_meta_file(&self.file_id) {
            return false;
        }
        if diag_index.is_file_disabled(&self.file_id, code) {
            return false;
        }
        if self.config.workspace_enabled.contains(code) {
            return true;
        }
        is_code_default_enable(code, self.config.level)
    }
}

pub fn check_file(
    context: &mut DiagnosticContext,
    model: &SemanticModel,
    vfs: &Vfs,
) {
    syntax_error::check(context, model, vfs);
    unused::check_unused(context, model);
    await_in_sync::check(context, model);
    unnecessary_assert::check(context, model);
    need_check_nil::check(context, model);
    local_const_reassign::check(context, model);
    redefined_local::check(context, model);
    check_field::check(context, model);
    unbalanced_assignments::check(context, model);
    duplicate_index::check(context, model);
    duplicate_require::check(context, model);
    unnecessary_if::check(context, model);
    code_style::invert_if::check(context, model);
    code_style::non_literal_expressions_in_assert::check(context, model);
    unknown_doc_tag::check(context, model);
    readonly_check::check(context, model);
    undefined_global::check(context, model);
    access_invisible::check(context, model);
    check_return_count::check(context, model);
    check_param_count::check(context, model);
    discard_returns::check(context, model);
    analyze_error::check(context, model);
    call_non_callable::check(context, model);
    circle_doc_class::check(context, model);
    deprecated::check(context, model);
param_type_check::check(context, model);
    return_type_mismatch::check(context, model);
    undefined_doc_param::check(context, model);
    // Bridge: old checkers via Checker trait
    {
        if let Some(tree) = context.db.get_vfs().get_syntax_tree(&model.get_file_id()) {
            let mut cache = crate::LuaInferCache::new(model.get_file_id(), Default::default());
            let old_model = crate::semantic::SemanticModel::new(
                model.get_file_id(), context.db, cache, model.get_emmyrc_arc(), tree.get_chunk_node(),
            );
            run_check::<assign_type_mismatch::AssignTypeMismatchChecker>(context, &old_model);
            run_check::<attribute_check::AttributeCheckChecker>(context, &old_model);
            run_check::<cast_type_mismatch::CastTypeMismatchChecker>(context, &old_model);
            // check_param_count migrated to new model
            run_check::<code_style::preferred_local_alias::PreferredLocalAliasChecker>(context, &old_model);
            run_check::<duplicate_field::DuplicateFieldChecker>(context, &old_model);
            run_check::<duplicate_type::DuplicateTypeChecker>(context, &old_model);
            run_check::<enum_value_mismatch::EnumValueMismatchChecker>(context, &old_model);
            run_check::<generic::generic_constraint_mismatch::GenericConstraintMismatchChecker>(context, &old_model);
            run_check::<global_non_module::GlobalInNonModuleChecker>(context, &old_model);
            run_check::<incomplete_signature_doc::IncompleteSignatureDocChecker>(context, &old_model);
            run_check::<missing_fields::MissingFieldsChecker>(context, &old_model);
            run_check::<require_module_visibility::RequireModuleVisibilityChecker>(context, &old_model);
            run_check::<type_access_modifier::InconsistentTypeAccessModifierChecker>(context, &old_model);
        }
    }

    // 以下 checkers 尚未迁移：
    // discard_returns::check(context, model);
    // undefined_global::check(context, model);
    // unnecessary_if::check(context, model);
    // access_invisible::check(context, model);
    // await_in_sync::check(context, model);
    // call_non_callable::check(context, model);
    // missing_fields::check(context, model);
    // param_type_check::check(context, model);
    // return_type_mismatch::check(context, model);
    // undefined_doc_param::check(context, model);
    // check_export::check(context, model);
    // check_field::check(context, model);
    // circle_doc_class::check(context, model);
    // incomplete_signature_doc::check(context, model);
    // assign_type_mismatch::check(context, model);
    // duplicate_require::check(context, model);
    // duplicate_type::check(context, model);
    // check_return_count::check(context, model);
    // unbalanced_assignments::check(context, model);
    // check_param_count::check(context, model);
    // duplicate_field::check(context, model);
    // duplicate_index::check(context, model);
    // generic::generic_constraint_mismatch::check(context, model);
    // cast_type_mismatch::check(context, model);
    // require_module_visibility::check(context, model);
    // unknown_doc_tag::check(context, model);
    // type_access_modifier::check(context, model);
    // enum_value_mismatch::check(context, model);
    // attribute_check::check(context, model);
    // code_style::preferred_local_alias::check(context, model);
    // code_style::invert_if::check(context, model);
    // readonly_check::check(context, model);
    // global_non_module::check(context, model);
    // check_export::check(context, model);
    // check_field::check(context, model);
    // circle_doc_class::check(context, model);
    // incomplete_signature_doc::check(context, model);
    // assign_type_mismatch::check(context, model);
    // duplicate_require::check(context, model);
    // duplicate_type::check(context, model);
    // check_return_count::check(context, model);
    // unbalanced_assignments::check(context, model);
    // check_param_count::check(context, model);
    // duplicate_field::check(context, model);
    // duplicate_index::check(context, model);
    // generic::generic_constraint_mismatch::check(context, model);
    // cast_type_mismatch::check(context, model);
    // require_module_visibility::check(context, model);
    // unknown_doc_tag::check(context, model);
    // type_access_modifier::check(context, model);
    // enum_value_mismatch::check(context, model);
    // attribute_check::check(context, model);
    // code_style::non_literal_expressions_in_assert::check(context, model);
    // code_style::preferred_local_alias::check(context, model);
    // code_style::invert_if::check(context, model);
    // readonly_check::check(context, model);
    // global_non_module::check(context, model);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 共享工具函数（被多个 checker 使用）
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub fn get_return_stats(closure_expr: &LuaClosureExpr) -> impl Iterator<Item = LuaReturnStat> + '_ {
    closure_expr
        .descendants::<LuaReturnStat>()
        .filter(move |stat| {
            stat.ancestors::<LuaClosureExpr>()
                .next()
                .is_some_and(|expr| expr == *closure_expr)
        })
}

fn get_closure_expr_comment(closure_expr: &LuaClosureExpr) -> Option<LuaComment> {
    let comment = closure_expr.ancestors::<LuaStat>().next()?.syntax().prev_sibling()?;
    match comment.kind().into() {
        LuaSyntaxKind::Comment => LuaComment::cast(comment),
        _ => None,
    }
}

pub fn humanize_lint_type(db: &DbIndex, typ: &LuaType) -> String {
    match typ {
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
