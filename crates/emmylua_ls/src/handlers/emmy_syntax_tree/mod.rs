mod emmy_syntax_tree_request;

use std::str::FromStr;

use emmylua_parser::LuaAstNode;
use lsp_types::Uri;
use tokio_util::sync::CancellationToken;

use crate::{
    context::{CancelStrategy, RequestOutcome, ServerContextSnapshot, snapshot_query},
    handlers::emmy_syntax_tree::emmy_syntax_tree_request::{
        EmmySyntaxTreeParams, SyntaxTreeResponse,
    },
};
pub use emmy_syntax_tree_request::*;

pub async fn on_emmy_syntax_tree_handler(
    context: ServerContextSnapshot,
    params: EmmySyntaxTreeParams,
    cancel_token: CancellationToken,
) -> RequestOutcome<SyntaxTreeResponse> {
    let Ok(uri) = Uri::from_str(&params.uri) else {
        return RequestOutcome::Missing;
    };
    snapshot_query(
        context.analysis(),
        CancelStrategy::RetryAfter(std::time::Duration::from_millis(30)),
        cancel_token,
        move |analysis| {
            let file_id = analysis.get_file_id(&uri)?;
            let semantic_model = analysis.semantic_model(file_id)?;
            let root = semantic_model.chunk()?;
            let content = format!("{:#?}", root.syntax());
            Some(SyntaxTreeResponse { content })
        },
    )
    .await
}
