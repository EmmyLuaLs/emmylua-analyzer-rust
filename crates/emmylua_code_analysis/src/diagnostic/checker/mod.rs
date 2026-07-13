pub(crate) mod access_invisible;
pub(crate) mod analyze_error;
pub(crate) mod assign_type_mismatch;
pub(crate) mod attribute_check;
pub(crate) mod await_in_sync;
pub(crate) mod call_non_callable;
pub(crate) mod cast_type_mismatch;
pub(crate) mod check_field;
pub(crate) mod check_param_count;
pub(crate) mod check_return_count;
pub(crate) mod circle_doc_class;
pub(crate) mod code_style;
pub(crate) mod deprecated;
pub(crate) mod discard_returns;
pub(crate) mod duplicate_field;
pub(crate) mod duplicate_index;
pub(crate) mod duplicate_require;
pub(crate) mod duplicate_type;
pub(crate) mod enum_value_mismatch;
pub(crate) mod generic;
pub(crate) mod global_non_module;
pub(crate) mod incomplete_signature_doc;
pub(crate) mod local_const_reassign;
pub(crate) mod missing_fields;
pub(crate) mod need_check_nil;
pub(crate) mod param_type_check;
pub(crate) mod readonly_check;
pub(crate) mod redefined_local;
pub(crate) mod return_type_mismatch;
pub(crate) mod syntax_error;
pub(crate) mod type_access_modifier;
pub(crate) mod unbalanced_assignments;
pub(crate) mod undefined_doc_param;
pub(crate) mod undefined_global;
pub(crate) mod unknown_doc_tag;
pub(crate) mod unnecessary_assert;
pub(crate) mod unnecessary_if;
pub(crate) mod unused;

// ⏳ 待迁移 (Checker trait bridge)
// mod check_export; // needs check_field::is_valid_member (old API)
// mod check_param_count; // migrated

use emmylua_parser::{
    LuaAstNode, LuaClosureExpr, LuaComment, LuaReturnStat, LuaStat, LuaSyntaxId, LuaSyntaxKind,
};
use hashbrown::HashMap;
use lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString};
use rowan::TextRange;
use std::sync::Arc;

use crate::semantic_model::SemanticModel;
use crate::semantic_model::humanize::humanize_type_salsa;
use crate::{DiagnosticCode, FileId, LuaType, RenderLevel, SalsaSummaryDatabase};

use super::lua_diagnostic_code::{get_default_severity, is_code_default_enable};
use super::lua_diagnostic_config::LuaDiagnosticConfig;

/// Old checker trait — retained for bridge migration.
#[allow(unused)]
pub trait Checker {
    const CODES: &[DiagnosticCode];
    fn check(context: &mut DiagnosticContext, semantic_model: &crate::semantic::SemanticModel);
}

#[allow(unused)]
fn run_check<T: Checker>(
    context: &mut DiagnosticContext,
    semantic_model: &crate::semantic::SemanticModel,
) {
    if T::CODES
        .iter()
        .any(|code| context.is_checker_enable_by_code(code))
    {
        T::check(context, semantic_model);
    }
}

pub struct DiagnosticContext<'a> {
    file_id: FileId,
    salsa_db: &'a SalsaSummaryDatabase,
    diagnostics: Vec<Diagnostic>,
    table_expr_check_cache: HashMap<(LuaSyntaxId, LuaType), bool>,
    pub config: Arc<LuaDiagnosticConfig>,
}

