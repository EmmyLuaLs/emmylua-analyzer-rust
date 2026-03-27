mod expr;
mod layout;
mod line_breaks;
mod model;
mod render;
mod sequence;
mod spacing;
mod trivia;

use std::cell::Cell;

use crate::config::LuaFormatConfig;
use crate::ir::{DocIR, GroupId};
use emmylua_parser::LuaChunk;

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

pub fn format_chunk(ctx: &FormatContext, chunk: &LuaChunk) -> Vec<DocIR> {
    let spacing_plan = spacing::analyze_root_spacing(ctx, chunk);
    let layout_plan = layout::analyze_root_layout(ctx, chunk, spacing_plan);
    let final_plan = line_breaks::analyze_root_line_breaks(ctx, chunk, layout_plan);
    render::render_root(ctx, chunk, &final_plan)
}
