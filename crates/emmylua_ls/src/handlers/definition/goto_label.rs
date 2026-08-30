//! # goto_label — pure-salsa label definition lookup (same-named label within the same closure).

use emmylua_code_analysis::{SalsaDatabase, SalsaSemanticModel};
use emmylua_parser::{LuaAstNode, LuaGotoStat, LuaLabelStat, LuaSyntaxToken};
use lsp_types::{GotoDefinitionResponse, Location};

use crate::handlers::common::label_definition_range;

pub(super) fn goto_label_definition(
    model: &SalsaSemanticModel<'_>,
    salsa: &SalsaDatabase,
    token: &LuaSyntaxToken,
) -> Option<GotoDefinitionResponse> {
    let parent = token.parent()?;
    if LuaGotoStat::cast(parent.clone()).is_none() && LuaLabelStat::cast(parent.clone()).is_none() {
        return None;
    }

    let label_range = label_definition_range(model, token)?;
    let document = salsa.document(model.file_id())?;
    let uri = document.get_uri()?;
    Some(GotoDefinitionResponse::Scalar(Location {
        uri,
        range: document.to_lsp_range(label_range)?,
    }))
}