impl<'a> DiagnosticContext<'a> {
    pub fn new(
        file_id: FileId,
        salsa_db: &'a SalsaSummaryDatabase,
        config: Arc<LuaDiagnosticConfig>,
    ) -> Self {
        Self {
            file_id,
            salsa_db,
            diagnostics: Vec::new(),
            table_expr_check_cache: HashMap::new(),
            config,
        }
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn get_salsa_db(&self) -> &SalsaSummaryDatabase {
        self.salsa_db
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

    fn should_report(&self, code: &DiagnosticCode, range: &TextRange) -> bool {
        !self.is_salsa_diag_disabled_at(code, range)
    }

    fn is_salsa_diag_disabled_at(&self, code: &DiagnosticCode, range: &TextRange) -> bool {
        let Some(status) = self.salsa_db.doc().diagnostic_status(self.file_id) else {
            return false;
        };
        for action in &status.actions {
            if !action.is_disable {
                continue;
            }
            if action.range.intersect(*range).is_none() {
                continue;
            }
            if action.code.is_none_or(|c| c == *code) {
                return true;
            }
        }
        false
    }

    fn get_severity(&self, code: DiagnosticCode) -> Option<DiagnosticSeverity> {
        self.config
            .severity
            .get(&code)
            .copied()
            .or_else(|| Some(get_default_severity(code)))
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
        let (start_line, start_character) = self
            .salsa_db
            .offset_to_line_col(self.file_id, range.start())?;
        let (end_line, end_character) = self
            .salsa_db
            .offset_to_line_col(self.file_id, range.end())?;
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
        // workspace-level config
        if self.config.workspace_disabled.contains(code) {
            return false;
        }
        if self.config.workspace_enabled.contains(code) {
            return true;
        }

        // salsa doc: @meta file — skip all diagnostics
        if self.is_salsa_meta_file() {
            return false;
        }

        // salsa doc: @diagnostic enable / disable per-file
        let (salsa_enable, salsa_disable) = self.salsa_diag_file_status(code);
        if salsa_enable {
            return true;
        }
        if salsa_disable {
            return false;
        }

        is_code_default_enable(code, self.config.level)
    }

    /// salsa: @meta 文件检查 (kind == Module 的 tag)
    fn is_salsa_meta_file(&self) -> bool {
        self.salsa_db
            .file()
            .summary(self.file_id)
            .map(|s| s.is_meta)
            .unwrap_or(false)
    }

    /// salsa: 从 doc tags 检查 @diagnostic enable/disable
    fn salsa_diag_file_status(&self, code: &DiagnosticCode) -> (bool, bool) {
        let Some(status) = self.salsa_db.doc().diagnostic_status(self.file_id) else {
            return (false, false);
        };
        (
            status.enabled_codes.contains(code),
            status.disabled_codes.contains(code),
        )
    }
}

/// Salsa-native humanize_lint_type_salsa.
pub fn humanize_lint_type_salsa(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    typ: &LuaType,
) -> String {
    humanize_type_salsa(db, file_id, typ, RenderLevel::Simple)
}

pub fn check_file(context: &mut DiagnosticContext, model: &SemanticModel) {
    syntax_error::check(context, model);
    attribute_check::check(context, model);
    unused::check_unused(context, model);
    unnecessary_assert::check(context, model);
    await_in_sync::check(context, model);
    unnecessary_assert::check(context, model);
    need_check_nil::check(context, model);
    local_const_reassign::check(context, model);
    redefined_local::check(context, model);
    check_field::check(context, model);
    assign_type_mismatch::check(context, model);
    unbalanced_assignments::check(context, model);
    duplicate_index::check(context, model);
    duplicate_require::check(context, model);
    unnecessary_if::check(context, model);
    code_style::invert_if::check(context, model);
    code_style::non_literal_expressions_in_assert::check(context, model);
    code_style::preferred_local_alias::check(context, model);
    unknown_doc_tag::check(context, model);
    readonly_check::check(context, model);
    undefined_global::check(context, model);
    access_invisible::check(context, model);
    check_param_count::check(context, model);
    check_return_count::check(context, model);
    discard_returns::check(context, model);
    analyze_error::check(context, model);
    call_non_callable::check(context, model);
    circle_doc_class::check(context, model);
    deprecated::check(context, model);
    generic::generic_constraint_mismatch::check(context, model);
    global_non_module::check(context, model);
    duplicate_type::check(context, model);
    incomplete_signature_doc::check(context, model);
    param_type_check::check(context, model);
    return_type_mismatch::check(context, model);
    undefined_doc_param::check(context, model);
    check_return_count::check(context, model);
    duplicate_field::check(context, model);
    cast_type_mismatch::check(context, model);
    missing_fields::check(context, model);
    enum_value_mismatch::check(context, model);
    type_access_modifier::check(context, model);
    duplicate_type::check(context, model);
    check_return_count::check(context, model);
    cast_type_mismatch::check(context, model);
    missing_fields::check(context, model);
    enum_value_mismatch::check(context, model);
}

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
    let comment = closure_expr
        .ancestors::<LuaStat>()
        .next()?
        .syntax()
        .prev_sibling()?;
    match comment.kind().into() {
        LuaSyntaxKind::Comment => LuaComment::cast(comment),
        _ => None,
    }
}
