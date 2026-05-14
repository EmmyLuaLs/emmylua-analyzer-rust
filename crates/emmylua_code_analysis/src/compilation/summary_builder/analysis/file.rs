use emmylua_parser::LuaChunk;

use crate::FileId;

use super::super::{
    SalsaDeclTreeSummary, SalsaDocOwnerBindingIndexSummary, SalsaDocSummary,
    SalsaDocTypeIndexSummary, SalsaFileSummary, SalsaFlowSummary, SalsaGlobalSummary,
    SalsaMemberIndexSummary, SalsaModuleSummary, SalsaPropertyIndexSummary,
    SalsaSignatureIndexSummary, SalsaTableShapeIndexSummary, SalsaUseSiteIndexSummary,
};
use super::{
    decl::analyze_decl_summary, doc::analyze_doc_summary, doc_type::analyze_doc_type_summary,
    flow::analyze_flow_summary, module::analyze_module_summary,
    owner_binding::analyze_doc_owner_binding_summary, property::analyze_property_summary,
    signature::analyze_signature_summary, table_shape::analyze_table_shape_summary,
    use_site::analyze_use_site_summary,
};

pub fn analyze_file_summary(file_id: FileId, chunk: LuaChunk) -> SalsaFileSummary {
    let mut analysis = SummaryFileAnalysis::new(file_id, chunk);
    analysis.run();
    analysis.finish()
}

struct SummaryFileAnalysis {
    file_id: FileId,
    chunk: LuaChunk,
    decl_tree: SalsaDeclTreeSummary,
    globals: SalsaGlobalSummary,
    members: SalsaMemberIndexSummary,
    properties: SalsaPropertyIndexSummary,
    table_shapes: SalsaTableShapeIndexSummary,
    doc: SalsaDocSummary,
    doc_types: SalsaDocTypeIndexSummary,
    signatures: SalsaSignatureIndexSummary,
    doc_owner_bindings: SalsaDocOwnerBindingIndexSummary,
    use_sites: SalsaUseSiteIndexSummary,
    flow: SalsaFlowSummary,
    module: Option<SalsaModuleSummary>,
}

impl SummaryFileAnalysis {
    fn new(file_id: FileId, chunk: LuaChunk) -> Self {
        Self {
            file_id,
            chunk,
            decl_tree: SalsaDeclTreeSummary {
                file_id,
                decls: Vec::new(),
                scopes: Vec::new(),
            },
            globals: SalsaGlobalSummary {
                entries: Vec::new(),
                variables: Vec::new(),
                functions: Vec::new(),
                members: Vec::new(),
            },
            members: SalsaMemberIndexSummary {
                members: Vec::new(),
            },
            properties: SalsaPropertyIndexSummary {
                properties: Vec::new(),
            },
            table_shapes: SalsaTableShapeIndexSummary { tables: Vec::new() },
            doc: SalsaDocSummary {
                type_defs: Vec::new(),
                type_tags: Vec::new(),
                fields: Vec::new(),
                generics: Vec::new(),
                params: Vec::new(),
                returns: Vec::new(),
                operators: Vec::new(),
                tags: Vec::new(),
            },
            doc_types: SalsaDocTypeIndexSummary { types: Vec::new() },
            signatures: SalsaSignatureIndexSummary {
                signatures: Vec::new(),
                calls: Vec::new(),
            },
            doc_owner_bindings: SalsaDocOwnerBindingIndexSummary {
                bindings: Vec::new(),
            },
            use_sites: SalsaUseSiteIndexSummary {
                names: Vec::new(),
                members: Vec::new(),
                calls: Vec::new(),
            },
            flow: SalsaFlowSummary {
                branch_count: 0,
                loop_count: 0,
                return_count: 0,
                block_count: 0,
                break_count: 0,
                goto_count: 0,
                label_count: 0,
                blocks: Vec::new(),
                statements: Vec::new(),
                conditions: Vec::new(),
                branches: Vec::new(),
                loops: Vec::new(),
                returns: Vec::new(),
                breaks: Vec::new(),
                gotos: Vec::new(),
                labels: Vec::new(),
            },
            module: None,
        }
    }

    fn run(&mut self) {
        self.run_decl_stage();
        self.run_property_stage();
        self.run_table_shape_stage();
        self.run_doc_stage();
        self.run_doc_type_stage();
        self.run_signature_stage();
        self.run_doc_owner_binding_stage();
        self.run_use_site_stage();
        self.run_flow_stage();
        self.run_module_stage();
    }

    fn run_decl_stage(&mut self) {
        let decl_analysis = analyze_decl_summary(self.file_id, self.chunk.clone());
        self.decl_tree = decl_analysis.decl_tree;
        self.globals = decl_analysis.globals;
        self.members = decl_analysis.members;
    }

    fn run_property_stage(&mut self) {
        self.properties =
            analyze_property_summary(&self.decl_tree, &self.members, self.chunk.clone());
    }

    fn run_doc_stage(&mut self) {
        self.doc = analyze_doc_summary(self.chunk.clone());
    }

    fn run_table_shape_stage(&mut self) {
        self.table_shapes = analyze_table_shape_summary(self.chunk.clone());
    }

    fn run_doc_type_stage(&mut self) {
        self.doc_types = analyze_doc_type_summary(self.chunk.clone());
    }

    fn run_signature_stage(&mut self) {
        self.signatures = analyze_signature_summary(self.chunk.clone(), &self.doc);
    }

    fn run_doc_owner_binding_stage(&mut self) {
        self.doc_owner_bindings = analyze_doc_owner_binding_summary(
            &self.decl_tree,
            &self.members,
            &self.properties,
            &self.signatures,
            self.chunk.clone(),
        );
    }

    fn run_use_site_stage(&mut self) {
        self.use_sites = analyze_use_site_summary(&self.decl_tree, self.chunk.clone());
    }

    fn run_flow_stage(&mut self) {
        self.flow = analyze_flow_summary(self.chunk.clone());
    }

    fn run_module_stage(&mut self) {
        self.module = analyze_module_summary(
            self.file_id,
            &self.decl_tree,
            &self.globals,
            &self.members,
            self.chunk.clone(),
        );
    }

    fn finish(self) -> SalsaFileSummary {
        SalsaFileSummary {
            file_id: self.file_id.id,
            decl_tree: self.decl_tree,
            globals: self.globals,
            members: self.members,
            properties: self.properties,
            table_shapes: self.table_shapes,
            doc: self.doc,
            doc_types: self.doc_types,
            signatures: self.signatures,
            doc_owner_bindings: self.doc_owner_bindings,
            use_sites: self.use_sites,
            flow: self.flow,
            module: self.module,
        }
    }
}
