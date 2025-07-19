use emmylua_diagnostic_macro::LuaDiagnosticMacro;
use emmylua_parser::LuaLanguageLevel;
use lsp_types::DiagnosticSeverity;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, LuaDiagnosticMacro,
)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticCode {
    /// Syntax error
    SyntaxError,
    /// Doc syntax error
    DocSyntaxError,
    /// Type not found
    TypeNotFound,
    /// Missing return statement
    MissingReturn,
    /// Param Type not match
    ParamTypeNotMatch,
    /// Missing parameter
    MissingParameter,
    /// Redundant parameter
    RedundantParameter,
    /// Unreachable code
    UnreachableCode,
    /// Unused
    Unused,
    /// Undefined global
    UndefinedGlobal,
    /// Deprecated
    Deprecated,
    /// Access invisible
    AccessInvisible,
    /// Discard return value
    DiscardReturns,
    /// Undefined field
    UndefinedField,
    /// Local const reassign
    LocalConstReassign,
    /// Iter variable reassign
    IterVariableReassign,
    /// Duplicate type
    DuplicateType,
    /// Redefined local
    RedefinedLocal,
    /// Redefined label
    RedefinedLabel,
    /// Code style check
    CodeStyleCheck,
    /// Need check nil
    NeedCheckNil,
    /// Await in sync
    AwaitInSync,
    /// Doc tag usage error
    AnnotationUsageError,
    /// Custom doc tag usage warning
    CustomAnnotationUsageWarning,
    /// Return type mismatch
    ReturnTypeMismatch,
    /// Missing return value
    MissingReturnValue,
    /// Redundant return value
    RedundantReturnValue,
    /// Undefined Doc Param
    UndefinedDocParam,
    /// Duplicate doc field
    DuplicateDocField,
    /// Unknown doc annotation
    UnknownDocTag,
    /// Missing fields
    MissingFields,
    /// Inject Field
    InjectField,
    /// Circle Doc Class
    CircleDocClass,
    /// Incomplete signature doc
    IncompleteSignatureDoc,
    /// Missing global doc
    MissingGlobalDoc,
    /// Assign type mismatch
    AssignTypeMismatch,
    /// Duplicate require
    DuplicateRequire,
    /// non-literal-expressions-in-assert
    NonLiteralExpressionsInAssert,
    /// Unbalanced assignments
    UnbalancedAssignments,
    /// unnecessary-assert
    UnnecessaryAssert,
    /// unnecessary-if
    UnnecessaryIf,
    /// duplicate-set-field
    DuplicateSetField,
    /// duplicate-index
    DuplicateIndex,
    /// generic-constraint-mismatch
    GenericConstraintMismatch,
    /// cast-type-mismatch
    CastTypeMismatch,
    /// require-module-not-visible
    RequireModuleNotVisible,

    #[serde(other)]
    None,
}

// Update functions to match enum variants
pub fn get_default_severity(code: DiagnosticCode) -> DiagnosticSeverity {
    match code {
        DiagnosticCode::SyntaxError => DiagnosticSeverity::ERROR,
        DiagnosticCode::DocSyntaxError => DiagnosticSeverity::ERROR,
        DiagnosticCode::TypeNotFound => DiagnosticSeverity::WARNING,
        DiagnosticCode::MissingReturn => DiagnosticSeverity::WARNING,
        DiagnosticCode::ParamTypeNotMatch => DiagnosticSeverity::WARNING,
        DiagnosticCode::MissingParameter => DiagnosticSeverity::WARNING,
        DiagnosticCode::UnreachableCode => DiagnosticSeverity::HINT,
        DiagnosticCode::Unused => DiagnosticSeverity::HINT,
        DiagnosticCode::UndefinedGlobal => DiagnosticSeverity::ERROR,
        DiagnosticCode::Deprecated => DiagnosticSeverity::HINT,
        DiagnosticCode::AccessInvisible => DiagnosticSeverity::WARNING,
        DiagnosticCode::DiscardReturns => DiagnosticSeverity::WARNING,
        DiagnosticCode::UndefinedField => DiagnosticSeverity::WARNING,
        DiagnosticCode::LocalConstReassign => DiagnosticSeverity::ERROR,
        DiagnosticCode::DuplicateType => DiagnosticSeverity::WARNING,
        DiagnosticCode::AnnotationUsageError => DiagnosticSeverity::ERROR,
        DiagnosticCode::RedefinedLocal => DiagnosticSeverity::HINT,
        DiagnosticCode::DuplicateRequire => DiagnosticSeverity::HINT,
        DiagnosticCode::IterVariableReassign => DiagnosticSeverity::ERROR,
        DiagnosticCode::CustomAnnotationUsageWarning => DiagnosticSeverity::WARNING,
        _ => DiagnosticSeverity::WARNING,
    }
}

pub fn is_code_default_enable(code: &DiagnosticCode, level: LuaLanguageLevel) -> bool {
    match code {
        DiagnosticCode::IterVariableReassign => level >= LuaLanguageLevel::Lua55,
        DiagnosticCode::CodeStyleCheck => false,
        DiagnosticCode::IncompleteSignatureDoc => false,
        DiagnosticCode::MissingGlobalDoc => false,
        // ... handle other variants

        // neovim-code-style
        DiagnosticCode::CustomAnnotationUsageWarning => false,
        DiagnosticCode::NonLiteralExpressionsInAssert => false,
        DiagnosticCode::UnknownDocTag => false,

        _ => true,
    }
}
