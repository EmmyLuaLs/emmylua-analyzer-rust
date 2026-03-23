mod block;
mod comment;
mod expression;
mod sequence;
pub mod spacing;
mod statement;
mod tokens;
mod trivia;

use std::cell::Cell;

use crate::config::LuaFormatConfig;
use crate::ir::{self, DocIR, GroupId};
use emmylua_parser::{LuaAstNode, LuaChunk, LuaKind, LuaTokenKind};

pub use block::format_block;
pub use statement::format_body_end_with_parent;

/// Formatting context, shared throughout the formatting process
pub struct FormatContext<'a> {
    pub config: &'a LuaFormatConfig,
    next_group_id: Cell<u32>,
}

impl<'a> FormatContext<'a> {
    pub fn new(config: &'a LuaFormatConfig) -> Self {
        Self {
            config,
            next_group_id: Cell::new(0),
        }
    }

    pub fn next_group_id(&self) -> GroupId {
        let next = self.next_group_id.get();
        self.next_group_id.set(next + 1);
        GroupId(next)
    }
}

/// Format a chunk (root node of the file)
pub fn format_chunk(ctx: &FormatContext, chunk: &LuaChunk) -> Vec<DocIR> {
    let mut docs = Vec::new();

    // Emit shebang if present (TkShebang is a trivia token in the syntax tree)
    if let Some(first_token) = chunk.syntax().first_token()
        && first_token.kind() == LuaKind::Token(LuaTokenKind::TkShebang)
    {
        docs.push(ir::text(first_token.text()));
        docs.push(DocIR::HardLine);
    }

    if let Some(block) = chunk.get_block() {
        docs.extend(format_block(ctx, &block));
    }

    // Ensure file ends with a newline
    if ctx.config.output.insert_final_newline {
        docs.push(DocIR::HardLine);
    }

    docs
}
