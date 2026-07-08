use emmylua_code_analysis::LuaCompilation;
use lsp_types::{CodeLens, Command, Location, Range, Uri};

use crate::{
    context::ClientId,
    handlers::references::{search_decl_references, search_member_references},
};

use super::CodeLensData;

// VSCode does not support calling editor.action.showReferences directly through LSP,
// it can only be converted through the VSCode plugin
const VSCODE_COMMAND_NAME: &str = "emmy.showReferences";
// In fact, VSCode ultimately uses this command
const OTHER_COMMAND_NAME: &str = "editor.action.showReferences";

pub fn resolve_code_lens(
    compilation: &LuaCompilation,
    code_lens: CodeLens,
    client_id: ClientId,
) -> Option<CodeLens> {
    let data = code_lens.data.as_ref()?;
    let data = serde_json::from_value(data.clone()).ok()?;
    match data {
        CodeLensData::Member(member_id) => {
            let file_id = member_id.file_id;
            let semantic_model = compilation.get_semantic_model(file_id)?;
            let mut results = Vec::new();
            search_member_references(&semantic_model, compilation, member_id, true, &mut results);
            let mut ref_count = results.len();
            ref_count = ref_count.saturating_sub(1);
            let uri = semantic_model.get_document().get_uri();
            let command = make_usage_command(uri, code_lens.range, ref_count, client_id, results);

            Some(CodeLens {
                range: code_lens.range,
                command: Some(command),
                data: None,
            })
        }
        CodeLensData::DeclId(decl_id) => {
            let file_id = decl_id.file_id;
            let semantic_model = compilation.get_semantic_model(file_id)?;
            let mut results = Vec::new();
            search_decl_references(&semantic_model, compilation, decl_id, &mut results);
            let ref_count = results.len();
            let uri = semantic_model.get_document().get_uri();
            let command = make_usage_command(uri, code_lens.range, ref_count, client_id, results);
            Some(CodeLens {
                range: code_lens.range,
                command: Some(command),
                data: None,
            })
        }
    }
}

fn get_command_name(client_id: ClientId) -> &'static str {
    match client_id {
        ClientId::VSCode => VSCODE_COMMAND_NAME,
        _ => OTHER_COMMAND_NAME,
    }
}

fn make_usage_command(
    uri: Uri,
    range: Range,
    ref_count: usize,
    client_id: ClientId,
    refs: Vec<Location>,
) -> Command {
    let title = format!(
        "{} usage{}",
        ref_count,
        if ref_count == 1 { "" } else { "s" }
    );
    let args = vec![
        serde_json::to_value(uri).unwrap(),
        serde_json::to_value(range.start).unwrap(),
        serde_json::to_value(refs).unwrap(),
    ];

    Command {
        title,
        command: get_command_name(client_id).to_string(),
        arguments: Some(args),
    }
}
