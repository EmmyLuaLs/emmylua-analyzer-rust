use super::{
    SalsaDeclTreeSummary, SalsaDocOwnerBindingIndexSummary, SalsaDocSummary,
    SalsaDocTypeIndexSummary, SalsaFlowSummary, SalsaGlobalSummary, SalsaMemberIndexSummary,
    SalsaModuleSummary, SalsaPropertyIndexSummary, SalsaSignatureIndexSummary,
    SalsaTableShapeIndexSummary, SalsaUseSiteIndexSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaFileSummary {
    pub file_id: u32,
    pub decl_tree: SalsaDeclTreeSummary,
    pub globals: SalsaGlobalSummary,
    pub members: SalsaMemberIndexSummary,
    pub properties: SalsaPropertyIndexSummary,
    pub table_shapes: SalsaTableShapeIndexSummary,
    pub doc: SalsaDocSummary,
    pub doc_types: SalsaDocTypeIndexSummary,
    pub signatures: SalsaSignatureIndexSummary,
    pub doc_owner_bindings: SalsaDocOwnerBindingIndexSummary,
    pub use_sites: SalsaUseSiteIndexSummary,
    pub flow: SalsaFlowSummary,
    pub module: Option<SalsaModuleSummary>,
}
