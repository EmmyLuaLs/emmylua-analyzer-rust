use std::{ops::Deref, sync::Arc};

use emmylua_parser::{LuaAstNode, LuaAstToken, LuaLocalName};
use lsp_types::{Diagnostic, NumberOrString};
use tokio_util::sync::CancellationToken;

use crate::{
    DbIndex, DiagnosticCode, EmmyLuaAnalysis, Emmyrc, FileId, LuaType, RenderLevel,
    VirtualUrlGenerator, humanize_type, is_assignable,
};

/// A virtual workspace for testing.
#[allow(unused)]
#[derive(Debug)]
pub struct VirtualWorkspace {
    pub virtual_url_generator: VirtualUrlGenerator,
    pub analysis: EmmyLuaAnalysis,
    id_counter: u32,
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
        }
    }

    pub fn def(&mut self, content: &str) -> FileId {
        let id = self.id_counter;
        self.id_counter += 1;
        let uri = self
            .virtual_url_generator
            .new_uri(&format!("virtual_{}.lua", id));

        self.analysis
            .update_file_by_uri(&uri, Some(content.to_string()))
            .expect("File ID must be present")
    }

    pub fn def_file(&mut self, file_name: &str, content: &str) -> FileId {
        let uri = self.virtual_url_generator.new_uri(file_name);

        self.analysis
            .update_file_by_uri(&uri, Some(content.to_string()))
            .expect("File ID must be present")
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

        file_ids
    }

    pub fn get_emmyrc(&self) -> Emmyrc {
        self.analysis.emmyrc.deref().clone()
    }

    pub fn update_emmyrc(&mut self, emmyrc: Emmyrc) {
        self.analysis.update_config(Arc::new(emmyrc));
    }

    pub fn get_node<Ast: LuaAstNode>(&self, file_id: FileId) -> Ast {
        let tree = self
            .analysis
            .compilation
            .get_db()
            .get_vfs()
            .get_syntax_tree(&file_id)
            .expect("Tree must exist");
        tree.get_chunk_node()
            .descendants::<Ast>()
            .next()
            .expect("Node must exist")
    }

    pub fn ty(&mut self, type_repr: &str) -> LuaType {
        let virtual_content = format!("---@type {}\nlocal t", type_repr);
        let file_id = self.def(&virtual_content);
        let local_name = self.get_node::<LuaLocalName>(file_id);
        let semantic_model = self
            .analysis
            .compilation
            .get_semantic_model(file_id)
            .expect("Semantic model must exist");
        let token = local_name.get_name_token().expect("Name token must exist");
        let info = semantic_model
            .get_semantic_info(token.syntax().clone().into())
            .expect("Semantic info must exist");
        info.typ
    }

    pub fn expr_ty(&mut self, expr: &str) -> LuaType {
        let virtual_content = format!("local t = {}", expr);
        let file_id = self.def(&virtual_content);
        let local_name = self.get_node::<LuaLocalName>(file_id);
        let semantic_model = self
            .analysis
            .compilation
            .get_semantic_model(file_id)
            .expect("Model must exist");
        let token = local_name.get_name_token().expect("Name token must exist");
        let info = semantic_model
            .get_semantic_info(token.syntax().clone().into())
            .expect("Semantic info must exist");
        info.typ
    }

    pub fn check_type(&self, source: &LuaType, target: &LuaType) -> bool {
        is_assignable(self.analysis.compilation.get_db(), source, target)
    }

    pub fn enable_check(&mut self, diagnostic_code: DiagnosticCode) {
        let mut emmyrc = Emmyrc::default();
        emmyrc.diagnostics.enables.push(diagnostic_code);
        self.analysis.diagnostic.update_config(Arc::new(emmyrc));
    }

    pub fn get_diagnostics(
        &mut self,
        diagnostic_code: DiagnosticCode,
        block_str: &str,
    ) -> Vec<Diagnostic> {
        self.analysis.diagnostic.enable_only(diagnostic_code);
        let file_id = self.def(block_str);
        let code_string = Some(NumberOrString::String(
            diagnostic_code.get_name().to_string(),
        ));
        self.analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap_or_default()
            .into_iter()
            .filter(|diagnostic| diagnostic.code == code_string)
            .collect()
    }

    pub fn get_first_diagnostic(
        &mut self,
        diagnostic_code: DiagnosticCode,
        block_str: &str,
    ) -> Option<Diagnostic> {
        self.get_diagnostics(diagnostic_code, block_str)
            .into_iter()
            .next()
    }

    pub fn has_no_diagnostic(&mut self, diagnostic_code: DiagnosticCode, block_str: &str) -> bool {
        self.get_first_diagnostic(diagnostic_code, block_str)
            .is_none()
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

    pub fn humanize_type(&self, ty: LuaType) -> String {
        let db = &self.analysis.compilation.get_db();
        humanize_type(db, &ty, RenderLevel::Brief)
    }

    pub fn humanize_type_detailed(&self, ty: LuaType) -> String {
        let db = &self.analysis.compilation.get_db();
        humanize_type(db, &ty, RenderLevel::Detailed)
    }

    pub fn get_db_mut(&mut self) -> &mut DbIndex {
        (self.analysis.compilation.get_db_mut()) as _
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
