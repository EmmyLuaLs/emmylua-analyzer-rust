use rowan::TextSize;
use smol_str::SmolStr;

use crate::{
    FileId, SalsaDocOwnerResolveSummary, SalsaDocTagPropertySummary, SalsaPropertySummary,
    SalsaSemanticTargetSummary,
};

use super::super::{
    SalsaDeclId, SalsaGlobalFunctionSummary, SalsaGlobalVariableSummary, SalsaMemberPathSummary,
    SalsaMemberSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaModuleSummary {
    pub file_id: FileId,
    pub export_target: Option<SalsaExportTargetSummary>,
    pub export: Option<SalsaModuleExportSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaExportTargetSummary {
    LocalName(SmolStr),
    Member(SalsaMemberPathSummary),
    Closure(TextSize),
    Table(TextSize),
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaModuleExportSummary {
    LocalDecl { name: SmolStr, decl_id: SalsaDeclId },
    Member(SalsaMemberSummary),
    GlobalVariable(SalsaGlobalVariableSummary),
    GlobalFunction(SalsaGlobalFunctionSummary),
    Closure { signature_offset: TextSize },
    Table { table_offset: TextSize },
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaModuleExportResolveStateSummary {
    Partial,
    Resolved,
    RecursiveDependency,
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaModuleExportQuerySummary {
    pub export_target: SalsaExportTargetSummary,
    pub export: Option<SalsaModuleExportSummary>,
    pub semantic_target: Option<SalsaSemanticTargetSummary>,
    pub doc_owners: Vec<SalsaDocOwnerResolveSummary>,
    pub tag_properties: Vec<SalsaDocTagPropertySummary>,
    pub properties: Vec<SalsaPropertySummary>,
    pub state: SalsaModuleExportResolveStateSummary,
}
