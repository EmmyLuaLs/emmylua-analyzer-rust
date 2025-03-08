use emmylua_diagnostic_macro::LuaDiagnosticMacro;
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
    /// Lua syntax error
    LuaSyntaxError,
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
    /// Inject field fail
    InjectFieldFail,
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
    /// Disable global define
    DisableGlobalDefine,
    /// Undefined field
    UndefinedField,
    /// Local const reassign
    LocalConstReassign,
    /// Iter variable reassign
    IterVariableReassign,
    /// Duplicate type
    DuplicateType,
    /// Multi env
    MultiEnv,
    /// Redefined local
    RedefinedLocal,
    /// Redefined label
    RedefinedLabel,
    /// Name Style check
    NameStyleCheck,
    /// Code style check
    CodeStyleCheck,
    /// Need check nil
    NeedCheckNil,
    /// Await in sync
    AwaitInSync,
    /// Doc tag usage error
    AnnotationUsageError,
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

    #[serde(other)]
    None,
}

// Update functions to match enum variants
pub fn get_default_severity(code: DiagnosticCode) -> DiagnosticSeverity {
    match code {
        DiagnosticCode::SyntaxError => DiagnosticSeverity::ERROR,
        DiagnosticCode::TypeNotFound => DiagnosticSeverity::WARNING,
        DiagnosticCode::MissingReturn => DiagnosticSeverity::WARNING,
        DiagnosticCode::ParamTypeNotMatch => DiagnosticSeverity::WARNING,
        DiagnosticCode::MissingParameter => DiagnosticSeverity::WARNING,
        DiagnosticCode::InjectFieldFail => DiagnosticSeverity::ERROR,
        DiagnosticCode::UnreachableCode => DiagnosticSeverity::HINT,
        DiagnosticCode::Unused => DiagnosticSeverity::HINT,
        DiagnosticCode::UndefinedGlobal => DiagnosticSeverity::ERROR,
        DiagnosticCode::Deprecated => DiagnosticSeverity::HINT,
        DiagnosticCode::AccessInvisible => DiagnosticSeverity::WARNING,
        DiagnosticCode::DiscardReturns => DiagnosticSeverity::WARNING,
        DiagnosticCode::DisableGlobalDefine => DiagnosticSeverity::ERROR,
        DiagnosticCode::UndefinedField => DiagnosticSeverity::WARNING,
        DiagnosticCode::LocalConstReassign => DiagnosticSeverity::ERROR,
        DiagnosticCode::DuplicateType => DiagnosticSeverity::WARNING,
        DiagnosticCode::AnnotationUsageError => DiagnosticSeverity::ERROR,
        _ => DiagnosticSeverity::WARNING,
    }
}

pub fn is_code_default_enable(code: &DiagnosticCode) -> bool {
    match code {
        DiagnosticCode::InjectFieldFail => false,
        DiagnosticCode::DisableGlobalDefine => false,
        // DiagnosticCode::UndefinedField => false,
        DiagnosticCode::IterVariableReassign => false,
        DiagnosticCode::CodeStyleCheck => false,
        // ... handle other variants
        _ => true,
    }
}
