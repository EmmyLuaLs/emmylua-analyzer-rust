mod block;
mod comment;
mod expression;
mod statement;
mod trivia;

use crate::config::LuaFormatConfig;
use crate::ir::DocIR;
use emmylua_parser::LuaChunk;

pub use block::format_block;
pub use statement::format_body_end_with_parent;

/// Formatting context, shared throughout the formatting process
pub struct FormatContext<'a> {
    pub config: &'a LuaFormatConfig,
}

impl<'a> FormatContext<'a> {
    pub fn new(config: &'a LuaFormatConfig) -> Self {
        Self { config }
    }
}

/// Format a chunk (root node of the file)
pub fn format_chunk(ctx: &FormatContext, chunk: &LuaChunk) -> Vec<DocIR> {
    let mut docs = Vec::new();

    if let Some(block) = chunk.get_block() {
        docs.extend(format_block(ctx, &block));
    }

    // Ensure file ends with a newline
    if ctx.config.insert_final_newline {
        docs.push(DocIR::HardLine);
    }

    docs
}
