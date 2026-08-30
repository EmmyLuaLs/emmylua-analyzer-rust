use emmylua_code_analysis::SalsaDatabase;
use lsp_types::{CodeLens, Command, Location, Range, Uri};

use crate::{
    context::ClientId,
    handlers::common::{decl_reference_ranges, member_reference_ranges},
};

use super::build_code_lens::CodeLensData;

// VSCode does not support calling editor.action.showReferences directly through LSP,
// it can only be converted through the VSCode plugin
const VSCODE_COMMAND_NAME: &str = "emmy.showReferences";
// In fact, VSCode ultimately uses this command
const OTHER_COMMAND_NAME: &str = "editor.action.showReferences";

pub fn resolve_code_lens(
    salsa: &SalsaDatabase,
    code_lens: CodeLens,
    client_id: ClientId,
) -> Option<CodeLens> {
    let data = code_lens.data.as_ref()?;
    let data = serde_json::from_value(data.clone()).ok()?;
    match data {
        CodeLensData::Member(data) => {
            let semantic_id = data.to_semantic_id()?;
            let file_id = emmylua_code_analysis::FileId::new(data.file_id);
            let results = member_reference_ranges(salsa, &semantic_id, true)
                .into_iter()
                .filter_map(|(fid, range)| location_of(salsa, fid, range))
                .collect::<Vec<_>>();
            let mut ref_count = results.len();
            ref_count = ref_count.saturating_sub(1);
            let uri = salsa.document(file_id)?.get_uri()?;
            let command = make_usage_command(uri, code_lens.range, ref_count, client_id, results);

            Some(CodeLens {
                range: code_lens.range,
                command: Some(command),
                data: None,
            })
        }
        CodeLensData::DeclId(data) => {
            let semantic_id = data.to_semantic_id()?;
            let file_id = emmylua_code_analysis::FileId::new(data.file_id);
            let results = decl_reference_ranges(salsa, &semantic_id, true)
                .into_iter()
                .filter_map(|(fid, range)| location_of(salsa, fid, range))
                .collect::<Vec<_>>();
            let ref_count = results.len();
            let uri = salsa.document(file_id)?.get_uri()?;
            let command = make_usage_command(uri, code_lens.range, ref_count, client_id, results);
            Some(CodeLens {
                range: code_lens.range,
                command: Some(command),
                data: None,
            })
        }
    }
}

fn location_of(
    salsa: &SalsaDatabase,
    file_id: emmylua_code_analysis::FileId,
    range: rowan::TextRange,
) -> Option<Location> {
    let document = salsa.document(file_id)?;
    Some(Location {
        uri: document.get_uri()?,
        range: document.to_lsp_range(range)?,
    })
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
