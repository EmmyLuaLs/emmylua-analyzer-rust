use std::{ops::Deref, sync::Arc};

use emmylua_parser::{LuaAstNode, LuaAstToken, LuaLocalName};
use lsp_types::NumberOrString;
use tokio_util::sync::CancellationToken;

use crate::{DiagnosticCode, EmmyLuaAnalysis, Emmyrc, FileId, LuaType, VirtualUrlGenerator};

/// A virtual workspace for testing (M4: via the salsa analysis layer only).
#[allow(unused)]
#[derive(Debug)]
pub struct VirtualWorkspace {
    pub virtual_url_generator: VirtualUrlGenerator,
    pub analysis: EmmyLuaAnalysis,
    id_counter: u32,
    last_file_id: Option<FileId>,
}

#[allow(unused, clippy::unwrap_used)]
impl Default for VirtualWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualWorkspace {
    pub fn new() -> Self {
        let generator = VirtualUrlGenerator::new();
        let mut analysis = EmmyLuaAnalysis::new();
        let base = &generator.base;
        analysis.add_main_workspace(base.clone());
        VirtualWorkspace {
            virtual_url_generator: generator,
            analysis,
            id_counter: 0,
            last_file_id: None,
        }
    }

    pub fn new_with_init_std_lib() -> Self {
        let generator = VirtualUrlGenerator::new();
        let mut analysis = EmmyLuaAnalysis::new();
        analysis.init_std_lib(None);
        let base = &generator.base;
        analysis.add_main_workspace(base.clone());
        VirtualWorkspace {
            virtual_url_generator: generator,
            analysis,
            id_counter: 0,
            last_file_id: None,
        }
    }

    pub fn def(&mut self, content: &str) -> FileId {
        let id = self.id_counter;
        self.id_counter += 1;
        let uri = self
            .virtual_url_generator
            .new_uri(&format!("virtual_{}.lua", id));

        let file_id = self
            .analysis
            .update_file_by_uri(&uri, Some(content.to_string()))
            .expect("File ID must be present");
        self.last_file_id = Some(file_id);
        file_id
    }

    pub fn def_file(&mut self, file_name: &str, content: &str) -> FileId {
        let uri = self.virtual_url_generator.new_uri(file_name);

        let file_id = self
            .analysis
            .update_file_by_uri(&uri, Some(content.to_string()))
            .expect("File ID must be present");
        self.last_file_id = Some(file_id);
        file_id
    }

    pub fn def_files(&mut self, files: Vec<(&str, &str)>) -> Vec<FileId> {
        let file_infos = files
            .iter()
            .map(|(file_name, content)| {
                let uri = self.virtual_url_generator.new_uri(file_name);
                (uri, Some(content.to_string()))
            })
            .collect();

        let mut file_ids = self.analysis.update_files_by_uri_sorted(file_infos);
        file_ids.sort();
        self.last_file_id = file_ids.last().copied();

        file_ids
    }

    pub fn get_emmyrc(&self) -> Emmyrc {
        self.analysis.emmyrc.deref().clone()
    }

    pub fn update_emmyrc(&mut self, emmyrc: Emmyrc) {
        self.analysis.update_config(Arc::new(emmyrc));
    }

    pub fn get_node<Ast: LuaAstNode>(&self, file_id: FileId) -> Ast {
        let model = self.analysis.semantic_model(file_id).expect("salsa model");
        let chunk = model.chunk().expect("chunk");
        chunk.descendants::<Ast>().next().expect("Node must exist")
    }

    pub fn ty(&mut self, type_repr: &str) -> LuaType {
        let virtual_content = format!("---@type {}\nlocal t", type_repr);
        let file_id = self.def(&virtual_content);
        let local_name = self.get_node::<LuaLocalName>(file_id);
        let model = self.analysis.semantic_model(file_id).expect("salsa model");
        let token = local_name.get_name_token().expect("Name token must exist");
        let decl = model
            .decl_by_offset(token.get_position())
            .expect("decl must exist");
        model.type_of_decl(&decl).expect("type must exist")
    }

    pub fn expr_ty(&mut self, expr: &str) -> LuaType {
        let virtual_content = format!("local t = {}", expr);
        let file_id = self.def(&virtual_content);
        let local_name = self.get_node::<LuaLocalName>(file_id);
        let model = self.analysis.semantic_model(file_id).expect("salsa model");
        let token = local_name.get_name_token().expect("Name token must exist");
        let decl = model
            .decl_by_offset(token.get_position())
            .expect("decl must exist");
        model.type_of_decl(&decl).expect("type must exist")
    }

    pub fn humanize_type(&self, ty: LuaType) -> String {
        let Some(file_id) = self.last_file_id else {
            return format!("{ty:?}");
        };
        let Some(model) = self.analysis.semantic_model(file_id) else {
            return format!("{ty:?}");
        };
        crate::semantic_model::render::humanize_type(&model, &ty)
    }

    pub fn humanize_type_detailed(&self, ty: LuaType) -> String {
        let Some(file_id) = self.last_file_id else {
            return format!("{ty:?}");
        };
        let Some(model) = self.analysis.semantic_model(file_id) else {
            return format!("{ty:?}");
        };
        crate::semantic_model::render::humanize_type_detailed(&model, &ty)
    }

    pub fn check_type(&self, source: &LuaType, target: &LuaType) -> bool {
        let Some(file_id) = self.last_file_id else {
            return false;
        };
        let Some(model) = self.analysis.semantic_model(file_id) else {
            return false;
        };
        model.type_check_subtype(source, target)
    }

    pub fn enable_check(&mut self, diagnostic_code: DiagnosticCode) {
        let mut emmyrc = Emmyrc::default();
        emmyrc.diagnostics.enables.push(diagnostic_code);
        self.analysis.diagnostic.update_config(Arc::new(emmyrc));
    }

    /// Only run the check for the corresponding diagnostic code; the corresponding `Checker` must add that code to its `const CODES`.
    pub fn has_no_diagnostic(&mut self, diagnostic_code: DiagnosticCode, block_str: &str) -> bool {
        // Only enable the corresponding diagnostic.
        self.analysis.diagnostic.enable_only(diagnostic_code);
        let file_id = self.def(block_str);
        let result = self
            .analysis
            .diagnose_file(file_id, CancellationToken::new());
        if let Some(diagnostics) = result {
            let code_string = Some(NumberOrString::String(
                diagnostic_code.get_name().to_string(),
            ));
            for diagnostic in diagnostics {
                if diagnostic.code == code_string {
                    return false;
                }
            }
        }

        true
    }

    pub fn has_no_diagnostic_in_namespace(
        &mut self,
        diagnostic_code: DiagnosticCode,
        block_str: &str,
    ) -> bool {
        self.has_no_diagnostic(
            diagnostic_code,
            &format!(
                "---@namespace TestNamespace{}\n{}",
                self.id_counter, block_str
            ),
        )
    }

    pub fn enable_full_diagnostic(&mut self) {
        let mut emmyrc = Emmyrc::default();
        let mut enables = emmyrc.diagnostics.enables;
        enables.push(DiagnosticCode::IncompleteSignatureDoc);
        enables.push(DiagnosticCode::MissingGlobalDoc);
        emmyrc.diagnostics.enables = enables;
        self.analysis.diagnostic.update_config(Arc::new(emmyrc));
    }
}

#[cfg(test)]
mod tests {
    use crate::LuaType;

    use super::VirtualWorkspace;

    #[test]
    fn test_basic() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@class a
        "#,
        );

        let ty = ws.ty("a");
        match ty {
            LuaType::Ref(i) => {
                assert_eq!(i.get_name(), "a");
            }
            _ => unreachable!(),
        }
    }
}
